use clap::ValueEnum;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::scenes::aquarium::Aquarium;
use crate::scenes::bonsai::Bonsai;
use crate::scenes::fire::Fire;
use crate::scenes::life::Life;
use crate::scenes::matrix::Matrix;
use crate::scenes::noise::Noise;
use crate::scenes::pipes::Pipes;

/// The one random source every scene draws from. A seed fixes every frame.
pub type Rng = ChaCha8Rng;

pub trait Scene {
    /// Advances the scene by one frame. The area is the whole terminal, so a
    /// scene sizes its own state from it and a resize needs no other signal.
    fn tick(&mut self, area: Rect);

    fn render(&self, area: Rect, buf: &mut Buffer);
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
        }
    }

    pub fn build(self, seed: u64) -> Box<dyn Scene> {
        let rng = Rng::seed_from_u64(seed);
        match self {
            SceneKind::Noise => Box::new(Noise::new(rng)),
            SceneKind::Matrix => Box::new(Matrix::new(rng)),
            SceneKind::Fire => Box::new(Fire::new(rng)),
            SceneKind::Life => Box::new(Life::new(rng)),
            SceneKind::Pipes => Box::new(Pipes::new(rng)),
            SceneKind::Bonsai => Box::new(Bonsai::new(rng)),
            SceneKind::Aquarium => Box::new(Aquarium::new(rng)),
        }
    }

    pub fn next(self) -> SceneKind {
        let index = ALL.iter().position(|kind| *kind == self).unwrap_or(0);
        ALL[(index + 1) % ALL.len()]
    }
}
