use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::octant::{self, CELL_HEIGHT, CELL_WIDTH, Canvas};
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// Cells that a pipe reaches, its soft edges included, as a share of the
/// area, at which the screen clears.
pub const FILL_PERCENT: usize = 60;

/// One tick in this many, a pipe turns.
const TURN_ODDS: u32 = 10;

const COLUMNS_PER_PIPE: u16 = 40;
const MOST_PIPES: usize = 6;

/// Each pipe lays this many cells for each this many ticks.
const STEPS: (u32, u32) = (3, 2);

/// The width of a run down in pixels. The pixels along its path are
/// covered whole, and the pixels either side a quarter, so the run has
/// soft edges and still reads as a line one pixel wide. A cell is two
/// pixels wide, so the path and one edge share a cell and the other edge
/// lies in the next.
const WIDTH_DOWN: f32 = 1.5;

/// The width of a run across in pixels: its path alone. A cell is four
/// pixels tall, so a soft edge above and below a run across shares a cell
/// with the path and the base. Where the edges outnumber the base, the
/// cell draws the base in the colour of the edge, and the pipe smears into
/// a band four pixels tall. A line one pixel wide has round ends that
/// leave the corners of each end pixel out, and a quarter more covers them
/// and still covers no pixel beside the path.
const WIDTH_ACROSS: f32 = 1.25;

/// The highlight is a thin line beside the left edge of a run down. It
/// lies this far from the path and is this wide, so it covers a quarter of
/// the pixel beside the path and none of the path itself, which keeps the
/// pipe's own colour. A quarter keeps the pixel nearer the base than the
/// path, so a cell that must drop one of them drops the highlight.
const SHINE_OFFSET: f32 = 0.7;
const SHINE_WIDTH: f32 = 0.3;

/// How far the highlight stops short of each end of a run, so at a turn it
/// stays off the path of the run it meets.
const SHINE_TRIM: f32 = 0.75;

/// The share of the way from a pipe's colour to the light end of the
/// palette that its highlight takes.
const SHINE: f32 = 0.85;

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
    /// The first pixel of the straight run the pipe lays now.
    start: (usize, usize),
    /// The last pixel the run has lit, or none before its first.
    lit: Option<(usize, usize)>,
}

impl Pipe {
    fn new(x: usize, y: usize, heading: Direction, colour: Color) -> Self {
        Self {
            x,
            y,
            heading,
            colour,
            start: (x, y),
            lit: None,
        }
    }
}

/// Each pipe draws a straight run at a time.
#[derive(Debug)]
pub struct Pipes {
    rng: Rng,
    colours: Vec<Color>,
    palette: Palette,
    size: (u16, u16),
    /// The runs each pipe has finished. The run a pipe lays now draws over
    /// a copy each tick, so its soft edges blend in once, when it ends.
    canvas: Canvas,
    /// The finished runs with the run each pipe lays now on top.
    shown: Canvas,
    /// Whether a pipe reaches each cell, its soft edges included.
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
            shown: Canvas::new(0, 0, palette),
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
        let mut pipe = Pipe::new(start.0, start.1, heading, scene.colours[0]);
        for &next in moves {
            scene.lay(&mut pipe, next);
        }
        scene.pipes.push(pipe);
        scene.show();
        scene
    }

    fn resize(&mut self, area: Rect) {
        self.size = (area.width, area.height);
        self.clear();
    }

    fn clear(&mut self) {
        let (width, height) = self.size;
        self.canvas = Canvas::new(width, height, &self.palette);
        self.shown = self.canvas.clone();
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
        Pipe::new(x, y, heading, colour)
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
        if next != pipe.heading {
            self.finish(pipe, (pipe.x, pipe.y));
        }
        pipe.heading = next;
        for _ in 0..next.reach() {
            self.light(pipe.x, pipe.y);
            pipe.lit = Some((pipe.x, pipe.y));
            if self.advance(pipe, next)
                && let Some(lit) = pipe.lit
            {
                self.finish(pipe, lit);
                pipe.colour = self.free_colour(Some(pipe.colour));
            }
        }
    }

    /// Draws the run of `pipe` to `end` for good, and starts the next run
    /// at its head.
    fn finish(&mut self, pipe: &mut Pipe, end: (usize, usize)) {
        draw_run(&mut self.canvas, &self.palette, pipe, end);
        pipe.start = (pipe.x, pipe.y);
        pipe.lit = None;
    }

    /// Counts each cell that the pipe through the pixel (x, y) can reach. A
    /// run down reaches the pixels either side of its path and one past
    /// each end, so the cells of all eight pixels round (x, y) count. A run
    /// across reaches fewer, so the count can run ahead of the cells shown
    /// and the screen clears no later than the fill share.
    fn light(&mut self, x: usize, y: usize) {
        let (width, height) = (self.canvas.width(), self.canvas.height());
        let columns = usize::from(self.size.0);
        for ny in y.saturating_sub(1)..(y + 2).min(height) {
            for nx in x.saturating_sub(1)..(x + 2).min(width) {
                let cell = ny / CELL_HEIGHT * columns + nx / CELL_WIDTH;
                if let Some(crossed) = self.crossed.get_mut(cell)
                    && !*crossed
                {
                    *crossed = true;
                    self.drawn += 1;
                }
            }
        }
    }

    fn show(&mut self) {
        self.shown.clone_from(&self.canvas);
        for pipe in &self.pipes {
            if let Some(lit) = pipe.lit {
                draw_run(&mut self.shown, &self.palette, pipe, lit);
            }
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
        self.show();
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.shown.render(area, buf);
    }
}

/// Draws the run of `pipe` from its start to `end`, and on a run down its
/// highlight over the left edge. The line has round ends, so two runs that
/// meet at a turn join in a round corner.
fn draw_run(canvas: &mut Canvas, palette: &Palette, pipe: &Pipe, end: (usize, usize)) {
    let from = (pipe.start.0 as f32, pipe.start.1 as f32);
    let to = (end.0 as f32, end.1 as f32);
    if matches!(pipe.heading, Direction::Left | Direction::Right) {
        canvas.line(from, to, WIDTH_ACROSS, pipe.colour);
        return;
    }
    canvas.line(from, to, WIDTH_DOWN, pipe.colour);
    let (top, bottom) = (from.1.min(to.1), from.1.max(to.1));
    if bottom - top <= 2.0 * SHINE_TRIM {
        return;
    }
    let light = if palette.is_light() {
        palette.overlay
    } else {
        palette.text
    };
    let x = from.0 - SHINE_OFFSET;
    canvas.line(
        (x, top + SHINE_TRIM),
        (x, bottom - SHINE_TRIM),
        SHINE_WIDTH,
        octant::mix(pipe.colour, light, SHINE),
    );
}
