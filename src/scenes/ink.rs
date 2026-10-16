use std::f32::consts::{PI, TAU};
use std::ops::RangeInclusive;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

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
/// share of a dither tile.
pub const FULL_INK: f32 = 1.0;

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
/// Keeps a clamped particle strictly inside the last pixel.
const INSIDE: f32 = 0.01;

/// A cell draws two by two pixels. The ink moves in square units, one
/// cell across by half a cell down, so a pixel is half a unit across.
const PIXELS_ACROSS: usize = 2;
/// A glyph's bits are its upper left, upper right, lower left and lower
/// right quarters, from the lowest bit, and a set bit draws in the
/// foreground.
const QUADRANTS: [char; 16] = [
    ' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█',
];
/// The order in which the cells of a 4 by 4 tile change colour, so each
/// share of the tile is spread across it.
const BAYER: [[usize; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
const SHADES: f32 = 16.0;

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

/// The glyph and its two colours for a cell of four pixels. A glyph holds
/// two colours, the two most common, and a pixel of a third takes the ink
/// of those two rather than the water, so thin ink is not lost.
fn quadrant(pixels: [Color; 4], base: Color) -> (char, Color, Color) {
    let mut counts: Vec<(Color, usize)> = Vec::with_capacity(pixels.len());
    for colour in pixels {
        match counts.iter_mut().find(|(each, _)| *each == colour) {
            Some((_, count)) => *count += 1,
            None => counts.push((colour, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    let first = counts.first().map_or(base, |(colour, _)| *colour);
    let Some(&(second, _)) = counts.get(1) else {
        return (QUADRANTS[0], first, first);
    };
    let (ink, water) = if first == base {
        (second, first)
    } else {
        (first, second)
    };
    let bits = pixels
        .iter()
        .enumerate()
        .filter(|(_, colour)| **colour != water)
        .fold(0, |bits, (quarter, _)| bits | 1 << quarter);
    (QUADRANTS[bits], ink, water)
}

/// The colour of one pixel that holds `inks`, one amount for each accent.
/// The total decides the share of the tile that is inked, and each
/// accent takes the part of that share its ink holds. The second choice
/// reads the tile two cells over, so it does not follow the first.
pub fn shade(palette: &Palette, inks: &[f32; 5], x: usize, row: usize) -> Color {
    let total: f32 = inks.iter().sum();
    let share = (total / FULL_INK).min(1.0);
    if share * SHADES <= BAYER[row % 4][x % 4] as f32 {
        return palette.base;
    }
    let pick = (BAYER[(row + 2) % 4][(x + 2) % 4] as f32 + 0.5) / SHADES * total;
    let mut reached = 0.0;
    for (accent, amount) in accents(palette).into_iter().zip(inks) {
        reached += amount;
        if pick < reached {
            return accent;
        }
    }
    palette.base
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
    /// The pane in square units, one a cell across and two a cell down.
    width: usize,
    rows: usize,
    drops: Vec<Drop>,
    currents: Currents,
    /// Ticks since the start, the time the currents read.
    time: u32,
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
            width: 0,
            rows: 0,
            drops: Vec::new(),
            currents,
            time: 0,
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
    /// among the four pixels nearest it, by how near each is, so a cloud
    /// that thins stays a cloud and does not break into lone dots.
    fn inks(&self) -> Vec<[f32; 5]> {
        let columns = self.width * PIXELS_ACROSS;
        let mut inks = vec![[0.0_f32; 5]; columns * self.rows];
        for drop in &self.drops {
            for particle in &drop.particles {
                let x = particle.x * PIXELS_ACROSS as f32 - 0.5;
                let y = particle.y - 0.5;
                let (left, top) = (x.floor(), y.floor());
                let (right_share, lower_share) = (x - left, y - top);
                for (column, across) in [(left, 1.0 - right_share), (left + 1.0, right_share)] {
                    for (row, down) in [(top, 1.0 - lower_share), (top + 1.0, lower_share)] {
                        if column < 0.0 || row < 0.0 {
                            continue;
                        }
                        let (column, row) = (column as usize, row as usize);
                        if column < columns && row < self.rows {
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
        let rows = usize::from(area.height) * 2;
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
                (particle.x, particle.y) = self.inside(
                    particle.x + particle.vx + across,
                    particle.y + particle.vy + down + SINK,
                );
                particle.left -= 1;
            }
            drop.particles.retain(|particle| particle.left > 0);
        }
        drops.retain(|drop| drop.emitting > 0 || !drop.particles.is_empty());
        self.drops = drops;
        self.time = self.time.wrapping_add(1);
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let inks = self.inks();
        let columns = self.width * PIXELS_ACROSS;
        for y in 0..area.height {
            let top = usize::from(y) * 2;
            if top + 1 >= self.rows {
                break;
            }
            for x in 0..area.width {
                if usize::from(x) >= self.width {
                    break;
                }
                let left = usize::from(x) * PIXELS_ACROSS;
                let pixels = std::array::from_fn(|quarter| {
                    let (column, row) = (left + quarter % 2, top + quarter / 2);
                    shade(self.palette, &inks[row * columns + column], column, row)
                });
                let (glyph, ink, water) = quadrant(pixels, self.palette.base);
                buf[(area.x + x, area.y + y)]
                    .set_char(glyph)
                    .set_fg(ink)
                    .set_bg(water);
            }
        }
    }
}
