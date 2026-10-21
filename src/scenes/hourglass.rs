use std::collections::BTreeSet;
use std::fmt;
use std::ops::{Range, RangeInclusive};
use std::time::Duration;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

use crate::clock::Clock;
use crate::octant::{CELL_HEIGHT, CELL_WIDTH, Canvas};
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// How long each phase of the cycle runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lengths {
    pub work: Duration,
    pub r#break: Duration,
    pub long_break: Duration,
}

impl Lengths {
    /// The lengths of the Pomodoro Technique.
    pub const DEFAULT: Lengths = Lengths {
        work: Duration::from_secs(25 * 60),
        r#break: Duration::from_secs(5 * 60),
        long_break: Duration::from_secs(15 * 60),
    };
    /// The range a length in the file can take, in whole minutes. Three
    /// hours is far past any useful round, so more is likely a slip.
    const MINUTES: RangeInclusive<u64> = 1..=180;

    /// A length in whole minutes, or the reason it is refused.
    pub fn minutes(value: i64) -> Result<Duration, String> {
        u64::try_from(value)
            .ok()
            .filter(|minutes| Self::MINUTES.contains(minutes))
            .map(|minutes| Duration::from_secs(minutes * 60))
            .ok_or_else(Self::range_message)
    }

    pub fn range_message() -> String {
        format!(
            "must be a whole number of minutes from {} to {}",
            Self::MINUTES.start(),
            Self::MINUTES.end()
        )
    }
}

impl Default for Lengths {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for Lengths {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let minutes = |length: Duration| length.as_secs() / 60;
        write!(
            f,
            "{}/{}/{} min",
            minutes(self.work),
            minutes(self.r#break),
            minutes(self.long_break)
        )
    }
}

/// Work rounds before the long break.
const ROUNDS: u32 = 4;
/// The share of a bulb that the sand fills. Under half, so the pile in the
/// bottom stays clear of the neck.
const FILL: f32 = 0.45;
/// Rows of cells kept free above and below the frame.
const MARGIN: i32 = 1;
/// Cells from the glass's widest wall out to a post of the frame.
const POST: i32 = 2;
/// Cells from the right post out to the round marks.
const MARKS: i32 = 2;
/// `r` builds the scene again on a new clock, so the cycle starts again at
/// the first work round.
const HINT: [(&str, &str); 2] = [("r", " start the cycle again"), ("+", " time left")];
const HINT_GAP: i32 = 3;
const WIDTH: i32 = CELL_WIDTH as i32;
const HEIGHT: i32 = CELL_HEIGHT as i32;

/// The glass in cells. Its walls and its inside are whole cells, so a wall
/// and the sand never share a cell. A cell holds two colours, and the
/// empty glass takes one wherever the sand does not fill the cell.
#[derive(Debug, Clone, Copy)]
struct Glass {
    /// The column of the neck, the middle of the glass.
    centre: i32,
    /// The row of the neck.
    neck: i32,
    bulb: i32,
    /// The half width of a bulb at its widest row. As many cells as the
    /// bulb has rows makes it round, because a cell holds two pixels across
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

    fn caps(&self) -> (i32, i32) {
        (
            (self.neck - self.bulb) * HEIGHT - 1,
            (self.neck + self.bulb + 1) * HEIGHT,
        )
    }

    /// The pixels of the inside on each side of the middle at a row of pixels
    /// between the caps: one in the neck, then out along the profile of a
    /// bulb. Each row takes its own width, so the outline curves at the
    /// size of a pixel and not of a cell.
    fn half_width(&self, y: i32) -> Option<i32> {
        let (top, bottom) = self.caps();
        if y <= top || y >= bottom {
            return None;
        }
        let gate = self.gate();
        let from_neck = if y < gate {
            gate - y
        } else {
            y - (gate + HEIGHT - 1)
        };
        if from_neck <= 0 {
            return Some(1);
        }
        let widest = self.reach * WIDTH + 1;
        let share = profile(from_neck as f32 / (self.bulb * HEIGHT) as f32);
        Some(1 + ((widest - 1) as f32 * share).round() as i32)
    }

    fn wall(&self, y: i32) -> Option<i32> {
        self.half_width(y).map(|half| self.centre * WIDTH - half)
    }

