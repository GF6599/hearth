use std::ops::{Range, RangeInclusive};

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::octant::{CELL_HEIGHT, CELL_WIDTH, Canvas};
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// The fire takes this share of the width, and no more than the widest
/// span, so a wide terminal shows a hearth and not a bonfire.
const FIRE_SHARE: (usize, usize) = (3, 5);
const WIDEST_FIRE: usize = 72 * CELL_WIDTH;
const NARROWEST_FIRE: usize = 12 * CELL_WIDTH;

/// The rows of pixels in each band of the hearth below the logs, and in
/// the lit top and the shadowed underside of a log: half a cell.
const BAND: usize = CELL_HEIGHT / 2;

/// Pixel rows below the logs: the grate bar, its legs, and two rows of
/// stones. A pane with no room for a log over them draws nothing.
const BELOW_LOGS: usize = 4 * BAND;
const LEAST_ROWS: usize = BELOW_LOGS + 2 * BAND;

/// Pixels in a stone, with a cell of mortar after it.
const STONE: usize = 6 * CELL_WIDTH;
const MORTAR: usize = CELL_WIDTH;
const GRATE_LEG_EVERY: usize = 8 * CELL_WIDTH;

/// Ticks for one pulse of a glowing pixel, so neighbours pulse out of step.
const GLOW_PERIODS: RangeInclusive<u8> = 48..=96;

/// Ticks between two settles, about 20 to 40 seconds at the normal rate.
pub const SETTLE_TICKS: RangeInclusive<u32> = 320..=640;
const BURST: RangeInclusive<usize> = 20..=30;
/// A burst fans out this many pixels either side, so its sparks fill
/// cells of their own at once.
const BURST_REACH: usize = 4 * CELL_WIDTH;
/// A settle lights the glow within this many pixels for this many ticks.
const FLARE_REACH: usize = 4 * CELL_WIDTH;
const FLARE_TICKS: u32 = 32;

/// Ticks one log burns, about three to five minutes at the normal rate.
pub const BURN_TICKS: RangeInclusive<u32> = 2880..=4800;
/// A burnt-out log keeps this share of its length and half its thickness.
const BURNT_LENGTH: f32 = 0.4;
const BURNT_THICKNESS: f32 = 0.5;
/// Past this share of its burn a log's wood chars a shade darker.
const CHARRED: f32 = 0.5;
/// A coal glows for most of its life and cools to ash for its last
/// quarter.
const COAL_TICKS: RangeInclusive<u32> = 600..=1200;
/// A new log drops from this many pixels above the pile, and falls this
/// many a tick.
const DROP_HEIGHT: usize = 4 * CELL_HEIGHT;
const FALL: usize = BAND;
/// A log glows this many pixels in from each end, where the fire eats into
/// it.
const ENDS: usize = 2 * CELL_WIDTH;

/// The chance that a tick lets one spark go, outside a settle.
const SPARK_ODDS: f64 = 0.2;
const SPARK_LIFE: RangeInclusive<u16> = 30..=70;
/// How far a spark climbs and drifts in a tick, in cells, as pixels.
const BURST_RISE: Range<f32> = 0.2 * DOWN..0.6 * DOWN;
const SPARK_RISE: Range<f32> = 0.125 * DOWN..0.3 * DOWN;
const DRIFT: f32 = 0.3 * ACROSS;
const DRIFT_START: f32 = 0.15 * ACROSS;
const DRIFT_STEP: f32 = 0.03 * ACROSS;
const ACROSS: f32 = CELL_WIDTH as f32;
const DOWN: f32 = CELL_HEIGHT as f32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pixel {
    Empty,
    Stone,
    Grate,
    /// The lit top edge of a log, its body, and its shadowed underside.
    Bark,
    Heart,
    Shadow,
    Ash,
    Glow {
        phase: u8,
        period: u8,
    },
}

/// One pixel of a log at its full size: its wood, the share of the burn at
/// which it starts to glow, and the pulse it glows with.
#[derive(Debug, Clone, Copy)]
struct Grain {
    wood: Pixel,
    ignite: f32,
    phase: u8,
    period: u8,
}

#[derive(Debug, Clone)]
struct Log {
    start: usize,
    end: usize,
    /// The bottom row at the start and at the end column.
    from: usize,
    to: usize,
    thickness: usize,
    /// Along the log, then up from its underside.
    grain: Vec<Grain>,
    burn: u32,
    left: u32,
    falling: bool,
}

