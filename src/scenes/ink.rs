use std::f32::consts::{PI, TAU};
use std::ops::RangeInclusive;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::dither;
use crate::octant::{CELL_HEIGHT, CELL_WIDTH, Canvas};
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// Ticks from one drop to the next, about 5 to 12 seconds. The first
/// drop falls on the first tick, so a preview is never empty water.
pub const DROP_EVERY: RangeInclusive<u32> = 80..=200;
/// A drop releases its ink over this many ticks as it sinks, which leaves
/// a thread behind the head.
pub const EMIT_TICKS: u32 = 24;
/// Ticks a particle's ink lasts, about 20 to 40 seconds. A spread of
/// lifetimes thins a cloud before it goes, where one lifetime would cut
/// it out all at once.
pub const LIFETIME: RangeInclusive<u32> = 320..=640;
/// The ink in one pixel that colours the whole of it. Less colours that
/// share of a dither tile. Two units of ink in a square unit colour it
/// whole, and a pixel is a quarter of a square unit.
pub const FULL_INK: f32 = 0.5;

const PARTICLES_PER_TICK: usize = 32;
/// Speeds in pixels a tick. The head enters fast and slows as the water
/// takes it, and each particle leaves with part of the head's speed.
const ENTRY_SPEED: RangeInclusive<f32> = 0.9..=1.4;
const HEAD_DRAG: f32 = 0.96;
const SHARE_OF_HEAD: RangeInclusive<f32> = 0.3..=1.0;
const SPREAD: f32 = 0.15;
const DRAG: f32 = 0.97;
/// Ink is a little heavier than water, so it keeps settling after its
/// own speed is gone.
const SINK: f32 = 0.03;
const JITTER: f32 = 0.01;
/// Waves in the stream function of the currents. Where several cross the
/// curls are less regular than any one of them.
const WAVES: usize = 5;
/// The length of one wave in pixels, so a wide pane holds more curls
/// rather than larger ones.
const WAVELENGTH: RangeInclusive<f32> = 24.0..=64.0;
/// The fastest current one wave makes, in pixels a tick.
const WAVE_SPEED: RangeInclusive<f32> = 0.05..=0.12;
/// How fast a wave's phase turns, in radians a tick, so the currents
/// shift over a minute or two.
const DRIFT: RangeInclusive<f32> = 0.002..=0.006;
/// A drop enters this far in from either side, as a share of the width.
const MARGIN: f32 = 0.15;
/// The pixels over which a particle spreads its ink, as the scale of a
/// cubic B-spline. It reaches twice this far on each side.
const SPLAT: f32 = 2.0;
/// Keeps a clamped particle strictly inside the last pixel.
const INSIDE: f32 = 0.01;

/// The ink moves in square units, one cell across by half a cell down, so
/// a pixel is half a unit across and half a unit down.
const UNITS_DOWN: usize = 2;
const PIXELS_ACROSS: usize = CELL_WIDTH;
const PIXELS_DOWN: usize = CELL_HEIGHT / UNITS_DOWN;
const SHADES: f32 = dither::SHADES as f32;

/// The roles a drop can take, in the order `shade` reads its inks.
pub fn accents(palette: &Palette) -> [Color; 5] {
    [
        palette.love,
        palette.iris,
        palette.foam,
        palette.pine,
        palette.rose,
    ]
}

/// The steps of the mix from the water to an accent. Each pixel takes the
/// two steps nearest its ink in an ordered dither, so a cloud that thins
/// slowly shows no bands.
pub const TONES: usize = 16;

/// The colour of each step of the mix from the water to each accent, mixed
/// once from the palette, so a frame mixes no colour.
#[derive(Debug)]
pub struct Tones {
    tones: [[Color; TONES + 1]; 5],
}

