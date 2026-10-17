use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::time::Duration;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

use crate::braille::{self, CELL_HEIGHT, CELL_WIDTH};
use crate::clock::Clock;
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

pub const WORK: Duration = Duration::from_secs(25 * 60);
pub const BREAK: Duration = Duration::from_secs(5 * 60);
pub const LONG_BREAK: Duration = Duration::from_secs(15 * 60);
/// Work rounds before the long break.
const ROUNDS: u32 = 4;
/// The share of a bulb that the sand fills. Under half, so the pile in the
/// bottom stays clear of the neck.
const FILL: f32 = 0.45;
/// Rows of cells kept free above and below the frame.
const MARGIN: i32 = 1;
/// How far round a sine each bulb runs from the neck. Past the peak, so a
/// bulb is widest a little beyond its middle and curves in toward its cap.
const ROUND: f32 = 0.8;
/// Cells from the glass's widest wall out to a post of the frame.
const POST: i32 = 2;
/// Cells from the right post out to the round marks.
const MARKS: i32 = 2;
const WIDTH: i32 = CELL_WIDTH as i32;
const HEIGHT: i32 = CELL_HEIGHT as i32;

/// The glass in cells. Its walls and its inside are whole cells, so a wall
/// and the sand never share a cell, which holds one colour.
#[derive(Debug, Clone, Copy)]
struct Glass {
    /// The column of the neck, the middle of the glass.
    centre: i32,
    /// The row of the neck.
    neck: i32,
    bulb: i32,
    /// The half width of a bulb at its widest row. As many cells as the
    /// bulb has rows makes it round, because a cell holds two dots across
    /// and four down.
    reach: i32,
}

impl Glass {
    /// The largest glass that fits the pane in its frame with its neck on
    /// the middle row, or `None` when not even one row a bulb fits. A cap
    /// and a plate lie past each end of the glass.
    fn fit(width: u16, height: u16) -> Option<Glass> {
        let (width, height) = (i32::from(width), i32::from(height));
        let (centre, neck) = (width / 2, height / 2);
        let bulb = (neck - 2 - MARGIN).min(height - neck - 3 - MARGIN);
        let reach = bulb
            .min(centre - POST - 1)
            .min(width - centre - POST - MARKS - 2);
        (bulb >= 1 && reach >= 1).then_some(Glass {
            centre,
            neck,
            bulb,
            reach,
        })
    }

    /// The half width of the inside at a row: one cell at the neck, out
    /// along a sine to the widest row and in again toward the cap.
    fn half_width(&self, row: i32) -> Option<i32> {
        let from_neck = (row - self.neck).abs();
        let turn = std::f32::consts::PI * ROUND * from_neck as f32 / self.bulb as f32;
        (from_neck <= self.bulb).then(|| (self.reach as f32 * turn.sin()).round() as i32)
    }

    /// Whether a dot is inside the glass, where sand can go.
    fn holds(&self, x: i32, y: i32) -> bool {
        x >= 0
            && y >= 0
            && self
                .half_width(y.div_euclid(HEIGHT))
                .is_some_and(|half| (x.div_euclid(WIDTH) - self.centre).abs() <= half)
    }

    /// The columns and rows of dots that the inside spans.
    fn span(&self) -> (Range<i32>, Range<i32>) {
        (
            (self.centre - self.reach) * WIDTH..(self.centre + self.reach + 1) * WIDTH,
            (self.neck - self.bulb) * HEIGHT..(self.neck + self.bulb + 1) * HEIGHT,
        )
    }

    /// The dots of the left wall and the caps, as far as the middle. Where
    /// two rows differ in width, the slant between them lies in the
    /// narrower row, whose outer cells are free, so a wall never enters a
    /// cell of the inside. The neck is narrower than both its neighbours,
    /// so it takes both slants and pinches in.
    fn walls(&self) -> Vec<(i32, i32)> {
        let edge = |half: i32| (self.centre - half - 1) * WIDTH + 1;
        let mut dots = Vec::new();
        let mut slant = |from: i32, to: i32, y: i32| {
            let xs: Vec<i32> = if from <= to {
                (from..=to).collect()
            } else {
                (to..=from).rev().collect()
            };
            let split = xs.len().div_ceil(2);
            for (step, x) in xs.into_iter().enumerate() {
                dots.push((x, if step < split { y } else { y + 1 }));
            }
        };
        for row in self.neck - self.bulb..=self.neck + self.bulb {
            let Some(half) = self.half_width(row) else {
                continue;
            };
            let y = row * HEIGHT;
            let above = self.half_width(row - 1).filter(|&wider| wider > half);
            let below = self.half_width(row + 1).filter(|&wider| wider > half);
            slant(edge(above.unwrap_or(half)), edge(half), y);
            slant(edge(half), edge(below.unwrap_or(half)), y + 2);
        }
        for (row, y) in [
            (self.neck - self.bulb, (self.neck - self.bulb) * HEIGHT - 1),
            (self.neck + self.bulb, (self.neck + self.bulb + 1) * HEIGHT),
        ] {
            let half = self.half_width(row).unwrap_or(0);
            dots.extend((edge(half)..=self.centre * WIDTH).map(|x| (x, y)));
        }
        dots
    }