impl Log {
    fn burnt(&self) -> f32 {
        1.0 - self.left as f32 / self.burn.max(1) as f32
    }

    /// The columns left, trimmed evenly from both ends as it burns.
    fn span(&self) -> (usize, usize) {
        let full = self.end - self.start;
        let kept = (full as f32 * (1.0 - (1.0 - BURNT_LENGTH) * self.burnt())) as usize;
        let trim = (full - kept.min(full)) / 2;
        (self.start + trim, self.end - trim)
    }

    fn depth(&self) -> usize {
        let kept = self.thickness as f32 * (1.0 - BURNT_THICKNESS * self.burnt());
        (kept.ceil() as usize).max(BAND)
    }

    fn bottom(&self, x: usize) -> usize {
        let (length, along) = (self.end - self.start, x - self.start);
        (self.from * (length - along) + self.to * along) / length.max(1)
    }

    /// Every pixel the log covers now. Its two ends always glow, where the
    /// fire eats into it, and the rest glows once the burn passes its grain.
    fn pixels(&self) -> Vec<(usize, usize, Pixel)> {
        let (start, end) = self.span();
        let (depth, burnt) = (self.depth(), self.burnt());
        let mut out = Vec::new();
        for x in start..=end {
            let bottom = self.bottom(x);
            for up in 0..depth {
                let Some(y) = bottom.checked_sub(up) else {
                    continue;
                };
                let grain = self.grain[(x - self.start) * self.thickness + up];
                let pixel = if x < start + ENDS || x + ENDS > end || burnt >= grain.ignite {
                    Pixel::Glow {
                        phase: grain.phase,
                        period: grain.period,
                    }
                } else if up + BAND >= depth {
                    Pixel::Bark
                } else {
                    grain.wood
                };
                out.push((x, y, char_if(pixel, burnt)));
            }
        }
        out
    }
}

/// Past `CHARRED`, each wood pixel takes the next darker role.
fn char_if(pixel: Pixel, burnt: f32) -> Pixel {
    match pixel {
        Pixel::Bark if burnt > CHARRED => Pixel::Heart,
        Pixel::Heart if burnt > CHARRED => Pixel::Shadow,
        other => other,
    }
}

#[derive(Debug, Clone)]
struct Coal {
    x: usize,
    y: usize,
    life: u32,
    left: u32,
    phase: u8,
    period: u8,
}

/// What holds a log up besides the other logs: the ground and the coals.
struct Bed<'a> {
    ground: &'a [Pixel],
    coals: &'a [Coal],
    width: usize,
    rows: usize,
}

impl Bed<'_> {
    /// Moves each log with nothing under it one pixel down, and gives the
    /// middle of a log that lands.
    fn drop_a_pixel(&self, logs: &mut [Log]) -> Option<(usize, usize)> {
        let mut landed = None;
        for index in 0..logs.len() {
            let log = &logs[index];
            let (start, end) = log.span();
            let below = |x: usize| log.bottom(x) + 1;
            let held = (start..=end).any(|x| {
                let y = below(x);
                y >= self.rows || self.holds(logs, x, y, index)
            });
            let log = &mut logs[index];
            if !held {
                log.from += 1;
                log.to += 1;
                log.falling = true;
            } else if log.falling {
                log.falling = false;
                let middle = (start + end) / 2;
                landed = Some((middle, log.bottom(middle)));
            }
        }
        landed
    }

    /// Whether the pixel at `(x, y)` holds something other than log
    /// `index`: the ground, a coal, or another log.
    fn holds(&self, logs: &[Log], x: usize, y: usize, index: usize) -> bool {
        if self.ground[y * self.width + x] != Pixel::Empty {
            return true;
        }
        if self.coals.iter().any(|coal| (coal.x, coal.y) == (x, y)) {
            return true;
        }
        logs.iter().enumerate().any(|(other, log)| {
            other != index && {
                let (start, end) = log.span();
                (start..=end).contains(&x) && {
                    let bottom = log.bottom(x);
                    y <= bottom && bottom - y < log.depth()
                }
            }
        })
    }
}

/// A spark in pixel space. Its glyph and colour cool with its age.
#[derive(Debug)]
struct Spark {
    x: f32,
    y: f32,
    rise: f32,
    drift: f32,
    age: u16,
    life: u16,
}

