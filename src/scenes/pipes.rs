use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::octant::{CELL_HEIGHT, CELL_WIDTH, Canvas};
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// Cells that a pipe crosses, as a share of the area, at which the screen
/// clears.
pub const FILL_PERCENT: usize = 60;

/// One tick in this many, a pipe turns.
const TURN_ODDS: u32 = 10;

const COLUMNS_PER_PIPE: u16 = 40;
const MOST_PIPES: usize = 6;

/// Each pipe lays this many cells for each this many ticks.
const STEPS: (u32, u32) = (3, 2);

/// Every accent but the marker, because a turn in a pane corner in that
/// colour reads as the focus mark.
fn colours(palette: &Palette) -> Vec<Color> {
    [
        palette.love,
        palette.gold,
        palette.rose,
        palette.pine,
        palette.foam,
        palette.iris,
    ]
    .into_iter()
    .filter(|&colour| colour != palette.marker())
    .fold(Vec::new(), |mut distinct, colour| {
        if !distinct.contains(&colour) {
            distinct.push(colour);
        }
        distinct
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    fn turned(self, clockwise: bool) -> Self {
        match (self, clockwise) {
            (Direction::Up, true) | (Direction::Down, false) => Direction::Right,
            (Direction::Right, true) | (Direction::Left, false) => Direction::Down,
            (Direction::Down, true) | (Direction::Up, false) => Direction::Left,
            (Direction::Left, true) | (Direction::Right, false) => Direction::Up,
        }
    }

    /// The pixels in one step. A step spans one cell either way, so a pipe
    /// crosses as many cells a second across as down.
    fn reach(self) -> usize {
        match self {
            Direction::Up | Direction::Down => CELL_HEIGHT,
            Direction::Left | Direction::Right => CELL_WIDTH,
        }
    }
}

/// A pipe's head is the next pixel it lights, so the first pixel of a turn
/// is also the corner, and the line has no gap there.
#[derive(Debug, Clone, Copy)]
struct Pipe {
    x: usize,
    y: usize,
    heading: Direction,
    colour: Color,
}

/// Each pipe is a line one pixel wide. A pixel is about square, so the
/// line is as thick across as down. A line two pixels wide fills a whole
/// cell on a run down and reads as a bar, not a pipe.
#[derive(Debug)]
pub struct Pipes {
    rng: Rng,
    colours: Vec<Color>,
    palette: Palette,
    size: (u16, u16),
    canvas: Canvas,
    crossed: Vec<bool>,
    pipes: Vec<Pipe>,
    drawn: usize,
    due: u32,
}

impl Pipes {
    pub fn new(rng: Rng, palette: &Palette) -> Self {
        Self {
            rng,
            colours: colours(palette),
            palette: *palette,
            size: (0, 0),
            canvas: Canvas::new(0, 0, palette),
            crossed: Vec::new(),
            pipes: Vec::new(),
            drawn: 0,
            due: 0,
        }
    }

    /// One pipe laid by hand: its head starts at the pixel `start` having
    /// arrived moving `heading`, then takes each move in turn. The same
    /// step as a tick, with the turns given instead of drawn.
    pub fn trace(
        rng: Rng,
        palette: &Palette,
        area: Rect,
        start: (usize, usize),
        heading: Direction,
        moves: &[Direction],
    ) -> Self {
        let mut scene = Self::new(rng, palette);
        scene.resize(area);
        scene.pipes.clear();
        let mut pipe = Pipe {
            x: start.0,
            y: start.1,
            heading,
            colour: scene.colours[0],
        };
        for &next in moves {
            scene.lay(&mut pipe, next);
        }
        scene.pipes.push(pipe);
        scene
    }

    fn resize(&mut self, area: Rect) {
        self.size = (area.width, area.height);
        self.clear();
    }

    fn clear(&mut self) {
        let (width, height) = self.size;
        self.canvas = Canvas::new(width, height, &self.palette);
        self.crossed = vec![false; usize::from(width) * usize::from(height)];
        self.drawn = 0;
        self.pipes.clear();
        if width == 0 || height == 0 {
            return;
        }
        let count = usize::from(width / COLUMNS_PER_PIPE).clamp(1, MOST_PIPES);
        for _ in 0..count {
            let pipe = self.spawn();
            self.pipes.push(pipe);
        }
    }

    fn spawn(&mut self) -> Pipe {
        let (width, height) = (self.canvas.width(), self.canvas.height());
        let colour = self.free_colour(None);
        let (x, y, heading) = match self.rng.random_range(0..4) {
            0 => (self.rng.random_range(0..width), 0, Direction::Down),
            1 => (self.rng.random_range(0..width), height - 1, Direction::Up),
            2 => (0, self.rng.random_range(0..height), Direction::Right),
            _ => (width - 1, self.rng.random_range(0..height), Direction::Left),
        };
        Pipe {
            x,
            y,
            heading,
            colour,
        }
    }

    /// A colour that no live pipe shows, and not `old`, while the roles
    /// allow.
    fn free_colour(&mut self, old: Option<Color>) -> Color {
        let taken: Vec<Color> = self.pipes.iter().map(|pipe| pipe.colour).collect();
        let fresh = |colour: &&Color| Some(**colour) != old;
        let mut choices: Vec<Color> = self
            .colours
            .iter()
            .filter(fresh)
            .filter(|colour| !taken.contains(colour))
            .copied()
            .collect();
        if choices.is_empty() {
            choices = self.colours.iter().filter(fresh).copied().collect();
        }
        match choices.len() {
            0 => self.colours[0],
            count => choices[self.rng.random_range(0..count)],
        }
    }

    /// The live pipes stay in place while each lays, so a pipe that loops
    /// sees the colours of the others.
    fn step(&mut self) {
        for index in 0..self.pipes.len() {
            let mut pipe = self.pipes[index];
            let next = if self.rng.random_range(0..TURN_ODDS) == 0 {
                pipe.heading.turned(self.rng.random_bool(0.5))
            } else {
                pipe.heading
            };
            self.lay(&mut pipe, next);
            self.pipes[index] = pipe;
        }
        if self.drawn * 100 >= self.crossed.len() * FILL_PERCENT {
            self.clear();
        }
    }

    fn lay(&mut self, pipe: &mut Pipe, next: Direction) {
        for _ in 0..next.reach() {
            self.light(pipe.x, pipe.y, pipe.colour);
            if self.advance(pipe, next) {
                pipe.colour = self.free_colour(Some(pipe.colour));
            }
        }
        pipe.heading = next;
    }

    fn light(&mut self, x: usize, y: usize, colour: Color) {
        self.canvas.set(x, y, colour);
        let cell = y / CELL_HEIGHT * usize::from(self.size.0) + x / CELL_WIDTH;
        if let Some(crossed) = self.crossed.get_mut(cell)
            && !*crossed
        {
            *crossed = true;
            self.drawn += 1;
        }
    }

    /// Moves the head one pixel, wrapping at an edge, and tells whether it
    /// wrapped.
    fn advance(&self, pipe: &mut Pipe, next: Direction) -> bool {
        let (width, height) = (self.canvas.width(), self.canvas.height());
        let looped = match next {
            Direction::Up => pipe.y == 0,
            Direction::Down => pipe.y + 1 == height,
            Direction::Left => pipe.x == 0,
            Direction::Right => pipe.x + 1 == width,
        };
        match next {
            Direction::Up => pipe.y = pipe.y.checked_sub(1).unwrap_or(height - 1),
            Direction::Down => pipe.y = (pipe.y + 1) % height,
            Direction::Left => pipe.x = pipe.x.checked_sub(1).unwrap_or(width - 1),
            Direction::Right => pipe.x = (pipe.x + 1) % width,
        }
        looped
    }
}

impl Scene for Pipes {
    fn tick(&mut self, area: Rect) {
        if (area.width, area.height) != self.size {
            self.resize(area);
        }
        if area.width == 0 || area.height == 0 {
            return;
        }
        self.due += STEPS.0;
        while self.due >= STEPS.1 {
            self.due -= STEPS.1;
            self.step();
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.canvas.render(area, buf);
    }
}
