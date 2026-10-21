use std::f32::consts::{FRAC_PI_2, TAU};
use std::ops::RangeInclusive;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::octant::{CELL_HEIGHT, CELL_WIDTH, Canvas};
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
/// The pixel rows under the foam line over which the foam fades into the
/// water, so the edge of a wave is soft and not one hard line.
const FADE: usize = 6;
/// The shades of the water from the shallows to the deep.
const SEA_SHADES: usize = 16;
/// The share of foam in the shallow water, and the share of the dark in the
/// deepest water.
const SHALLOW: f32 = 0.3;
const DEEP: f32 = 0.55;
/// The shades of dry sand, from a light that leans to rose to a shade that
/// leans to muted, with gold in the middle. Each cell takes one shade, and
/// the shade changes slowly over the beach, so the sand shows no grain.
const SAND_SHADES: usize = 8;
const SAND_LIGHT: f32 = 0.35;
const SAND_SHADOW: f32 = 0.15;
/// The steps of wetness, and the share of the dark in sand the sea has
/// just left. The share falls as the sand dries, so the wet band fades into
/// the dry and does not end in a hard line.
const WET_SHADES: usize = 8;
const WET: f32 = 0.28;
/// The share of the dark in a shell or a pebble the sea has just left. A
/// shell or a pebble darkens less than sand, so it keeps the colour of its
/// own kind.
const WET_OBJECT: f32 = 0.11;
/// The most a shell leans from love to rose, and a pebble from subtle to
/// muted, so each one has a shade of its own.
const SHELL_TINT: f32 = 0.5;
const PEBBLE_TINT: f32 = 0.3;
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
    /// The shade of the dry sand in each cell, by row and then by column.
    sand: Vec<usize>,
    shades: Shades,
}

#[derive(Debug, Clone, Copy)]
struct Object {
    /// The pixels it fills, by the bits of an octant glyph.
    mask: u8,
    /// Its colour dry and at each step of wetness.
    tones: [Color; WET_SHADES + 1],
}

/// Every colour the beach draws, mixed once from the palette, so a frame
/// mixes no colour.
#[derive(Debug)]
struct Shades {
    /// The water at each shade from the shallows to the deep, under each
    /// step of the fading foam, from none to the most.
    sea: [[Color; FADE + 1]; SEA_SHADES],
    /// The sand at each shade, dry and at each step of wetness.
    sand: [[Color; WET_SHADES + 1]; SAND_SHADES],
    foam: Color,
    /// What a wet thing darkens toward.
    dark: Color,
}

impl Shades {
    fn new(palette: &Palette) -> Self {
        // Deep water and wet sand darken toward overlay, the darkest role
        // that stays painted over the terminal's background, or toward a
        // light theme's text, which is dark.
        let dark = if palette.is_light() {
            palette.text
        } else {
            palette.overlay
        };
        let water = [
            blended(palette, palette.pine, palette.foam, SHALLOW),
            palette.pine,
            blended(palette, palette.pine, dark, DEEP),
        ];
        let sea = std::array::from_fn(|shade| {
            let body = ramp(palette, &water, step(shade, SEA_SHADES));
            std::array::from_fn(|foam| {
                blended(palette, body, palette.foam, foam as f32 / (FADE + 1) as f32)
            })
        });
        let dry = [
            blended(palette, palette.gold, palette.rose, SAND_LIGHT),
            palette.gold,
            blended(palette, palette.gold, palette.muted, SAND_SHADOW),
        ];
        let sand = std::array::from_fn(|shade| {
            wet_tones(
                palette,
                ramp(palette, &dry, step(shade, SAND_SHADES)),
                dark,
                WET,
            )
        });
        Self {
            sea,
            sand,
            foam: palette.foam,
            dark,
        }
    }
}

/// The place of `shade` along `shades` evenly spread steps, from 0 to 1.
fn step(shade: usize, shades: usize) -> f32 {
    shade as f32 / (shades - 1) as f32
}

/// The colour `over` gives a pixel of `under` at `share`, as the canvas
/// blends it, so a role that is the see-through base never mixes.
fn blended(palette: &Palette, under: Color, over: Color, share: f32) -> Color {
    let mut pixel = Canvas::new(1, 1, palette);
    pixel.set(0, 0, under);
    pixel.blend(0, 0, over, share);
    pixel.get(0, 0).unwrap_or(under)
}

/// The colour at `at` along `stops`, as `octant::ramp` places it, each
/// step blended as the canvas blends it.
fn ramp(palette: &Palette, stops: &[Color], at: f32) -> Color {
    let last = stops.len() - 1;
    let place = at.clamp(0.0, 1.0) * last as f32;
    let below = (place as usize).min(last - 1);
    blended(
        palette,
        stops[below],
        stops[below + 1],
        place - below as f32,
    )
}

