use std::sync::Arc;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::corpus::Corpus;
use crate::scene::{QUARTERS_PER_TICK, Rate, Scene, SceneKind};
use crate::scenes::typing::Speed;

/// One scene and the seed it was built from. Each pane keeps its own seed,
/// so a restart in one pane replays nothing in another.
pub struct Pane {
    pub kind: SceneKind,
    pub seed: u64,
    scene: Box<dyn Scene>,
    /// Quarter ticks owed and not yet run, carried from frame to frame.
    credit: u32,
}

impl std::fmt::Debug for Pane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pane")
            .field("kind", &self.kind)
            .field("seed", &self.seed)
            .finish_non_exhaustive()
    }
}

impl Pane {
    pub fn new(kind: SceneKind, seed: u64, corpus: &Arc<Corpus>, speed: Speed) -> Self {
        Self {
            kind,
            seed,
            scene: kind.build(seed, corpus, speed),
            credit: 0,
        }
    }

    /// Rebuilds the scene from the next seed in the chain, so a run with
    /// `--seed` replays the same sequence of restarts.
    pub fn restart(&mut self, corpus: &Arc<Corpus>, speed: Speed) {
        self.seed = self.seed.wrapping_add(1);
        self.rebuild(corpus, speed);
    }

    pub fn rebuild(&mut self, corpus: &Arc<Corpus>, speed: Speed) {
        self.credit = 0;
        self.scene = self.kind.build(self.seed, corpus, speed);
    }

    pub fn set_speed(&mut self, speed: Speed) {
        self.scene.set_speed(speed);
    }

    pub fn tick(&mut self, rate: Rate, area: Rect) {
        advance(&mut *self.scene, self.kind, rate, &mut self.credit, area);
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        self.scene.render(area, buf);
    }
}

/// Typing keeps one tick a frame and counts its own speed in keystrokes.
/// Every other scene ticks once for each whole tick its rate has paid for.
pub fn advance(scene: &mut dyn Scene, kind: SceneKind, rate: Rate, credit: &mut u32, area: Rect) {
    if kind == SceneKind::Typing {
        scene.tick(area);
        return;
    }
    *credit += rate.quarters();
    while *credit >= QUARTERS_PER_TICK {
        *credit -= QUARTERS_PER_TICK;
        scene.tick(area);
    }
}
