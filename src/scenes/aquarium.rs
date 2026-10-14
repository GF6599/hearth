use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::scene::{Rng, Scene};
use crate::theme;

pub const BUBBLE: &str = "o";
const WATERLINE: char = '~';

/// The row the waterline sits on. Water is every row below it.
const SURFACE: u16 = 0;

/// Each sprite faces right. The mirror for a fish heading left swaps the
/// glyphs that have a handed twin and reverses each row.
const SPRITES: [&[&str]; 3] = [
    &["><>"],
    &[" _", ">(o>"],
    &[" \\  /\\", "  >(o )>", " /  \\/"],
];

const FISH_COLOURS: [Color; 5] = [
    theme::LOVE,
    theme::GOLD,
    theme::ROSE,
    theme::IRIS,
    theme::FOAM,
];
const BUBBLE_COLOUR: Color = theme::FOAM;
const WEED_COLOUR: Color = theme::PINE;
const WATER_COLOUR: Color = theme::PINE;

const COLUMNS_PER_FISH: u16 = 20;
const MOST_FISH: usize = 8;
const SLOWEST: u16 = 3;

/// One tick in this many, a fish lets out a bubble.
const BUBBLE_ODDS: u32 = 30;

const COLUMNS_PER_WEED: u16 = 12;
const TALLEST_WEED: u16 = 4;

const SWAY_EVERY: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heading {
    Left,
    Right,
}

fn mirrored(glyph: char) -> char {
    match glyph {
        '<' => '>',
        '>' => '<',
        '(' => ')',
        ')' => '(',
        '/' => '\\',
        '\\' => '/',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        other => other,
    }
}

#[derive(Debug)]
struct Fish {
    rows: Vec<Vec<char>>,
    width: u16,
    heading: Heading,
    x: i32,
    y: u16,
    ticks_per_column: u16,
    ticks_since_move: u16,
    colour: Color,
}

impl Fish {
    fn new(sprite: &[&str], heading: Heading, x: i32, y: u16, colour: Color) -> Self {
        let width = sprite
            .iter()
            .map(|row| row.chars().count())
            .max()
            .unwrap_or(0);
        let rows = sprite
            .iter()
            .map(|row| {
                let mut cells: Vec<char> = row.chars().collect();
                cells.resize(width, ' ');
                if heading == Heading::Left {
                    cells.reverse();
                    for cell in &mut cells {
                        *cell = mirrored(*cell);
                    }
                }
                cells
            })
            .collect();
        Self {
            rows,
            width: u16::try_from(width).unwrap_or(u16::MAX),
            heading,
            x,
            y,
            ticks_per_column: 1,
            ticks_since_move: 0,
            colour,
        }
    }

    fn height(&self) -> u16 {
        u16::try_from(self.rows.len()).unwrap_or(u16::MAX)
    }

    /// The column of the fish's mouth, where a bubble starts.
    fn mouth(&self) -> i32 {
        match self.heading {
            Heading::Right => self.x + i32::from(self.width) - 1,
            Heading::Left => self.x,
        }
    }
}

#[derive(Debug)]
struct Bubble {
    x: u16,
    y: u16,
}

#[derive(Debug)]
struct Weed {
    x: u16,
    height: u16,
}

#[derive(Debug)]
pub struct Aquarium {
    rng: Rng,
    size: (u16, u16),
    fish: Vec<Fish>,
    bubbles: Vec<Bubble>,
    weeds: Vec<Weed>,
    stocked_by_hand: bool,
    ticks: u32,
}

impl Aquarium {
    pub fn new(rng: Rng) -> Self {
        Self {
            rng,
            size: (0, 0),
            fish: Vec::new(),
            bubbles: Vec::new(),
            weeds: Vec::new(),
            stocked_by_hand: false,
            ticks: 0,
        }
    }

    /// A tank with one fish of the given sprite and nothing else, which
    /// moves one column per tick. Random stocking stays off.
    pub fn with_fish(
        rng: Rng,
        area: Rect,
        sprite: &[&str],
        heading: Heading,
        x: i32,
        y: u16,
    ) -> Self {
        let mut tank = Self::new(rng);
        tank.size = (area.width, area.height);
        tank.stocked_by_hand = true;
        tank.release(Fish::new(sprite, heading, x, y, FISH_COLOURS[0]));
        tank
    }

    /// A tank with one bubble and nothing else. Random stocking stays off.
    pub fn with_bubble(rng: Rng, area: Rect, x: u16, y: u16) -> Self {
        let mut tank = Self::new(rng);
        tank.size = (area.width, area.height);
        tank.stocked_by_hand = true;
        tank.bubbles.push(Bubble { x, y });
        tank
    }

    fn water_rows(&self) -> u16 {
        self.size.1.saturating_sub(SURFACE + 1)
    }

    /// Lets a fish into the tank when the water is deep enough for it.
    fn release(&mut self, fish: Fish) {
        if fish.height() == 0 || fish.height() > self.water_rows() {
            return;
        }
        self.fish.push(fish);
    }