impl Tones {
    /// Each step is the colour the canvas blends over the water, so a mix
    /// with a see-through base stays hard as the canvas keeps it.
    pub fn new(palette: &Palette) -> Self {
        let tones = accents(palette).map(|accent| {
            std::array::from_fn(|step| {
                let mut water = Canvas::new(1, 1, palette);
                water.blend(0, 0, accent, step as f32 / TONES as f32);
                water.get(0, 0).unwrap_or(palette.base)
            })
        });
        Self { tones }
    }

    /// The colour of one pixel that holds `inks`, one amount for each
    /// accent. The total mixes the water toward an accent, all the way at
    /// `FULL_INK`, and each accent takes the part of the tile its ink
    /// holds. The choice of accent reads the tile two cells over, so it
    /// does not follow the step of the mix.
    pub fn shade(&self, inks: &[f32; 5], x: usize, row: usize) -> Color {
        let total: f32 = inks.iter().sum();
        let share = (total / FULL_INK).min(1.0);
        let step = (share * TONES as f32 + (dither::rank(x, row) as f32 + 0.5) / SHADES) as usize;
        let step = step.min(TONES);
        let pick = (dither::rank(x + 2, row + 2) as f32 + 0.5) / SHADES * total;
        let mut reached = 0.0;
        for (tones, amount) in self.tones.iter().zip(inks) {
            reached += amount;
            if pick < reached {
                return tones[step];
            }
        }
        self.tones[0][0]
    }
}

#[derive(Debug, Clone, Copy)]
struct Wave {
    across: f32,
    down: f32,
    height: f32,
    drift: f32,
    phase: f32,
}

/// The flow of the water: the curl of a stream function made of seeded
/// sine waves. A curl has no divergence, so the ink neither piles up nor
/// drains. The stream function is held at zero on the walls, so nothing
/// flows through one and the ink slides along it instead.
#[derive(Debug)]
pub struct Currents {
    waves: Vec<Wave>,
}

impl Currents {
    pub fn new(rng: &mut Rng) -> Self {
        let waves = (0..WAVES)
            .map(|_| {
                let heading = rng.random_range(0.0..TAU);
                let number = TAU / rng.random_range(WAVELENGTH);
                let turn = if rng.random_bool(0.5) { 1.0 } else { -1.0 };
                Wave {
                    across: number * heading.cos(),
                    down: number * heading.sin(),
                    height: rng.random_range(WAVE_SPEED) / number,
                    drift: turn * rng.random_range(DRIFT),
                    phase: rng.random_range(0.0..TAU),
                }
            })
            .collect();
        Self { waves }
    }

    /// The velocity at a point of a pane `width` by `rows` pixels, in
    /// pixels a tick, `time` ticks after the start.
    pub fn velocity(&self, x: f32, y: f32, time: f32, (width, rows): (f32, f32)) -> (f32, f32) {
        let (mut stream, mut slope_x, mut slope_y) = (0.0, 0.0, 0.0);
        for wave in &self.waves {
            let angle = wave.across * x + wave.down * y + wave.drift * time + wave.phase;
            let (sin, cos) = angle.sin_cos();
            stream += wave.height * sin;
            slope_x += wave.height * wave.across * cos;
            slope_y += wave.height * wave.down * cos;
        }
        let (wall_x, wall_slope_x) = window(x, width);
        let (wall_y, wall_slope_y) = window(y, rows);
        (
            wall_x * (slope_y * wall_y + stream * wall_slope_y),
            -wall_y * (slope_x * wall_x + stream * wall_slope_x),
        )
    }
}

/// The cubic B-spline at `distance`: a smooth hump that is none from a
/// distance of two on, and whose samples one apart always sum to one.
fn spline(distance: f32) -> f32 {
    let distance = distance.abs();
    if distance < 1.0 {
        (4.0 - 6.0 * distance.powi(2) + 3.0 * distance.powi(3)) / 6.0
    } else if distance < 2.0 {
        (2.0 - distance).powi(3) / 6.0
    } else {
        0.0
    }
}

