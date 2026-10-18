use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::symbols::border;

use crate::scene::{Rng, Scene};
use crate::theme::{self, Palette};

/// Drawn cells, as a share of the area, at which the screen clears.
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

/// The runes come from the same set as the frame, so the two match.
const RUNES: border::Set<'static> = theme::BORDER.to_border_set();

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
}

/// The rune for a cell entered moving `from` and left moving `to`.
fn rune(from: Direction, to: Direction) -> &'static str {
    use Direction::{Down, Left, Right, Up};
    match (from, to) {
        (Left | Right, Left | Right) => RUNES.horizontal_top,
        (Up | Down, Up | Down) => RUNES.vertical_left,
        (Right, Down) | (Up, Left) => RUNES.top_right,
        (Left, Down) | (Up, Right) => RUNES.top_left,
        (Right, Up) | (Down, Left) => RUNES.bottom_right,
        (Left, Up) | (Down, Right) => RUNES.bottom_left,
    }
}

#[derive(Debug, Clone, Copy)]
struct Pipe {
    x: u16,
    y: u16,
    heading: Direction,
    colour: Color,
}

#[derive(Debug)]
pub struct Pipes {
    rng: Rng,
    colours: Vec<Color>,
    size: (u16, u16),
    cells: Vec<Option<(&'static str, Color)>>,
    pipes: Vec<Pipe>,
    drawn: usize,
    due: u32,
}

impl Pipes {
    pub fn new(rng: Rng, palette: &Palette) -> Self {
        Self {
            rng,
            colours: colours(palette),
            size: (0, 0),
            cells: Vec::new(),
            pipes: Vec::new(),
            drawn: 0,
            due: 0,
        }
    }

    /// One pipe laid by hand: it starts at `start` having arrived moving
    /// `heading`, then takes each move in turn. The same step as a tick,
    /// with the turns given instead of drawn.
    pub fn trace(
        rng: Rng,
        palette: &Palette,
        area: Rect,
        start: (u16, u16),
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
        self.cells = vec![None; usize::from(width) * usize::from(height)];
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
        let (width, height) = self.size;
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
        if self.drawn * 100 >= self.cells.len() * FILL_PERCENT {
            self.clear();
        }
    }

    fn lay(&mut self, pipe: &mut Pipe, next: Direction) {
        let (width, height) = self.size;
        let index = usize::from(pipe.y) * usize::from(width) + usize::from(pipe.x);
        if let Some(cell) = self.cells.get_mut(index) {
            if cell.is_none() {
                self.drawn += 1;
            }
            *cell = Some((rune(pipe.heading, next), pipe.colour));
        }
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
        pipe.heading = next;
        if looped {
            pipe.colour = self.free_colour(Some(pipe.colour));
        }
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
        let (width, height) = self.size;
        for y in 0..area.height.min(height) {
            for x in 0..area.width.min(width) {
                let index = usize::from(y) * usize::from(width) + usize::from(x);
                if let Some((symbol, colour)) = self.cells[index] {
                    buf[(area.x + x, area.y + y)]
                        .set_symbol(symbol)
                        .set_fg(colour);
                }
            }
        }
    }
}
