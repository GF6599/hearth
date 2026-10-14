use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::scene::{Scene, SceneKind};

pub struct App {
    kind: SceneKind,
    seed: u64,
    scene: Box<dyn Scene>,
    pub should_quit: bool,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("kind", &self.kind)
            .field("seed", &self.seed)
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
            should_quit: false,
        }
    }

    pub fn tick(&mut self, area: Rect) {
        self.scene.tick(area);
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        self.scene.render(area, buf);
    }

    /// Rebuilds the scene from the next seed in the chain, so a run with
    /// `--seed` replays the same sequence of restarts.
    pub fn restart(&mut self) {
        self.seed = self.seed.wrapping_add(1);
        self.scene = self.kind.build(self.seed);
    }

    pub fn quit(&mut self) {
        self.should_quit = true;
    }
}