/// `dry` at each step of wetness, mixed toward `wet` by up to `most` as the
/// sea just left it, so a wet thing darkens and dries back to its own
/// colour.
fn wet_tones(palette: &Palette, dry: Color, wet: Color, most: f32) -> [Color; WET_SHADES + 1] {
    std::array::from_fn(|wetness| {
        blended(palette, dry, wet, most * wetness as f32 / WET_SHADES as f32)
    })
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
            sand: Vec::new(),
            shades: Shades::new(palette),
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

    /// The colour of each pixel of the cell whose top left pixel is at
    /// (left, top). A pixel under the sea takes the shade of the water of
    /// its row, under the foam that fades from the waterline. A pixel of
    /// sand takes the shade of its cell, darkened by its own wetness. A cell
    /// draws two colours, so where the sea meets the sand its sea is all
    /// foam and its sand takes the mean wetness of its pixels. With more
    /// colours, the cell would keep the pair nearest its pixels, and a dark
    /// wet pixel could take the colour of the water.
    fn colours(
        &self,
        (left, top): (usize, usize),
        pane: (usize, usize),
        waterlines: &[usize],
        shades: &[usize],
    ) -> [Color; 8] {
        let rows = pane.1;
        let at = |pixel: usize| (left + pixel % CELL_WIDTH, top + pixel / CELL_WIDTH);
        let below: [Option<usize>; 8] = std::array::from_fn(|pixel| {
            let (x, row) = at(pixel);
            waterlines[x].checked_sub(rows - row)
        });
        let wetness = |pixel: usize| {
            let (x, row) = at(pixel);
            self.wetness(x, row) as usize
        };
        let wet_step = |wetness: usize, pixels: usize| {
            (wetness * WET_SHADES).div_ceil(pixels * DRY_TICKS as usize)
        };
        let sand = &self.shades.sand[self.sand_shade(left, top, pane)];
        let dry = below.iter().filter(|below| below.is_none()).count();
        if dry == 0 {
            return std::array::from_fn(|pixel| match below[pixel] {
                Some(below) if below >= FOAM => {
                    self.shades.sea[shades[at(pixel).1]][FADE.saturating_sub(below - FOAM)]
                }
                _ => self.shades.foam,
            });
        }
        if dry < 8 {
            let wet = (0..8)
                .filter(|pixel| below[*pixel].is_none())
                .map(wetness)
                .sum();
            let shore = sand[wet_step(wet, dry)];
            return below.map(|below| below.map_or(shore, |_| self.shades.foam));
        }
        let object = self.object(left / CELL_WIDTH, top / CELL_HEIGHT);
        std::array::from_fn(|pixel| {
            let wet = wet_step(wetness(pixel), 1);
            match object {
                Some(object) if object.mask >> pixel & 1 == 1 => object.tones[wet],
                _ => sand[wet],
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
                let tint = self.rng.random_range(0.0..1.0);
                let tone = if shell {
                    blended(
                        self.palette,
                        self.palette.love,
                        self.palette.rose,
                        SHELL_TINT * tint,
                    )
                } else {
                    blended(
                        self.palette,
                        self.palette.subtle,
                        self.palette.muted,
                        PEBBLE_TINT * tint,
                    )
                };
                Some(Object {
                    mask,
                    tones: wet_tones(self.palette, tone, self.shades.dark, WET_OBJECT),
                })
            })
            .collect();
        self.sand = (0..across * down)
            .map(|index| dune_shade(index % across, index / across))
            .collect();
    }

    /// The shade of the dry sand of the cell at (left, top) on a pane
    /// `columns` by `rows` pixels, from the shades laid at the last tick
    /// when the pane is the size it was then.
    fn sand_shade(&self, left: usize, top: usize, (columns, rows): (usize, usize)) -> usize {
        let (column, line) = (left / CELL_WIDTH, top / CELL_HEIGHT);
        let laid = (columns, rows) == (self.columns, self.rows);
        laid.then(|| self.sand.get(line * (columns / CELL_WIDTH) + column))
            .flatten()
            .copied()
            .unwrap_or_else(|| dune_shade(column, line))
    }

    /// The shade of the water in each pixel row: the shallows above the
    /// still water, where a wave runs up the sand, and darker with each
    /// row down to the bottom of the pane. The rows of one cell take two
    /// shades side by side at the most, the lower rows the darker, so a
    /// cell keeps to two colours and the water darkens a pixel row at a
    /// time.
    fn sea_shades(&self, rows: usize, time: f32) -> Vec<usize> {
        let still = rows as f32 * (1.0 - self.level(time));
        let depth = (rows as f32 - still).max(1.0);
        (0..rows)
            .map(|row| {
                let middle = (row / CELL_HEIGHT * CELL_HEIGHT) as f32 + CELL_HEIGHT as f32 / 2.0;
                let place = ((middle - still) / depth).clamp(0.0, 1.0) * (SEA_SHADES - 1) as f32;
                let darker = (place.fract() * CELL_HEIGHT as f32).round() as usize;
                let shade = place as usize + usize::from(row % CELL_HEIGHT >= CELL_HEIGHT - darker);
                shade.min(SEA_SHADES - 1)
            })
            .collect()
    }
}

/// The shade of the dry sand of the cell at (column, line): long, low
/// dunes across the beach, over a slower swell, so the sand changes by a
/// shade at the most from one cell to the next.
fn dune_shade(column: usize, line: usize) -> usize {
    let (x, y) = (column as f32, line as f32);
    let dunes = (0.35 * y + 0.12 * x + 0.6 * (0.05 * x).sin()).sin();
    let swell = (0.07 * x - 0.11 * y).sin();
    let share = 0.5 + 0.22 * dunes + 0.22 * swell;
    (share * (SAND_SHADES - 1) as f32).round() as usize
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
        let time = self.time as f32 + self.progress;
        let waterlines = self.waterlines(columns, rows, time);
        let shades = self.sea_shades(rows, time);
        for top in (0..rows).step_by(CELL_HEIGHT) {
            for left in (0..columns).step_by(CELL_WIDTH) {
                let colours = self.colours((left, top), (columns, rows), &waterlines, &shades);
                for (pixel, colour) in colours.into_iter().enumerate() {
                    canvas.set(left + pixel % CELL_WIDTH, top + pixel / CELL_WIDTH, colour);
                }
            }
        }
        canvas.render(area, buf);
    }
}
