use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Widget;

use crate::picker::Picker;
use crate::scene::{ALL, Scene, SceneKind};
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Picking,
    Running,
}

pub struct App {
    kind: SceneKind,
    seed: u64,
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
            .field("mode", &self.mode)
            .field("selected", &self.selected)
            .field("should_quit", &self.should_quit)
            .finish_non_exhaustive()
    }
}

impl App {
    pub fn new(kind: SceneKind, seed: u64) -> Self {
        Self {
            kind,
            seed,
            scene: kind.build(seed),
            mode: Mode::Running,
            selected: ALL.iter().position(|k| *k == kind).unwrap_or(0),
            should_quit: false,
        }
    }

    pub fn picker(seed: u64) -> Self {
        let mut app = Self::new(ALL[0], seed);
        app.mode = Mode::Picking;
        app
    }

    pub fn is_picking(&self) -> bool {
        self.mode == Mode::Picking
    }

    pub fn tick(&mut self, area: Rect) {
        if self.mode == Mode::Running {
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
            }
        }
    }

    /// Rebuilds the scene from the next seed in the chain, so a run with
    /// `--seed` replays the same sequence of restarts.
    pub fn restart(&mut self) {
        self.seed = self.seed.wrapping_add(1);
        self.scene = self.kind.build(self.seed);
    }

    pub fn next_scene(&mut self) {
        self.switch_to(self.kind.next());
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
