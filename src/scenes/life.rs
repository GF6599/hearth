use std::collections::VecDeque;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::scene::{Rng, Scene};
use crate::theme;

pub const YOUNG: Color = theme::ROSE;
pub const OLD: Color = theme::IRIS;

/// Generations a still life or a period-two oscillator may hold before the
/// board reseeds. About three seconds at the tick rate.
pub const HOLD_GENERATIONS: usize = 48;

/// One live cell in this many at a reseed.
const SEED_SPARSITY: u32 = 3;

const HALF_BLOCK: char = '▀';

#[derive(Debug)]
pub struct Life {
    rng: Rng,
    width: usize,
    height: usize,
    /// Generations alive, zero for a dead cell.
    age: Vec<u8>,
    /// The two generations before this one, newest last.
    history: VecDeque<Vec<bool>>,
    held: usize,
}

impl Life {
    pub fn new(rng: Rng) -> Self {
        Self {
            rng,
            width: 0,
            height: 0,
            age: Vec::new(),
            history: VecDeque::with_capacity(2),
            held: 0,
        }
    }

    /// A board from rows of `#` for a live cell and any other character for
    /// a dead one. The board is as wide as the longest row.
    pub fn from_pattern(rng: Rng, rows: &[&str]) -> Self {
        let width = rows
            .iter()
            .map(|row| row.chars().count())
            .max()
            .unwrap_or(0);
        let mut life = Self::new(rng);
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
        let width = usize::from(area.width);
        let height = usize::from(area.height) * 2;
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

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let colour = |age: u8| match age {
            0 => theme::BASE,
            1 => YOUNG,
            _ => OLD,
        };
        for y in 0..area.height {
            let upper = usize::from(y) * 2;
            if upper + 1 >= self.height {
                break;
            }
            for x in 0..area.width {
                let column = usize::from(x);
                if column >= self.width {
                    break;
                }
                buf[(area.x + x, area.y + y)]
                    .set_char(HALF_BLOCK)
                    .set_fg(colour(self.age[upper * self.width + column]))
                    .set_bg(colour(self.age[(upper + 1) * self.width + column]));
            }
        }
    }
}
