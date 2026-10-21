use std::collections::VecDeque;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::octant::{CELL_HEIGHT, CELL_WIDTH, Canvas};
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// Generations a still life or a period-two oscillator may hold before the
/// board reseeds. About three seconds at the tick rate.
pub const HOLD_GENERATIONS: usize = 48;

/// One live cell in this many at a reseed.
const SEED_SPARSITY: u32 = 3;

#[derive(Debug)]
pub struct Life {
    rng: Rng,
    palette: &'static Palette,
    width: usize,
    height: usize,
    /// Generations alive, zero for a dead cell.
    age: Vec<u8>,
    /// The two generations before this one, newest last.
    history: VecDeque<Vec<bool>>,
    held: usize,
}

impl Life {
    pub fn new(rng: Rng, palette: &'static Palette) -> Self {
        Self {
            rng,
            palette,
            width: 0,
            height: 0,
            age: Vec::new(),
            history: VecDeque::with_capacity(2),
            held: 0,
        }
    }

    /// A board from rows of `#` for a live cell and any other character for
    /// a dead one. The board is as wide as the longest row.
    pub fn from_pattern(rng: Rng, palette: &'static Palette, rows: &[&str]) -> Self {
        let width = rows
            .iter()
            .map(|row| row.chars().count())
            .max()
            .unwrap_or(0);
        let mut life = Self::new(rng, palette);
        life.width = width;
        life.height = rows.len();
        life.age = vec![0; width * rows.len()];
        for (y, row) in rows.iter().enumerate() {
            for (x, cell) in row.chars().enumerate() {
                if cell == '#' {
                    life.age[y * width + x] = 2;
                }
            }
        }
        life.history.push_back(life.alive());
        life
    }

    fn alive(&self) -> Vec<bool> {
        self.age.iter().map(|&age| age > 0).collect()
    }

    fn reseed(&mut self) {
        for age in &mut self.age {
            *age = u8::from(self.rng.random_range(0..SEED_SPARSITY) == 0);
        }
        self.history.clear();
        self.history.push_back(self.alive());
        self.held = 0;
    }

    fn live_neighbours(&self, x: usize, y: usize) -> usize {
        let mut count = 0;
        for dy in [self.height - 1, 0, 1] {
            for dx in [self.width - 1, 0, 1] {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (x + dx) % self.width;
                let ny = (y + dy) % self.height;
                if self.age[ny * self.width + nx] > 0 {
                    count += 1;
                }
            }
        }
        count
    }

    fn step(&mut self) {
        let alive = self.alive();
        let mut next = self.age.clone();
        for y in 0..self.height {
            for x in 0..self.width {
                let index = y * self.width + x;
                let neighbours = self.live_neighbours(x, y);
                next[index] = match (alive[index], neighbours) {
                    (true, 2 | 3) => self.age[index].saturating_add(1),
                    (false, 3) => 1,
                    _ => 0,
                };
            }
        }
        self.age = next;

        let now = self.alive();
        if self.history.iter().any(|past| *past == now) {
            self.held += 1;
        } else {
            self.held = 0;
        }
        if self.history.len() == 2 {
            self.history.pop_front();
        }
        self.history.push_back(now);
    }
}

impl Scene for Life {
    fn tick(&mut self, area: Rect) {
        let width = usize::from(area.width) * CELL_WIDTH;
        let height = usize::from(area.height) * CELL_HEIGHT;
        if (width, height) != (self.width, self.height) {
            self.width = width;
            self.height = height;
            self.age = vec![0; width * height];
            self.reseed();
        }
        if width == 0 || height == 0 {
            return;
        }
        self.step();
        let dead = self.age.iter().all(|&age| age == 0);
        if dead || self.held >= HOLD_GENERATIONS {
            self.reseed();
        }
    }

    /// A cell holds two colours, and in nearly every cell the dead cells
    /// take one of them. So the live cells of a cell take one colour, the
    /// colour of most of them, and the cell keeps the shape of the board.
    fn render(&self, area: Rect, buf: &mut Buffer) {
        let mut canvas = Canvas::new(area.width, area.height, self.palette);
        for top in (0..self.height.min(canvas.height())).step_by(CELL_HEIGHT) {
            for left in (0..self.width.min(canvas.width())).step_by(CELL_WIDTH) {
                let age = |x: usize, y: usize| self.age[y * self.width + x];
                let live = || {
                    (top..top + CELL_HEIGHT)
                        .flat_map(|y| (left..left + CELL_WIDTH).map(move |x| (x, y)))
                        .filter(|(x, y)| *x < self.width && *y < self.height && age(*x, *y) > 0)
                };
                let (count, young) = live().fold((0, 0), |(count, young), (x, y)| {
                    (count + 1, young + usize::from(age(x, y) == 1))
                });
                let colour = if young * 2 > count {
                    self.palette.rose
                } else {
                    self.palette.iris
                };
                for (x, y) in live() {
                    canvas.set(x, y, colour);
                }
            }
        }
        canvas.render(area, buf);
    }
}
