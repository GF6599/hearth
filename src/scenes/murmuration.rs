use std::ops::{Range, RangeInclusive};

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::config::Background;
use crate::dither;
use crate::octant::{self, CELL_HEIGHT, CELL_WIDTH, Canvas};
use crate::scene::{Rng, Scene};
use crate::theme::{self, Palette};

/// The flock holds one bird for this many pixels of the pane, within the
/// bounds of `FLOCK`.
pub const PIXELS_PER_BIRD: usize = 80;
pub const FLOCK: RangeInclusive<usize> = 60..=500;

/// A cell with more pixels than this draws in the bright role.
const FAINT_PIXELS: u32 = 2;

/// Distances in pixels, speeds in pixels a tick.
const VISION: f32 = 8.0;
const PERSONAL_SPACE: f32 = 2.0;
const MIN_SPEED: f32 = 0.6;
const MAX_SPEED: f32 = 1.4;
const ALIGNMENT: f32 = 0.06;
const COHESION: f32 = 0.004;
const SEPARATION: f32 = 0.08;
/// A bird this close to an edge turns back in, so the flock folds at the
/// edge instead of piling against it.
const EDGE_MARGIN: f32 = 10.0;
const EDGE_TURN: f32 = 0.12;
/// The flock starts as a disc whose radius is this many pixels times the
/// square root of its size, so a flock of any size starts as dense.
const SPAWN_SPREAD: f32 = 1.5;
/// Keeps a clamped bird strictly inside the last pixel.
const INSIDE: f32 = 0.01;

/// Ticks from one pass of the hawk to the next, about 30 to 60 seconds.
pub const HAWK_EVERY: RangeInclusive<u32> = 480..=960;
/// No bird comes this close to the hawk, in pixels.
pub const HAWK_CLEARANCE: f32 = 6.0;
/// Slower than a fleeing bird, so a bird that flees always gains on it.
const HAWK_SPEED: f32 = 1.2;
/// A bird this close to the hawk flees, and the hawk starts and ends its
/// pass this far outside the pane, so the birds at the edge see it come.
const FLEE_RADIUS: f32 = 12.0;
/// The hawk flies at this share of the sky's height, so a bird that flees
/// up or down never meets the top or the trees near it.
const HAWK_BAND: (f32, f32) = (0.25, 0.6);
/// The hawk's pixels around its centre: two wings swept up from a body.
const HAWK_SHAPE: [(isize, isize); 8] = [
    (-3, -1),
    (-2, -1),
    (-1, 0),
    (0, 0),
    (1, 0),
    (2, -1),
    (3, -1),
    (0, 1),
];

/// The trees take one cell row for each this many rows of the pane, and
/// no more than `MOST_TREE_ROWS`, so a short pane keeps its sky.
const PANE_ROWS_PER_TREE_ROW: usize = 8;
const MOST_TREE_ROWS: usize = 3;
/// Widths of a crown and gaps between two, in cells.
const DOME_WIDTH: RangeInclusive<usize> = 4..=7;
const POINTED_WIDTH: usize = 3;
/// One crown in this many is pointed.
const POINTED_ODDS: u32 = 3;
const CROWN_GAP: RangeInclusive<usize> = 0..=3;

#[derive(Debug, Clone, Copy)]
struct Bird {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

#[derive(Debug, Clone, Copy)]
struct Hawk {
    x: f32,
    y: f32,
    vx: f32,
}

/// The flock, the hawk and the random source as they stand on one tick.
#[derive(Debug, Clone)]
struct Flight {
    rng: Rng,
    birds: Vec<Bird>,
    hawk: Option<Hawk>,
    hawk_wait: u32,
}

#[derive(Debug)]
pub struct Murmuration {
    rng: Rng,
    palette: &'static Palette,
    /// The pane in pixels.
    width: usize,
    height: usize,
    /// The pixels above the trees, where the flock flies.
    sky: usize,
    tree_rows: usize,
    /// The height of the trees in each cell column, in halves of a cell.
    trees: Vec<usize>,
    birds: Vec<Bird>,
    hawk: Option<Hawk>,
    /// Ticks until the next hawk.
    hawk_wait: u32,
    /// The flight one tick ahead, so a frame between two ticks draws each
    /// bird and the hawk on the line between their two places. A bird
    /// steers by the whole flock, so its next place is known only once the
    /// next tick has run.
    next: Option<Flight>,
    /// The share of a tick since the last.
    progress: f64,
    background: Background,
}

impl Murmuration {
    pub fn new(rng: Rng, palette: &'static Palette) -> Self {
        Self {
            rng,
            palette,
            width: 0,
            height: 0,
            sky: 0,
            tree_rows: 0,
            trees: Vec::new(),
            birds: Vec::new(),
            hawk: None,
            hawk_wait: 0,
            next: None,
            progress: 0.0,
            background: Background::Theme,
        }
    }

