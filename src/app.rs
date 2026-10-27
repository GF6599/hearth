use std::path::PathBuf;
use std::sync::Arc;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::config::{self, Background, Config};
use crate::corpus::Corpus;
use crate::octant;
use crate::pane::{self, Direction, Pane, Panes, Split, advance};
use crate::picker::{Entry, Picker};
use crate::scene::{ALL, FRAMES_PER_TICK, Rate, Scene, SceneKind, Settings};
use crate::scenes::typing::Speed;
use crate::sound::Cue;
use crate::theme::{self, Palette};
use toml_edit::Value;

/// Frames a changed speed stays on screen: 16 ticks, about one second.
const NOTICE_FRAMES: u32 = 16 * FRAMES_PER_TICK;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Picking,
    Running,
}

pub struct App {
    /// The place of the theme in `THEMES`, so `t` knows the next one.
    theme: usize,
    background: Background,
    panes: Panes,
    corpus: Arc<Corpus>,
    speed: Speed,
    settings: Settings,
    /// One for each scene in `ALL`, so each keeps its own through a
    /// restart, Tab and the picker.
    rates: [Rate; ALL.len()],
    notice: u32,
    /// What the last change set: a speed or a theme name.
    notice_text: String,
    /// Where a changed speed is saved. `None` saves nothing, which is what
    /// a test that presses `+` needs.
    saving: Option<PathBuf>,
    /// Why the last change was not saved, shown with it in the notice.
    unsaved: Option<String>,
    /// The marked scene, running in the picker's pane. Built when the
    /// picker opens or the mark moves, so it always starts from its first
    /// frame.
    preview: Option<Box<dyn Scene>>,
    preview_credit: u32,
    mode: Mode,
    selected: usize,
    /// The first scene in the picker list on the last frame.
    picker_top: usize,
    /// What the user typed after `/` in the picker. `None` lists every
    /// scene.
    query: Option<String>,
    frames: u32,
    /// The focused pane came from a split and its picker has not started
    /// a scene yet, so Esc closes it.
    fresh: bool,
    /// The screen may differ from the last draw, so the next one is due.
    redraw: bool,
    /// The area of the last frame, so a resize draws the screen again.
    area: Rect,
    sound: bool,
    /// Why no sound can play, when none can.
    silent: Option<String>,
    /// The cues of the focused pane since the last `take_cues`.
    cues: Vec<Cue>,
    /// The cues of every other pane, dropped on each frame, so a pane that
    /// gets the focus does not play the keys it typed without it.
    unheard: Vec<Cue>,
    pub should_quit: bool,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("panes", &self.panes)
            .field("speed", &self.speed)
            .field("rates", &self.rates)
            .field("mode", &self.mode)
            .field("selected", &self.selected)
            .field("should_quit", &self.should_quit)
            .finish_non_exhaustive()
    }
}

impl App {
    pub fn new(kind: SceneKind, seed: u64, corpus: Arc<Corpus>) -> Self {
        Self::configured(Some(kind), seed, corpus, &Config::default())
    }

    pub fn picker(seed: u64, corpus: Arc<Corpus>) -> Self {
        Self::configured(None, seed, corpus, &Config::default())
    }

    /// Each scene is built once, with the config already known, because
    /// some scenes start work when they are built: the river runs `git log`.
    pub fn configured(
        scene: Option<SceneKind>,
        seed: u64,
        corpus: Arc<Corpus>,
        config: &Config,
    ) -> Self {
        let kind = scene.unwrap_or(ALL[0]);
        let theme = config.theme.unwrap_or(0);
        let speed = config.typing.unwrap_or(Speed::DEFAULT);
        let settings = Settings {
            lengths: config.hourglass,
            shortest: config.shortest,
        };
        let mut app = Self {
            theme,
            background: config.background,
            panes: Panes::new(Pane::new(
                kind,
                seed,
                palette(theme, config.background),
                config.background,
                &corpus,
                speed,
                settings,
            )),
            corpus,
            speed,
            settings,
            rates: config.rates.map(|saved| saved.unwrap_or(Rate::NORMAL)),
            notice: 0,
            notice_text: String::new(),
            saving: None,
            unsaved: None,
            preview: None,
            preview_credit: 0,
            mode: Mode::Running,
            selected: slot_of(kind),
            picker_top: 0,
            query: None,
            frames: 0,
            fresh: false,
            redraw: true,
            area: Rect::default(),
            sound: config.sound.unwrap_or(true),
            silent: None,
            cues: Vec::new(),
            unheard: Vec::new(),
            should_quit: false,
        };
        if scene.is_none() {
            app.open_picker();
        }
        app
    }

