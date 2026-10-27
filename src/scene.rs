use std::sync::Arc;
use std::time::Duration;

use clap::ValueEnum;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::clock::Clock;
use crate::config::Background;
use crate::corpus::Corpus;
use crate::scenes::aquarium::Aquarium;
use crate::scenes::bonsai::Bonsai;
use crate::scenes::city::City;
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
use crate::scenes::river::River;
use crate::scenes::solar::Solar;
use crate::scenes::study::Study;
use crate::scenes::tide::Tide;
use crate::scenes::typing::{Shortest, Speed, Typing};
use crate::sound::Cue;
use crate::theme::Palette;

/// The one random source every scene draws from. A seed fixes every frame.
pub type Rng = ChaCha8Rng;

const TICK_MILLIS: u64 = 60;

/// The unit every scene counts its time in. A scene at 1× ticks once in
/// this time, about 16 times a second.
pub const TICK_RATE: Duration = Duration::from_millis(TICK_MILLIS);

/// The frames the app draws in one tick, so a scene can glide between two
/// ticks.
pub const FRAMES_PER_TICK: u32 = 4;

/// About 60 frames a second. Fast enough that motion looks smooth, slow
/// enough that an idle terminal stays cheap.
pub const FRAME_RATE: Duration = Duration::from_millis(TICK_MILLIS / FRAMES_PER_TICK as u64);

/// Each step in quarters of a tick for each tick at 1×, so the count stays
/// whole and a seeded replay ticks on the same frames.
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

/// What the config file fixes for a whole run, read by a scene when it is
/// built. A speed stays apart, because `+` and `-` change it while a scene
/// runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub lengths: Lengths,
    pub shortest: Shortest,
}

impl Settings {
    pub const DEFAULT: Settings = Settings {
        lengths: Lengths::DEFAULT,
        shortest: Shortest::DEFAULT,
    };
}

/// How fast a scene runs against one tick in each `TICK_RATE`, for a scene
/// whose own unit means nothing to a reader.
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

/// A line of text in the middle of `area`, for a scene with nothing else
/// to draw.
pub fn draw_notice(text: &str, area: Rect, palette: &Palette, buf: &mut Buffer) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let length = text.chars().count() as u16;
    let x = area.x + area.width.saturating_sub(length) / 2;
    buf.set_stringn(
        x,
        area.y + area.height / 2,
        text,
        usize::from(area.width - (x - area.x)),
        Style::default().fg(palette.subtle),
    );
}

pub trait Scene {
    /// Advances the scene by one tick, or by one frame for typing. The area
    /// is the whole terminal, so a scene sizes its own state from it and a
    /// resize needs no other signal.
    fn tick(&mut self, area: Rect);

    fn render(&self, area: Rect, buf: &mut Buffer);

    /// Only the typing scene counts its speed in keystrokes per second. The
    /// others run at a `Rate` that `App` applies, so they ignore this.
    fn set_speed(&mut self, _speed: Speed) {}

    /// How far the scene has run from its last tick towards its next, from
    /// 0 up to but not including 1, given after each frame. A scene that
    /// glides between ticks draws from it. The others ignore it and show
    /// their last tick.
    ///
    /// Returns whether the new progress moves anything the scene draws. A
    /// frame that moves nothing is not drawn, so a scene that returns false
    /// and draws a different picture leaves a stale one on the screen.
    fn set_progress(&mut self, _progress: f64) -> bool {
        false
    }

    /// Moves the cues of the ticks since the last call into `out`. A scene
    /// keeps its cues until then, so a frame that ticks twice loses none.
    fn drain_cues(&mut self, _out: &mut Vec<Cue>) {}

    /// What a scene that keeps real time has left to run, for `+` and `-`
    /// to show. The rate means nothing to such a scene. The others have
    /// none.
    fn time_left(&self) -> Option<String> {
        None
    }
}

