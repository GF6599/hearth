use std::collections::BTreeMap;
use std::f32::consts::TAU;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

use crate::braille::{self, CELL_HEIGHT, CELL_WIDTH};
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// Each orbit's half width as a share of the outermost, Mercury to
/// Neptune. The real distances run from 0.4 to 30 times the Earth's, which
/// would put the inner four inside the sun at this scale, so the orbits are
/// spaced evenly instead.
const ORBITS: [f32; 8] = [0.2, 0.31, 0.43, 0.54, 0.66, 0.77, 0.89, 1.0];
/// Ticks for Mercury to go round, about eight seconds. The others take
/// longer by Kepler's third law on their shares, so Neptune takes about a
/// minute and a half.
const MERCURY_TICKS: f32 = 133.0;
/// The height of each orbit as a share of its width, for a view from above
/// at a slant. Low enough that Mercury passes in front of the sun and
/// behind it.
const TILT: f32 = 0.4;
/// Dots between the outermost orbit and the edge of the pane.
const MARGIN: f32 = 2.0;
/// The sun's radius as a share of the outermost orbit's half width.
const SUN_SHARE: f32 = 0.13;
/// Dots along an orbit from one drawn dot to the next.
const ORBIT_GAP: f32 = 3.0;
/// Each planet's radius in dots, Mercury to Neptune, round a core of two by
/// two dots: the rocky four small, the gas giants large.
const RADII: [f32; 8] = [1.0, 1.3, 1.3, 1.0, 2.2, 1.8, 1.5, 1.5];
const SATURN: usize = 5;
const EARTH: usize = 2;
/// Saturn's ring, as a half width in dots and a height as a share of it.
const RING_ACROSS: f32 = 4.5;
const RING_TILT: f32 = 0.3;
/// The Moon's orbit round Earth, as a half width in dots, and its period.
const MOON_ACROSS: f32 = 4.4;
const MOON_TICKS: f32 = 40.0;
/// How far a trail reaches behind its planet, as a share of a turn.
const TRAIL_TURN: f32 = 1.0 / 8.0;
/// Dots along an orbit from one trail place to the next.
const TRAIL_GAP: f32 = 1.0;
/// The fraction of the golden ratio. Its multiples spread the trail's dither
/// thresholds evenly with no pattern that the eye picks up.
const GOLDEN: f32 = 0.618_034;

/// What a cell shows when more than one thing falls in it, from the
/// lowest. One braille cell holds one colour, so a cell shows the dots of
/// its highest layer alone. A body or a trail on the far half of its orbit
/// is behind the sun and one on the near half in front of it, a body is
/// over every trail on its side, and between two bodies the nearer wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Layer {
    Orbit,
    TrailBehind,
    Behind(i32),
    Sun,
    TrailFront,
    Front(i32),
}

#[derive(Debug)]
pub struct Solar {
    palette: &'static Palette,
    width: u16,
    height: u16,
    time: u32,
    /// Where each planet starts on its orbit, as an angle.
    phases: [f32; ORBITS.len()],
}

impl Solar {
    pub fn new(mut rng: Rng, palette: &'static Palette) -> Self {
        Self {
            palette,
            width: 0,
            height: 0,
            time: 0,
            phases: std::array::from_fn(|_| rng.random_range(0.0..TAU)),
        }
    }

    /// Where the Moon is, in dots from the top left of the pane.
    pub fn moon(&self) -> (f32, f32) {
        let angle = TAU * self.time as f32 / MOON_TICKS;
        place(self.planets()[EARTH], MOON_ACROSS, angle)
    }

    /// Each planet's middle, Mercury first, in dots from the top left of
    /// the pane.
    pub fn planets(&self) -> Vec<(f32, f32)> {
        let (centre, across) = self.frame();
        ORBITS
            .iter()
            .zip(self.angles())
            .map(|(share, angle)| place(centre, across * share, angle))
            .collect()
    }

    /// Where each planet is on its orbit, Mercury first, as an angle.
    fn angles(&self) -> [f32; ORBITS.len()] {
        std::array::from_fn(|index| {
            let period = MERCURY_TICKS * (ORBITS[index] / ORBITS[0]).powf(1.5);
            (self.phases[index] + TAU * self.time as f32 / period).rem_euclid(TAU)
        })
    }

    /// The middle of the pane and the half width of the outermost orbit, in
    /// dots. The orbit fits the width or the height, whichever is tighter.
    fn frame(&self) -> ((f32, f32), f32) {
        let width = f32::from(self.width) * CELL_WIDTH as f32;
        let height = f32::from(self.height) * CELL_HEIGHT as f32;
        let centre = ((width - 1.0) / 2.0, (height - 1.0) / 2.0);
        let across = (width / 2.0 - MARGIN).min((height / 2.0 - MARGIN) / TILT);
        (centre, across.max(0.0))
    }
}

/// The place at `angle` on the orbit `across` dots wide each side of the
/// centre. The angle grows the way the planets go, against the clock seen
/// from above, so a planet crosses the near side, the bottom of the pane,
/// from left to right.
fn place(centre: (f32, f32), across: f32, angle: f32) -> (f32, f32) {
    (
        centre.0 + across * angle.cos(),
        centre.1 - across * TILT * angle.sin(),
    )
}

