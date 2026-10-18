use std::path::PathBuf;
use std::sync::Arc;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use crate::config::{self, Background, Config};
use crate::corpus::Corpus;
use crate::pane::{self, Direction, Pane, Panes, Split, advance};
use crate::picker::{Entry, Picker};
use crate::quadrant;
use crate::scene::{ALL, Rate, Scene, SceneKind};
use crate::scenes::hourglass::Lengths;
use crate::scenes::typing::Speed;
use crate::theme::{self, Palette};
use toml_edit::Value;

/// Frames a changed speed stays on screen, about one second.
const NOTICE_TICKS: u32 = 16;

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
    /// The hourglass phase lengths, from the file or the defaults.
    lengths: Lengths,
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
    /// The first scene in the picker list on the last tick.
    picker_top: usize,
    ticks: u32,
    /// The focused pane came from a split and its picker has not started
    /// a scene yet, so Esc closes it.
    fresh: bool,
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
        Self {
            theme: 0,
            background: Background::Theme,
            panes: Panes::new(Pane::new(
                kind,
                seed,
                theme::THEMES[0].1,
                Background::Theme,
                &corpus,
                Speed::DEFAULT,
                Lengths::DEFAULT,
            )),
            corpus,
            speed: Speed::DEFAULT,
            lengths: Lengths::DEFAULT,
            rates: [Rate::NORMAL; ALL.len()],
            notice: 0,
            notice_text: String::new(),
            saving: None,
            unsaved: None,
            preview: None,
            preview_credit: 0,
            mode: Mode::Running,
            selected: slot_of(kind),
            picker_top: 0,
            ticks: 0,
            fresh: false,
            should_quit: false,
        }
    }

    pub fn picker(seed: u64, corpus: Arc<Corpus>) -> Self {
        let mut app = Self::new(ALL[0], seed, corpus);
        app.open_picker();
        app
    }

    fn with_speed(mut self, speed: Speed) -> Self {
        self.speed = speed;
        self.panes
            .focused_mut()
            .rebuild(&self.corpus, speed, self.lengths);
        if self.mode == Mode::Picking {
            self.build_preview();
        }
        self
    }

    pub fn with_config(mut self, config: &Config) -> Self {
        for (rate, saved) in self.rates.iter_mut().zip(&config.rates) {
            if let Some(saved) = saved {
                *rate = *saved;
            }
        }
        self.background = config.background;
        if let Some(theme) = config.theme {
            self.theme = theme;
        }
        if config.theme.is_some() || self.background != Background::Theme {
            self.repaint();
        }
        if config.hourglass != self.lengths {
            self.lengths = config.hourglass;
            self.panes
                .focused_mut()
                .rebuild(&self.corpus, self.speed, self.lengths);
            if self.mode == Mode::Picking {
                self.build_preview();
            }
        }
        match config.typing {
            Some(speed) => self.with_speed(speed),
            None => self,
        }
    }

    fn palette(&self) -> &'static Palette {
        match self.background {
            Background::Theme => theme::THEMES[self.theme].1,
            Background::Terminal => &theme::FLAT_THEMES[self.theme],
        }
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
                self.lengths,
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
        self.notice = NOTICE_TICKS;
    }

    pub fn saving_to(mut self, path: PathBuf) -> Self {
        self.saving = Some(path);
        self
    }

    pub fn is_picking(&self) -> bool {
        self.mode == Mode::Picking
    }

    /// The pane under the picker holds still, and the picker's preview
    /// runs in its place.
    pub fn tick(&mut self, area: Rect) {
        self.ticks = self.ticks.wrapping_add(1);
        self.notice = self.notice.saturating_sub(1);
        let layout = self.panes.layout(area);
        let picking = self.is_picking().then(|| self.panes.focus());
        for (index, (pane, inside)) in self.panes.iter_mut().zip(&layout.panes).enumerate() {
            if Some(index) != picking {
                pane.tick(self.rates[slot_of(pane.kind)], *inside);
            }
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
            }
        }
    }

    fn with_picker<R>(&self, f: impl FnOnce(Picker) -> R) -> R {
        let speeds: Vec<String> = ALL.iter().map(|kind| self.speed_label(*kind)).collect();
        let entries: Vec<Entry> = ALL
            .iter()
            .zip(&speeds)
            .map(|(kind, speed)| Entry {
                name: kind.name(),
                summary: kind.summary(),
                speed,
                credit: kind.credit(),
            })
            .collect();
        f(Picker {
            palette: self.palette(),
            entries: &entries,
            selected: self.selected,
            top: self.picker_top,
            tick: self.ticks,
        })
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
    /// too. A block glyph drawn in the base turns over, so the other colour
    /// stays in front and the picture holds.
    fn clear_background(&self, area: Rect, buf: &mut Buffer) {
        let base = self.palette().base;
        let clear = |colour: Color| colour == base;
        for position in area.positions() {
            let cell = &mut buf[position];
            let turned = clear(cell.fg)
                .then(|| quadrant::complement(cell.symbol()))
                .flatten();
            if let Some(glyph) = turned {
                let front = if clear(cell.bg) {
                    Color::Reset
                } else {
                    cell.bg
                };
                let symbol = if front == Color::Reset { ' ' } else { glyph };
                cell.set_char(symbol).set_fg(front).set_bg(Color::Reset);
            } else if clear(cell.bg) {
                cell.set_bg(Color::Reset);
            }
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
            SceneKind::Hourglass => self.lengths.to_string(),
            _ => self.rates[slot_of(kind)].to_string(),
        }
    }

    fn slot(&self) -> usize {
        slot_of(self.panes.focused().kind)
    }

    pub fn restart(&mut self) {
        self.panes
            .focused_mut()
            .restart(&self.corpus, self.speed, self.lengths);
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
            self.notice = NOTICE_TICKS;
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
        self.notice = NOTICE_TICKS;
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

    pub fn select_previous(&mut self) {
        self.select(self.selected.saturating_sub(1));
    }

    pub fn select_next(&mut self) {
        self.select((self.selected + 1).min(ALL.len() - 1));
    }

    /// Tab walks the list round, as it walks the scenes round outside the
    /// picker.
    pub fn select_next_wrapped(&mut self) {
        self.select((self.selected + 1) % ALL.len());
    }

    pub fn select_previous_wrapped(&mut self) {
        self.select(before(self.selected));
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
            self.lengths,
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
            self.lengths,
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

fn slot_of(kind: SceneKind) -> usize {
    ALL.iter().position(|k| *k == kind).unwrap_or(0)
}
