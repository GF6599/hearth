use std::f32::consts::{FRAC_PI_2, TAU};
use std::ops::RangeInclusive;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::dither;
use crate::octant::{CELL_HEIGHT, CELL_WIDTH, Canvas};
use crate::scene::{Rng, Scene};
use crate::splitmix;
use crate::theme::Palette;

/// Ticks in one tide, from low water to high and back, about four minutes.
pub const TIDE_TICKS: u32 = 4000;
/// The still water at low and at high tide, as a share of the pane's
/// height from the bottom.
pub const LOW_TIDE: f32 = 0.2;
pub const HIGH_TIDE: f32 = 0.5;
/// The furthest a wave runs up the sand past the still water, as a share
/// of the pane's height.
pub const SWASH: f32 = 0.12;
/// The sixteenths of each tile of dry sand that are grains.
pub const GRAINS: usize = 1;
/// Ticks from the sea leaving a pixel of sand to the sand being dry, about
/// half a minute.
pub const DRY_TICKS: u32 = 500;

/// Ticks from one wave to the next, about seven to ten seconds.
const WAVE_TICKS: RangeInclusive<f32> = 110.0..=160.0;
/// The share of a wave's time it runs up the sand. It draws back for the
/// rest, so it rushes in and drains away slowly.
const RUN_UP: f32 = 0.35;
/// The most a wave lags from one part of the beach to another, in waves,
/// so it breaks along the shore at a slant and not all at once.
const LAG: f32 = 0.4;
/// The length along the beach over which the lag turns, in pixels.
const LAG_LENGTH: RangeInclusive<f32> = 90.0..=180.0;
/// The share of the run-up that ripples along the edge, so the wave line is
/// uneven.
const RAGGED: f32 = 0.2;
const RIPPLE_LENGTH: RangeInclusive<f32> = 9.0..=17.0;
/// How fast the ripples move along the edge, in waves a tick.
const RIPPLE_DRIFT: f32 = 0.004;
/// The weakest wave, as a share of the strongest. Each wave in a set of
/// `SET` has its own strength, so the run-up varies from wave to wave.
const WEAKEST: f32 = 0.5;
const SET: usize = 7;
/// The pixel rows of the foam line at the top of the sea, half a cell.
const FOAM: usize = CELL_HEIGHT / 2;
/// The sixteenths of the pixels in the `FOAM` rows below the foam line that
/// are foam too, so the edge froths and is not one hard line.
const FROTH: usize = 6;
/// The sixteenths of sand the sea has just left that are drawn muted. The
/// share falls as the sand dries, so the wet band fades and does not end in
/// a hard line.
const WET: u32 = 8;
/// The chance that a cell the sea can reach holds a shell or a pebble.
const OBJECT_CHANCE: f64 = 1.0 / 40.0;
/// The pixels a shell fills, by the bits of an octant glyph, row by row
/// from the top: a cone that opens up to the right, one that opens up to
/// the left, and a clam across the middle of its cell.
const SHELLS: [u8; 3] = [0b1111_1000, 0b1111_0100, 0b0011_1100];
/// A pebble is one pixel across and two down, on the lower left or the
/// lower right of its cell.
const PEBBLES: [u8; 2] = [0b0101_0000, 0b1010_0000];

#[derive(Debug)]
pub struct Tide {
    rng: Rng,
    palette: &'static Palette,
    /// Ticks since the start.
    time: u32,
    /// How far the scene has run towards its next tick, so the sea draws
    /// where it is between two ticks.
    progress: f32,
    /// Where in the tide the scene starts, as a share of one tide.
    tide_phase: f32,
    wave_ticks: f32,
    lag_length: f32,
    lag_phase: f32,
    ripple_length: f32,
    ripple_phase: f32,
    strengths: [f32; SET],
    /// The pane in pixels, two a cell across and four a cell down.
    columns: usize,
    rows: usize,
    /// Ticks until each pixel of sand is dry, by row and then by column.
    wetness: Vec<u32>,
    /// The shell or pebble in each cell, by row and then by column.
    objects: Vec<Option<Object>>,
}

#[derive(Debug, Clone, Copy)]
struct Object {
    /// The pixels it fills, by the bits of an octant glyph.
    mask: u8,
    shell: bool,
}

