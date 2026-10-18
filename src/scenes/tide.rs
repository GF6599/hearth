use std::f32::consts::{FRAC_PI_2, TAU};
use std::ops::RangeInclusive;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::dither;
use crate::quadrant;
use crate::scene::{Rng, Scene};
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
/// The sixteenths of the pixels below the foam line that are foam too, so
/// the edge froths and is not one hard line.
const FROTH: usize = 6;
/// The sixteenths of sand the sea has just left that are drawn muted. The
/// share falls as the sand dries, so the wet band fades and does not end in
/// a hard line.
const WET: u32 = 8;
/// The chance that a cell the sea can reach holds a shell or a pebble.
const OBJECT_CHANCE: f64 = 1.0 / 40.0;
/// The pixels a shell fills, by the bits of a quadrant glyph: ▟, ▙ and ▄.
const SHELLS: [u8; 3] = [0b1110, 0b1101, 0b1100];
/// A pebble is one pixel, on the lower left or lower right of its cell.
const PEBBLES: [u8; 2] = [0b0100, 0b1000];

#[derive(Debug)]
pub struct Tide {
    rng: Rng,
    palette: &'static Palette,
    /// Ticks since the start.
    time: u32,
    /// Where in the tide the scene starts, as a share of one tide.
    tide_phase: f32,
    wave_ticks: f32,
    lag_length: f32,
    lag_phase: f32,
    ripple_length: f32,
    ripple_phase: f32,
    strengths: [f32; SET],
    /// The pane in pixels, two a cell across and two a cell down.
    columns: usize,
    rows: usize,
    /// Ticks until each pixel of sand is dry, by row and then by column.
    wetness: Vec<u32>,
    /// The shell or pebble in each cell, by row and then by column.
    objects: Vec<Option<Object>>,
}

#[derive(Debug, Clone, Copy)]
struct Object {
    /// The pixels it fills, by the bits of a quadrant glyph.
    mask: u8,
    shell: bool,
}