/// The dots of a body of `radius` round `middle`: a core of two by two dots
/// round the middle, so the smallest body still shows, and every dot within
/// the radius.
fn disc(middle: (f32, f32), radius: f32) -> Vec<(i32, i32)> {
    let (left, top) = (
        (middle.0 - 0.5).floor() as i32,
        (middle.1 - 0.5).floor() as i32,
    );
    let mut dots = vec![
        (left, top),
        (left + 1, top),
        (left, top + 1),
        (left + 1, top + 1),
    ];
    let reach = radius.ceil() as i32;
    let (x, y) = (middle.0.round() as i32, middle.1.round() as i32);
    for dot_y in y - reach..=y + reach {
        for dot_x in x - reach..=x + reach {
            let (dx, dy) = (dot_x as f32 - middle.0, dot_y as f32 - middle.1);
            if dx * dx + dy * dy <= radius * radius {
                dots.push((dot_x, dot_y));
            }
        }
    }
    dots
}

impl Scene for Solar {
    fn tick(&mut self, area: Rect) {
        self.width = area.width;
        self.height = area.height;
        self.time = self.time.wrapping_add(1);
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let (centre, across) = self.frame();
        let mut cells: BTreeMap<(i32, i32), (Layer, u32, Color)> = BTreeMap::new();
        let mut plot = |x: i32, y: i32, layer: Layer, colour: Color| {
            if x < 0 || y < 0 {
                return;
            }
            let cell = (x / CELL_WIDTH as i32, y / CELL_HEIGHT as i32);
            let bit = braille::dot(
                (x % CELL_WIDTH as i32) as usize,
                (y % CELL_HEIGHT as i32) as usize,
            );
            let entry = cells.entry(cell).or_insert((layer, 0, colour));
            if layer > entry.0 {
                *entry = (layer, bit, colour);
            } else if layer == entry.0 {
                entry.1 |= bit;
            }
        };

        for share in ORBITS {
            let reach = across * share;
            let length = TAU * reach;
            let steps = (length / ORBIT_GAP).max(1.0) as usize;
            for step in 0..steps {
                let (x, y) = place(centre, reach, TAU * step as f32 / steps as f32);
                plot(
                    x.round() as i32,
                    y.round() as i32,
                    Layer::Orbit,
                    self.palette.muted,
                );
            }
        }
        let depth = |place: (f32, f32)| {
            let nearness = place.1.round() as i32;
            if place.1 > centre.1 {
                Layer::Front(nearness)
            } else {
                Layer::Behind(nearness)
            }
        };
        let planets = self.planets();
        let angles = self.angles();
        let colours = [
            self.palette.subtle,
            self.palette.text,
            self.palette.foam,
            self.palette.love,
            self.palette.rose,
            self.palette.gold,
            self.palette.pine,
            self.palette.iris,
        ];
        for (index, (&share, &angle)) in ORBITS.iter().zip(&angles).enumerate() {
            let reach = across * share;
            let steps = ((TAU * reach / TRAIL_GAP) as i32).max(1);
            let gap = TAU / steps as f32;
            let newest = (angle / gap).floor() as i32;
            for step in (newest - (TRAIL_TURN * steps as f32) as i32..=newest).rev() {
                // Each place keeps its threshold as the planet moves on, so a
                // lit dot stays lit until the fade passes its threshold.
                let fade = (angle - step as f32 * gap) / (TAU * TRAIL_TURN);
                let threshold = (step.rem_euclid(steps) as f32 * GOLDEN).fract();
                if fade > 1.0 || threshold < fade {
                    continue;
                }
                let (x, y) = place(centre, reach, step as f32 * gap);
                let layer = if y > centre.1 {
                    Layer::TrailFront
                } else {
                    Layer::TrailBehind
                };
                plot(x.round() as i32, y.round() as i32, layer, colours[index]);
            }
        }
        for (index, &body) in planets.iter().enumerate() {
            let layer = depth(body);
            for (x, y) in disc(body, RADII[index]) {
                plot(x, y, layer, colours[index]);
            }
            if index == SATURN {
                let steps = 24;
                for step in 0..steps {
                    let angle = TAU * step as f32 / steps as f32;
                    let (x, y) = (
                        body.0 + RING_ACROSS * angle.cos(),
                        body.1 - RING_ACROSS * RING_TILT * angle.sin(),
                    );
                    plot(x.round() as i32, y.round() as i32, layer, colours[index]);
                }
            }
        }
        let moon = self.moon();
        for (x, y) in disc(moon, 0.0) {
            plot(x, y, depth(moon), self.palette.subtle);
        }
        for (x, y) in disc(centre, across * SUN_SHARE) {
            plot(x, y, Layer::Sun, self.palette.gold);
        }

        for ((column, row), (_, dots, colour)) in cells {
            let (Ok(column), Ok(row)) = (u16::try_from(column), u16::try_from(row)) else {
                continue;
            };
            if column >= area.width || row >= area.height {
                continue;
            }
            buf[Position::new(area.x + column, area.y + row)]
                .set_char(braille::glyph(dots))
                .set_fg(colour);
        }
    }
}
