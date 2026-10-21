use std::collections::HashSet;
use std::ops::RangeInclusive;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

use crate::dither;
use crate::octant::{CELL_HEIGHT, CELL_WIDTH, Canvas};
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// The names a figure takes, one at random.
pub const NAMES: [&str; 12] = [
    "the kettle",
    "the lantern",
    "the heron",
    "the key",
    "the ladder",
    "the fox",
    "the boat",
    "the lamp",
    "the anchor",
    "the kite",
    "the bell",
    "the hearth",
];

/// A star in ASCII, so it is one cell wide on every terminal, from the
/// dimmest to the brightest.
const GLYPHS: [char; 3] = ['.', '+', '*'];
const STAR_CHANCE: f64 = 1.0 / 40.0;
/// How many stars of each brightness there are, from the dimmest, against
/// the others. Most stars are dim, as in a real sky.
const BRIGHTNESS_WEIGHTS: [u32; 3] = [6, 3, 1];
/// The chance that a star starts to twinkle on a tick, so each star
/// twinkles about once in 24 seconds.
const TWINKLE_CHANCE: f64 = 1.0 / 400.0;
/// How long a twinkle lasts, in ticks, about half a second.
const TWINKLE_TICKS: RangeInclusive<u32> = 4..=12;

/// Ticks before the first figure, about 2 to 6 seconds, so a new pane
/// shows one soon.
const FIRST_WAIT: RangeInclusive<u32> = 40..=100;
/// Ticks from the end of one figure to the next, about 15 to 30 seconds.
const WAIT: RangeInclusive<u32> = 250..=500;
const FIGURE_STARS: RangeInclusive<usize> = 4..=6;
/// The shortest and the longest line from one star of a figure to the
/// next, in pixels. A pixel is about as wide as it is high. Without the
/// shortest, the walk to the nearest star coils through a cluster and draws
/// a scribble, and without the longest a figure crosses the sky.
const HOP: RangeInclusive<f32> = 12.0..=32.0;
/// The starts a figure tries before the scene waits for the next one, in a
/// sky too sparse or too small for a figure and its name.
const TRIES: usize = 20;
/// Ticks for the line to draw itself, to hold, and to fade, about 2, 8 and
/// 2 seconds.
const DRAW_TICKS: u32 = 40;
const HOLD_TICKS: u32 = 133;
const FADE_TICKS: u32 = 40;
const FIGURE_TICKS: u32 = DRAW_TICKS + HOLD_TICKS + FADE_TICKS;

/// The longest trail of a shooting star behind its head, in pixels.
pub const TRAIL: usize = 10;
/// Ticks from one shooting star to the next, about 18 to 45 seconds. A
/// shooting star waits while a figure shows, and a figure waits while a
/// shooting star flies. A cell holds two colours and the sky takes one, so
/// where the two lines crossed one would draw in the colour of the other.
const METEOR_WAIT: RangeInclusive<u32> = 300..=750;
/// How far a shooting star flies before it burns out, in pixels.
const METEOR_RUN: RangeInclusive<f32> = 40.0..=80.0;
/// Pixels a tick, so a shooting star crosses in about a second.
const METEOR_SPEED: f32 = 4.0;
/// Its angle below level, in radians, about 20 to 57 degrees.
const METEOR_SLANT: RangeInclusive<f32> = 0.35..=1.0;

#[derive(Debug)]
struct Star {
    x: u16,
    y: u16,
    brightness: usize,
    /// The brightness the star shows while it twinkles, and the ticks the
    /// twinkle has left.
    twinkle: Option<(usize, u32)>,
}

impl Star {
    fn shown(&self) -> usize {
        self.twinkle
            .map_or(self.brightness, |(brightness, _)| brightness)
    }
}

#[derive(Debug)]
struct Figure {
    /// The cells of its stars, in the order the line joins them.
    stars: Vec<Position>,
    name: &'static str,
    name_at: Position,
    colour: Color,
    age: u32,
}

#[derive(Debug)]
struct Meteor {
    /// Where it starts, in pixels, and the pixel it moves each unit of its run.
    from: (f32, f32),
    direction: (f32, f32),
    run: f32,
    flown: f32,
}

impl Meteor {
    /// The pixels from the end of its trail to its head, on one line. Both
    /// ends lie on its run, which starts and ends inside the pane.
    fn pixels(&self) -> Vec<(i32, i32)> {
        let at = |along: f32| {
            (
                (self.from.0 + self.direction.0 * along).round() as i32,
                (self.from.1 + self.direction.1 * along).round() as i32,
            )
        };
        let head = self.flown.min(self.run);
        let tail = (self.flown - TRAIL as f32).max(0.0);
        line(at(tail), at(head))
    }