    /// With the terminal's background the whole sky is the base, so it
    /// clears with the rest of the background and leaves no band.
    pub fn with_background(mut self, background: Background) -> Self {
        self.background = background;
        self
    }

    /// The centre of the hawk in pixels, while it crosses the sky.
    pub fn hawk(&self) -> Option<(f32, f32)> {
        self.hawk.map(|hawk| (hawk.x, hawk.y))
    }

    /// Every bird's place, in pixels from the top left of the pane.
    pub fn birds(&self) -> impl Iterator<Item = (f32, f32)> + '_ {
        self.birds.iter().map(|bird| (bird.x, bird.y))
    }

    /// Puts `flight` in place of the flight on show, and gives back the one
    /// it replaces.
    fn swap(&mut self, mut flight: Flight) -> Flight {
        std::mem::swap(&mut self.rng, &mut flight.rng);
        std::mem::swap(&mut self.birds, &mut flight.birds);
        std::mem::swap(&mut self.hawk, &mut flight.hawk);
        std::mem::swap(&mut self.hawk_wait, &mut flight.hawk_wait);
        flight
    }

    /// The place a share `progress` of the way from `from` on this tick to
    /// `to` on the next.
    fn between(&self, from: f32, to: f32) -> f32 {
        from + (to - from) * self.progress as f32
    }