    /// The columns and rows of pixels that the inside can span.
    fn span(&self) -> (Range<i32>, Range<i32>) {
        let widest = self.reach * WIDTH + 1;
        let (top, bottom) = self.caps();
        (
            self.centre * WIDTH - widest + 1..self.centre * WIDTH + widest + 1,
            top + 1..bottom,
        )
    }

    /// The pixels of the left wall and the caps, as far as the middle. Where
    /// the wall moves more than a pixel from one row to the next, a run on
    /// the narrower row joins the two, outside its inside, so the outline
    /// has no break.
    fn walls(&self) -> Vec<(i32, i32)> {
        let (top, bottom) = self.caps();
        let mut pixels = Vec::new();
        for y in top + 1..bottom {
            let Some(x) = self.wall(y) else {
                continue;
            };
            pixels.push((x, y));
            if let Some(next) = self.wall(y + 1) {
                let (row, run) = if next > x {
                    (y + 1, x + 1..next)
                } else {
                    (y, next + 1..x)
                };
                pixels.extend(run.map(|x| (x, row)));
            }
        }
        for (cap, beside) in [(top, top + 1), (bottom, bottom - 1)] {
            let from = self.wall(beside).unwrap_or(self.centre * WIDTH - 1);
            pixels.extend((from..=self.centre * WIDTH).map(|x| (x, cap)));
        }
        pixels
    }

    /// The pixel at the same height on the other side of the middle.
    fn mirror(&self, x: i32) -> i32 {
        2 * self.centre * WIDTH + 1 - x
    }