#[derive(Debug)]
pub struct Embers {
    rng: Rng,
    palette: &'static Palette,
    width: usize,
    rows: usize,
    /// The stones and the grate, which never change.
    ground: Vec<Pixel>,
    /// The ground with the coals and the logs drawn over it this tick.
    pixels: Vec<Pixel>,
    logs: Vec<Log>,
    coals: Vec<Coal>,
    /// The fire's columns and the row its logs and coals rest on.
    left: usize,
    span: usize,
    floor: usize,
    /// Each glowing pixel, and the top of the stack in each column, so a
    /// spark starts above the wood where it can be seen.
    glows: Vec<(usize, usize)>,
    tops: Vec<Option<usize>>,
    sparks: Vec<Spark>,
    settle_in: u32,
    /// The pixel a settle flared, and the tick its flare ends.
    flare: Option<(usize, usize, u32)>,
    ticks: u32,
    /// The pixels each log falls on the next tick.
    falls: Vec<usize>,
    /// The share of the way from the last tick to the next. A falling log
    /// and a spark draw on from their tick by it.
    progress: f64,
}

impl Embers {
    pub fn new(mut rng: Rng, palette: &'static Palette) -> Self {
        let settle_in = rng.random_range(SETTLE_TICKS);
        Self {
            rng,
            palette,
            width: 0,
            rows: 0,
            ground: Vec::new(),
            pixels: Vec::new(),
            logs: Vec::new(),
            coals: Vec::new(),
            left: 0,
            span: 0,
            floor: 0,
            glows: Vec::new(),
            tops: Vec::new(),
            sparks: Vec::new(),
            settle_in,
            flare: None,
            ticks: 0,
            falls: Vec::new(),
            progress: 0.0,
        }
    }

    fn pixel(&self, x: usize, y: usize) -> Pixel {
        self.pixels[y * self.width + x]
    }

    /// Sparks from the top of the stack over column `x`, fanned out when
    /// there are many.
    fn throw(&mut self, x: usize, count: usize, rise: Range<f32>) {
        for _ in 0..count {
            let spread = if count > 1 {
                self.rng.random_range(0..=2 * BURST_REACH)
            } else {
                BURST_REACH
            };
            let column = (x + spread).saturating_sub(BURST_REACH).min(self.width - 1);
            // A fan past a log's end starts at the height it came from.
            let Some(top) = self.tops[column].or(self.tops[x]) else {
                continue;
            };
            let spark = Spark {
                x: column as f32 + 0.5,
                y: top as f32 - 0.5,
                rise: self.rng.random_range(rise.clone()),
                drift: self.rng.random_range(-DRIFT_START..DRIFT_START),
                age: 0,
                life: self.rng.random_range(SPARK_LIFE),
            };
            self.sparks.push(spark);
        }
    }

    fn move_sparks(&mut self) {
        let width = self.width as f32;
        for spark in &mut self.sparks {
            spark.age += 1;
            spark.y -= spark.rise;
            spark.drift =
                (spark.drift + self.rng.random_range(-DRIFT_STEP..DRIFT_STEP)).clamp(-DRIFT, DRIFT);
            spark.x += spark.drift;
        }
        self.sparks.retain(|spark| {
            spark.age < spark.life && spark.y >= 0.0 && (0.0..width).contains(&spark.x)
        });
    }

    /// A log shifts at `(x, y)`: a burst of fast sparks from there, and the
    /// glow around it flares.
    fn settle(&mut self, x: usize, y: usize) {
        let count = self.rng.random_range(BURST);
        self.throw(x, count, BURST_RISE);
        self.flare = Some((x, y, self.ticks + FLARE_TICKS));
        self.settle_in = self.rng.random_range(SETTLE_TICKS);
    }