    /// Each bird's place between this tick and the next. With no flight
    /// ahead, each bird keeps its place.
    fn birds_between(&self) -> impl Iterator<Item = (f32, f32)> + '_ {
        let next = self.next.as_ref().map_or(&self.birds, |next| &next.birds);
        self.birds
            .iter()
            .zip(next)
            .map(|(from, to)| (self.between(from.x, to.x), self.between(from.y, to.y)))
    }

    /// The hawk's place between this tick and the next. A hawk that leaves
    /// on the next tick is past the edge already, so it keeps its place.
    fn hawk_between(&self) -> Option<(f32, f32)> {
        let from = self.hawk?;
        let to = self.next.as_ref().and_then(|next| next.hawk);
        let x = to.map_or(from.x, |to| self.between(from.x, to.x));
        Some((x, from.y))
    }

    fn spawn(&mut self) {
        self.birds.clear();
        self.hawk = None;
        self.hawk_wait = self.rng.random_range(HAWK_EVERY);
        let pixels = self.width * self.height;
        if pixels == 0 || self.sky == 0 {
            return;
        }
        let count = (pixels / PIXELS_PER_BIRD).clamp(*FLOCK.start(), *FLOCK.end());
        let (width, height) = (self.width as f32, self.sky as f32);
        let centre = (width / 2.0, height / 2.0);
        let radius = (count as f32).sqrt() * SPAWN_SPREAD;
        let heading = self.rng.random_range(0.0..std::f32::consts::TAU);
        for _ in 0..count {
            let angle = self.rng.random_range(0.0..std::f32::consts::TAU);
            let reach = radius * self.rng.random_range(0.0f32..1.0).sqrt();
            let turn = heading + self.rng.random_range(-0.3..0.3);
            let speed = self.rng.random_range(MIN_SPEED..MAX_SPEED);
            self.birds.push(Bird {
                x: (centre.0 + reach * angle.cos()).clamp(0.0, width - INSIDE),
                y: (centre.1 + reach * angle.sin()).clamp(0.0, height - INSIDE),
                vx: speed * turn.cos(),
                vy: speed * turn.sin(),
            });
        }
    }

    /// Each bird in buckets one `VISION` square, so a bird reads only the
    /// nine buckets around it and not the whole flock.
    fn buckets(&self) -> (usize, Vec<Vec<usize>>) {
        let columns = (self.width as f32 / VISION).ceil() as usize;
        let rows = (self.sky as f32 / VISION).ceil() as usize;
        let mut buckets = vec![Vec::new(); columns * rows];
        for (index, bird) in self.birds.iter().enumerate() {
            let column = ((bird.x / VISION) as usize).min(columns - 1);
            let row = ((bird.y / VISION) as usize).min(rows - 1);
            buckets[row * columns + column].push(index);
        }
        (columns, buckets)
    }

    fn steer(&self, bird: &Bird, columns: usize, buckets: &[Vec<usize>]) -> (f32, f32) {
        let rows = buckets.len() / columns;
        let column = ((bird.x / VISION) as usize).min(columns - 1);
        let row = ((bird.y / VISION) as usize).min(rows - 1);
        let mut heading = (0.0, 0.0);
        let mut middle = (0.0, 0.0);
        let mut away = (0.0, 0.0);
        let mut seen = 0.0;
        for near_row in row.saturating_sub(1)..=(row + 1).min(rows - 1) {
            for near_column in column.saturating_sub(1)..=(column + 1).min(columns - 1) {
                for &other in &buckets[near_row * columns + near_column] {
                    let other = &self.birds[other];
                    let (dx, dy) = (other.x - bird.x, other.y - bird.y);
                    let distance = dx.hypot(dy);
                    if distance == 0.0 || distance > VISION {
                        continue;
                    }
                    heading = (heading.0 + other.vx, heading.1 + other.vy);
                    middle = (middle.0 + other.x, middle.1 + other.y);
                    seen += 1.0;
                    if distance < PERSONAL_SPACE {
                        away = (away.0 - dx / distance, away.1 - dy / distance);
                    }
                }
            }
        }

        let mut push = (away.0 * SEPARATION, away.1 * SEPARATION);
        if seen > 0.0 {
            push.0 +=
                (heading.0 / seen - bird.vx) * ALIGNMENT + (middle.0 / seen - bird.x) * COHESION;
            push.1 +=
                (heading.1 / seen - bird.vy) * ALIGNMENT + (middle.1 / seen - bird.y) * COHESION;
        }
        let (width, height) = (self.width as f32, self.sky as f32);
        if bird.x < EDGE_MARGIN {
            push.0 += EDGE_TURN;
        }
        if bird.x > width - EDGE_MARGIN {
            push.0 -= EDGE_TURN;
        }
        if bird.y < EDGE_MARGIN {
            push.1 += EDGE_TURN;
        }
        if bird.y > height - EDGE_MARGIN {
            push.1 -= EDGE_TURN;
        }
        push
    }

    /// The hawk waits, then crosses the sky once from a side picked at
    /// random, and is gone once it is as far past the other side.
    fn fly_hawk(&mut self) {
        let width = self.width as f32;
        match &mut self.hawk {
            Some(hawk) => {
                hawk.x += hawk.vx;
                if !(-FLEE_RADIUS..=width + FLEE_RADIUS).contains(&hawk.x) {
                    self.hawk = None;
                    self.hawk_wait = self.rng.random_range(HAWK_EVERY);
                }
            }
            None => {
                self.hawk_wait = self.hawk_wait.saturating_sub(1);
                if self.hawk_wait == 0 {
                    let from_left = self.rng.random_range(0..2) == 0;
                    let share = self.rng.random_range(HAWK_BAND.0..HAWK_BAND.1);
                    self.hawk = Some(Hawk {
                        x: if from_left {
                            -FLEE_RADIUS
                        } else {
                            width + FLEE_RADIUS
                        },
                        y: self.sky as f32 * share,
                        vx: if from_left { HAWK_SPEED } else { -HAWK_SPEED },
                    });
                }
            }
        }
    }

    fn step(&mut self) {
        self.fly_hawk();
        let (columns, buckets) = self.buckets();
        let pushes: Vec<(f32, f32)> = self
            .birds
            .iter()
            .map(|bird| self.steer(bird, columns, &buckets))
            .collect();
        let (width, height) = (self.width as f32, self.sky as f32);
        let hawk = self.hawk;
        for (bird, push) in self.birds.iter_mut().zip(pushes) {
            let (mut vx, mut vy) = (bird.vx + push.0, bird.vy + push.1);
            let speed = vx.hypot(vy);
            if speed > 0.0 {
                let kept = speed.clamp(MIN_SPEED, MAX_SPEED) / speed;
                (vx, vy) = (vx * kept, vy * kept);
            }
            // A bird near the hawk drops the flock and climbs or dives away
            // from the hawk's line at full speed, so the flock opens around
            // it. The hawk is slower, so the gap only grows.
            if let Some(hawk) = hawk
                && (bird.x - hawk.x).hypot(bird.y - hawk.y) < FLEE_RADIUS
            {
                let away = if bird.y < hawk.y { -1.0 } else { 1.0 };
                (vx, vy) = (0.0, away * MAX_SPEED);
            }
            bird.x += vx;
            bird.y += vy;
            if !(0.0..width).contains(&bird.x) {
                bird.x = bird.x.clamp(0.0, width - INSIDE);
                vx = -vx;
            }
            if !(0.0..height).contains(&bird.y) {
                bird.y = bird.y.clamp(0.0, height - INSIDE);
                vy = -vy;
            }
            (bird.vx, bird.vy) = (vx, vy);
        }
    }
}