    fn palette(&self) -> &'static Palette {
        palette(self.theme, self.background)
    }

    /// Every pane starts its scene again in the theme's colours, because
    /// some scenes keep the colour of each cell they drew.
    fn repaint(&mut self) {
        let palette = self.palette();
        for pane in self.panes.iter_mut() {
            pane.repaint(
                palette,
                self.background,
                &self.corpus,
                self.speed,
                self.settings,
            );
        }
        if self.mode == Mode::Picking {
            self.build_preview();
        }
    }

    pub fn next_theme(&mut self) {
        self.theme = (self.theme + 1) % theme::THEMES.len();
        self.repaint();
        let (name, _) = theme::THEMES[self.theme];
        let saved = match &self.saving {
            Some(path) => config::save_theme(path, name),
            None => Ok(()),
        };
        self.unsaved = saved.err().map(|e| e.to_string());
        self.notice_text = name.to_string();
        self.notice = NOTICE_FRAMES;
    }

    pub fn sound_on(&self) -> bool {
        self.sound
    }

    pub fn toggle_sound(&mut self) {
        self.sound = !self.sound;
        let saved = match &self.saving {
            Some(path) => config::save_sound(path, self.sound),
            None => Ok(()),
        };
        self.unsaved = saved.err().map(|e| e.to_string());
        self.notice_text = match (self.sound, &self.silent) {
            (false, _) => "sound off".to_string(),
            (true, None) => "sound on".to_string(),
            (true, Some(reason)) => format!("sound on, no sound: {reason}"),
        };
        self.notice = NOTICE_FRAMES;
        self.redraw = true;
    }

    /// No sound can play, for `reason`. The notice says so while sound is
    /// on, and a user who turned it off hears of it only on turning it on.
    pub fn without_sound(&mut self, reason: &str) {
        self.silent = Some(reason.to_string());
        if self.sound {
            self.unsaved = None;
            self.notice_text = format!("no sound: {reason}");
            self.notice = NOTICE_FRAMES;
            self.redraw = true;
        }
    }

    pub fn take_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    pub fn saving_to(mut self, path: PathBuf) -> Self {
        self.saving = Some(path);
        self
    }

    /// Whether the screen may differ from the last draw. A frame that asks
    /// for no draw shows the same picture, so the loop skips its render,
    /// its diff and its write.
    pub fn take_redraw(&mut self) -> bool {
        std::mem::take(&mut self.redraw)
    }

    /// An event can change anything on the screen, and a resize reaches the
    /// terminal only through a draw.
    pub fn request_redraw(&mut self) {
        self.redraw = true;
    }

    pub fn is_picking(&self) -> bool {
        self.mode == Mode::Picking
    }

    /// Runs the frames of one tick, so a test can count in the unit the
    /// scenes count in.
    pub fn tick(&mut self, area: Rect) {
        for _ in 0..FRAMES_PER_TICK {
            self.frame(area);
        }
    }

    /// The pane under the picker holds still, and the picker's preview
    /// runs in its place.
    pub fn frame(&mut self, area: Rect) {
        self.frames = self.frames.wrapping_add(1);
        // The notice holds still while it counts down, so only the frame
        // that takes it off the screen needs a draw.
        self.redraw |= self.notice == 1 || area != self.area;
        self.notice = self.notice.saturating_sub(1);
        self.area = area;
        let layout = self.panes.layout(area);
        let picking = self.is_picking().then(|| self.panes.focus());
        // The picker's title glows and its preview runs on every frame.
        self.redraw |= picking.is_some();
        let focus = self.panes.focus();
        for (index, (pane, inside)) in self.panes.iter_mut().zip(&layout.panes).enumerate() {
            if Some(index) != picking {
                self.redraw |= pane.tick(self.rates[slot_of(pane.kind)], *inside);
            }
            let heard = index == focus && picking.is_none();
            pane.drain_cues(if heard {
                &mut self.cues
            } else {
                &mut self.unheard
            });
        }
        if let Some(focus) = picking {
            let kind = ALL[self.selected];
            let rate = self.rates[self.selected];
            let inside = layout.panes[focus];
            let (preview, top) = self
                .with_picker(|picker| (picker.preview_area(inside), picker.first_in_view(inside)));
            self.picker_top = top;
            if let (Some(scene), Some(preview)) = (&mut self.preview, preview) {
                advance(&mut **scene, kind, rate, &mut self.preview_credit, preview);
                scene.drain_cues(&mut self.unheard);
            }
        }
        self.unheard.clear();
    }

    fn with_picker<R>(&self, f: impl FnOnce(Picker) -> R) -> R {
        let listed = self.listed();
        let speeds: Vec<String> = ALL.iter().map(|kind| self.speed_label(*kind)).collect();
        let all: Vec<Entry> = ALL
            .iter()
            .zip(&speeds)
            .map(|(kind, speed)| Entry {
                name: kind.name(),
                summary: kind.summary(),
                speed,
                credit: kind.credit(),
            })
            .collect();
        let entries: Vec<Entry> = listed.iter().map(|slot| all[*slot]).collect();
        f(Picker {
            palette: self.palette(),
            entries: &entries,
            sized_by: &all,
            selected: listed
                .iter()
                .position(|slot| *slot == self.selected)
                .unwrap_or(0),
            top: self.picker_top,
            frame: self.frames,
            query: self.query.as_deref(),
        })
    }

    /// The places in `ALL` of the scenes the picker lists: every scene, or
    /// in a search the scenes whose name holds the typed letters in order,
    /// then those whose summary holds them as one run.
    fn listed(&self) -> Vec<usize> {
        let Some(query) = &self.query else {
            return (0..ALL.len()).collect();
        };
        let query = query.to_lowercase();
        let by_name = |kind: SceneKind| {
            let mut letters = kind.name().chars();
            query.chars().all(|wanted| letters.any(|c| c == wanted))
        };
        let by_summary = |kind: SceneKind| kind.summary().to_lowercase().contains(&query);
        let named = (0..ALL.len()).filter(|slot| by_name(ALL[*slot]));
        let summed = (0..ALL.len()).filter(|slot| !by_name(ALL[*slot]) && by_summary(ALL[*slot]));
        named.chain(summed).collect()
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        buf.set_style(area, Style::default().bg(self.palette().base));
        let layout = self.panes.layout(area);
        let picking = self.is_picking().then(|| self.panes.focus());
        for (index, (pane, inside)) in self.panes.iter().zip(&layout.panes).enumerate() {
            if Some(index) == picking {
                self.with_picker(|picker| {
                    let preview = picker.preview_area(*inside);
                    picker.render(*inside, buf);
                    if let (Some(scene), Some(preview)) = (&self.preview, preview) {
                        scene.render(preview, buf);
                    }
                });
            } else {
                pane.render(*inside, buf);
            }
        }
        pane::render_rules(&layout.rules, self.palette(), buf);
        let focused = layout.panes[self.panes.focus()];
        if self.panes.is_split() {
            pane::render_focus(focused, self.palette(), buf);
        }
        if picking.is_none() && self.notice > 0 {
            self.render_notice(focused, buf);
        }
        if self.background == Background::Terminal {
            self.clear_background(area, buf);
        }
    }

    /// Gives every cell in the theme's base the terminal's own background.
    /// The surface takes the base's colour in `FLAT_THEMES`, so it clears
    /// too.
    fn clear_background(&self, area: Rect, buf: &mut Buffer) {
        let base = self.palette().base;
        for position in area.positions() {
            octant::clear(&mut buf[position], base);
        }
    }

    /// A notice wider than the area is cut at its left edge, so the reason
    /// a save failed is never hidden by the speed in front of it.
    fn render_notice(&self, area: Rect, buf: &mut Buffer) {
        let mut notice = self.notice_text.clone();
        if let Some(reason) = &self.unsaved {
            notice = format!("{notice}, not saved: {reason}");
        }
        let width = u16::try_from(notice.chars().count()).unwrap_or(u16::MAX);
        let x = area.right() - width.min(area.width);
        let style = Style::default().fg(self.palette().subtle);
        buf.set_stringn(x, area.y, notice, usize::from(area.width), style);
    }

    fn speed_label(&self, kind: SceneKind) -> String {
        match kind {
            SceneKind::Typing => format!("{} cps", self.speed),
            SceneKind::Hourglass => self.settings.lengths.to_string(),
            _ => self.rates[slot_of(kind)].to_string(),
        }
    }

    fn slot(&self) -> usize {
        slot_of(self.panes.focused().kind)
    }

    pub fn restart(&mut self) {
        self.panes
            .focused_mut()
            .restart(&self.corpus, self.speed, self.settings);
    }

    pub fn next_scene(&mut self) {
        self.switch_to(self.panes.focused().kind.next());
    }

    pub fn previous_scene(&mut self) {
        self.switch_to(ALL[before(self.slot())]);
    }

    pub fn faster(&mut self) {
        self.change_speed(Speed::faster, Rate::faster);
    }

    pub fn slower(&mut self) {
        self.change_speed(Speed::slower, Rate::slower);
    }

    /// A scene that keeps real time has no speed to change, so it shows
    /// the time it has left instead, and nothing is saved.
    fn change_speed(&mut self, speed: fn(Speed) -> Speed, rate: fn(Rate) -> Rate) {
        if let Some(left) = self.panes.focused().time_left() {
            self.unsaved = None;
            self.notice_text = left;
            self.notice = NOTICE_FRAMES;
            return;
        }
        let saved = if self.panes.focused().kind == SceneKind::Typing {
            self.speed = speed(self.speed);
            for pane in self.panes.iter_mut() {
                pane.set_speed(self.speed);
            }
            self.save(Value::from(i64::from(self.speed.cps())))
        } else {
            let slot = self.slot();
            self.rates[slot] = rate(self.rates[slot]);
            self.save(Value::from(self.rates[slot].times()))
        };
        self.unsaved = saved.err().map(|e| e.to_string());
        self.notice_text = self.speed_label(self.panes.focused().kind);
        self.notice = NOTICE_FRAMES;
    }

    fn save(&self, speed: Value) -> std::io::Result<()> {
        match &self.saving {
            Some(path) => config::save_speed(path, self.panes.focused().kind, speed),
            None => Ok(()),
        }
    }

    pub fn open_picker(&mut self) {
        self.mode = Mode::Picking;
        self.build_preview();
    }

    pub fn is_searching(&self) -> bool {
        self.query.is_some()
    }

    pub fn start_search(&mut self) {
        self.query = Some(String::new());
        self.picker_top = 0;
    }

    pub fn type_in_search(&mut self, letter: char) {
        if let Some(query) = &mut self.query {
            query.push(letter);
        }
        self.search_changed();
    }

    pub fn erase_in_search(&mut self) {
        if let Some(query) = &mut self.query {
            query.pop();
        }
        self.search_changed();
    }

    pub fn end_search(&mut self) {
        self.query = None;
        self.picker_top = 0;
    }

    /// The mark stays on its scene while that scene still matches, and
    /// moves to the first match when it does not. With no match the pane
    /// shows nothing, because no scene would start.
    fn search_changed(&mut self) {
        self.picker_top = 0;
        let listed = self.listed();
        let Some(&first) = listed.first() else {
            self.preview = None;
            return;
        };
        let target = if listed.contains(&self.selected) {
            self.selected
        } else {
            first
        };
        if target != self.selected || self.preview.is_none() {
            self.selected = target;
            self.build_preview();
        }
    }

    pub fn select_previous(&mut self) {
        self.step(|at, _| at.saturating_sub(1));
    }

    pub fn select_next(&mut self) {
        self.step(|at, count| (at + 1).min(count - 1));
    }

    /// Tab walks the list round, as it walks the scenes round outside the
    /// picker.
    pub fn select_next_wrapped(&mut self) {
        self.step(|at, count| (at + 1) % count);
    }

    pub fn select_previous_wrapped(&mut self) {
        self.step(|at, count| (at + count - 1) % count);
    }

    fn step(&mut self, to: impl Fn(usize, usize) -> usize) {
        let listed = self.listed();
        if listed.is_empty() {
            return;
        }
        let at = listed.iter().position(|slot| *slot == self.selected);
        let next = at.map_or(0, |at| to(at, listed.len()));
        self.select(listed[next]);
    }

    fn select(&mut self, index: usize) {
        if index != self.selected {
            self.selected = index;
            self.build_preview();
        }
    }

    fn build_preview(&mut self) {
        let kind = ALL[self.selected];
        self.preview = Some(kind.build(
            self.panes.focused().seed,
            self.palette(),
            self.background,
            &self.corpus,
            self.speed,
            self.settings,
        ));
        self.preview_credit = 0;
    }

    pub fn move_focus(&mut self, direction: Direction) {
        self.panes.move_focus(direction);
    }

    pub fn split_beside(&mut self) {
        self.split(Split::Beside);
    }

    pub fn split_below(&mut self) {
        self.split(Split::Below);
    }

    /// The new pane opens on the picker, marking the scene after the
    /// focused one, so Enter alone gives two different scenes.
    fn split(&mut self, split: Split) {
        let focused = self.panes.focused();
        let kind = focused.kind.next();
        let seed = focused.seed.wrapping_add(1);
        let pane = Pane::new(
            kind,
            seed,
            self.palette(),
            self.background,
            &self.corpus,
            self.speed,
            self.settings,
        );
        self.panes.split(split, pane);
        self.selected = slot_of(kind);
        self.fresh = true;
        self.open_picker();
    }

    pub fn close_pane(&mut self) {
        self.panes.close();
    }

    /// Esc in the picker of a new pane takes the split back. In any other
    /// picker it does nothing.
    pub fn cancel_picker(&mut self) {
        if self.fresh {
            self.fresh = false;
            self.mode = Mode::Running;
            self.preview = None;
            self.panes.close();
        }
    }

    pub fn start_selected(&mut self) {
        if self.listed().is_empty() {
            return;
        }
        self.query = None;
        self.switch_to(ALL[self.selected]);
    }

    fn switch_to(&mut self, kind: SceneKind) {
        self.fresh = false;
        self.panes.focused_mut().kind = kind;
        self.selected = slot_of(kind);
        self.mode = Mode::Running;
        self.preview = None;
        self.restart();
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }
}

/// The place before `slot` in `ALL`, round from the first to the last.
fn before(slot: usize) -> usize {
    (slot + ALL.len() - 1) % ALL.len()
}

fn palette(theme: usize, background: Background) -> &'static Palette {
    match background {
        Background::Theme => theme::THEMES[theme].1,
        Background::Terminal => &theme::FLAT_THEMES[theme],
    }
}

fn slot_of(kind: SceneKind) -> usize {
    ALL.iter().position(|k| *k == kind).unwrap_or(0)
}
