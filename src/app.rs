use std::path::PathBuf;
use std::sync::Arc;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Widget;

use crate::config::{self, Config};
use crate::corpus::Corpus;
use crate::picker::Picker;
use crate::scene::{ALL, QUARTERS_PER_TICK, Rate, Scene, SceneKind};
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
    kind: SceneKind,
    seed: u64,
    corpus: Arc<Corpus>,
    speed: Speed,
    /// One for each scene in `ALL`, so each keeps its own through a
    /// restart, Tab and the picker.
    rates: [Rate; ALL.len()],
    /// Quarter ticks owed and not yet run, carried from frame to frame.
    credit: u32,
    notice: u32,
    /// Where a changed speed is saved. `None` saves nothing, which is what
    /// a test that presses `+` needs.
    saving: Option<PathBuf>,
    /// Why the last change was not saved, shown with it in the notice.
    unsaved: Option<String>,
    scene: Box<dyn Scene>,
    mode: Mode,
    selected: usize,
    pub should_quit: bool,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("kind", &self.kind)
            .field("seed", &self.seed)
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
            kind,
            seed,
            scene: kind.build(seed, &corpus, Speed::DEFAULT),
            corpus,
            speed: Speed::DEFAULT,
            rates: [Rate::NORMAL; ALL.len()],
            credit: 0,
            notice: 0,
            saving: None,
            unsaved: None,
            mode: Mode::Running,
            selected: ALL.iter().position(|k| *k == kind).unwrap_or(0),
            should_quit: false,
        }
    }

    pub fn picker(seed: u64, corpus: Arc<Corpus>) -> Self {
        let mut app = Self::new(ALL[0], seed, corpus);
        app.mode = Mode::Picking;
        app
    }

    fn with_speed(mut self, speed: Speed) -> Self {
        self.speed = speed;
        self.scene = self.kind.build(self.seed, &self.corpus, speed);
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

    /// Typing keeps one tick a frame and counts its own speed in
    /// keystrokes. Every other scene ticks once for each whole tick its
    /// rate has paid for.
    pub fn tick(&mut self, area: Rect) {
        self.notice = self.notice.saturating_sub(1);
        if self.mode != Mode::Running {
            return;
        }
        if self.kind == SceneKind::Typing {
            self.scene.tick(area);
            return;
        }
        self.credit += self.rates[self.slot()].quarters();
        while self.credit >= QUARTERS_PER_TICK {
            self.credit -= QUARTERS_PER_TICK;
            self.scene.tick(area);
        }
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        match self.mode {
            Mode::Picking => {
                let labels: Vec<&str> = ALL.iter().map(|kind| kind.name()).collect();
                Picker {
                    labels: &labels,
                    selected: self.selected,
                }
                .render(area, buf);
            }
            Mode::Running => {
                buf.set_style(area, Style::default().bg(theme::BASE));
                self.scene.render(area, buf);
                if self.notice > 0 {
                    self.render_notice(area, buf);
                }
            }
        }
    }

    /// A notice wider than the area is cut at its left edge, so the reason
    /// a save failed is never hidden by the speed in front of it.
    fn render_notice(&self, area: Rect, buf: &mut Buffer) {
        let mut notice = if self.kind == SceneKind::Typing {
            format!("{} cps", self.speed)
        } else {
            self.rates[self.slot()].to_string()
        };
        if let Some(reason) = &self.unsaved {
            notice = format!("{notice}, not saved: {reason}");
        }
        let width = u16::try_from(notice.chars().count()).unwrap_or(u16::MAX);
        let x = area.right() - width.min(area.width);
        let style = Style::default().fg(NOTICE_COLOUR);
        buf.set_stringn(x, area.y, notice, usize::from(area.width), style);
    }

    fn slot(&self) -> usize {
        ALL.iter().position(|k| *k == self.kind).unwrap_or(0)
    }

    /// Rebuilds the scene from the next seed in the chain, so a run with
    /// `--seed` replays the same sequence of restarts.
    pub fn restart(&mut self) {
        self.seed = self.seed.wrapping_add(1);
        self.credit = 0;
        self.scene = self.kind.build(self.seed, &self.corpus, self.speed);
    }

    pub fn next_scene(&mut self) {
        self.switch_to(self.kind.next());
    }

    pub fn faster(&mut self) {
        self.change_speed(Speed::faster, Rate::faster);
    }

    pub fn slower(&mut self) {
        self.change_speed(Speed::slower, Rate::slower);
    }

    fn change_speed(&mut self, speed: fn(Speed) -> Speed, rate: fn(Rate) -> Rate) {
        let saved = if self.kind == SceneKind::Typing {
            self.speed = speed(self.speed);
            self.scene.set_speed(self.speed);
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
            Some(path) => config::save_speed(path, self.kind, speed),
            None => Ok(()),
        }
    }

    pub fn open_picker(&mut self) {
        self.mode = Mode::Picking;
    }

    pub fn select_previous(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn select_next(&mut self) {
        self.selected = (self.selected + 1).min(ALL.len() - 1);
    }

    pub fn start_selected(&mut self) {
        self.switch_to(ALL[self.selected]);
    }

    fn switch_to(&mut self, kind: SceneKind) {
        self.kind = kind;
        self.selected = ALL.iter().position(|k| *k == kind).unwrap_or(0);
        self.mode = Mode::Running;
        self.restart();
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }
}
