use clap::ValueEnum;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::scenes::noise::Noise;

/// The one random source every scene draws from. A seed fixes every frame.
pub type Rng = ChaCha8Rng;

pub trait Scene {
    /// Advances the scene by one frame. The area is the whole terminal, so a
    /// scene sizes its own state from it and a resize needs no other signal.
    fn tick(&mut self, area: Rect);

    fn render(&self, area: Rect, buf: &mut Buffer);
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum SceneKind {
    Noise,
}

impl SceneKind {
    pub fn build(self, seed: u64) -> Box<dyn Scene> {
        let rng = Rng::seed_from_u64(seed);
        match self {
            SceneKind::Noise => Box::new(Noise::new(rng)),
        }
    }
}