/// The cells a scene drew at its last tick, for a scene whose picture
/// changes only at a tick. Each frame between two ticks copies the cells,
/// which costs less than drawing the picture again.
#[derive(Debug, Default)]
pub struct Cells(Buffer);

impl Cells {
    /// `draw` must write every cell of `area`, because a cell it leaves
    /// keeps what the last tick drew there.
    pub fn draw(&mut self, area: Rect, draw: impl FnOnce(&mut Buffer)) {
        if self.0.area != area {
            self.0 = Buffer::empty(area);
        }
        draw(&mut self.0);
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, draw: impl FnOnce(&mut Buffer)) {
        if self.0.area == area {
            for position in area.positions() {
                buf[position].clone_from(&self.0[position]);
            }
        } else {
            draw(buf);
        }
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
    City,
    River,
    Matrix,
    Fire,
    Life,
    Pipes,
    Bonsai,
    Aquarium,
    Murmuration,
    Hourglass,
    Study,
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
    SceneKind::City,
    SceneKind::River,
    SceneKind::Matrix,
    SceneKind::Fire,
    SceneKind::Life,
    SceneKind::Pipes,
    SceneKind::Bonsai,
    SceneKind::Aquarium,
    SceneKind::Murmuration,
    SceneKind::Hourglass,
    SceneKind::Study,
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
            SceneKind::City => "city",
            SceneKind::River => "river",
            SceneKind::Hourglass => "hourglass",
            SceneKind::Study => "study",
            SceneKind::Typing => "typing",
        }
    }

    /// One line for the picker. The README's scene table says the same at
    /// length.
    pub fn summary(self) -> &'static str {
        match self {
            SceneKind::Noise => "static of random pixels over the whole screen",
            SceneKind::Matrix => "katakana rain that fades from foam to pine",
            SceneKind::Fire => "the doom fire, rising in octant pixels",
            SceneKind::Embers => "logs that burn down on a grate, throwing sparks",
            SceneKind::Life => "Conway's game of life on a torus",
            SceneKind::Pipes => "pipes one pixel wide that walk and turn",
            SceneKind::Bonsai => "a bonsai tree that grows from a pot",
            SceneKind::Aquarium => "fish, bubbles and swaying seaweed",
            SceneKind::Murmuration => "a flock of starlings that folds at dusk",
            SceneKind::Ink => "drops of ink that open out in still water",
            SceneKind::Tide => "waves on a beach as the tide comes and goes",
            SceneKind::Constellations => "stars that join into named figures",
            SceneKind::Solar => "eight planets with trails round the sun",
            SceneKind::Drive => "a night drive over hills and bends, past slower cars",
            SceneKind::City => "a city at night, its windows lit over the water",
            SceneKind::River => "the git history as a river of lanes and stones",
            SceneKind::Hourglass => "an hourglass that keeps a Pomodoro cycle",
            SceneKind::Study => "a desk by a rainy window, a cat on the sill",
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
            SceneKind::City => None,
            SceneKind::River => None,
            SceneKind::Hourglass => Some("after the Pomodoro Technique by Francesco Cirillo"),
            SceneKind::Study => Some("after the lofi study streams"),
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
        settings: Settings,
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
            SceneKind::City => Box::new(City::new(rng, palette)),
            SceneKind::River => Box::new(River::new(rng, palette, Arc::clone(corpus))),
            SceneKind::Hourglass => Box::new(Hourglass::new(
                rng,
                palette,
                Clock::monotonic(),
                settings.lengths,
            )),
            SceneKind::Study => Box::new(Study::new(rng, palette)),
            SceneKind::Typing => Box::new(
                Typing::new(rng, palette, Arc::clone(corpus), speed)
                    .with_background(background)
                    .with_shortest(settings.shortest),
            ),
        }
    }

    pub fn next(self) -> SceneKind {
        let index = ALL.iter().position(|kind| *kind == self).unwrap_or(0);
        ALL[(index + 1) % ALL.len()]
    }
}
