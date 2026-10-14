use std::sync::Arc;
use std::time::Duration;

use clap::ValueEnum;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::corpus::Corpus;
use crate::scenes::aquarium::Aquarium;
use crate::scenes::bonsai::Bonsai;
use crate::scenes::fire::Fire;
use crate::scenes::life::Life;
use crate::scenes::matrix::Matrix;
use crate::scenes::noise::Noise;
use crate::scenes::pipes::Pipes;
use crate::scenes::typing::{Speed, Typing};

/// The one random source every scene draws from. A seed fixes every frame.
pub type Rng = ChaCha8Rng;

/// About 16 frames per second. Fast enough for rain and fire, slow enough
/// that an idle terminal stays cheap.
pub const TICK_RATE: Duration = Duration::from_millis(60);

/// Each step in quarters of a tick for each frame, so the count stays whole
/// and a seeded replay ticks on the same frames.
const RATES: &[(u32, &str)] = &[
    (1, "¼×"),
    (2, "½×"),
    (3, "¾×"),
    (4, "1×"),
    (6, "1.5×"),
    (8, "2×"),
    (12, "3×"),
    (16, "4×"),
];

pub const QUARTERS_PER_TICK: u32 = 4;

/// How fast a scene runs against one tick a frame, for a scene whose own
/// unit means nothing to a reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rate(usize);

impl Rate {
    pub const NORMAL: Rate = Rate(3);

    pub fn faster(self) -> Rate {
        Rate((self.0 + 1).min(RATES.len() - 1))
    }

    pub fn slower(self) -> Rate {
        Rate(self.0.saturating_sub(1))
    }

    pub fn quarters(self) -> u32 {
        RATES[self.0].0
    }

    pub fn times(self) -> f64 {
        f64::from(self.quarters()) / f64::from(QUARTERS_PER_TICK)
    }

    /// The step that runs `times` as fast as normal. Every step is a whole
    /// number of quarters, so each compares exactly as a float.
    pub fn from_times(times: f64) -> Option<Rate> {
        let quarters = times * f64::from(QUARTERS_PER_TICK);
        RATES
            .iter()
            .position(|&(step, _)| f64::from(step) == quarters)
            .map(Rate)
    }

    pub fn all_times() -> String {
        RATES
            .iter()
            .map(|&(step, _)| (f64::from(step) / f64::from(QUARTERS_PER_TICK)).to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl std::fmt::Display for Rate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(RATES[self.0].1)
    }
}

pub trait Scene {
    /// Advances the scene by one frame. The area is the whole terminal, so a
    /// scene sizes its own state from it and a resize needs no other signal.
    fn tick(&mut self, area: Rect);

    fn render(&self, area: Rect, buf: &mut Buffer);

    /// Only the typing scene counts its speed in keystrokes per second. The
    /// others run at a `Rate` that `App` applies, so they ignore this.
    fn set_speed(&mut self, _speed: Speed) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SceneKind {
    Noise,
    Matrix,
    Fire,
    Life,
    Pipes,
    Bonsai,
    Aquarium,
    Typing,
}

/// The order the picker lists and Tab cycles.
pub const ALL: &[SceneKind] = &[
    SceneKind::Noise,
    SceneKind::Matrix,
    SceneKind::Fire,
    SceneKind::Life,
    SceneKind::Pipes,
    SceneKind::Bonsai,
    SceneKind::Aquarium,
    SceneKind::Typing,
];

impl SceneKind {
    pub fn name(self) -> &'static str {
        match self {
            SceneKind::Noise => "noise",
            SceneKind::Matrix => "matrix",
            SceneKind::Fire => "fire",
            SceneKind::Life => "life",
            SceneKind::Pipes => "pipes",
            SceneKind::Bonsai => "bonsai",
            SceneKind::Aquarium => "aquarium",
            SceneKind::Typing => "typing",
        }
    }

    pub fn from_name(name: &str) -> Option<SceneKind> {
        ALL.iter().copied().find(|kind| kind.name() == name)
    }

    pub fn build(self, seed: u64, corpus: &Arc<Corpus>, speed: Speed) -> Box<dyn Scene> {
        let rng = Rng::seed_from_u64(seed);
        match self {
            SceneKind::Noise => Box::new(Noise::new(rng)),
            SceneKind::Matrix => Box::new(Matrix::new(rng)),
            SceneKind::Fire => Box::new(Fire::new(rng)),
            SceneKind::Life => Box::new(Life::new(rng)),
            SceneKind::Pipes => Box::new(Pipes::new(rng)),
            SceneKind::Bonsai => Box::new(Bonsai::new(rng)),
            SceneKind::Aquarium => Box::new(Aquarium::new(rng)),
            SceneKind::Typing => Box::new(Typing::new(rng, Arc::clone(corpus), speed)),
        }
    }

    pub fn next(self) -> SceneKind {
        let index = ALL.iter().position(|kind| *kind == self).unwrap_or(0);
        ALL[(index + 1) % ALL.len()]
    }
}
