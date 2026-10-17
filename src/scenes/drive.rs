use std::collections::VecDeque;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

use crate::quadrant;
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// The horizon as a share of the pane's height, from the top.
const HORIZON: f64 = 0.4;
/// The road's width on the bottom row, as a share of the pane's width.
const ROAD: f64 = 0.84;
/// The rumble strip at each edge, as a share of the road's half width.
const RUMBLE: f64 = 0.12;
/// The centre dash, as a share of the road's half width.
const DASH: f64 = 0.05;
/// The length of one stripe, in the track's units. The bottom row is one
/// unit ahead, and a row is as many units ahead as the rows from the
/// horizon to the bottom over the rows from the horizon to it.
const STRIPE: f64 = 0.25;
/// The track units the car covers in a tick.
const SPEED: f64 = 0.08;
/// How far a full curve bends the road at the horizon, as a share of the
/// pane's width.
const BEND: f64 = 0.6;
/// The track units over which one curve eases into the next.
const EASE: f64 = 8.0;

/// A stretch of track with one curve: negative bends left, positive right.
#[derive(Debug, Clone, Copy)]
struct Segment {
    start: f64,
    length: f64,
    /// The curve the stretch eases in from.
    from: f64,
    curve: f64,
}

#[derive(Debug)]
pub struct Drive {
    palette: &'static Palette,
    rng: Rng,
    width: u16,
    height: u16,
    /// How far the car has come, in track units. In f64, because in f32 a
    /// distance that grows for days would make the stripes jump.
    travelled: f64,
    /// The track from behind the car to past the horizon.
    track: VecDeque<Segment>,
}

impl Drive {
    pub fn new(rng: Rng, palette: &'static Palette) -> Self {
        let mut drive = Self {
            palette,
            rng,
            width: 0,
            height: 0,
            travelled: 0.0,
            track: VecDeque::new(),
        };
        drive.track.push_back(Segment {
            start: -EASE,
            length: EASE + 40.0,
            from: 0.0,
            curve: 0.0,
        });
        drive
    }

    /// Lays track to `reach`: a straight after each curve, and a curve of
    /// a random side and strength after each straight.
    fn lay(&mut self, reach: f64) {
        while let Some(last) = self.track.back().copied() {
            let end = last.start + last.length;
            if end > reach {
                break;
            }
            let (length, curve) = if last.curve == 0.0 {
                let strength = if self.rng.random_bool(0.5) { 0.5 } else { 1.0 };
                let side = if self.rng.random_bool(0.5) { -1.0 } else { 1.0 };
                (self.rng.random_range(30.0..70.0), side * strength)
            } else {
                (self.rng.random_range(40.0..80.0), 0.0)
            };
            self.track.push_back(Segment {
                start: end,
                length,
                from: last.curve,
                curve,
            });
        }
        while self
            .track
            .front()
            .is_some_and(|first| first.start + first.length < self.travelled - EASE)
        {
            self.track.pop_front();
        }
    }

    fn curve(&self, at: f64) -> f64 {
        self.track
            .iter()
            .find(|segment| at < segment.start + segment.length)
            .map_or(0.0, |segment| {
                let eased = ((at - segment.start) / EASE).clamp(0.0, 1.0);
                segment.from + (segment.curve - segment.from) * eased
            })
    }

    /// The colour of each pixel, row by row.
    fn pixels(&self) -> Vec<Vec<Color>> {
        let across = usize::from(self.width) * 2;
        let down = usize::from(self.height) * 2;
        let palette = self.palette;
        let mut rows = vec![vec![palette.base; across]; down];
        let horizon = (down as f64 * HORIZON) as usize;
        let road_rows = down.saturating_sub(horizon + 1);
        if road_rows == 0 {
            return rows;
        }
        let width = across as f64;
        let share = 1.0 / road_rows as f64;
        let (mut shift, mut drift) = (0.0, 0.0);
        for y in (horizon + 1..down).rev() {
            let near = (y - horizon) as f64 / road_rows as f64;
            let ahead = self.travelled + 1.0 / near;
            drift += self.curve(ahead) * share;
            shift += drift * share;
            let centre = width / 2.0 + shift * width * BEND;
            let half = width * ROAD / 2.0 * near;
            let band = (ahead / STRIPE).floor() as i64 & 1 == 0;
            for (x, pixel) in rows[y].iter_mut().enumerate() {
                let off = (x as f64 + 0.5 - centre).abs();
                *pixel = if off < half * DASH && band {
                    palette.text
                } else if off < half * (1.0 - RUMBLE) {
                    palette.overlay
                } else if off < half {
                    if band { palette.love } else { palette.text }
                } else if band {
                    palette.pine
                } else {
                    palette.base
                };
            }
        }
        rows
    }
}

impl Scene for Drive {
    fn tick(&mut self, area: Rect) {
        self.width = area.width;
        self.height = area.height;
        self.travelled += SPEED;
        let reach = self.travelled + f64::from(area.height) * 2.0 + EASE;
        self.lay(reach);
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let rows = self.pixels();
        for row in 0..self.height.min(area.height) {
            for column in 0..self.width.min(area.width) {
                let (x, y) = (usize::from(column) * 2, usize::from(row) * 2);
                let quarters = [
                    rows[y][x],
                    rows[y][x + 1],
                    rows[y + 1][x],
                    rows[y + 1][x + 1],
                ];
                let (glyph, fore, back) = quadrant::cell(quarters, self.palette.base);
                buf[Position::new(area.x + column, area.y + row)]
                    .set_char(glyph)
                    .set_fg(fore)
                    .set_bg(back);
            }
        }
    }
}