    /// Stones along the bottom, a grate over them, and a stack of two or
    /// three logs on the grate.
    fn lay(&mut self) {
        let (width, rows) = (self.width, self.rows);
        self.ground = vec![Pixel::Empty; width * rows];
        self.logs.clear();
        self.coals.clear();
        if width == 0 || rows < LEAST_ROWS {
            self.draw();
            return;
        }
        let mut ground = std::mem::take(&mut self.ground);
        let mut set = |x: usize, y: usize, pixel| ground[y * width + x] = pixel;
        for (offset, top) in [(0, rows - 2 * BAND), (STONE / 2, rows - BAND)] {
            for y in top..top + BAND {
                for x in 0..width {
                    let stone = (x + offset) % (STONE + MORTAR) < STONE;
                    set(x, y, if stone { Pixel::Stone } else { Pixel::Shadow });
                }
            }
        }
        self.span = (width * FIRE_SHARE.0 / FIRE_SHARE.1)
            .clamp(NARROWEST_FIRE.min(width), WIDEST_FIRE.min(width));
        self.left = (width - self.span) / 2;
        self.floor = rows - BELOW_LOGS - 1;
        let (bar, legs) = (rows - BELOW_LOGS, rows - BELOW_LOGS + BAND);
        let right = self.left + self.span;
        for x in self.left..right {
            let leg = (x - self.left) % GRATE_LEG_EVERY < CELL_WIDTH || x + CELL_WIDTH >= right;
            let bottom = if leg { legs + BAND } else { legs };
            for y in bar..bottom {
                set(x, y, Pixel::Grate);
            }
        }
        self.ground = ground;
        self.stack();
        self.draw();
    }

    /// Two logs, one leaning on the other, or three, one across a pair.
    fn stack(&mut self) {
        let (left, span, floor) = (self.left, self.span, self.floor);
        let thickness = self.thickness();
        let jitter = |rng: &mut Rng| rng.random_range(0..=span / 12);
        let tenth = |n: usize| left + span * n / 10;
        if self.rng.random_bool(0.5) {
            let a_start = tenth(0) + jitter(&mut self.rng);
            let (a_end, b_start) = (tenth(7) - jitter(&mut self.rng), tenth(3));
            self.add_log(a_start, a_end, floor, floor, thickness);
            let lifted = floor.saturating_sub(thickness);
            self.add_log(b_start, tenth(10) - 1, lifted, floor, thickness);
        } else {
            let a_end = tenth(6) - jitter(&mut self.rng);
            let b_start = tenth(4) + jitter(&mut self.rng);
            self.add_log(tenth(0), a_end, floor, floor, thickness);
            self.add_log(b_start, tenth(10) - 1, floor, floor, thickness);
            let top = floor.saturating_sub(thickness);
            let tilt = self.rng.random_range(0..=BAND);
            self.add_log(tenth(2), tenth(8), top, top.saturating_sub(tilt), thickness);
        }
    }

    fn thickness(&self) -> usize {
        (self.rows / 8).clamp(3 * BAND, 8 * BAND)
    }

    /// One log from column `start` to `end`, its bottom row going from
    /// `from` to `to`. A few cracks and part of the underside, where the
    /// coals are, glow from the start, and the rest catches as it burns.
    fn add_log(&mut self, start: usize, end: usize, from: usize, to: usize, thickness: usize) {
        if end <= start + ENDS {
            return;
        }
        let length = end - start;
        let middle = start + ENDS..end - CELL_WIDTH;
        let count = if middle.is_empty() { 0 } else { length / 6 };
        let cracks: Vec<(usize, usize, usize)> = (0..count)
            .map(|_| {
                (
                    self.rng.random_range(middle.clone()),
                    self.rng.random_range(1..thickness.max(2)),
                    self.rng.random_range(2 * CELL_WIDTH..=4 * CELL_WIDTH),
                )
            })
            .collect();
        let mut grain = Vec::with_capacity((length + 1) * thickness);
        for x in start..=end {
            for up in 0..thickness {
                let cracked = cracks
                    .iter()
                    .any(|&(cx, cd, run)| up == cd && (cx..cx + run).contains(&x));
                let underside = up < BAND && self.rng.random_bool(0.35);
                let ignite = if cracked || underside {
                    0.0
                } else {
                    self.rng.random_range(0.15..1.0)
                };
                let wood = if up < BAND {
                    Pixel::Shadow
                } else {
                    Pixel::Heart
                };
                grain.push(Grain {
                    wood,
                    ignite,
                    phase: self.rng.random(),
                    period: self.rng.random_range(GLOW_PERIODS),
                });
            }
        }
        let burn = self.rng.random_range(BURN_TICKS);
        self.logs.push(Log {
            start,
            end,
            from,
            to,
            thickness,
            grain,
            burn,
            left: burn,
            falling: false,
        });
    }