/// A hump that is zero at both ends of `0..=length` and one in the middle,
/// with its slope.
fn window(at: f32, length: f32) -> (f32, f32) {
    let (sin, cos) = (PI * at / length).sin_cos();
    (sin, PI / length * cos)
}

#[derive(Debug, Clone, Copy)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    /// The move of the last tick, in square units. The next move draws on
    /// the random jitter, which a frame cannot know, but it is close to
    /// this one. So a frame between two ticks carries the particle on by
    /// its share of this move.
    step: (f32, f32),
    /// Ticks of ink left, out of `lifetime`.
    left: u32,
    lifetime: u32,
}

impl Particle {
    fn ink(&self) -> f32 {
        self.left as f32 / self.lifetime as f32
    }
}

#[derive(Debug)]
struct Drop {
    serial: u64,
    accent: usize,
    x: f32,
    y: f32,
    speed: f32,
    /// Ticks of ink the head has still to release.
    emitting: u32,
    particles: Vec<Particle>,
}

#[derive(Debug)]
pub struct Ink {
    rng: Rng,
    palette: &'static Palette,
    tones: Tones,
    /// The pane in square units, one a cell across and two a cell down.
    width: usize,
    rows: usize,
    drops: Vec<Drop>,
    currents: Currents,
    /// Ticks since the start, the time the currents read.
    time: u32,
    /// How far the scene has run towards its next tick.
    progress: f32,
    next_serial: u64,
    last_accent: Option<usize>,
    /// Ticks until the next drop.
    wait: u32,
}

impl Ink {
    pub fn new(mut rng: Rng, palette: &'static Palette) -> Self {
        let currents = Currents::new(&mut rng);
        Self {
            rng,
            palette,
            tones: Tones::new(palette),
            width: 0,
            rows: 0,
            drops: Vec::new(),
            currents,
            time: 0,
            progress: 0.0,
            next_serial: 0,
            last_accent: None,
            wait: 0,
        }
    }

    /// Every drop that still holds ink, by the order it fell, with its
    /// accent.
    pub fn drops(&self) -> impl Iterator<Item = (u64, Color)> + '_ {
        let accents = accents(self.palette);
        self.drops
            .iter()
            .map(move |drop| (drop.serial, accents[drop.accent]))
    }

    /// Every particle's place, in square units from the top left of the
    /// pane, one a cell across and two a cell down.
    pub fn particles(&self) -> impl Iterator<Item = (f32, f32)> + '_ {
        self.drops
            .iter()
            .flat_map(|drop| drop.particles.iter().map(|each| (each.x, each.y)))
    }

    fn drop_one(&mut self) {
        let count = accents(self.palette).len();
        let accent = match self.last_accent {
            None => self.rng.random_range(0..count),
            Some(last) => {
                let other = self.rng.random_range(0..count - 1);
                if other >= last { other + 1 } else { other }
            }
        };
        self.last_accent = Some(accent);
        let width = self.width as f32;
        let x = self
            .rng
            .random_range(width * MARGIN..=width * (1.0 - MARGIN));
        let speed = self.rng.random_range(ENTRY_SPEED);
        self.drops.push(Drop {
            serial: self.next_serial,
            accent,
            x,
            y: 0.0,
            speed,
            emitting: EMIT_TICKS,
            particles: Vec::new(),
        });
        self.next_serial += 1;
    }

    /// The ink of each accent in each pixel. A particle shares its ink
    /// among the pixels near it by a cubic B-spline `SPLAT` pixels wide,
    /// so a cloud is one smooth density that fades at its edge and does not
    /// break into lone pixels. The shares always sum to one, so a cloud
    /// that glides keeps its ink.
    fn inks(&self) -> Vec<[f32; 5]> {
        let (columns, rows) = (self.width * PIXELS_ACROSS, self.rows * PIXELS_DOWN);
        let mut inks = vec![[0.0_f32; 5]; columns * rows];
        let taps = |at: f32| {
            let first = (at - 2.0 * SPLAT).ceil();
            (0..=(4.0 * SPLAT) as usize).map(move |tap| {
                let place = first + tap as f32;
                (place, spline((place - at) / SPLAT) / SPLAT)
            })
        };
        for drop in &self.drops {
            for particle in &drop.particles {
                let (x, y) = self.inside(
                    particle.x + particle.step.0 * self.progress,
                    particle.y + particle.step.1 * self.progress,
                );
                let x = x * PIXELS_ACROSS as f32 - 0.5;
                let y = y * PIXELS_DOWN as f32 - 0.5;
                for (column, across) in taps(x) {
                    for (row, down) in taps(y) {
                        if column < 0.0 || row < 0.0 {
                            continue;
                        }
                        let (column, row) = (column as usize, row as usize);
                        if column < columns && row < rows {
                            inks[row * columns + column][drop.accent] +=
                                particle.ink() * across * down;
                        }
                    }
                }
            }
        }
        inks
    }

    fn inside(&self, x: f32, y: f32) -> (f32, f32) {
        (
            x.clamp(0.0, self.width as f32 - INSIDE),
            y.clamp(0.0, self.rows as f32 - INSIDE),
        )
    }
}

