use std::collections::VecDeque;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::octant::{self, CELL_HEIGHT, CELL_WIDTH, Canvas};
use crate::scene::{Cells, Rng, Scene};
use crate::theme::Palette;

/// Generations a still life or a period-two oscillator may hold before the
/// board reseeds. About three seconds at the tick rate.
pub const HOLD_GENERATIONS: usize = 48;

/// One live cell in this many at a reseed.
const SEED_SPARSITY: u32 = 3;

/// The generations a cell takes to turn from the young colour at its birth
/// to the settled colour. A cell that lives past them rests in the settled
/// colour.
const SETTLE_GENERATIONS: u8 = 2;

/// The ticks a dead cell takes to fade to the base. A quarter of a second
/// at the tick rate, so what moves leaves a short trail and the board does
/// not smear.
const FADE_TICKS: u8 = 4;

/// The coverage of a cell's colour over the base in the tick it dies. A
/// trail fainter than a live cell does not read as one.
const TRAIL: f32 = 0.6;

#[derive(Debug)]
pub struct Life {
    rng: Rng,
    palette: &'static Palette,
    width: usize,
    height: usize,
    /// Generations alive, zero for a dead cell.
    age: Vec<u8>,
    /// Ticks since a cell died, zero for a live cell and for a cell whose
    /// fade has ended.
    dead_for: Vec<u8>,
    /// The age at which each fading cell died, which picks the colour it
    /// fades from.
    died_at: Vec<u8>,
    shades: Shades,
    cells: Cells,
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
            dead_for: Vec::new(),
            died_at: Vec::new(),
            shades: Shades::new(palette),
            cells: Cells::default(),
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
        life.resize(width, rows.len());
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

    /// The live cells, as (column, row) on the board.
    pub fn live_cells(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        (0..self.height)
            .flat_map(|y| (0..self.width).map(move |x| (x, y)))
            .filter(|(x, y)| self.age[y * self.width + x] > 0)
    }

    fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.age = vec![0; width * height];
        self.dead_for = vec![0; width * height];
        self.died_at = vec![0; width * height];
    }

    /// Starts the fade of each cell that was alive in `before` and is dead
    /// now, and moves each fade on by a tick.
    fn bury(&mut self, before: &[u8]) {
        let fades = self.dead_for.iter_mut().zip(&mut self.died_at);
        for ((&now, &was), (dead_for, died_at)) in self.age.iter().zip(before).zip(fades) {
            if now > 0 {
                *dead_for = 0;
            } else if was > 0 {
                (*dead_for, *died_at) = (1, was);
            } else if *dead_for > 0 {
                *dead_for = (*dead_for + 1) % (FADE_TICKS + 1);
            }
        }
    }

    /// Each cell is one pixel. A disc or a soft square would cover only
    /// part of its pixel and blur the board's exact shape into the base, so
    /// a cell fills its pixel, and the softness is in time: a cell shades
    /// by age while it lives and fades to the base once it dies.
    fn draw(&self, area: Rect, buf: &mut Buffer) {
        let mut canvas = Canvas::new(area.width, area.height, self.palette);
        for y in 0..self.height.min(canvas.height()) {
            for x in 0..self.width.min(canvas.width()) {
                let index = y * self.width + x;
                let colour = match (self.age[index], self.dead_for[index]) {
                    (0, 0) => continue,
                    (0, dead_for) => self.shades.fading(self.died_at[index], dead_for),
                    (age, _) => self.shades.live(age),
                };
                canvas.set(x, y, colour);
            }
        }
        canvas.render(area, buf);
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
            self.resize(width, height);
            self.reseed();
        }
        if width == 0 || height == 0 {
            return;
        }
        let before = self.age.clone();
        self.step();
        let dead = self.age.iter().all(|&age| age == 0);
        if dead || self.held >= HOLD_GENERATIONS {
            self.reseed();
        }
        self.bury(&before);
        let mut cells = std::mem::take(&mut self.cells);
        cells.draw(area, |buf| self.draw(area, buf));
        self.cells = cells;
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.cells.render(area, buf, |buf| self.draw(area, buf));
    }
}

/// The colour of a live cell by its age, and of a dead cell by the age it
/// died at and the ticks since. A frame holds thousands of cells and only
/// a few ages, so the scene mixes each colour once and not once for each
/// cell.
#[derive(Debug)]
struct Shades {
    live: [Color; SETTLE_GENERATIONS as usize + 1],
    fading: [[Color; FADE_TICKS as usize]; SETTLE_GENERATIONS as usize + 1],
}

impl Shades {
    fn new(palette: &Palette) -> Self {
        let settling = |since_birth: usize| {
            octant::mix(
                palette.rose,
                palette.iris,
                since_birth as f32 / f32::from(SETTLE_GENERATIONS),
            )
        };
        let over_base =
            |colour: Color, coverage: f32| octant::blend(palette, palette.base, colour, coverage);
        Self {
            live: std::array::from_fn(settling),
            fading: std::array::from_fn(|since_birth| {
                let colour = settling(since_birth);
                std::array::from_fn(|ticks| {
                    let left = f32::from(FADE_TICKS) - ticks as f32;
                    over_base(colour, TRAIL * left / f32::from(FADE_TICKS))
                })
            }),
        }
    }

    /// The colour of a cell `age` generations old.
    fn live(&self, age: u8) -> Color {
        self.live[usize::from(age.min(SETTLE_GENERATIONS + 1) - 1)]
    }

    /// The colour of a cell that died at `age`, `dead_for` ticks ago, from
    /// 1 to `FADE_TICKS`.
    fn fading(&self, age: u8, dead_for: u8) -> Color {
        let age = usize::from(age.clamp(1, SETTLE_GENERATIONS + 1) - 1);
        self.fading[age][usize::from(dead_for.clamp(1, FADE_TICKS) - 1)]
    }
}