    fn burnt_out(&self) -> bool {
        self.flown - TRAIL as f32 > self.run
    }
}

#[derive(Debug)]
pub struct Constellations {
    rng: Rng,
    palette: &'static Palette,
    width: u16,
    height: u16,
    stars: Vec<Star>,
    figure: Option<Figure>,
    /// Ticks until the next figure joins.
    wait: u32,
    last_name: Option<&'static str>,
    meteor: Option<Meteor>,
    /// Ticks until the next shooting star.
    meteor_wait: u32,
}

impl Constellations {
    pub fn new(rng: Rng, palette: &'static Palette) -> Self {
        Self {
            rng,
            palette,
            width: 0,
            height: 0,
            stars: Vec::new(),
            figure: None,
            wait: 0,
            last_name: None,
            meteor: None,
            meteor_wait: 0,
        }
    }

    /// The name and the star cells of the figure on show, from the top left
    /// of the pane, so a test can hold a frame against it.
    pub fn figure(&self) -> Option<(&'static str, Vec<Position>)> {
        self.figure
            .as_ref()
            .map(|figure| (figure.name, figure.stars.clone()))
    }

    fn scatter(&mut self) {
        let total: u32 = BRIGHTNESS_WEIGHTS.iter().sum();
        self.stars.clear();
        for y in 0..self.height {
            for x in 0..self.width {
                if !self.rng.random_bool(STAR_CHANCE) {
                    continue;
                }
                let pick = self.rng.random_range(0..total);
                let brightness = (0..GLYPHS.len())
                    .find(|level| pick < BRIGHTNESS_WEIGHTS[..=*level].iter().sum())
                    .unwrap_or(0);
                self.stars.push(Star {
                    x,
                    y,
                    brightness,
                    twinkle: None,
                });
            }
        }
    }

    fn role(&self, brightness: usize) -> Color {
        match brightness {
            0 => self.palette.muted,
            1 => self.palette.subtle,
            _ => self.palette.text,
        }
    }

    /// A path from a random star through its nearest neighbours, and a
    /// place for its name, or none when no start gives both.
    fn join(&mut self) -> Option<Figure> {
        if self.stars.len() < *FIGURE_STARS.start() {
            return None;
        }
        for _ in 0..TRIES {
            let length = self.rng.random_range(FIGURE_STARS);
            let mut path = vec![self.rng.random_range(0..self.stars.len())];
            while path.len() < length {
                let last = &self.stars[path[path.len() - 1]];
                let next = (0..self.stars.len())
                    .filter(|index| !path.contains(index))
                    .map(|index| (index, hop(last, &self.stars[index])))
                    .filter(|(_, length)| HOP.contains(length))
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                match next {
                    Some((index, _)) => path.push(index),
                    None => break,
                }
            }
            if path.len() < *FIGURE_STARS.start() {
                continue;
            }
            let stars: Vec<Position> = path
                .iter()
                .map(|index| Position::new(self.stars[*index].x, self.stars[*index].y))
                .collect();
            let names: Vec<&'static str> = NAMES
                .into_iter()
                .filter(|name| Some(*name) != self.last_name)
                .collect();
            let name = names[self.rng.random_range(0..names.len())];
            let Some(name_at) = self.name_place(&stars, name) else {
                continue;
            };
            let colour = if self.rng.random_bool(0.5) {
                self.palette.iris
            } else {
                self.palette.foam
            };
            return Some(Figure {
                stars,
                name,
                name_at,
                colour,
                age: 0,
            });
        }
        None
    }

    /// The first row just below or above the figure, then inside it, where
    /// the name lies over the figure's columns, inside the pane, with no
    /// star under it or beside it.
    fn name_place(&self, figure: &[Position], name: &str) -> Option<Position> {
        let width = i32::try_from(name.chars().count()).ok()?;
        let top = i32::from(figure.iter().map(|star| star.y).min()?);
        let bottom = i32::from(figure.iter().map(|star| star.y).max()?);
        let left = i32::from(figure.iter().map(|star| star.x).min()?);
        let right = i32::from(figure.iter().map(|star| star.x).max()?);
        let taken: HashSet<(i32, i32)> = self
            .stars
            .iter()
            .map(|star| (i32::from(star.x), i32::from(star.y)))
            .collect();
        let rows = [bottom + 1, top - 1, bottom + 2, top - 2]
            .into_iter()
            .chain((top..=bottom).rev());
        let centred = (left + right + 1 - width) / 2;
        let starts: Vec<i32> = (0..=width + right - left)
            .flat_map(|shift| [centred - shift, centred + shift])
            .filter(|start| *start <= right && start + width > left)
            .collect();
        for row in rows {
            if row < 0 || row >= i32::from(self.height) {
                continue;
            }
            for start in &starts {
                if *start < 0 || start + width > i32::from(self.width) {
                    continue;
                }
                if (start - 1..=start + width).all(|x| !taken.contains(&(x, row))) {
                    return Some(Position::new(
                        u16::try_from(*start).ok()?,
                        u16::try_from(row).ok()?,
                    ));
                }
            }
        }
        None
    }

