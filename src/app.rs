use std::path::PathBuf;
use std::sync::Arc;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use crate::config::{self, Config};
use crate::corpus::Corpus;
use crate::pane::{Pane, advance};
use crate::picker::{Entry, Picker};
use crate::scene::{ALL, Rate, Scene, SceneKind};
use crate::scenes::typing::Speed;
use crate::theme;
use toml_edit::Value;

/// Frames a changed speed stays on screen, about one second.
const NOTICE_TICKS: u32 = 16;
const NOTICE_COLOUR: Color = theme::SUBTLE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Picking,
    Running,
}

pub struct App {
    pane: Pane,
    corpus: Arc<Corpus>,
    speed: Speed,
    /// One for each scene in `ALL`, so each keeps its own through a
    /// restart, Tab and the picker.
    rates: [Rate; ALL.len()],
    notice: u32,
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
    pub should_quit: bool,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("kind", &self.pane.kind)
            .field("seed", &self.pane.seed)
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
            pane: Pane::new(kind, seed, &corpus, Speed::DEFAULT),
            corpus,
            speed: Speed::DEFAULT,
            rates: [Rate::NORMAL; ALL.len()],
            notice: 0,
            saving: None,
            unsaved: None,
            preview: None,
            preview_credit: 0,
            mode: Mode::Running,
            selected: slot_of(kind),
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
        self.pane.rebuild(&self.corpus, speed);
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
        match config.typing {
            Some(speed) => self.with_speed(speed),
            None => self,
        }
    }

    pub fn saving_to(mut self, path: PathBuf) -> Self {
        self.saving = Some(path);
        self
    }

    pub fn is_picking(&self) -> bool {
        self.mode == Mode::Picking
    }

    pub fn tick(&mut self, area: Rect) {
        self.notice = self.notice.saturating_sub(1);
        match self.mode {
            Mode::Running => {
                let rate = self.rates[self.slot()];
                self.pane.tick(rate, area);
            }
            Mode::Picking => {
                let kind = ALL[self.selected];
                let rate = self.rates[self.selected];
                let pane = self.with_picker(|picker| picker.preview_area(area));
                if let (Some(scene), Some(pane)) = (&mut self.preview, pane) {
                    advance(&mut **scene, kind, rate, &mut self.preview_credit, pane);
                }
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
            })
            .collect();
        f(Picker {
            entries: &entries,
            selected: self.selected,
        })
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        match self.mode {
            Mode::Picking => self.with_picker(|picker| {
                let pane = picker.preview_area(area);
                picker.render(area, buf);
                if let (Some(scene), Some(pane)) = (&self.preview, pane) {
                    scene.render(pane, buf);
                }
            }),
            Mode::Running => {
                buf.set_style(area, Style::default().bg(theme::BASE));
                self.pane.render(area, buf);
                if self.notice > 0 {
                    self.render_notice(area, buf);
                }
            }
        }
    }

    /// A notice wider than the area is cut at its left edge, so the reason
    /// a save failed is never hidden by the speed in front of it.
    fn render_notice(&self, area: Rect, buf: &mut Buffer) {
        let mut notice = self.speed_label(self.pane.kind);
        if let Some(reason) = &self.unsaved {
            notice = format!("{notice}, not saved: {reason}");
        }
        let width = u16::try_from(notice.chars().count()).unwrap_or(u16::MAX);
        let x = area.right() - width.min(area.width);
        let style = Style::default().fg(NOTICE_COLOUR);
        buf.set_stringn(x, area.y, notice, usize::from(area.width), style);
    }

    fn speed_label(&self, kind: SceneKind) -> String {
        if kind == SceneKind::Typing {
            format!("{} cps", self.speed)
        } else {
            self.rates[slot_of(kind)].to_string()
        }
    }

    fn slot(&self) -> usize {
        slot_of(self.pane.kind)
    }

    pub fn restart(&mut self) {
        self.pane.restart(&self.corpus, self.speed);
    }

    pub fn next_scene(&mut self) {
        self.switch_to(self.pane.kind.next());
    }

    pub fn faster(&mut self) {
        self.change_speed(Speed::faster, Rate::faster);
    }

    pub fn slower(&mut self) {
        self.change_speed(Speed::slower, Rate::slower);
    }

    fn change_speed(&mut self, speed: fn(Speed) -> Speed, rate: fn(Rate) -> Rate) {
        let saved = if self.pane.kind == SceneKind::Typing {
            self.speed = speed(self.speed);
            self.pane.set_speed(self.speed);
            self.save(Value::from(i64::from(self.speed.cps())))
        } else {
            let slot = self.slot();
            self.rates[slot] = rate(self.rates[slot]);
            self.save(Value::from(self.rates[slot].times()))
        };
        self.unsaved = saved.err().map(|e| e.to_string());
        self.notice = NOTICE_TICKS;
    }

    fn save(&self, speed: Value) -> std::io::Result<()> {
        match &self.saving {
            Some(path) => config::save_speed(path, self.pane.kind, speed),
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

    fn select(&mut self, index: usize) {
        if index != self.selected {
            self.selected = index;
            self.build_preview();
        }
    }

    fn build_preview(&mut self) {
        let kind = ALL[self.selected];
        self.preview = Some(kind.build(self.pane.seed, &self.corpus, self.speed));
        self.preview_credit = 0;
    }

    pub fn start_selected(&mut self) {
        self.switch_to(ALL[self.selected]);
    }

    fn switch_to(&mut self, kind: SceneKind) {
        self.pane.kind = kind;
        self.selected = slot_of(kind);
        self.mode = Mode::Running;
        self.preview = None;
        self.restart();
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }
}

fn slot_of(kind: SceneKind) -> usize {
    ALL.iter().position(|k| *k == kind).unwrap_or(0)
}