impl Tide {
    pub fn new(mut rng: Rng, palette: &'static Palette) -> Self {
        Self {
            palette,
            time: 0,
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

    /// The still water now, as a share of the height.
    fn level(&self) -> f32 {
        let turn = self.time as f32 / TIDE_TICKS as f32 + self.tide_phase;
        LOW_TIDE + (HIGH_TIDE - LOW_TIDE) * (1.0 - (TAU * turn).cos()) / 2.0
    }

    /// How far the water reaches up pixel column `x` now, as a share of the
    /// height.
    fn reach(&self, x: f32) -> f32 {
        let lag = LAG * (1.0 + (TAU * (x / self.lag_length + self.lag_phase)).sin()) / 2.0;
        let waves = self.time as f32 / self.wave_ticks - lag;
        let phase = waves.rem_euclid(1.0);
        let run = if phase < RUN_UP {
            (phase / RUN_UP * FRAC_PI_2).sin()
        } else {
            ((phase - RUN_UP) / (1.0 - RUN_UP) * FRAC_PI_2).cos()
        };
        let strength = self.strengths[(waves.floor() as i64).rem_euclid(SET as i64) as usize];
        let drift = self.time as f32 * RIPPLE_DRIFT;
        let ripple =
            (1.0 + (TAU * (x / self.ripple_length + self.ripple_phase + drift)).sin()) / 2.0;
        self.level() + SWASH * ((1.0 - RAGGED) * strength * run + RAGGED * ripple * run)
    }

    /// How many pixels of each column, from the bottom, are sea.
    fn waterlines(&self, columns: usize, rows: usize) -> Vec<usize> {
        (0..columns)
            .map(|x| ((self.reach(x as f32) * rows as f32).round() as usize).clamp(1, rows))
            .collect()
    }

    fn pixel(x: usize, row: usize, rows: usize, waterline: usize) -> Pixel {
        let from_bottom = rows - row;
        if from_bottom == waterline
            || (from_bottom + 1 == waterline && dither::lights(FROTH, x, row))
        {
            Pixel::Foam
        } else if from_bottom < waterline {
            Pixel::Sea
        } else {
            Pixel::Sand
        }
    }

    /// The colours of one cell. A cell draws two colours, so where the sea
    /// meets the sand its sea is all foam and its sand is bare, and sand
    /// with any wet pixel shows no grains.
    fn colours(&self, pixels: [Pixel; 4], left: usize, top: usize) -> [Color; 4] {
        let shore = pixels.contains(&Pixel::Sand) && pixels.iter().any(|each| *each != Pixel::Sand);
        let at = |quarter: usize| (left + quarter % 2, top + quarter / 2);
        let wet = (0..4).any(|quarter| {
            let (x, row) = at(quarter);
            pixels[quarter] == Pixel::Sand && self.wetness(x, row) > 0
        });
        if let Some(object) = self.object(left / 2, top / 2)
            && pixels.iter().all(|each| *each == Pixel::Sand)
        {
            let colour = if object.shell {
                self.palette.love
            } else {
                self.palette.subtle
            };
            return std::array::from_fn(|quarter| {
                if object.mask >> quarter & 1 == 1 {
                    colour
                } else {
                    self.palette.gold
                }
            });
        }
        std::array::from_fn(|quarter| {
            let (x, row) = at(quarter);
            match pixels[quarter] {
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
        let across = self.columns / 2;
        self.objects.get(y * across + x).copied().flatten()
    }

    /// Lays shells and pebbles on the cells the sea reaches and also
    /// leaves, so each one is covered and uncovered as the tide turns.
    fn scatter(&mut self) {
        let (across, down) = (self.columns / 2, self.rows / 2);
        let rows = self.rows as f32;
        let reached = self.rows - ((HIGH_TIDE + SWASH) * rows).floor() as usize;
        let left = self
            .rows
            .saturating_sub((LOW_TIDE * rows).ceil() as usize + 2);
        self.objects = (0..across * down)
            .map(|index| {
                let top = index / across * 2;
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
    let tile = mix(((x / 4) as u64) << 32 | (row / 4) as u64);
    let (across, down) = ((tile >> 60) as usize, (tile >> 56 & 0xf) as usize);
    dither::lights(GRAINS, x + across, row + down)
}

/// The splitmix64 finaliser, so neighbouring tiles take unrelated offsets.
fn mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
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
        let (columns, rows) = (usize::from(area.width) * 2, usize::from(area.height) * 2);
        if (columns, rows) != (self.columns, self.rows) {
            self.columns = columns;
            self.rows = rows;
            self.wetness = vec![0; columns * rows];
            self.scatter();
        }
        if columns == 0 || rows == 0 {
            return;
        }
        let waterlines = self.waterlines(columns, rows);
        for (index, wetness) in self.wetness.iter_mut().enumerate() {
            let (x, row) = (index % columns, index / columns);
            *wetness = if rows - row <= waterlines[x] {
                DRY_TICKS
            } else {
                wetness.saturating_sub(1)
            };
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let (columns, rows) = (usize::from(area.width) * 2, usize::from(area.height) * 2);
        if columns == 0 || rows == 0 {
            return;
        }
        let waterlines = self.waterlines(columns, rows);
        for y in 0..area.height {
            for x in 0..area.width {
                let (left, top) = (usize::from(x) * 2, usize::from(y) * 2);
                let pixels = std::array::from_fn(|quarter| {
                    let (column, row) = (left + quarter % 2, top + quarter / 2);
                    Self::pixel(column, row, rows, waterlines[column])
                });
                let colours = self.colours(pixels, left, top);
                let (glyph, fore, back) = quadrant::cell(colours, self.palette.base);
                buf[(area.x + x, area.y + y)]
                    .set_char(glyph)
                    .set_fg(fore)
                    .set_bg(back);
            }
        }
    }
}