impl Tide {
    pub fn new(mut rng: Rng, palette: &'static Palette) -> Self {
        Self {
            palette,
            time: 0,
            progress: 0.0,
            tide_phase: rng.random_range(0.0..1.0),
            wave_ticks: rng.random_range(WAVE_TICKS),
            lag_length: rng.random_range(LAG_LENGTH),
            lag_phase: rng.random_range(0.0..1.0),
            ripple_length: rng.random_range(RIPPLE_LENGTH),
            ripple_phase: rng.random_range(0.0..1.0),
            strengths: std::array::from_fn(|_| rng.random_range(WEAKEST..=1.0)),
            columns: 0,
            rows: 0,
            wetness: Vec::new(),
            objects: Vec::new(),
            rng,
        }
    }

    /// Ticks until the sand at pixel (x, row) is dry, from the top left. A
    /// pixel under the sea reads `DRY_TICKS`.
    pub fn wetness(&self, x: usize, row: usize) -> u32 {
        if x < self.columns && row < self.rows {
            self.wetness[row * self.columns + x]
        } else {
            0
        }
    }

    /// The still water at `time` ticks, as a share of the height.
    fn level(&self, time: f32) -> f32 {
        let turn = time / TIDE_TICKS as f32 + self.tide_phase;
        LOW_TIDE + (HIGH_TIDE - LOW_TIDE) * (1.0 - (TAU * turn).cos()) / 2.0
    }

    /// How far the water reaches up pixel column `x` at `time` ticks, as a
    /// share of the height.
    fn reach(&self, x: f32, time: f32) -> f32 {
        let lag = LAG * (1.0 + (TAU * (x / self.lag_length + self.lag_phase)).sin()) / 2.0;
        let waves = time / self.wave_ticks - lag;
        let phase = waves.rem_euclid(1.0);
        let run = if phase < RUN_UP {
            (phase / RUN_UP * FRAC_PI_2).sin()
        } else {
            ((phase - RUN_UP) / (1.0 - RUN_UP) * FRAC_PI_2).cos()
        };
        let strength = self.strengths[(waves.floor() as i64).rem_euclid(SET as i64) as usize];
        let drift = time * RIPPLE_DRIFT;
        let ripple =
            (1.0 + (TAU * (x / self.ripple_length + self.ripple_phase + drift)).sin()) / 2.0;
        self.level(time) + SWASH * ((1.0 - RAGGED) * strength * run + RAGGED * ripple * run)
    }

    /// How many pixels of each column, from the bottom, are sea at `time`
    /// ticks.
    fn waterlines(&self, columns: usize, rows: usize, time: f32) -> Vec<usize> {
        (0..columns)
            .map(|x| ((self.reach(x as f32, time) * rows as f32).round() as usize).clamp(1, rows))
            .collect()
    }

    fn pixel(x: usize, row: usize, rows: usize, waterline: usize) -> Pixel {
        let Some(below) = waterline.checked_sub(rows - row) else {
            return Pixel::Sand;
        };
        if below < FOAM || (below < 2 * FOAM && dither::lights(FROTH, x, row)) {
            Pixel::Foam
        } else {
            Pixel::Sea
        }
    }

    /// The colours of one cell. A cell draws two colours, so where the sea
    /// meets the sand its sea is all foam and its sand is bare, and sand
    /// with any wet pixel shows no grains.
    fn colours(&self, pixels: [Pixel; 8], left: usize, top: usize) -> [Color; 8] {
        let shore = pixels.contains(&Pixel::Sand) && pixels.iter().any(|each| *each != Pixel::Sand);
        let at = |pixel: usize| (left + pixel % CELL_WIDTH, top + pixel / CELL_WIDTH);
        let wet = (0..pixels.len()).any(|pixel| {
            let (x, row) = at(pixel);
            pixels[pixel] == Pixel::Sand && self.wetness(x, row) > 0
        });
        if let Some(object) = self.object(left / CELL_WIDTH, top / CELL_HEIGHT)
            && pixels.iter().all(|each| *each == Pixel::Sand)
        {
            let colour = if object.shell {
                self.palette.love
            } else {
                self.palette.subtle
            };
            return std::array::from_fn(|pixel| {
                if object.mask >> pixel & 1 == 1 {
                    colour
                } else {
                    self.palette.gold
                }
            });
        }
        std::array::from_fn(|pixel| {
            let (x, row) = at(pixel);
            match pixels[pixel] {
                Pixel::Foam => self.palette.foam,
                Pixel::Sea if shore => self.palette.foam,
                Pixel::Sea => self.palette.pine,
                Pixel::Sand if shore => self.palette.gold,
                Pixel::Sand if wet && self.damp(x, row) => self.palette.muted,
                Pixel::Sand if !wet && grain(x, row) => self.palette.rose,
                Pixel::Sand => self.palette.gold,
            }
        })
    }