impl Murmuration {
    /// A line of crowns over a ground one pixel high: mostly domes, and now
    /// and then a narrow pointed one. No crown fills every tree row, so the
    /// sky shows between the tallest trees.
    fn grow_trees(&mut self, columns: usize) {
        self.trees = vec![1; columns];
        let tallest = self.tree_rows * 2;
        if tallest < 3 {
            return;
        }
        let mut x = self.rng.random_range(CROWN_GAP);
        while x < columns {
            let pointed = self.rng.random_range(0..POINTED_ODDS) == 0;
            let width = if pointed {
                POINTED_WIDTH
            } else {
                self.rng.random_range(DOME_WIDTH)
            };
            let top = self.rng.random_range(2..tallest);
            for (offset, height) in self.trees.iter_mut().skip(x).take(width).enumerate() {
                // Twice the distance from the middle, so an even width has
                // a middle between two cells.
                let off_middle = (2 * offset).abs_diff(width - 1);
                let drop = if pointed {
                    off_middle
                } else {
                    off_middle * off_middle / (2 * width)
                };
                *height = (*height).max(top.saturating_sub(drop).max(1));
            }
            x += width + self.rng.random_range(CROWN_GAP);
        }
    }

    /// Base, surface and overlay from the darkest to the lightest, so the
    /// sky lightens low over the trees on a light theme too.
    fn sky_bands(&self) -> [Color; 3] {
        if self.background == Background::Terminal {
            return [self.palette.base; 3];
        }
        let mut bands = [
            self.palette.base,
            self.palette.surface,
            self.palette.overlay,
        ];
        bands.sort_by_key(|band| theme::luma(*band));
        bands
    }

    /// The sky in thirds of `bands` from the top. Two thirds blend over the
    /// rows either side of their edge, a quarter, a half and three quarters
    /// of the cells in the lower one, in an ordered dither that reads as a
    /// gradient and not as a pattern.
    fn sky_colour(bands: [Color; 3], x: usize, row: usize, sky_rows: usize) -> Color {
        let mut band = 0;
        for edge in 1..bands.len() {
            let start = edge * sky_rows / bands.len();
            let quarters = (row + 2).saturating_sub(start).min(4);
            if dither::lights(quarters * 4, x, row) {
                band = edge;
            }
        }
        bands[band]
    }
}

impl Scene for Murmuration {
    fn tick(&mut self, area: Rect) {
        let width = usize::from(area.width) * CELL_WIDTH;
        let height = usize::from(area.height) * CELL_HEIGHT;
        if (width, height) != (self.width, self.height) {
            self.width = width;
            self.height = height;
            let rows = usize::from(area.height);
            self.tree_rows = (rows / PANE_ROWS_PER_TREE_ROW).min(MOST_TREE_ROWS);
            self.sky = (rows - self.tree_rows) * CELL_HEIGHT;
            self.grow_trees(usize::from(area.width));
            self.spawn();
            self.next = None;
        }
        if self.birds.is_empty() {
            return;
        }
        // The flight ahead is what a step from here gives, so it stands in
        // for the step and the flock steers once a tick.
        match self.next.take() {
            Some(next) => drop(self.swap(next)),
            None => self.step(),
        }
        let now = Flight {
            rng: self.rng.clone(),
            birds: self.birds.clone(),
            hawk: self.hawk,
            hawk_wait: self.hawk_wait,
        };
        self.step();
        self.next = Some(self.swap(now));
    }