    fn random_depth(&mut self, height: u16) -> u16 {
        let deepest = self.size.1 - height;
        self.rng.random_range(SURFACE + 1..=deepest)
    }

    fn stock(&mut self) {
        let (width, _) = self.size;
        let wanted = usize::from(width / COLUMNS_PER_FISH).clamp(1, MOST_FISH);
        if self.fish.len() >= wanted {
            return;
        }
        let sprite = SPRITES[self.rng.random_range(0..SPRITES.len())];
        let heading = if self.rng.random_bool(0.5) {
            Heading::Right
        } else {
            Heading::Left
        };
        let colour = FISH_COLOURS[self.rng.random_range(0..FISH_COLOURS.len())];
        let mut fish = Fish::new(sprite, heading, 0, 0, colour);
        if fish.height() > self.water_rows() {
            return;
        }
        fish.x = match heading {
            Heading::Right => -i32::from(fish.width),
            Heading::Left => i32::from(width),
        };
        fish.y = self.random_depth(fish.height());
        fish.ticks_per_column = self.rng.random_range(1..=SLOWEST);
        self.release(fish);
    }

    fn plant_weeds(&mut self) {
        let (width, height) = self.size;
        self.weeds.clear();
        if height <= SURFACE + 1 {
            return;
        }
        let tallest = TALLEST_WEED.min(height - SURFACE - 1);
        for _ in 0..width / COLUMNS_PER_WEED {
            let weed = Weed {
                x: self.rng.random_range(0..width),
                height: self.rng.random_range(1..=tallest),
            };
            self.weeds.push(weed);
        }
    }

    fn swim(&mut self) {
        let (width, height) = self.size;
        let mut fish = std::mem::take(&mut self.fish);
        for one in &mut fish {
            one.ticks_since_move += 1;
            if one.ticks_since_move < one.ticks_per_column {
                continue;
            }
            one.ticks_since_move = 0;
            match one.heading {
                Heading::Right => {
                    one.x += 1;
                    if one.x >= i32::from(width) {
                        one.x = 1 - i32::from(one.width);
                        one.y = self.random_depth(one.height());
                    }
                }
                Heading::Left => {
                    one.x -= 1;
                    if one.x + i32::from(one.width) <= 0 {
                        one.x = i32::from(width) - 1;
                        one.y = self.random_depth(one.height());
                    }
                }
            }
            let mouth = one.mouth();
            if self.rng.random_range(0..BUBBLE_ODDS) == 0
                && let Ok(x) = u16::try_from(mouth)
                && x < width
                && one.y > SURFACE
                && one.y < height
            {
                self.bubbles.push(Bubble { x, y: one.y });
            }
        }
        self.fish = fish;
    }

    fn rise(&mut self) {
        for bubble in &mut self.bubbles {
            bubble.y = bubble.y.saturating_sub(1);
        }
        self.bubbles.retain(|bubble| bubble.y > SURFACE);
    }
}

impl Scene for Aquarium {
    fn tick(&mut self, area: Rect) {
        if (area.width, area.height) != self.size {
            self.size = (area.width, area.height);
            self.fish.clear();
            self.bubbles.clear();
            if !self.stocked_by_hand {
                self.plant_weeds();
            }
        }
        if area.width == 0 || area.height == 0 {
            return;
        }
        self.ticks = self.ticks.wrapping_add(1);
        if !self.stocked_by_hand {
            self.stock();
        }
        self.swim();
        self.rise();
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let (width, height) = (area.width.min(self.size.0), area.height.min(self.size.1));
        let inside =
            |x: i32, y: i32| x >= 0 && y >= 0 && x < i32::from(width) && y < i32::from(height);
        let mut put = |x: i32, y: i32, glyph: char, colour: Color| {
            if inside(x, y) {
                let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) else {
                    return;
                };
                buf[(area.x + x, area.y + y)].set_char(glyph).set_fg(colour);
            }
        };

        if SURFACE < height {
            for x in 0..width {
                put(i32::from(x), i32::from(SURFACE), WATERLINE, WATER_COLOUR);
            }
        }

        let sway = (self.ticks / SWAY_EVERY).is_multiple_of(2);
        for weed in &self.weeds {
            for row in 0..weed.height {
                let y = i32::from(height) - 1 - i32::from(row);
                let leans_left = row.is_multiple_of(2) == sway;
                let glyph = if leans_left { '(' } else { ')' };
                put(i32::from(weed.x), y, glyph, WEED_COLOUR);
            }
        }

        for fish in &self.fish {
            for (row, cells) in fish.rows.iter().enumerate() {
                let y = i32::from(fish.y) + i32::try_from(row).unwrap_or(0);
                for (column, &glyph) in cells.iter().enumerate() {
                    if glyph == ' ' {
                        continue;
                    }
                    let x = fish.x + i32::try_from(column).unwrap_or(0);
                    put(x, y, glyph, fish.colour);
                }
            }
        }

        for bubble in &self.bubbles {
            let glyph = BUBBLE.chars().next().unwrap_or('o');
            put(
                i32::from(bubble.x),
                i32::from(bubble.y),
                glyph,
                BUBBLE_COLOUR,
            );
        }
    }
}
