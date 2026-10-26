use std::collections::HashSet;
use std::ops::RangeInclusive;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

use crate::octant::{self, CELL_HEIGHT, CELL_WIDTH, Canvas};
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
/// How long a twinkle lasts, in ticks, about half a second. A twinkle turns
/// a star's colour toward the next brightness and back, and keeps its
/// glyph.
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
/// The width of a figure's line and of a shooting star, in pixels.
const LINE_WIDTH: f32 = 1.0;

#[derive(Debug)]
struct Star {
    x: u16,
    y: u16,
    brightness: usize,
    /// The brightness the star turns toward while it twinkles, the ticks
    /// the twinkle has left, and the ticks it had at its start.
    twinkle: Option<(usize, u32, u32)>,
}

impl Star {
    /// The brightness the star shows, from 0 for the dimmest to 2 for the
    /// brightest, a share `progress` of a tick after its last tick. A
    /// twinkle turns it toward the brightness of the twinkle and back on
    /// half a wave, so it starts and ends at the star's own.
    fn shown(&self, progress: f64) -> f32 {
        let own = self.brightness as f32;
        let Some((toward, left, length)) = self.twinkle else {
            return own;
        };
        let along = ((length - left) as f32 + progress as f32) / (length + 1) as f32;
        own + (toward as f32 - own) * (std::f32::consts::PI * along).sin()
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
    /// Its stroke from the end of its trail to its head, a share `progress`
    /// of a tick after its last tick, or none once it burns out. Both ends
    /// lie on its run, which starts and ends inside the pane. The trail
    /// fades from the head back to the end of a full trail, so a shooting
    /// star that burns out fades from its head too.
    fn stroke(&self, progress: f64, colour: Color) -> Option<Stroke> {
        let flown = self.flown + METEOR_SPEED * progress as f32;
        if self.burnt_out(flown) {
            return None;
        }
        let at = |along: f32| {
            (
                self.from.0 + self.direction.0 * along,
                self.from.1 + self.direction.1 * along,
            )
        };
        let (head, tail) = (flown.min(self.run), (flown - TRAIL as f32).max(0.0));
        let length = head - tail;
        Some(Stroke {
            path: vec![at(tail), at(head)],
            colour,
            strength: Box::new(move |along| {
                (1.0 - (flown - tail - along * length) / TRAIL as f32).clamp(0.0, 1.0)
            }),
        })
    }

    /// Whether the end of its trail has passed the end of its run once it
    /// has flown `flown` pixels.
    fn burnt_out(&self, flown: f32) -> bool {
        flown - TRAIL as f32 > self.run
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
    /// The share of a tick since the last, so a shooting star flies and a
    /// line draws itself between two ticks.
    progress: f64,
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
            progress: 0.0,
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
            if meteor.burnt_out(meteor.flown) {
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

/// The pixel nearest the middle of a star's cell. The middle lies between
/// pixels, and a level line through it would light two rows at half
/// strength.
fn centre(star: Position) -> (f32, f32) {
    (
        f32::from(star.x) * CELL_WIDTH as f32 + 1.0,
        f32::from(star.y) * CELL_HEIGHT as f32 + 2.0,
    )
}

/// A line along `path` in `colour`, mixed into the sky by its coverage
/// times its strength. The strength takes the share of the path's length
/// from its start, from 0 to 1.
struct Stroke {
    path: Vec<(f32, f32)>,
    colour: Color,
    strength: Box<dyn Fn(f32) -> f32>,
}

impl std::fmt::Debug for Stroke {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Stroke")
            .field("path", &self.path)
            .field("colour", &self.colour)
            .finish_non_exhaustive()
    }
}

impl Stroke {
    /// The pixels the stroke can touch: left, top, right and bottom.
    fn bounds(&self) -> [f32; 4] {
        let reach = LINE_WIDTH / 2.0 + 1.0;
        self.path.iter().fold(
            [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
            |[left, top, right, bottom], &(x, y)| {
                [
                    left.min(x - reach),
                    top.min(y - reach),
                    right.max(x + reach),
                    bottom.max(y + reach),
                ]
            },
        )
    }

    /// The share of the pixel at `point` the stroke mixes in. Where two
    /// legs of the path meet, the nearer leg counts once, so a joint is no
    /// brighter than the legs.
    fn share(&self, point: (f32, f32)) -> f32 {
        let lengths: Vec<f32> = self
            .path
            .windows(2)
            .map(|leg| (leg[1].0 - leg[0].0).hypot(leg[1].1 - leg[0].1))
            .collect();
        let total: f32 = lengths.iter().sum();
        let mut start = 0.0;
        let mut nearest = (f32::MAX, 0.0);
        for (leg, length) in self.path.windows(2).zip(&lengths) {
            let (along, off) = octant::nearest(point, leg[0], leg[1]);
            if off < nearest.0 {
                nearest = (off, (start + along * length) / total.max(f32::MIN_POSITIVE));
            }
            start += length;
        }
        let coverage = (LINE_WIDTH / 2.0 + 0.5 - nearest.0).clamp(0.0, 1.0);
        let coverage = octant::stepped(coverage, octant::COVERAGE_STEPS);
        coverage * (self.strength)(nearest.1)
    }
}

impl Figure {
    /// The line at its age, a share `progress` of a tick after its last
    /// tick: from the first star to a pen that moves along the path while
    /// it draws, all of it while it holds, and all of it at a falling
    /// strength while it fades.
    fn stroke(&self, progress: f64) -> Stroke {
        let stars: Vec<(f32, f32)> = self.stars.iter().copied().map(centre).collect();
        let total: f32 = stars
            .windows(2)
            .map(|leg| (leg[1].0 - leg[0].0).hypot(leg[1].1 - leg[0].1))
            .sum();
        let ticks = f64::from(self.age) + 1.0 + progress;
        let drawn = total * (ticks / f64::from(DRAW_TICKS)).min(1.0) as f32;
        let mut path = vec![stars[0]];
        let mut left = drawn;
        for leg in stars.windows(2) {
            let length = (leg[1].0 - leg[0].0).hypot(leg[1].1 - leg[0].1);
            if left >= length {
                path.push(leg[1]);
                left -= length;
            } else {
                let share = left / length;
                path.push((
                    leg[0].0 + (leg[1].0 - leg[0].0) * share,
                    leg[0].1 + (leg[1].1 - leg[0].1) * share,
                ));
                break;
            }
        }
        let strength = self.strength(progress);
        Stroke {
            path,
            colour: self.colour,
            strength: Box::new(move |_| strength),
        }
    }

    /// How strongly the figure shows, from 1 while it draws and holds down
    /// to 0 at the end of its fade.
    fn strength(&self, progress: f64) -> f32 {
        let left = f64::from(FIGURE_TICKS.saturating_sub(self.age)) - progress;
        (left / f64::from(FADE_TICKS)).clamp(0.0, 1.0) as f32
    }

    /// How far its name has turned from muted to subtle: up while the line
    /// draws, in full while it holds, and down while it fades.
    fn name_share(&self, progress: f64) -> f32 {
        let drawn = (f64::from(self.age) + progress) / f64::from(DRAW_TICKS);
        (drawn.min(1.0) as f32).min(self.strength(progress))
    }
}

impl Constellations {
    /// Draws the strokes on a canvas over the cells they cross alone. The
    /// lines are a few short strokes, so a canvas over the whole pane would
    /// spend most of each frame on empty sky.
    fn draw_strokes(&self, strokes: &[Stroke], area: Rect, buf: &mut Buffer) {
        let Some([left, top, right, bottom]) = strokes.iter().map(Stroke::bounds).reduce(
            |[left, top, right, bottom], [l, t, r, b]| {
                [left.min(l), top.min(t), right.max(r), bottom.max(b)]
            },
        ) else {
            return;
        };
        let (width, height) = (CELL_WIDTH as f32, CELL_HEIGHT as f32);
        let column = (left / width).floor().max(0.0) as u16;
        let row = (top / height).floor().max(0.0) as u16;
        let columns = ((right / width).floor() as i32 + 1).min(i32::from(area.width));
        let rows = ((bottom / height).floor() as i32 + 1).min(i32::from(area.height));
        let (Ok(columns), Ok(rows)) = (
            u16::try_from(columns - i32::from(column)),
            u16::try_from(rows - i32::from(row)),
        ) else {
            return;
        };
        if columns == 0 || rows == 0 {
            return;
        }
        let mut canvas = Canvas::new(columns, rows, self.palette);
        let (left, top) = (
            usize::from(column) * CELL_WIDTH,
            usize::from(row) * CELL_HEIGHT,
        );
        for stroke in strokes {
            let [from_x, from_y, to_x, to_y] = stroke.bounds();
            let xs = (from_x.floor().max(left as f32) as usize)..=(to_x.ceil().max(0.0) as usize);
            for y in (from_y.floor().max(top as f32) as usize)..=(to_y.ceil().max(0.0) as usize) {
                for x in xs.clone() {
                    let share = stroke.share((x as f32, y as f32));
                    if share > 0.0 {
                        canvas.blend(x - left, y - top, stroke.colour, share);
                    }
                }
            }
        }
        canvas.render(Rect::new(area.x + column, area.y + row, columns, rows), buf);
    }

    /// The colour of a star of `brightness` from 0 for the dimmest to 2 for
    /// the brightest, along the roles of the three.
    fn shade(&self, brightness: f32) -> Color {
        let roles = [self.palette.muted, self.palette.subtle, self.palette.text];
        octant::ramp(&roles, brightness / (roles.len() - 1) as f32)
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
                Some((_, 0, _)) => None,
                Some((brightness, left, length)) => Some((brightness, left - 1, length)),
                None if self.rng.random_bool(TWINKLE_CHANCE) => {
                    let brightness = match star.brightness {
                        0 => 1,
                        1 if self.rng.random_bool(0.5) => 0,
                        1 => 2,
                        _ => 1,
                    };
                    let length = self.rng.random_range(TWINKLE_TICKS);
                    Some((brightness, length, length))
                }
                None => None,
            };
        }
        self.figure_tick();
        self.meteor_tick();
    }

    fn set_progress(&mut self, progress: f64) -> bool {
        std::mem::replace(&mut self.progress, progress) != progress
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let inside = |x: i32, y: i32| {
            (0..i32::from(area.width)).contains(&x) && (0..i32::from(area.height)).contains(&y)
        };
        let at = |x: i32, y: i32| Position::new(area.x + x as u16, area.y + y as u16);

        let strokes: Vec<Stroke> = self
            .figure
            .iter()
            .map(|figure| figure.stroke(self.progress))
            .chain(
                self.meteor
                    .iter()
                    .filter_map(|meteor| meteor.stroke(self.progress, self.palette.gold)),
            )
            .collect();
        self.draw_strokes(&strokes, area, buf);

        for star in &self.stars {
            let (x, y) = (i32::from(star.x), i32::from(star.y));
            if !inside(x, y) {
                continue;
            }
            buf[at(x, y)]
                .set_char(GLYPHS[star.brightness])
                .set_fg(self.shade(star.shown(self.progress)));
        }

        if let Some(figure) = &self.figure {
            let colour = octant::mix(
                self.palette.muted,
                self.palette.subtle,
                figure.name_share(self.progress),
            );
            let (x, y) = (i32::from(figure.name_at.x), i32::from(figure.name_at.y));
            for (offset, letter) in (0..).zip(figure.name.chars()) {
                if inside(x + offset, y) {
                    buf[at(x + offset, y)].set_char(letter).set_fg(colour);
                }
            }
        }
    }
}
