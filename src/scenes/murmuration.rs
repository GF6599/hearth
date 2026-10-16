use std::ops::RangeInclusive;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::scene::{Rng, Scene};
use crate::theme::{self, Palette};

/// The flock holds one bird for this many dots of the pane, within the
/// bounds of `FLOCK`.
pub const DOTS_PER_BIRD: usize = 80;
pub const FLOCK: RangeInclusive<usize> = 60..=500;

/// A braille cell is two dots wide and four tall.
const CELL_WIDTH: usize = 2;
const CELL_HEIGHT: usize = 4;
const BLANK_BRAILLE: u32 = 0x2800;
/// A cell with more dots than this draws in the bright role.
const FAINT_DOTS: u32 = 2;

/// Distances in dots, speeds in dots a tick.
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
/// The flock starts as a disc whose radius is this many dots times the
/// square root of its size, so a flock of any size starts as dense.
const SPAWN_SPREAD: f32 = 1.5;
/// Keeps a clamped bird strictly inside the last dot.
const INSIDE: f32 = 0.01;

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
const HALF_BLOCK: char = '▀';
/// The order in which the cells of a 4 by 4 tile change colour, so each
/// share of the tile is spread across it.
const BAYER: [[usize; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

#[derive(Debug, Clone, Copy)]
struct Bird {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

#[derive(Debug)]
pub struct Murmuration {
    rng: Rng,
    palette: &'static Palette,
    /// The pane in dots.
    width: usize,
    height: usize,
    /// The dots above the trees, where the flock flies.
    sky: usize,
    tree_rows: usize,
    /// The height of the trees in each cell column, in half-block pixels.
    trees: Vec<usize>,
    birds: Vec<Bird>,
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
        }
    }

    /// Every bird's place, in dots from the top left of the pane.
    pub fn birds(&self) -> impl Iterator<Item = (f32, f32)> + '_ {
        self.birds.iter().map(|bird| (bird.x, bird.y))
    }

    fn spawn(&mut self) {
        self.birds.clear();
        let dots = self.width * self.height;
        if dots == 0 || self.sky == 0 {
            return;
        }
        let count = (dots / DOTS_PER_BIRD).clamp(*FLOCK.start(), *FLOCK.end());
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

    fn step(&mut self) {
        let (columns, buckets) = self.buckets();
        let pushes: Vec<(f32, f32)> = self
            .birds
            .iter()
            .map(|bird| self.steer(bird, columns, &buckets))
            .collect();
        let (width, height) = (self.width as f32, self.sky as f32);
        for (bird, push) in self.birds.iter_mut().zip(pushes) {
            let (mut vx, mut vy) = (bird.vx + push.0, bird.vy + push.1);
            let speed = vx.hypot(vy);
            if speed > 0.0 {
                let kept = speed.clamp(MIN_SPEED, MAX_SPEED) / speed;
                (vx, vy) = (vx * kept, vy * kept);
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

/// The bit of a dot in a braille glyph. Dots 7 and 8, the bottom row, came
/// to the standard after the six dots of the two columns, so they take the
/// two high bits.
fn dot_bit(column: usize, row: usize) -> u32 {
    match (column, row) {
        (0, 3) => 1 << 6,
        (_, 3) => 1 << 7,
        (0, row) => 1 << row,
        (_, row) => 1 << (row + 3),
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
            if BAYER[row % 4][x % 4] < quarters * 4 {
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
        }
        if !self.birds.is_empty() {
            self.step();
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let columns = usize::from(area.width);
        let rows = usize::from(area.height);
        let sky_rows = rows.saturating_sub(self.tree_rows);
        let bands = self.sky_bands();
        let mut cells = vec![0; columns * sky_rows];
        for bird in &self.birds {
            let (x, y) = (bird.x as usize, bird.y as usize);
            let (column, row) = (x / CELL_WIDTH, y / CELL_HEIGHT);
            if column < columns && row < sky_rows {
                cells[row * columns + column] |= dot_bit(x % CELL_WIDTH, y % CELL_HEIGHT);
            }
        }
        for (index, &dots) in cells.iter().enumerate() {
            let (column, row) = (index % columns, index / columns);
            let cell = &mut buf[(area.x + column as u16, area.y + row as u16)];
            cell.set_bg(Self::sky_colour(bands, column, row, sky_rows));
            if dots == 0 {
                cell.set_char(' ');
                continue;
            }
            let glyph = char::from_u32(BLANK_BRAILLE + dots).unwrap_or(' ');
            let colour = if dots.count_ones() > FAINT_DOTS {
                self.palette.text
            } else {
                self.palette.subtle
            };
            cell.set_char(glyph).set_fg(colour);
        }

        let tallest = self.tree_rows * 2;
        for row in sky_rows..rows {
            for column in 0..columns {
                let height = self.trees.get(column).copied().unwrap_or(0);
                let pixel = |upper: usize| {
                    if tallest - upper <= height {
                        self.palette.muted
                    } else {
                        bands[2]
                    }
                };
                let upper = (row - sky_rows) * 2;
                buf[(area.x + column as u16, area.y + row as u16)]
                    .set_char(HALF_BLOCK)
                    .set_fg(pixel(upper))
                    .set_bg(pixel(upper + 1));
            }
        }
    }
}