impl Scene for Ink {
    fn tick(&mut self, area: Rect) {
        let width = usize::from(area.width);
        let rows = usize::from(area.height) * UNITS_DOWN;
        if (width, rows) != (self.width, self.rows) {
            self.width = width;
            self.rows = rows;
            self.drops.clear();
            self.wait = 0;
        }
        if width == 0 || rows == 0 {
            return;
        }
        if self.wait == 0 {
            self.drop_one();
            self.wait = self.rng.random_range(DROP_EVERY);
        } else {
            self.wait -= 1;
        }

        let pane = (width as f32, rows as f32);
        let time = self.time as f32;
        let mut drops = std::mem::take(&mut self.drops);
        for drop in &mut drops {
            if drop.emitting > 0 {
                for _ in 0..PARTICLES_PER_TICK {
                    let lifetime = self.rng.random_range(LIFETIME);
                    drop.particles.push(Particle {
                        x: drop.x,
                        y: drop.y,
                        vx: self.rng.random_range(-SPREAD..=SPREAD),
                        vy: drop.speed * self.rng.random_range(SHARE_OF_HEAD),
                        step: (0.0, 0.0),
                        left: lifetime,
                        lifetime,
                    });
                }
                let (across, down) = self.currents.velocity(drop.x, drop.y, time, pane);
                (drop.x, drop.y) = self.inside(drop.x + across, drop.y + down + drop.speed);
                drop.speed *= HEAD_DRAG;
                drop.emitting -= 1;
            }
            for particle in &mut drop.particles {
                particle.vx = particle.vx * DRAG + self.rng.random_range(-JITTER..=JITTER);
                particle.vy *= DRAG;
                let (across, down) = self.currents.velocity(particle.x, particle.y, time, pane);
                let (x, y) = self.inside(
                    particle.x + particle.vx + across,
                    particle.y + particle.vy + down + SINK,
                );
                particle.step = (x - particle.x, y - particle.y);
                (particle.x, particle.y) = (x, y);
                particle.left -= 1;
            }
            drop.particles.retain(|particle| particle.left > 0);
        }
        drops.retain(|drop| drop.emitting > 0 || !drop.particles.is_empty());
        self.drops = drops;
        self.time = self.time.wrapping_add(1);
    }

    fn set_progress(&mut self, progress: f64) {
        self.progress = progress as f32;
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let width = u16::try_from(self.width).unwrap_or(u16::MAX);
        let height = u16::try_from(self.rows / UNITS_DOWN).unwrap_or(u16::MAX);
        let inks = self.inks();
        let mut canvas = Canvas::new(width, height, self.palette);
        let columns = canvas.width();
        for (index, ink) in inks.iter().enumerate() {
            let (column, row) = (index % columns, index / columns);
            canvas.set(column, row, self.tones.shade(ink, column, row));
        }
        canvas.render(area, buf);
    }
}
