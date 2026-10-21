use std::ops::RangeInclusive;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::config::Background;
use crate::octant::{self, CELL_HEIGHT, CELL_WIDTH, Canvas};
use crate::scene::{Rng, Scene};
use crate::theme::{self, Palette};

/// The flock holds one bird for this many pixels of the pane, within the
/// bounds of `FLOCK`.
pub const PIXELS_PER_BIRD: usize = 80;
pub const FLOCK: RangeInclusive<usize> = 60..=500;

/// How strongly a bird covers its pixels, against the share of its square
/// on each, so a bird between four pixels still reads as a bird and not as
/// a smudge.
const BIRD_STRENGTH: f32 = 1.6;
/// The cover past which a pixel turns from subtle toward text, and the
/// cover more at which it is text, so a dense fold of the flock is
/// brighter than its edge.
const FAINT_COVER: f32 = 1.0;
const BRIGHT_COVER: f32 = 1.5;
/// The steps a cover rounds to. The canvas samples its own shapes in
/// sixteenths of a pixel, and a cell shows two colours, so a finer cover
/// shows no more and only adds colours for each cell to choose between.
const COVER_STEPS: f32 = 16.0;

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
/// The hawk's outline around its centre, in pixels: two wings swept up
/// from a body that hangs below them.
const HAWK_SHAPE: [(f32, f32); 6] = [
    (-3.5, -1.4),
    (0.0, -0.2),
    (3.5, -1.4),
    (3.5, -0.4),
    (0.0, 1.4),
    (-3.5, -0.4),
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
/// The points across each pixel column at which the skyline is read, so a
/// slope of the trees mixes into the sky across its pixels too.
const SKYLINE_SAMPLES: [f32; 2] = [-0.25, 0.25];

/// One crown of the trees: its first cell column, its width in cells, its
/// height in halves of a cell, and whether it is pointed.
#[derive(Debug, Clone, Copy)]
struct Crown {
    start: usize,
    width: usize,
    top: usize,
    pointed: bool,
}

impl Crown {
    /// Its height in halves of a cell, `across` cells from its left edge,
    /// or none past its edges.
    fn height(&self, across: f32) -> Option<f32> {
        if !(0.0..self.width as f32).contains(&across) {
            return None;
        }
        // Twice the distance from the middle, so the drop matches the
        // crown's cells at their middles.
        let off_middle = (2.0 * across - self.width as f32).abs();
        let drop = if self.pointed {
            off_middle
        } else {
            off_middle * off_middle / (2 * self.width) as f32
        };
        Some((self.top as f32 - drop).max(1.0))
    }
}

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
    /// The crowns of the trees, left to right.
    crowns: Vec<Crown>,
    /// The sky and the trees, which change only with the size of the pane,
    /// drawn once for each size. A frame copies it, so each mixes only the
    /// birds and the hawk.
    backdrop: Option<Canvas>,
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
            crowns: Vec::new(),
            backdrop: None,
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
    /// A line of crowns over a ground half a cell high: mostly domes, and
    /// now and then a narrow pointed one. No crown fills every tree row, so
    /// the sky shows between the tallest trees.
    fn grow_trees(&mut self, columns: usize) {
        self.crowns.clear();
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
            self.crowns.push(Crown {
                start: x,
                width,
                top,
                pointed,
            });
            x += width + self.rng.random_range(CROWN_GAP);
        }
    }

    /// The height of the trees at `x` pixels from the left, in pixels: the
    /// tallest crown there, or the ground.
    fn skyline(&self, x: f32) -> f32 {
        let across = x / CELL_WIDTH as f32;
        let halves = self
            .crowns
            .iter()
            .filter_map(|crown| crown.height(across - crown.start as f32))
            .fold(1.0, f32::max);
        halves * (CELL_HEIGHT / 2) as f32
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

    /// The sky of a pane `columns` by `rows` cells, lightening half a cell
    /// at a time from the darkest band at the top through the middle band
    /// to the lightest over the trees, with the trees in front. The three
    /// bands lie a few levels apart, so a finer step shows no more, and a
    /// cell of two halves keeps both colours exactly.
    fn backdrop(&self, columns: u16, rows: u16) -> Canvas {
        let mut canvas = Canvas::new(columns, rows, self.palette);
        let bands = self.sky_bands();
        let sky = usize::from(rows).saturating_sub(self.tree_rows) * CELL_HEIGHT;
        let bottom = usize::from(rows) * CELL_HEIGHT;
        let half = CELL_HEIGHT / 2;
        let halves = sky / half;
        // A gradient one pixel a half cell, so the canvas mixes the bands.
        let mut ruler = Canvas::new(1, rows, self.palette);
        ruler.gradient(0..halves / 2, bands[0], bands[1]);
        ruler.gradient(halves / 2..halves, bands[1], bands[2]);
        for step in 0..halves {
            let colour = ruler.get(0, step).unwrap_or(bands[2]);
            canvas.gradient(step * half..(step + 1) * half, colour, colour);
        }
        canvas.gradient(sky..bottom, bands[2], bands[2]);
        if self.tree_rows == 0 {
            return canvas;
        }
        // The centre of the pixel at row y lies at y, so the ground's lower
        // edge lies half a pixel below the last row.
        let ground = bottom as f32 - 0.5;
        for x in 0..canvas.width() {
            let tops = SKYLINE_SAMPLES.map(|offset| ground - self.skyline(x as f32 + offset));
            for y in sky..bottom {
                let coverage = tops
                    .iter()
                    .map(|top| (y as f32 + 0.5 - top).clamp(0.0, 1.0))
                    .sum::<f32>()
                    / tops.len() as f32;
                if coverage > 0.0 {
                    canvas.blend(x, y, self.palette.muted, coverage);
                }
            }
        }
        canvas
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
            self.backdrop = Some(self.backdrop(area.width, area.height));
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
        let fresh;
        let backdrop = match &self.backdrop {
            Some(backdrop)
                if (backdrop.width(), backdrop.height())
                    == (columns * CELL_WIDTH, rows * CELL_HEIGHT) =>
            {
                backdrop
            }
            _ => {
                fresh = self.backdrop(area.width, area.height);
                &fresh
            }
        };
        let mut canvas = backdrop.clone();
        let (width, sky) = (canvas.width(), sky_rows * CELL_HEIGHT);

        // How many birds cover each pixel of the sky. A bird is one pixel
        // across, shared between the four pixels nearest its middle by how
        // much of each it covers. A bird at (x, y) sits on the pixel whose
        // centre lies at the middle of its square, half a pixel on.
        let mut cover = vec![0.0_f32; width * sky];
        for (x, y) in self.birds_between() {
            let (x, y) = (x - 0.5, y - 0.5);
            let (left, top) = (x.floor(), y.floor());
            let (right, low) = (x - left, y - top);
            for (dx, dy, share) in [
                (0, 0, (1.0 - right) * (1.0 - low)),
                (1, 0, right * (1.0 - low)),
                (0, 1, (1.0 - right) * low),
                (1, 1, right * low),
            ] {
                let (column, row) = (left as isize + dx, top as isize + dy);
                if let (Ok(column), Ok(row)) = (usize::try_from(column), usize::try_from(row))
                    && column < width
                    && row < sky
                {
                    cover[row * width + column] += share * BIRD_STRENGTH;
                }
            }
        }
        let mut shades = [None; (COVER_STEPS * (FAINT_COVER + BRIGHT_COVER)) as usize + 1];
        let mut flown = vec![false; columns * sky_rows];
        for (index, &birds) in cover.iter().enumerate() {
            let step = (birds.min(FAINT_COVER + BRIGHT_COVER) * COVER_STEPS).round();
            if step > 0.0 {
                let (x, y) = (index % width, index / width);
                flown[y / CELL_HEIGHT * columns + x / CELL_WIDTH] = true;
                let birds = step / COVER_STEPS;
                let colour = *shades[step as usize].get_or_insert_with(|| {
                    octant::mix(
                        self.palette.subtle,
                        self.palette.text,
                        (birds - FAINT_COVER) / BRIGHT_COVER,
                    )
                });
                canvas.blend(x, y, colour, birds.min(1.0));
            }
        }
        // The flock keeps clear of the hawk, so a cell of the hawk holds the
        // sky and the hawk.
        if let Some((x, y)) = self.hawk_between() {
            let corners = HAWK_SHAPE.map(|(dx, dy)| (x + dx, y + dy));
            canvas.polygon(&corners, self.palette.love);
            for (x, y) in corners {
                let (column, row) = (x / CELL_WIDTH as f32, y / CELL_HEIGHT as f32);
                for row in row.floor() as isize - 1..=row.floor() as isize + 1 {
                    for column in column.floor() as isize - 1..=column.floor() as isize + 1 {
                        if let (Ok(column), Ok(row)) =
                            (usize::try_from(column), usize::try_from(row))
                            && column < columns
                            && row < sky_rows
                        {
                            flown[row * columns + column] = true;
                        }
                    }
                }
            }
        }
        canvas.render(area, buf);

        // A cell of the sky keeps the sky behind its birds, as the base
        // stays behind, so the sky reads as one background.
        for (index, _) in flown.iter().enumerate().filter(|(_, flown)| **flown) {
            let (column, row) = (index % columns, index / columns);
            let cell = &mut buf[(area.x + column as u16, area.y + row as u16)];
            let skies = (0..CELL_WIDTH * CELL_HEIGHT).filter_map(|pixel| {
                backdrop.get(
                    column * CELL_WIDTH + pixel % CELL_WIDTH,
                    row * CELL_HEIGHT + pixel / CELL_WIDTH,
                )
            });
            let (front, back) = skies.fold((false, false), |(front, back), sky| {
                (front || sky == cell.fg, back || sky == cell.bg)
            });
            if !front || back {
                continue;
            }
            if let Some(turned) = octant::complement(cell.symbol()) {
                let (bird, sky) = (cell.bg, cell.fg);
                cell.set_char(turned).set_fg(bird).set_bg(sky);
            }
        }
    }
}