    /// The dot at the same height on the other side of the middle.
    fn mirror(&self, x: i32) -> i32 {
        2 * self.centre * WIDTH + 1 - x
    }

    /// The first row of dots in the neck. A grain above it is in the top.
    fn gate(&self) -> i32 {
        self.neck * HEIGHT
    }

    /// The dots of the inside between two rows of cells, from `first` on
    /// toward `last`, and in each row from the middle out, the order in
    /// which sand poured in settles.
    fn room(&self, first: i32, last: i32) -> Vec<(i32, i32)> {
        let rows: Vec<i32> = if first <= last {
            (first * HEIGHT..(last + 1) * HEIGHT).collect()
        } else {
            (last * HEIGHT..(first + 1) * HEIGHT).rev().collect()
        };
        let middle = 2 * self.centre * WIDTH + 1;
        rows.into_iter()
            .flat_map(|y| {
                let mut row: Vec<(i32, i32)> = self
                    .span()
                    .0
                    .filter(|&x| self.holds(x, y))
                    .map(|x| (x, y))
                    .collect();
                row.sort_by_key(|&(x, _)| (2 * x - middle).abs());
                row
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Work,
    Break,
}

/// One phase of the cycle, placed in time.
#[derive(Debug, Clone, Copy)]
struct Phase {
    kind: Kind,
    start: Duration,
    length: Duration,
    /// The work rounds finished in the cycle.
    done: u32,
}

impl Phase {
    /// The phase running at `elapsed`. A cycle is a work round and a break
    /// `ROUNDS` times, the long break after the last, and then it starts
    /// again.
    fn at(elapsed: Duration) -> Phase {
        let phases: Vec<(Kind, Duration, u32)> = (1..=ROUNDS)
            .flat_map(|round| {
                let rest = if round == ROUNDS { LONG_BREAK } else { BREAK };
                [(Kind::Work, WORK, round - 1), (Kind::Break, rest, round)]
            })
            .collect();
        let cycle: Duration = phases.iter().map(|(_, length, _)| *length).sum();
        let cycles = u32::try_from(elapsed.as_nanos() / cycle.as_nanos()).unwrap_or(u32::MAX);
        let mut start = cycle.saturating_mul(cycles);
        for (kind, length, done) in phases {
            if elapsed < start + length {
                return Phase {
                    kind,
                    start,
                    length,
                    done,
                };
            }
            start += length;
        }
        Phase {
            kind: Kind::Work,
            start,
            length: WORK,
            done: 0,
        }
    }

    /// The share of the phase still to run at `elapsed`.
    fn left(&self, elapsed: Duration) -> f64 {
        let run = elapsed.saturating_sub(self.start).as_secs_f64() / self.length.as_secs_f64();
        1.0 - run.min(1.0)
    }
}

#[derive(Debug)]
pub struct Hourglass {
    palette: &'static Palette,
    rng: Rng,
    clock: Clock,
    width: u16,
    height: u16,
    glass: Option<Glass>,
    /// The phase the sand shows. The glass turns when the clock moves on
    /// to the next.
    phase: Phase,
    /// Whether each dot of the pane holds a grain, row by row.
    sand: Vec<bool>,
    total: usize,
    /// The grains above the neck.
    top: usize,
    /// The grains that may move on the next tick, by row and column. A
    /// grain at rest stays at rest until a dot below it empties, so the
    /// rest of the pile needs no look.
    moving: BTreeSet<(i32, i32)>,
}

impl Hourglass {
    pub fn new(rng: Rng, palette: &'static Palette, clock: Clock) -> Self {
        Self {
            palette,
            rng,
            clock,
            width: 0,
            height: 0,
            glass: None,
            phase: Phase::at(Duration::ZERO),
            sand: Vec::new(),
            total: 0,
            top: 0,
            moving: BTreeSet::new(),
        }
    }

    /// The grains the top holds at this time: all of them at the start of
    /// the phase and none at its end.
    fn due(&self) -> usize {
        (self.total as f64 * self.phase.left(self.clock.elapsed())).ceil() as usize
    }

    fn grain(&self, x: i32, y: i32) -> bool {
        self.place(x, y).is_some_and(|index| self.sand[index])
    }

    fn set(&mut self, x: i32, y: i32, grain: bool) {
        if let Some(index) = self.place(x, y) {
            self.sand[index] = grain;
        }
    }

    fn place(&self, x: i32, y: i32) -> Option<usize> {
        let across = i32::from(self.width) * WIDTH;
        let down = i32::from(self.height) * HEIGHT;
        ((0..across).contains(&x) && (0..down).contains(&y))
            .then(|| usize::try_from(y * across + x).ok())
            .flatten()
    }

    /// Lays the glass out for the pane and pours the sand in again, with
    /// the share in the top that the time left gives.
    fn lay_out(&mut self) {
        let dots = usize::from(self.width) * CELL_WIDTH * usize::from(self.height) * CELL_HEIGHT;
        self.sand = vec![false; dots];
        self.moving.clear();
        self.phase = Phase::at(self.clock.elapsed());
        self.glass = Glass::fit(self.width, self.height);
        let Some(glass) = self.glass else {
            self.total = 0;
            self.top = 0;
            return;
        };
        let top = glass.room(glass.neck - 1, glass.neck - glass.bulb);
        self.total = (top.len() as f32 * FILL) as usize;
        let due = self.due();
        let bottom = glass.room(glass.neck + glass.bulb, glass.neck + 1);
        for (x, y) in top
            .into_iter()
            .take(due)
            .chain(bottom.into_iter().take(self.total - due))
        {
            self.set(x, y, true);
            self.moving.insert((y, x));
        }
        self.top = due;
    }

    /// Turns the glass over: each grain moves to the same place on the
    /// other side of the neck, and then falls toward the neck.
    fn turn(&mut self, glass: Glass) {
        let across = usize::from(self.width) * CELL_WIDTH;
        let grains: Vec<(i32, i32)> = self
            .sand
            .iter()
            .enumerate()
            .filter(|(_, grain)| **grain)
            .filter_map(|(index, _)| {
                Some((
                    i32::try_from(index % across).ok()?,
                    i32::try_from(index / across).ok()?,
                ))
            })
            .collect();
        self.sand.fill(false);
        self.moving.clear();
        let gate = glass.gate();
        self.top = 0;
        for (x, y) in grains {
            let y = 2 * gate + HEIGHT - 1 - y;
            self.set(x, y, true);
            self.moving.insert((y, x));
            if y < gate {
                self.top += 1;
            }
        }
    }

    /// Lets grains into the neck until the top holds what is due, at most
    /// one for each dot across the neck in a tick. The grain that goes is
    /// the lowest in the top, nearest the middle.
    fn release(&mut self, glass: Glass) {
        let gate = glass.gate();
        let due = self.due();
        for entry in [glass.centre * WIDTH, glass.centre * WIDTH + 1] {
            if self.top <= due {
                break;
            }
            if self.grain(entry, gate) {
                continue;
            }
            let Some((x, y)) = self.lowest(glass) else {
                break;
            };
            self.set(x, y, false);
            self.wake_above(x, y);
            self.set(entry, gate, true);
            self.moving.insert((gate, entry));
            self.top -= 1;
        }
    }

    /// Marks the grains over a dot that just emptied as free to move.
    fn wake_above(&mut self, x: i32, y: i32) {
        for over in x - 1..=x + 1 {
            if self.grain(over, y - 1) {
                self.moving.insert((y - 1, over));
            }
        }
    }

    fn lowest(&self, glass: Glass) -> Option<(i32, i32)> {
        let middle = 2 * glass.centre * WIDTH + 1;
        (0..glass.gate()).rev().find_map(|y| {
            glass
                .span()
                .0
                .filter(|&x| self.grain(x, y))
                .min_by_key(|&x| (2 * x - middle).abs())
                .map(|x| (x, y))
        })
    }

    /// Moves each grain one dot down, or down to one side when the dot
    /// below is full, so the sand settles at its natural slope. A grain in
    /// the top cannot pass the gate by itself.
    fn fall(&mut self, glass: Glass) {
        let gate = glass.gate();
        for (y, x) in std::mem::take(&mut self.moving).into_iter().rev() {
            let below = y + 1;
            if !self.grain(x, y) || (y < gate && below >= gate) {
                continue;
            }
            let open = |s: &Self, to: i32| glass.holds(to, below) && !s.grain(to, below);
            let to = if open(self, x) {
                Some(x)
            } else {
                match (open(self, x - 1), open(self, x + 1)) {
                    (true, true) if self.rng.random_bool(0.5) => Some(x - 1),
                    (true, true) | (false, true) => Some(x + 1),
                    (true, false) => Some(x - 1),
                    (false, false) => None,
                }
            };
            if let Some(to) = to {
                self.set(x, y, false);
                self.set(to, below, true);
                self.moving.insert((below, to));
                self.wake_above(x, y);
            }
        }
    }
}

impl Scene for Hourglass {
    fn tick(&mut self, area: Rect) {
        if (area.width, area.height) != (self.width, self.height) {
            self.width = area.width;
            self.height = area.height;
            self.lay_out();
        }
        let Some(glass) = self.glass else {
            return;
        };
        let phase = Phase::at(self.clock.elapsed());
        if phase.start != self.phase.start {
            self.phase = phase;
            self.turn(glass);
        }
        self.release(glass);
        self.fall(glass);
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let Some(glass) = self.glass else {
            return;
        };
        let mut cells: BTreeMap<(i32, i32), (u32, Color)> = BTreeMap::new();
        let plot =
            |cells: &mut BTreeMap<(i32, i32), (u32, Color)>, x: i32, y: i32, colour: Color| {
                let entry = cells
                    .entry((x.div_euclid(WIDTH), y.div_euclid(HEIGHT)))
                    .or_insert((0, colour));
                entry.0 |= braille::dot(
                    usize::try_from(x.rem_euclid(WIDTH)).unwrap_or(0),
                    usize::try_from(y.rem_euclid(HEIGHT)).unwrap_or(0),
                );
            };
        for (x, y) in glass.walls() {
            plot(&mut cells, x, y, self.palette.muted);
            plot(&mut cells, glass.mirror(x), y, self.palette.muted);
        }
        let across = usize::from(self.width) * CELL_WIDTH;
        for (index, _) in self.sand.iter().enumerate().filter(|(_, grain)| **grain) {
            let (x, y) = (index % across, index / across);
            let (Ok(x), Ok(y)) = (i32::try_from(x), i32::try_from(y)) else {
                continue;
            };
            let colour = match self.phase.kind {
                Kind::Work => self.palette.gold,
                Kind::Break => self.palette.foam,
            };
            plot(&mut cells, x, y, colour);
        }
        let mut glyphs: Vec<((i32, i32), char, Color)> = cells
            .into_iter()
            .map(|(cell, (dots, colour))| (cell, braille::glyph(dots), colour))
            .collect();
        let (left, right) = (
            glass.centre - glass.reach - POST - 1,
            glass.centre + glass.reach + POST + 1,
        );
        let (top, bottom) = (glass.neck - glass.bulb - 1, glass.neck + glass.bulb + 1);
        for column in left..=right {
            glyphs.push(((column, top - 1), '▄', self.palette.rose));
            glyphs.push(((column, bottom + 1), '▀', self.palette.rose));
        }
        for row in top..=bottom {
            glyphs.push(((left, row), '┃', self.palette.rose));
            glyphs.push(((right, row), '┃', self.palette.rose));
        }
        for round in 0..ROUNDS {
            let row = glass.neck - 3 + 2 * i32::try_from(round).unwrap_or(0);
            let mark = if round < self.phase.done {
                '⣿'
            } else {
                '⠶'
            };
            glyphs.push(((right + MARKS, row), mark, self.palette.iris));
        }

        for ((column, row), glyph, colour) in glyphs {
            let (Ok(column), Ok(row)) = (u16::try_from(column), u16::try_from(row)) else {
                continue;
            };
            if column >= area.width || row >= area.height {
                continue;
            }
            buf[Position::new(area.x + column, area.y + row)]
                .set_char(glyph)
                .set_fg(colour);
        }
    }
}
