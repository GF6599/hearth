use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// Draws a cell as two pixels: the foreground is the upper one and the
/// background the lower one.
const HALF_BLOCK: char = '▀';

/// The fire takes this share of the width, and no more than the widest
/// span, so a wide terminal shows a hearth and not a bonfire.
const FIRE_SHARE: (usize, usize) = (3, 5);
const WIDEST_FIRE: usize = 72;
const NARROWEST_FIRE: usize = 12;

/// Pixel rows below the logs: the grate bar, its legs, and two rows of
/// stones. A pane with no room for a log over them draws nothing.
const BELOW_LOGS: usize = 4;
const LEAST_ROWS: usize = BELOW_LOGS + 2;

/// Pixels in a stone, with one pixel of mortar after it.
const STONE: usize = 6;
const GRATE_LEG_EVERY: usize = 8;

/// Ticks for one pulse of a glowing pixel, so neighbours pulse out of step.
const GLOW_PERIODS: std::ops::RangeInclusive<u8> = 48..=96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pixel {
    Empty,
    Stone,
    Grate,
    /// The lit top edge of a log, its body, and its shadowed underside.
    Bark,
    Heart,
    Shadow,
    Glow {
        phase: u8,
        period: u8,
    },
}

#[derive(Debug)]
pub struct Embers {
    rng: Rng,
    palette: &'static Palette,
    width: usize,
    rows: usize,
    pixels: Vec<Pixel>,
    ticks: u32,
}

impl Embers {
    pub fn new(rng: Rng, palette: &'static Palette) -> Self {
        Self {
            rng,
            palette,
            width: 0,
            rows: 0,
            pixels: Vec::new(),
            ticks: 0,
        }
    }

    fn set(&mut self, x: usize, y: usize, pixel: Pixel) {
        if x < self.width && y < self.rows {
            self.pixels[y * self.width + x] = pixel;
        }
    }

    /// Stones along the bottom, a grate over them, and a stack of two or
    /// three logs on the grate.
    fn lay(&mut self) {
        let (width, rows) = (self.width, self.rows);
        self.pixels = vec![Pixel::Empty; width * rows];
        if width == 0 || rows < LEAST_ROWS {
            return;
        }
        for (offset, y) in [(0, rows - 2), (STONE / 2, rows - 1)] {
            for x in 0..width {
                let stone = (x + offset) % (STONE + 1) != STONE;
                self.set(x, y, if stone { Pixel::Stone } else { Pixel::Shadow });
            }
        }
        let span = (width * FIRE_SHARE.0 / FIRE_SHARE.1)
            .clamp(NARROWEST_FIRE.min(width), WIDEST_FIRE.min(width));
        let left = (width - span) / 2;
        for x in left..left + span {
            self.set(x, rows - BELOW_LOGS, Pixel::Grate);
            if (x - left) % GRATE_LEG_EVERY == 0 || x + 1 == left + span {
                self.set(x, rows - BELOW_LOGS + 1, Pixel::Grate);
            }
        }
        self.stack(left, span, rows - BELOW_LOGS - 1);
    }

    /// Two logs, one leaning on the other, or three, one across a pair.
    fn stack(&mut self, left: usize, span: usize, floor: usize) {
        let thickness = (self.rows / 8).clamp(3, 8);
        let jitter = |rng: &mut Rng| rng.random_range(0..=span / 12);
        let tenth = |n: usize| left + span * n / 10;
        if self.rng.random_bool(0.5) {
            let a_start = tenth(0) + jitter(&mut self.rng);
            let (a_end, b_start) = (tenth(7) - jitter(&mut self.rng), tenth(3));
            self.log(a_start, a_end, floor, floor, thickness);
            let lifted = floor.saturating_sub(thickness);
            self.log(b_start, tenth(10) - 1, lifted, floor, thickness);
        } else {
            let a_end = tenth(6) - jitter(&mut self.rng);
            let b_start = tenth(4) + jitter(&mut self.rng);
            self.log(tenth(0), a_end, floor, floor, thickness);
            self.log(b_start, tenth(10) - 1, floor, floor, thickness);
            let top = floor.saturating_sub(thickness);
            let tilt = self.rng.random_range(0..=1);
            self.log(tenth(2), tenth(8), top, top.saturating_sub(tilt), thickness);
        }
    }

    /// One log from column `start` to `end`, its bottom row going from
    /// `from` to `to`. The cut ends glow, and so do a few cracks and part of
    /// the underside, where the coals are.
    fn log(&mut self, start: usize, end: usize, from: usize, to: usize, thickness: usize) {
        if end <= start + 2 {
            return;
        }
        let length = end - start;
        let cracks: Vec<(usize, usize, usize)> = (0..length / 6)
            .map(|_| {
                (
                    self.rng.random_range(start + 2..end - 1),
                    self.rng.random_range(1..thickness.max(2)),
                    self.rng.random_range(2..=4),
                )
            })
            .collect();
        for x in start..=end {
            let along = x - start;
            let bottom = (from * (length - along) + to * along) / length;
            for depth in 0..thickness {
                let Some(y) = bottom.checked_sub(depth) else {
                    continue;
                };
                let end_grain = x <= start + 1 || x + 1 >= end;
                let cracked = cracks
                    .iter()
                    .any(|&(cx, cd, run)| depth == cd && (cx..cx + run).contains(&x));
                let underside = depth == 0 && self.rng.random_bool(0.35);
                let pixel = if end_grain || cracked || underside {
                    Pixel::Glow {
                        phase: self.rng.random(),
                        period: self.rng.random_range(GLOW_PERIODS),
                    }
                } else if depth == 0 {
                    Pixel::Shadow
                } else if depth + 1 == thickness {
                    Pixel::Bark
                } else {
                    Pixel::Heart
                };
                self.set(x, y, pixel);
            }
        }
    }

    fn colour(&self, pixel: Pixel) -> Color {
        let palette = self.palette;
        match pixel {
            Pixel::Empty => palette.base,
            Pixel::Stone | Pixel::Heart => palette.muted,
            Pixel::Grate | Pixel::Bark => palette.subtle,
            Pixel::Shadow => palette.overlay,
            Pixel::Glow { phase, period } => {
                let ramp = [palette.love, palette.rose, palette.gold];
                let step = u64::from(self.ticks) * 256 / u64::from(period.max(1));
                let at = (step + u64::from(phase)) % 256;
                let level = if at < 128 { at * 2 } else { (255 - at) * 2 };
                let index = usize::try_from(level * 3 / 256).unwrap_or(0);
                ramp[index.min(ramp.len() - 1)]
            }
        }
    }
}

impl Scene for Embers {
    fn tick(&mut self, area: Rect) {
        let width = usize::from(area.width);
        let rows = usize::from(area.height) * 2;
        if (width, rows) != (self.width, self.rows) {
            self.width = width;
            self.rows = rows;
            self.lay();
        }
        self.ticks = self.ticks.wrapping_add(1);
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        for y in 0..area.height {
            let upper = usize::from(y) * 2;
            if upper + 1 >= self.rows {
                break;
            }
            for x in 0..area.width {
                let column = usize::from(x);
                if column >= self.width {
                    break;
                }
                let at = |row: usize| self.colour(self.pixels[row * self.width + column]);
                buf[(area.x + x, area.y + y)]
                    .set_char(HALF_BLOCK)
                    .set_fg(at(upper))
                    .set_bg(at(upper + 1));
            }
        }
    }
}
