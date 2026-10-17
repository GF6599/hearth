use std::sync::Arc;
use std::time::Duration;

use clap::ValueEnum;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::clock::Clock;
use crate::config::Background;
use crate::corpus::Corpus;
use crate::scenes::aquarium::Aquarium;
use crate::scenes::bonsai::Bonsai;
use crate::scenes::constellations::Constellations;
use crate::scenes::drive::Drive;
use crate::scenes::embers::Embers;
use crate::scenes::fire::Fire;
use crate::scenes::hourglass::{Hourglass, Lengths};
use crate::scenes::ink::Ink;
use crate::scenes::life::Life;
use crate::scenes::matrix::Matrix;
use crate::scenes::murmuration::Murmuration;
use crate::scenes::noise::Noise;
use crate::scenes::pipes::Pipes;
use crate::scenes::solar::Solar;
use crate::scenes::tide::Tide;
use crate::scenes::typing::{Speed, Typing};
use crate::theme::Palette;

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

    /// What a scene that keeps real time has left to run, for `+` and `-`
    /// to show. The rate means nothing to such a scene. The others have
    /// none.
    fn time_left(&self) -> Option<String> {
        None
    }
}

/// In the order of `ALL`, because `--help` lists the scenes in the order
/// of this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SceneKind {
    Embers,
    Ink,
    Tide,
    Constellations,
    Solar,
    Drive,
    Matrix,
    Fire,
    Life,
    Pipes,
    Bonsai,
    Aquarium,
    Murmuration,
    Hourglass,
    Typing,
    Noise,
}

/// The order the picker lists and Tab cycles.
pub const ALL: &[SceneKind] = &[
    SceneKind::Embers,
    SceneKind::Ink,
    SceneKind::Tide,
    SceneKind::Constellations,
    SceneKind::Solar,
    SceneKind::Drive,
    SceneKind::Matrix,
    SceneKind::Fire,
    SceneKind::Life,
    SceneKind::Pipes,
    SceneKind::Bonsai,
    SceneKind::Aquarium,
    SceneKind::Murmuration,
    SceneKind::Hourglass,
    SceneKind::Typing,
    SceneKind::Noise,
];

impl SceneKind {
    pub fn name(self) -> &'static str {
        match self {
            SceneKind::Noise => "noise",
            SceneKind::Matrix => "matrix",
            SceneKind::Fire => "fire",
            SceneKind::Embers => "embers",
            SceneKind::Life => "life",
            SceneKind::Pipes => "pipes",
            SceneKind::Bonsai => "bonsai",
            SceneKind::Aquarium => "aquarium",
            SceneKind::Murmuration => "murmuration",
            SceneKind::Ink => "ink",
            SceneKind::Tide => "tide",
            SceneKind::Constellations => "constellations",
            SceneKind::Solar => "solar",
            SceneKind::Drive => "drive",
            SceneKind::Hourglass => "hourglass",
            SceneKind::Typing => "typing",
        }
    }

    /// One line for the picker. The README's scene table says the same at
    /// length.
    pub fn summary(self) -> &'static str {
        match self {
            SceneKind::Noise => "random glyphs over the whole screen",
            SceneKind::Matrix => "katakana rain that fades from foam to pine",
            SceneKind::Fire => "the doom fire, rising in half blocks",
            SceneKind::Embers => "logs that burn down on a grate, throwing sparks",
            SceneKind::Life => "Conway's game of life on a torus",
            SceneKind::Pipes => "pipes that walk and turn in box runes",
            SceneKind::Bonsai => "a bonsai tree that grows from a pot",
            SceneKind::Aquarium => "fish, bubbles and swaying seaweed",
            SceneKind::Murmuration => "a flock of starlings that folds at dusk",
            SceneKind::Ink => "drops of ink that open out in still water",
            SceneKind::Tide => "waves on a beach as the tide comes and goes",
            SceneKind::Constellations => "stars that join into named figures",
            SceneKind::Solar => "eight planets with trails round the sun",
            SceneKind::Drive => "a night drive down a winding road",
            SceneKind::Hourglass => "an hourglass that keeps a Pomodoro cycle",
            SceneKind::Typing => "functions typed from a git repository",
        }
    }

    /// The work and the person whose idea the scene reuses. The README's
    /// scene table links each one.
    pub fn credit(self) -> Option<&'static str> {
        match self {
            SceneKind::Noise => None,
            SceneKind::Matrix => Some("after cmatrix by Chris Allegretta"),
            SceneKind::Fire => Some("after the PSX Doom fire, via Fabien Sanglard"),
            SceneKind::Embers => None,
            SceneKind::Life => Some("after John Conway's Game of Life"),
            SceneKind::Pipes => Some("after pipes.sh by Matthew Simpson"),
            SceneKind::Bonsai => Some("after cbonsai by John Allbritten"),
            SceneKind::Aquarium => Some("after asciiquarium by Kirk Baucom"),
            SceneKind::Murmuration => Some("after the boids of Craig Reynolds"),
            SceneKind::Ink => None,
            SceneKind::Tide => None,
            SceneKind::Constellations => None,
            SceneKind::Solar => None,
            SceneKind::Drive => None,
            SceneKind::Hourglass => Some("after the Pomodoro Technique by Francesco Cirillo"),
            SceneKind::Typing => Some("after gittype by Yuji Ueki"),
        }
    }

    pub fn from_name(name: &str) -> Option<SceneKind> {
        ALL.iter().copied().find(|kind| kind.name() == name)
    }

    pub fn build(
        self,
        seed: u64,
        palette: &'static Palette,
        background: Background,
        corpus: &Arc<Corpus>,
        speed: Speed,
        lengths: Lengths,
    ) -> Box<dyn Scene> {
        let rng = Rng::seed_from_u64(seed);
        match self {
            SceneKind::Noise => Box::new(Noise::new(rng, palette)),
            SceneKind::Matrix => Box::new(Matrix::new(rng, palette)),
            SceneKind::Fire => Box::new(Fire::new(rng, palette)),
            SceneKind::Embers => Box::new(Embers::new(rng, palette)),
            SceneKind::Life => Box::new(Life::new(rng, palette)),
            SceneKind::Pipes => Box::new(Pipes::new(rng, palette)),
            SceneKind::Bonsai => Box::new(Bonsai::new(rng, palette)),
            SceneKind::Aquarium => Box::new(Aquarium::new(rng, palette)),
            SceneKind::Murmuration => {
                Box::new(Murmuration::new(rng, palette).with_background(background))
            }
            SceneKind::Ink => Box::new(Ink::new(rng, palette)),
            SceneKind::Tide => Box::new(Tide::new(rng, palette)),
            SceneKind::Constellations => Box::new(Constellations::new(rng, palette)),
            SceneKind::Solar => Box::new(Solar::new(rng, palette)),
            SceneKind::Drive => Box::new(Drive::new(rng, palette)),
            SceneKind::Hourglass => {
                Box::new(Hourglass::new(rng, palette, Clock::monotonic(), lengths))
            }
            SceneKind::Typing => Box::new(Typing::new(rng, palette, Arc::clone(corpus), speed)),
        }
    }

    pub fn next(self) -> SceneKind {
        let index = ALL.iter().position(|kind| *kind == self).unwrap_or(0);
        ALL[(index + 1) % ALL.len()]
    }
}