    /// A log that has burnt out falls apart into coals where it lay, and a
    /// new log drops onto the pile from above.
    fn burn(&mut self) {
        for log in &mut self.logs {
            log.left = log.left.saturating_sub(1);
        }
        let Some(gone) = self.logs.iter().position(|log| log.left == 0) else {
            return;
        };
        let log = self.logs.swap_remove(gone);
        let (start, end) = log.span();
        for x in start..=end {
            for y in self.floor + 1 - BAND..=self.floor {
                let life = self.rng.random_range(COAL_TICKS);
                self.coals.push(Coal {
                    x,
                    y,
                    life,
                    left: life,
                    phase: self.rng.random(),
                    period: self.rng.random_range(GLOW_PERIODS),
                });
            }
        }
        let length = self.span * self.rng.random_range(5..=7) / 10;
        let start = self.left + self.rng.random_range(0..=self.span - length);
        let end = start + length - 1;
        let pile = (start..=end)
            .filter_map(|x| self.tops.get(x).copied().flatten())
            .min()
            .unwrap_or(self.floor);
        let from = pile.saturating_sub(DROP_HEIGHT);
        let to = from.saturating_sub(self.rng.random_range(0..=BAND));
        let thickness = self.thickness();
        self.add_log(start, end, from, to, thickness);
        if let Some(new) = self.logs.last_mut() {
            new.falling = true;
        }
    }

    fn cool(&mut self) {
        for coal in &mut self.coals {
            coal.left = coal.left.saturating_sub(1);
        }
        self.coals.retain(|coal| coal.left > 0);
    }

    /// A log with nothing under it falls `FALL` pixels a tick, one at a
    /// time, and one that lands settles with a burst.
    fn fall(&mut self) {
        for _ in 0..FALL {
            let bed = Bed {
                ground: &self.ground,
                coals: &self.coals,
                width: self.width,
                rows: self.rows,
            };
            if let Some((x, y)) = bed.drop_a_pixel(&mut self.logs) {
                self.settle(x, y);
            }
        }
    }

    /// The pixels each log falls on the next tick. That tick burns and cools
    /// before the logs fall, so the logs and the coals here burn and cool
    /// first. A tick that burns a log out lays a new one from the random
    /// source, which only the tick can draw, so its logs do not glide.
    fn next_falls(&self) -> Vec<usize> {
        if self.glows.is_empty() || self.logs.iter().any(|log| log.left <= 1) {
            return Vec::new();
        }
        let mut logs = self.logs.clone();
        for log in &mut logs {
            log.left -= 1;
        }
        let coals: Vec<Coal> = self
            .coals
            .iter()
            .filter(|coal| coal.left > 1)
            .cloned()
            .collect();
        let bed = Bed {
            ground: &self.ground,
            coals: &coals,
            width: self.width,
            rows: self.rows,
        };
        for _ in 0..FALL {
            bed.drop_a_pixel(&mut logs);
        }
        logs.iter()
            .zip(&self.logs)
            .map(|(fallen, log)| fallen.from - log.from)
            .collect()
    }

    /// The ground, then the coals, then the logs, and the glow and the top
    /// of each column read back from that.
    fn draw(&mut self) {
        let (width, rows) = (self.width, self.rows);
        self.pixels = self.compose(&[]);
        self.glows = (0..rows)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .filter(|&(x, y)| matches!(self.pixel(x, y), Pixel::Glow { .. }))
            .collect();
        let above_grate = rows.saturating_sub(BELOW_LOGS);
        self.tops = (0..width)
            .map(|x| {
                (0..above_grate).find(|&y| {
                    matches!(
                        self.pixel(x, y),
                        Pixel::Bark
                            | Pixel::Heart
                            | Pixel::Shadow
                            | Pixel::Ash
                            | Pixel::Glow { .. }
                    )
                })
            })
            .collect();
    }

    /// The ground, then the coals, then the logs, each log `shifts` pixels
    /// lower than it lies.
    fn compose(&self, shifts: &[usize]) -> Vec<Pixel> {
        let (width, rows) = (self.width, self.rows);
        let mut pixels = self.ground.clone();
        for coal in &self.coals {
            let pixel = if coal.left > coal.life / 4 {
                Pixel::Glow {
                    phase: coal.phase,
                    period: coal.period,
                }
            } else {
                Pixel::Ash
            };
            pixels[coal.y * width + coal.x] = pixel;
        }
        for (index, log) in self.logs.iter().enumerate() {
            let shift = shifts.get(index).copied().unwrap_or(0);
            for (x, y, pixel) in log.pixels() {
                let y = y + shift;
                if x < width && y < rows {
                    pixels[y * width + x] = pixel;
                }
            }
        }
        pixels
    }