    /// The first row of pixels in the neck. A grain above it is in the top.
    fn gate(&self) -> i32 {
        self.neck * HEIGHT
    }
}

/// The width of a bulb at a share of its height out from the neck, as a
/// share of its widest. It swells from the neck along a quarter sine to the
/// widest row a little past the middle, then rounds over along a quarter
/// ellipse to the cap, which stays wide enough to sit on the plate.
fn profile(along: f32) -> f32 {
    const WIDEST_AT: f32 = 0.55;
    const CAP: f32 = 0.62;
    if along <= WIDEST_AT {
        (std::f32::consts::FRAC_PI_2 * along / WIDEST_AT)
            .sin()
            .powf(0.7)
    } else {
        let over = (along - WIDEST_AT) / (1.0 - WIDEST_AT);
        CAP + (1.0 - CAP) * (1.0 - over * over).max(0.0).sqrt()
    }
}

#[derive(Debug, Clone, Copy)]
struct Colours {
    base: Color,
    frame: Color,
    glass: Color,
    work: Color,
    rest: Color,
    done: Color,
    to_do: Color,
    key: Color,
    words: Color,
}

impl Colours {
    fn of(palette: &Palette) -> Self {
        Self {
            base: palette.base,
            frame: palette.pine,
            glass: palette.subtle,
            work: palette.gold,
            rest: palette.foam,
            done: palette.iris,
            to_do: palette.muted,
            key: palette.foam,
            words: palette.subtle,
        }
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
    fn at(elapsed: Duration, lengths: Lengths) -> Phase {
        let phases: Vec<(Kind, Duration, u32)> = (1..=ROUNDS)
            .flat_map(|round| {
                let rest = if round == ROUNDS {
                    lengths.long_break
                } else {
                    lengths.r#break
                };
                [
                    (Kind::Work, lengths.work, round - 1),
                    (Kind::Break, rest, round),
                ]
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
            length: lengths.work,
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
    colours: Colours,
    rng: Rng,
    clock: Clock,
    lengths: Lengths,
    width: u16,
    height: u16,
    glass: Option<Glass>,
    /// The phase the sand shows. The glass turns when the clock moves on
    /// to the next.
    phase: Phase,
    /// Whether each pixel of the pane holds a grain, row by row.
    sand: Vec<bool>,
    /// Whether each pixel of the pane is inside the glass, row by row: inside
    /// the outline, in a cell that holds no pixel of a wall.
    inside: Vec<bool>,
    total: usize,
    /// The grains above the neck.
    top: usize,
    /// The grains that may move on the next tick, by row and column. A
    /// grain at rest stays at rest until a pixel below it empties, so the
    /// rest of the pile needs no look.
    moving: BTreeSet<(i32, i32)>,
}

impl Hourglass {
    pub fn new(rng: Rng, palette: &'static Palette, clock: Clock, lengths: Lengths) -> Self {
        Self {
            colours: Colours::of(palette),
            rng,
            clock,
            lengths,
            width: 0,
            height: 0,
            glass: None,
            phase: Phase::at(Duration::ZERO, lengths),
            sand: Vec::new(),
            inside: Vec::new(),
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

    fn holds(&self, x: i32, y: i32) -> bool {
        self.place(x, y).is_some_and(|index| self.inside[index])
    }

    /// The pixels of the inside between two rows of cells, from `first` on
    /// toward `last`, and in each row from the middle out, the order in
    /// which sand poured in settles.
    fn room(&self, glass: Glass, first: i32, last: i32) -> Vec<(i32, i32)> {
        let rows: Vec<i32> = if first <= last {
            (first * HEIGHT..(last + 1) * HEIGHT).collect()
        } else {
            (last * HEIGHT..(first + 1) * HEIGHT).rev().collect()
        };
        let middle = 2 * glass.centre * WIDTH + 1;
        rows.into_iter()
            .flat_map(|y| {
                let mut row: Vec<(i32, i32)> = glass
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

    fn fill_inside(&mut self, glass: Glass) {
        let cells: BTreeSet<(i32, i32)> = glass
            .walls()
            .into_iter()
            .flat_map(|(x, y)| [(x, y), (glass.mirror(x), y)])
            .map(|(x, y)| (x.div_euclid(WIDTH), y.div_euclid(HEIGHT)))
            .collect();
        self.inside = vec![false; self.sand.len()];
        let (columns, rows) = glass.span();
        for y in rows {
            let Some(half) = glass.half_width(y) else {
                continue;
            };
            for x in columns.clone() {
                let from_middle = (2 * x - (2 * glass.centre * WIDTH + 1)).abs();
                let free = !cells.contains(&(x.div_euclid(WIDTH), y.div_euclid(HEIGHT)));
                if from_middle < 2 * half
                    && free
                    && let Some(index) = self.place(x, y)
                {
                    self.inside[index] = true;
                }
            }
        }
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
        let pixels = usize::from(self.width) * CELL_WIDTH * usize::from(self.height) * CELL_HEIGHT;
        self.sand = vec![false; pixels];
        self.moving.clear();
        self.phase = Phase::at(self.clock.elapsed(), self.lengths);
        self.glass = Glass::fit(self.width, self.height);
        let Some(glass) = self.glass else {
            self.inside = Vec::new();
            self.total = 0;
            self.top = 0;
            return;
        };
        self.fill_inside(glass);
        let top = self.room(glass, glass.neck - 1, glass.neck - glass.bulb);
        self.total = (top.len() as f32 * FILL) as usize;
        let due = self.due();
        let bottom = self.room(glass, glass.neck + glass.bulb, glass.neck + 1);
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
    /// one for each pixel across the neck in a tick. The grain that goes is
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

    /// Marks the grains over a pixel that just emptied as free to move.
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

    /// Moves each grain one pixel down, or down to one side when the pixel
    /// below is full, so the sand settles at its natural slope. A grain in
    /// the top cannot pass the gate by itself.
    fn fall(&mut self, glass: Glass) {
        let gate = glass.gate();
        for (y, x) in std::mem::take(&mut self.moving).into_iter().rev() {
            let below = y + 1;
            if !self.grain(x, y) || (y < gate && below >= gate) {
                continue;
            }
            let open = |s: &Self, to: i32| s.holds(to, below) && !s.grain(to, below);
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

    fn hint(&self, glass: Glass, row: i32) -> Vec<((i32, i32), char, Color)> {
        let letters: usize = HINT
            .iter()
            .map(|(key, words)| key.chars().count() + words.chars().count())
            .sum();
        let (Ok(letters), Ok(gaps)) = (i32::try_from(letters), i32::try_from(HINT.len() - 1))
        else {
            return Vec::new();
        };
        let length = letters + gaps * HINT_GAP;
        if length + 2 > i32::from(self.width) {
            return Vec::new();
        }
        let mut column = glass.centre - length / 2;
        let mut glyphs = Vec::new();
        for (index, (key, words)) in HINT.iter().enumerate() {
            if index > 0 {
                column += HINT_GAP;
            }
            for (text, colour) in [(key, self.colours.key), (words, self.colours.words)] {
                for glyph in text.chars() {
                    glyphs.push(((column, row), glyph, colour));
                    column += 1;
                }
            }
        }
        glyphs
    }
}

/// Paints the pixel at (x, y), and drops one left of or above the pane.
fn plot(canvas: &mut Canvas, x: i32, y: i32, colour: Color) {
    if let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) {
        canvas.set(x, y, colour);
    }
}

/// Paints `rows` of the pixels of a cell. A plate is half a cell thick, on
/// the side of its cell that faces the glass. A finished round fills its
/// cell, and a round to come the middle two rows of its cell.
fn fill(canvas: &mut Canvas, (column, row): (i32, i32), rows: Range<i32>, colour: Color) {
    for y in rows {
        for x in 0..WIDTH {
            plot(canvas, column * WIDTH + x, row * HEIGHT + y, colour);
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
        let phase = Phase::at(self.clock.elapsed(), self.lengths);
        if phase.start != self.phase.start {
            self.phase = phase;
            self.turn(glass);
        }
        self.release(glass);
        self.fall(glass);
    }

    /// The phase, its round and the time left, rounded up to the second,
    /// such as "work 2/4, 18:32 left".
    fn time_left(&self) -> Option<String> {
        let elapsed = self.clock.elapsed();
        let phase = Phase::at(elapsed, self.lengths);
        let left = (phase.start + phase.length).saturating_sub(elapsed);
        let seconds = left.as_secs() + u64::from(left.subsec_nanos() > 0);
        let name = match phase.kind {
            Kind::Work => format!("work {}/{ROUNDS}", phase.done + 1),
            Kind::Break if phase.done == ROUNDS => "long break".to_string(),
            Kind::Break => format!("break {}/{ROUNDS}", phase.done),
        };
        Some(format!("{name}, {}:{:02} left", seconds / 60, seconds % 60))
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let Some(glass) = self.glass else {
            return;
        };
        let mut canvas = Canvas::new(area.width, area.height, self.colours.base);
        for (x, y) in glass.walls() {
            plot(&mut canvas, x, y, self.colours.glass);
            plot(&mut canvas, glass.mirror(x), y, self.colours.glass);
        }
        let across = usize::from(self.width) * CELL_WIDTH;
        let sand = match self.phase.kind {
            Kind::Work => self.colours.work,
            Kind::Break => self.colours.rest,
        };
        for (index, _) in self.sand.iter().enumerate().filter(|(_, grain)| **grain) {
            let (x, y) = (index % across, index / across);
            if let (Ok(x), Ok(y)) = (i32::try_from(x), i32::try_from(y)) {
                plot(&mut canvas, x, y, sand);
            }
        }
        let (left, right) = (
            glass.centre - glass.reach - POST - 1,
            glass.centre + glass.reach + POST + 1,
        );
        let (top, bottom) = (glass.neck - glass.bulb - 1, glass.neck + glass.bulb + 1);
        let frame = self.colours.frame;
        for column in left..=right {
            fill(&mut canvas, (column, top - 1), HEIGHT / 2..HEIGHT, frame);
            fill(&mut canvas, (column, bottom + 1), 0..HEIGHT / 2, frame);
        }
        for round in 0..ROUNDS {
            let row = glass.neck - 3 + 2 * i32::try_from(round).unwrap_or(0);
            let (rows, colour) = if round < self.phase.done {
                (0..HEIGHT, self.colours.done)
            } else {
                (HEIGHT / 4..HEIGHT * 3 / 4, self.colours.to_do)
            };
            fill(&mut canvas, (right + MARKS, row), rows, colour);
        }
        canvas.render(area, buf);

        let mut glyphs: Vec<((i32, i32), char, Color)> = Vec::new();
        for row in top..=bottom {
            glyphs.push(((left, row), '┃', self.colours.frame));
            glyphs.push(((right, row), '┃', self.colours.frame));
        }
        glyphs.extend(self.hint(glass, bottom + 2));

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
