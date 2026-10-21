use std::f32::consts::TAU;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::octant::{self, CELL_HEIGHT, CELL_WIDTH, Canvas};
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
/// Pixels between the outermost orbit and the edge of the pane.
const MARGIN: f32 = 2.0;
/// The sun's radius as a share of the outermost orbit's half width.
const SUN_SHARE: f32 = 0.13;
/// Pixels along an orbit from one drawn pixel to the next.
const ORBIT_GAP: f32 = 3.0;
/// Each planet's radius in pixels, Mercury to Neptune: the rocky four small,
/// the gas giants large. A radius of 2.6 or more always lights five rows of
/// pixels, so every planet spans two rows of cells.
const RADII: [f32; 8] = [2.6, 2.8, 2.8, 2.6, 4.4, 3.8, 3.2, 3.2];
const SATURN: usize = 5;
const EARTH: usize = 2;
/// Saturn's ring, as a half width in pixels and a height as a share of it.
const RING_ACROSS: f32 = 8.0;
const RING_TILT: f32 = 0.3;
/// The Moon's orbit round Earth, as a half width in pixels, and its period.
const MOON_ACROSS: f32 = 7.0;
const MOON_TICKS: f32 = 40.0;
/// How far a trail reaches behind its planet, as a share of a turn.
const TRAIL_TURN: f32 = 1.0 / 8.0;
/// Pixels along an orbit from one trail place to the next.
const TRAIL_GAP: f32 = 1.0;
/// The fraction of the golden ratio. Its multiples spread the trail's dither
/// thresholds evenly with no pattern that the eye picks up.
const GOLDEN: f32 = 0.618_034;

/// What a pixel shows when more than one thing falls on it, from the
/// lowest. A body or a trail on the far half of its orbit is behind the sun
/// and one on the near half in front of it, a body is over every trail on
/// its side and over its own trail, and between two bodies the nearer
/// wins.
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
    /// The share of a tick since the last, so a planet glides along its
    /// orbit between two ticks.
    progress: f64,
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
            progress: 0.0,
            phases: std::array::from_fn(|_| rng.random_range(0.0..TAU)),
        }
    }

    /// Where the Moon is, in pixels from the top left of the pane, at the
    /// share of a tick the scene was last given.
    pub fn moon(&self) -> (f32, f32) {
        let angle = TAU * turned(self.ticks(), MOON_TICKS);
        place(self.planets()[EARTH], MOON_ACROSS, angle)
    }

    /// Each planet's middle, Mercury first, in pixels from the top left of
    /// the pane, at the share of a tick the scene was last given.
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
            (self.phases[index] + TAU * turned(self.ticks(), period)).rem_euclid(TAU)
        })
    }

    /// The ticks run, with the share of the next one.
    fn ticks(&self) -> f64 {
        f64::from(self.time) + self.progress
    }

    /// The middle of the pane and the half width of the outermost orbit, in
    /// pixels. The orbit fits the width or the height, whichever is tighter.
    fn frame(&self) -> ((f32, f32), f32) {
        let width = f32::from(self.width) * CELL_WIDTH as f32;
        let height = f32::from(self.height) * CELL_HEIGHT as f32;
        let centre = ((width - 1.0) / 2.0, (height - 1.0) / 2.0);
        let across = (width / 2.0 - MARGIN).min((height / 2.0 - MARGIN) / TILT);
        (centre, across.max(0.0))
    }
}

/// The share of a turn made after `ticks`, at `period` ticks a turn.
/// Whole turns come off in f64 first, because an angle that grows without
/// end in f32 loses the precision of one tick within a day.
fn turned(ticks: f64, period: f32) -> f32 {
    (ticks / f64::from(period)).fract() as f32
}

/// The place at `angle` on the orbit `across` pixels wide each side of the
/// centre. The angle grows the way the planets go, against the clock seen
/// from above, so a planet crosses the near side, the bottom of the pane,
/// from left to right.
fn place(centre: (f32, f32), across: f32, angle: f32) -> (f32, f32) {
    (
        centre.0 + across * angle.cos(),
        centre.1 - across * TILT * angle.sin(),
    )
}

/// The pixels of a body of `radius` round `middle`: a core of two by two pixels
/// round the middle, so the smallest body still shows, and every pixel within
/// the radius.
fn disc(middle: (f32, f32), radius: f32) -> Vec<(i32, i32)> {
    let (left, top) = (
        (middle.0 - 0.5).floor() as i32,
        (middle.1 - 0.5).floor() as i32,
    );
    let mut pixels = vec![
        (left, top),
        (left + 1, top),
        (left, top + 1),
        (left + 1, top + 1),
    ];
    let reach = radius.ceil() as i32;
    let (x, y) = (middle.0.round() as i32, middle.1.round() as i32);
    for pixel_y in y - reach..=y + reach {
        for pixel_x in x - reach..=x + reach {
            let (dx, dy) = (pixel_x as f32 - middle.0, pixel_y as f32 - middle.1);
            if dx * dx + dy * dy <= radius * radius {
                pixels.push((pixel_x, pixel_y));
            }
        }
    }
    pixels
}

/// A pixel of an orbit or a trail beside a body drops out of its cell, and
/// so does the part of a body that shares a cell of sky with a nearer one.
impl octant::Layer for Layer {}

impl Scene for Solar {
    fn tick(&mut self, area: Rect) {
        self.width = area.width;
        self.height = area.height;
        self.time = self.time.wrapping_add(1);
    }

    fn set_progress(&mut self, progress: f64) {
        self.progress = progress;
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let (centre, across) = self.frame();
        let mut canvas = Canvas::new(area.width, area.height, self.palette);
        let (width, height) = (canvas.width(), canvas.height());
        let mut layers: Vec<Option<(Layer, Color)>> = vec![None; width * height];
        let mut plot = |x: i32, y: i32, layer: Layer, colour: Color| {
            let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
                return;
            };
            if x >= width || y >= height {
                return;
            }
            let pixel = &mut layers[y * width + x];
            if pixel.is_none_or(|(under, _)| layer > under) {
                *pixel = Some((layer, colour));
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
                // lit pixel stays lit until the fade passes its threshold.
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

        for top in (0..height).step_by(CELL_HEIGHT) {
            for left in (0..width).step_by(CELL_WIDTH) {
                let at = |pixel: usize| (left + pixel % CELL_WIDTH, top + pixel / CELL_WIDTH);
                let pixels = std::array::from_fn(|pixel| {
                    let (x, y) = at(pixel);
                    layers[y * width + x]
                });
                for (pixel, colour) in octant::settle(pixels, self.palette.base)
                    .into_iter()
                    .enumerate()
                {
                    let (x, y) = at(pixel);
                    canvas.set(x, y, colour);
                }
            }
        }
        canvas.render(area, buf);
    }
}