    fn object(&self, x: usize, y: usize) -> Option<Object> {
        let across = self.columns / CELL_WIDTH;
        self.objects.get(y * across + x).copied().flatten()
    }

    /// Lays shells and pebbles on the cells the sea reaches and also
    /// leaves, so each one is covered and uncovered as the tide turns.
    fn scatter(&mut self) {
        let (across, down) = (self.columns / CELL_WIDTH, self.rows / CELL_HEIGHT);
        let rows = self.rows as f32;
        let reached = self.rows - ((HIGH_TIDE + SWASH) * rows).floor() as usize;
        let left = self
            .rows
            .saturating_sub((LOW_TIDE * rows).ceil() as usize + CELL_HEIGHT);
        self.objects = (0..across * down)
            .map(|index| {
                let top = index / across * CELL_HEIGHT;
                if !(reached..=left).contains(&top) || !self.rng.random_bool(OBJECT_CHANCE) {
                    return None;
                }
                let shell = self.rng.random_bool(0.5);
                let masks: &[u8] = if shell { &SHELLS } else { &PEBBLES };
                let mask = masks[self.rng.random_range(0..masks.len())];
                Some(Object { mask, shell })
            })
            .collect();
    }

    /// Whether the sand pixel at (x, row) is drawn wet, by the share of
    /// `WET` that its wetness still holds.
    fn damp(&self, x: usize, row: usize) -> bool {
        let sixteenths = (WET * self.wetness(x, row)).div_ceil(DRY_TICKS);
        dither::lights(sixteenths as usize, x, row)
    }
}

/// Whether the sand pixel at (x, row) is a grain. Each tile of the dither
/// holds `GRAINS`, shifted by an offset of its own, so the grains scatter
/// and do not line up into a grid.
fn grain(x: usize, row: usize) -> bool {
    let tile = splitmix::mix(((x / 4) as u64) << 32 | (row / 4) as u64);
    let (across, down) = ((tile >> 60) as usize, (tile >> 56 & 0xf) as usize);
    dither::lights(GRAINS, x + across, row + down)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pixel {
    Sea,
    Foam,
    Sand,
}

impl Scene for Tide {
    fn tick(&mut self, area: Rect) {
        self.time = self.time.wrapping_add(1);
        let (columns, rows) = (
            usize::from(area.width) * CELL_WIDTH,
            usize::from(area.height) * CELL_HEIGHT,
        );
        if (columns, rows) != (self.columns, self.rows) {
            self.columns = columns;
            self.rows = rows;
            self.wetness = vec![0; columns * rows];
            self.scatter();
        }
        if columns == 0 || rows == 0 {
            return;
        }
        let waterlines = self.waterlines(columns, rows, self.time as f32);
        for (index, wetness) in self.wetness.iter_mut().enumerate() {
            let (x, row) = (index % columns, index / columns);
            *wetness = if rows - row <= waterlines[x] {
                DRY_TICKS
            } else {
                wetness.saturating_sub(1)
            };
        }
    }

    fn set_progress(&mut self, progress: f64) {
        self.progress = progress as f32;
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let mut canvas = Canvas::new(area.width, area.height, self.palette);
        let (columns, rows) = (canvas.width(), canvas.height());
        if columns == 0 || rows == 0 {
            return;
        }
        let waterlines = self.waterlines(columns, rows, self.time as f32 + self.progress);
        for top in (0..rows).step_by(CELL_HEIGHT) {
            for left in (0..columns).step_by(CELL_WIDTH) {
                let at = |pixel: usize| (left + pixel % CELL_WIDTH, top + pixel / CELL_WIDTH);
                let pixels = std::array::from_fn(|pixel| {
                    let (column, row) = at(pixel);
                    Self::pixel(column, row, rows, waterlines[column])
                });
                for (pixel, colour) in self.colours(pixels, left, top).into_iter().enumerate() {
                    let (column, row) = at(pixel);
                    canvas.set(column, row, colour);
                }
            }
        }
        canvas.render(area, buf);
    }
}