    fn meteor_tick(&mut self) {
        if let Some(meteor) = &mut self.meteor {
            meteor.flown += METEOR_SPEED;
            if meteor.burnt_out() {
                self.meteor = None;
                self.meteor_wait = self.rng.random_range(METEOR_WAIT);
            }
        } else if self.meteor_wait > 0 {
            self.meteor_wait -= 1;
        } else if self.figure.is_none() {
            self.meteor = self.launch();
            if self.meteor.is_none() {
                self.meteor_wait = self.rng.random_range(METEOR_WAIT);
            }
        }
    }

    /// A shooting star from a random pixel in the upper two thirds, falling
    /// to the left or the right, whose whole run lies inside the pane.
    fn launch(&mut self) -> Option<Meteor> {
        let width = f32::from(self.width) * CELL_WIDTH as f32;
        let height = f32::from(self.height) * CELL_HEIGHT as f32;
        if width < 1.0 || height < 1.0 {
            return None;
        }
        for _ in 0..TRIES {
            let from = (
                self.rng.random_range(0.0..width),
                self.rng.random_range(0.0..height * 2.0 / 3.0),
            );
            let slant = self.rng.random_range(METEOR_SLANT);
            let across = if self.rng.random_bool(0.5) { 1.0 } else { -1.0 };
            let direction = (across * slant.cos(), slant.sin());
            let run = self.rng.random_range(METEOR_RUN);
            let end = (from.0 + direction.0 * run, from.1 + direction.1 * run);
            if (0.0..width - 0.5).contains(&end.0) && (0.0..height - 0.5).contains(&end.1) {
                return Some(Meteor {
                    from,
                    direction,
                    run,
                    flown: 0.0,
                });
            }
        }
        None
    }

    fn figure_tick(&mut self) {
        if let Some(figure) = &mut self.figure {
            figure.age += 1;
            if figure.age >= FIGURE_TICKS {
                self.figure = None;
                self.wait = self.rng.random_range(WAIT);
            }
        } else if self.wait == 0 {
            if self.meteor.is_some() {
                return;
            }
            self.figure = self.join();
            match &self.figure {
                Some(figure) => self.last_name = Some(figure.name),
                None => self.wait = self.rng.random_range(WAIT),
            }
        } else {
            self.wait -= 1;
        }
    }
}

/// The length of a line between two stars, in pixels.
fn hop(from: &Star, to: &Star) -> f32 {
    let across = (f32::from(to.x) - f32::from(from.x)) * CELL_WIDTH as f32;
    let down = (f32::from(to.y) - f32::from(from.y)) * CELL_HEIGHT as f32;
    across.hypot(down)
}

/// The pixel in the middle of a star's cell.
fn centre(star: Position) -> (i32, i32) {
    (
        i32::from(star.x) * CELL_WIDTH as i32 + 1,
        i32::from(star.y) * CELL_HEIGHT as i32 + 2,
    )
}

/// The pixels of a line from one pixel to another, in order, by Bresenham.
fn line(from: (i32, i32), to: (i32, i32)) -> Vec<(i32, i32)> {
    let (across, down) = ((to.0 - from.0).abs(), -(to.1 - from.1).abs());
    let (step_x, step_y) = ((to.0 - from.0).signum(), (to.1 - from.1).signum());
    let (mut x, mut y, mut error) = (from.0, from.1, across + down);
    let mut pixels = vec![(x, y)];
    while (x, y) != to {
        let twice = 2 * error;
        if twice >= down {
            error += down;
            x += step_x;
        }
        if twice <= across {
            error += across;
            y += step_y;
        }
        pixels.push((x, y));
    }
    pixels
}

impl Figure {
    /// The pixels the line shows at its age: a growing share of the path while
    /// it draws, all of it while it holds, and a thinning dither while it
    /// fades.
    fn pixels(&self) -> Vec<(i32, i32)> {
        let segments: Vec<Vec<(i32, i32)>> = self
            .stars
            .windows(2)
            .map(|pair| line(centre(pair[0]), centre(pair[1])))
            .collect();
        let total: usize = segments.iter().map(Vec::len).sum();
        let drawn = if self.age < DRAW_TICKS {
            total * (self.age as usize + 1) / DRAW_TICKS as usize
        } else {
            total
        };
        let left = FIGURE_TICKS.saturating_sub(self.age) as usize;
        let sixteenths = if left < FADE_TICKS as usize {
            (left * dither::SHADES).div_ceil(FADE_TICKS as usize)
        } else {
            dither::SHADES
        };
        segments
            .into_iter()
            .flatten()
            .take(drawn)
            .filter(|(x, y)| {
                dither::lights(
                    sixteenths,
                    x.unsigned_abs() as usize,
                    y.unsigned_abs() as usize,
                )
            })
            .collect()
    }