    fn colour(&self, pixel: Pixel, x: usize, y: usize) -> Color {
        let palette = self.palette;
        let flared = self.flare.is_some_and(|(fx, fy, until)| {
            self.ticks < until && x.abs_diff(fx) <= FLARE_REACH && y.abs_diff(fy) <= FLARE_REACH
        });
        match pixel {
            Pixel::Glow { .. } if flared => palette.gold,
            Pixel::Empty => palette.base,
            Pixel::Stone | Pixel::Heart | Pixel::Ash => palette.muted,
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

impl Embers {
    /// A spark shows only in a cell with nothing else in it, so it never
    /// cuts a hole in a log.
    ///
    /// A spark keeps its column between two ticks, because its next drift
    /// is a random step that only the tick can draw.
    fn render_spark(&self, spark: &Spark, pixels: &[Pixel], area: Rect, buf: &mut Buffer) {
        let y = spark.y - spark.rise * self.progress as f32;
        if y < 0.0 {
            return;
        }
        let (column, row) = (spark.x as usize / CELL_WIDTH, y as usize / CELL_HEIGHT);
        let (Ok(x), Ok(y)) = (u16::try_from(column), u16::try_from(row)) else {
            return;
        };
        let (left, top) = (column * CELL_WIDTH, row * CELL_HEIGHT);
        if x >= area.width
            || y >= area.height
            || left + CELL_WIDTH > self.width
            || top + CELL_HEIGHT > self.rows
        {
            return;
        }
        let empty = (top..top + CELL_HEIGHT)
            .all(|y| (left..left + CELL_WIDTH).all(|x| pixels[y * self.width + x] == Pixel::Empty));
        if !empty {
            return;
        }
        let third = spark.life / 3;
        let (glyph, colour) = if spark.age < third {
            ('•', self.palette.gold)
        } else if spark.age < third * 2 {
            ('·', self.palette.rose)
        } else {
            ('.', self.palette.muted)
        };
        buf[(area.x + x, area.y + y)]
            .set_char(glyph)
            .set_fg(colour)
            .set_bg(self.palette.base);
    }

    fn step(&mut self, area: Rect) {
        let width = usize::from(area.width) * CELL_WIDTH;
        let rows = usize::from(area.height) * CELL_HEIGHT;
        if (width, rows) != (self.width, self.rows) {
            self.width = width;
            self.rows = rows;
            self.sparks.clear();
            self.lay();
        }
        self.ticks = self.ticks.wrapping_add(1);
        if self.glows.is_empty() {
            return;
        }
        self.burn();
        self.cool();
        self.fall();
        self.draw();
        self.move_sparks();
        if self.glows.is_empty() {
            return;
        }
        if self.rng.random_bool(SPARK_ODDS) {
            let (x, _) = self.glows[self.rng.random_range(0..self.glows.len())];
            self.throw(x, 1, SPARK_RISE);
        }
        self.settle_in = self.settle_in.saturating_sub(1);
        if self.settle_in == 0 {
            let (x, y) = self.glows[self.rng.random_range(0..self.glows.len())];
            self.settle(x, y);
        }
    }
}

impl Scene for Embers {
    fn tick(&mut self, area: Rect) {
        self.step(area);
        self.falls = self.next_falls();
    }

    fn set_progress(&mut self, progress: f64) {
        self.progress = progress;
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let shifts: Vec<usize> = self
            .falls
            .iter()
            .map(|&fall| (fall as f64 * self.progress) as usize)
            .collect();
        let glided;
        let pixels = if shifts.iter().all(|&shift| shift == 0) {
            &self.pixels
        } else {
            glided = self.compose(&shifts);
            &glided
        };
        let mut canvas = Canvas::new(area.width, area.height, self.palette);
        for y in 0..self.rows.min(canvas.height()) {
            for x in 0..self.width.min(canvas.width()) {
                canvas.set(x, y, self.colour(pixels[y * self.width + x], x, y));
            }
        }
        canvas.render(area, buf);
        for spark in &self.sparks {
            self.render_spark(spark, pixels, area, buf);
        }
    }
}