    fn set_progress(&mut self, progress: f64) {
        self.progress = progress;
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let columns = usize::from(area.width);
        let rows = usize::from(area.height);
        let sky_rows = rows.saturating_sub(self.tree_rows);
        let bands = self.sky_bands();
        let mut canvas = Canvas::new(area.width, area.height, self.palette);
        let fill = |canvas: &mut Canvas, column: usize, rows: Range<usize>, colour: Color| {
            for y in rows {
                for x in column * CELL_WIDTH..(column + 1) * CELL_WIDTH {
                    canvas.set(x, y, colour);
                }
            }
        };
        let skies: Vec<Color> = (0..columns * sky_rows)
            .map(|index| Self::sky_colour(bands, index % columns, index / columns, sky_rows))
            .collect();
        for (index, &sky) in skies.iter().enumerate() {
            let (column, row) = (index % columns, index / columns);
            fill(
                &mut canvas,
                column,
                row * CELL_HEIGHT..(row + 1) * CELL_HEIGHT,
                sky,
            );
        }

        // The pixels of the birds in each cell, so the birds of a cell take
        // one shade.
        let mut cells = vec![0_u8; columns * sky_rows];
        for (x, y) in self.birds_between() {
            let (x, y) = (x as usize, y as usize);
            let (column, row) = (x / CELL_WIDTH, y / CELL_HEIGHT);
            if column < columns && row < sky_rows {
                cells[row * columns + column] |=
                    1 << (y % CELL_HEIGHT * CELL_WIDTH + x % CELL_WIDTH);
            }
        }
        for (index, &birds) in cells.iter().enumerate() {
            let (column, row) = (index % columns, index / columns);
            let colour = if birds.count_ones() > FAINT_PIXELS {
                self.palette.text
            } else {
                self.palette.subtle
            };
            for pixel in (0..8).filter(|pixel| birds >> pixel & 1 == 1) {
                canvas.set(
                    column * CELL_WIDTH + pixel % CELL_WIDTH,
                    row * CELL_HEIGHT + pixel / CELL_WIDTH,
                    colour,
                );
            }
        }
        // The flock keeps clear of the hawk, so a cell of the hawk holds the
        // sky and the hawk.
        if let Some(hawk) = self.hawk_between() {
            let centre = (hawk.0.floor() as isize, hawk.1.floor() as isize);
            for (dx, dy) in HAWK_SHAPE {
                if let (Ok(x), Ok(y)) = (
                    usize::try_from(centre.0 + dx),
                    usize::try_from(centre.1 + dy),
                ) && y < sky_rows * CELL_HEIGHT
                {
                    canvas.set(x, y, self.palette.love);
                }
            }
        }

        let tallest = self.tree_rows * 2;
        for row in sky_rows..rows {
            for column in 0..columns {
                let height = self.trees.get(column).copied().unwrap_or(0);
                let upper = (row - sky_rows) * 2;
                for half in [upper, upper + 1] {
                    let colour = if tallest - half <= height {
                        self.palette.muted
                    } else {
                        bands[2]
                    };
                    let top = row * CELL_HEIGHT + (half - upper) * CELL_HEIGHT / 2;
                    fill(&mut canvas, column, top..top + CELL_HEIGHT / 2, colour);
                }
            }
        }
        canvas.render(area, buf);

        // A cell of the sky keeps the sky behind its birds, as the base
        // stays behind, so the bands of the sky read as one background.
        for (index, &sky) in skies.iter().enumerate() {
            let (column, row) = (index % columns, index / columns);
            let cell = &mut buf[(area.x + column as u16, area.y + row as u16)];
            if cell.fg != sky || cell.bg == sky {
                continue;
            }
            if let Some(turned) = octant::complement(cell.symbol()) {
                let bird = cell.bg;
                cell.set_char(turned).set_fg(bird).set_bg(sky);
            }
        }
    }
}