    fn holding(&self) -> bool {
        (DRAW_TICKS..DRAW_TICKS + HOLD_TICKS).contains(&self.age)
    }
}

impl Constellations {
    /// Draws the pixels of the lines on a canvas over the cells they cross
    /// alone. The lines are a few short strokes, so a canvas over the whole
    /// pane would spend most of each frame on empty sky.
    fn draw_lines(&self, lines: &[((i32, i32), Color)], area: Rect, buf: &mut Buffer) {
        let (width, height) = (CELL_WIDTH as i32, CELL_HEIGHT as i32);
        let cells: Vec<(i32, i32)> = lines
            .iter()
            .map(|((x, y), _)| (x.div_euclid(width), y.div_euclid(height)))
            .filter(|(column, row)| {
                (0..i32::from(area.width)).contains(column)
                    && (0..i32::from(area.height)).contains(row)
            })
            .collect();
        let (Some(left), Some(right), Some(top), Some(bottom)) = (
            cells.iter().map(|cell| cell.0).min(),
            cells.iter().map(|cell| cell.0).max(),
            cells.iter().map(|cell| cell.1).min(),
            cells.iter().map(|cell| cell.1).max(),
        ) else {
            return;
        };
        let (Ok(columns), Ok(rows), Ok(column), Ok(row)) = (
            u16::try_from(right - left + 1),
            u16::try_from(bottom - top + 1),
            u16::try_from(left),
            u16::try_from(top),
        ) else {
            return;
        };
        let mut canvas = Canvas::new(columns, rows, self.palette.base);
        for &((x, y), colour) in lines {
            let (Ok(x), Ok(y)) = (
                usize::try_from(x - left * width),
                usize::try_from(y - top * height),
            ) else {
                continue;
            };
            canvas.set(x, y, colour);
        }
        canvas.render(Rect::new(area.x + column, area.y + row, columns, rows), buf);
    }
}

impl Scene for Constellations {
    fn tick(&mut self, area: Rect) {
        if (area.width, area.height) != (self.width, self.height) {
            self.width = area.width;
            self.height = area.height;
            self.scatter();
            self.figure = None;
            self.wait = self.rng.random_range(FIRST_WAIT);
            self.meteor = None;
            self.meteor_wait = self.rng.random_range(METEOR_WAIT);
        }
        for star in &mut self.stars {
            star.twinkle = match star.twinkle {
                Some((_, 0)) => None,
                Some((brightness, left)) => Some((brightness, left - 1)),
                None if self.rng.random_bool(TWINKLE_CHANCE) => {
                    let brightness = match star.brightness {
                        0 => 1,
                        1 if self.rng.random_bool(0.5) => 0,
                        1 => 2,
                        _ => 1,
                    };
                    Some((brightness, self.rng.random_range(TWINKLE_TICKS)))
                }
                None => None,
            };
        }
        self.figure_tick();
        self.meteor_tick();
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let inside = |x: i32, y: i32| {
            (0..i32::from(area.width)).contains(&x) && (0..i32::from(area.height)).contains(&y)
        };
        let at = |x: i32, y: i32| Position::new(area.x + x as u16, area.y + y as u16);

        let mut lines: Vec<((i32, i32), Color)> = Vec::new();
        if let Some(figure) = &self.figure {
            lines.extend(
                figure
                    .pixels()
                    .into_iter()
                    .map(|pixel| (pixel, figure.colour)),
            );
        }
        if let Some(meteor) = &self.meteor {
            lines.extend(
                meteor
                    .pixels()
                    .into_iter()
                    .map(|pixel| (pixel, self.palette.gold)),
            );
        }
        self.draw_lines(&lines, area, buf);

        for star in &self.stars {
            let (x, y) = (i32::from(star.x), i32::from(star.y));
            if !inside(x, y) {
                continue;
            }
            let brightness = star.shown();
            buf[at(x, y)]
                .set_char(GLYPHS[brightness])
                .set_fg(self.role(brightness));
        }

        if let Some(figure) = &self.figure {
            let colour = if figure.holding() {
                self.palette.subtle
            } else {
                self.palette.muted
            };
            let (x, y) = (i32::from(figure.name_at.x), i32::from(figure.name_at.y));
            for (offset, letter) in (0..).zip(figure.name.chars()) {
                if inside(x + offset, y) {
                    buf[at(x + offset, y)].set_char(letter).set_fg(colour);
                }
            }
        }
    }
}
