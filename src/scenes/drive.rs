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
/// How high the hills rise and fall, as a share of the height of the eye
/// over the road. Far higher than the eye, as in the arcade racers, so on
/// a climb the road rises past the horizon into the sky.
const HILLS: f64 = 2.0;
/// How fast the two waves of the hills turn, in radians a track unit. A
/// slope moves the whole road by the slope times its rows only while it
/// holds over the road in view, so both waves are far longer than that.
/// Two waves of unlike length keep the hills from repeating.
const HILL_WAVES: [f64; 2] = [0.06, 0.023];
/// How many stars the sky holds, at any size of pane.
const STARS: usize = 120;
/// The share of the pane's width the sky slides in a tick in a full curve.
const SKY: f64 = 0.004;
/// The track units from one post beside the road to the next.
const POST_GAP: f64 = 2.0;
/// A post's height as a share of the pane's height, its width as a share
/// of the pane's width, and how far out it stands as a share of the road's
/// half width, all on the bottom row and shrinking with distance.
const POST: (f64, f64, f64) = (0.3, 0.01, 1.25);
/// The pixels a post keeps clear of the road's edge. A far post nearer
/// than this would stand on the edge, so it is left out, like a post lost
/// in the dark.
const CLEAR: f64 = 2.0;
/// The car from behind, a pixel to a letter: body, window, tail light,
/// plate and tyre. A dot is clear.
const CAR: [&str; 9] = [
    ".......BBBBBBBBBB.......",
    "......BWWWWWWWWWWB......",
    ".....BWWWWWWWWWWWWB.....",
    "..BBBBBBBBBBBBBBBBBBBB..",
    ".BBBBBBBBBBBBBBBBBBBBBB.",
    "BLLLBBBBBBBBBBBBBBBBLLLB",
    "BLLLBBBBBBPPPPBBBBBBLLLB",
    "BBBBBBBBBBBBBBBBBBBBBBBB",
    ".TTTT..............TTTT.",
];
/// Where the car's middle runs, as a share of the road's half width right
/// of its middle.
const LANE: f64 = 0.45;
/// The speed of a car ahead, as a share of the car's speed.
const TRAFFIC: f64 = 0.4;
/// The track units the car covers from one car ahead to the next, at the
/// least and the most.
const TRAFFIC_GAP: (f64, f64) = (30.0, 80.0);
/// The fewest cell rows at which a car ahead shows its window, lights and
/// plate. A smaller car is its outline in its body colour: its sampled
/// rows miss the roof, and a window with no roof reads as two ears.
const DETAIL: usize = 5;
/// The letters of the car from the one a cell of a car ahead shows first,
/// when its columns of the car hold more than one. The body comes before
/// the window, so the pillars beside the window stay at any size.
const SHOWN_FIRST: [u8; 6] = *b"LPBWT.";
/// The moon's radius and the height of its middle, as shares of the pane's
/// height. A pixel is half a cell across and half a cell down, so twice as
/// tall as wide, and the moon spans twice its radius in pixels across.
const MOON: (f64, f64) = (0.07, 0.14);

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
    /// Where each wave of the hills starts, as an angle.
    hills: [f64; 2],
    /// Each star's place, as shares of the pane's width and of the sky's
    /// height.
    stars: Vec<(f64, f64)>,
    /// How far the sky has slid, as a share of the pane's width.
    sky: f64,
    /// Where each car ahead is on the track, the nearest first.
    traffic: Vec<f64>,
    /// How far the car has come when the next car ahead appears.
    next_car: f64,
}

impl Drive {
    pub fn new(mut rng: Rng, palette: &'static Palette) -> Self {
        let hills = [
            rng.random_range(0.0..std::f64::consts::TAU),
            rng.random_range(0.0..std::f64::consts::TAU),
        ];
        let stars = (0..STARS)
            .map(|_| (rng.random_range(0.0..1.0), rng.random_range(0.0..1.0)))
            .collect();
        let mut drive = Self {
            palette,
            rng,
            width: 0,
            height: 0,
            travelled: 0.0,
            track: VecDeque::new(),
            hills,
            stars,
            sky: 0.0,
            traffic: Vec::new(),
            next_car: 0.0,
        };
        drive.next_car = drive.rng.random_range(TRAFFIC_GAP.0..TRAFFIC_GAP.1);
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

    /// The height of the road at `at`, as a share of the height of the eye
    /// over it.
    fn altitude(&self, at: f64) -> f64 {
        let [first, second] = HILL_WAVES;
        HILLS * ((at * first + self.hills[0]).sin() + 0.6 * (at * second + self.hills[1]).sin())
    }

    /// The stars, then the moon, in the sky above `horizon`. Both slide
    /// with the sky and come round again at the other side.
    fn draw_sky(&self, rows: &mut [Vec<Color>], horizon: usize) {
        let across = rows.first().map_or(0, Vec::len);
        let width = across as f64;
        let height = rows.len() as f64;
        let room = horizon.saturating_sub(2) as f64;
        for &(x, y) in &self.stars {
            let column = ((x + self.sky).rem_euclid(1.0) * width) as usize;
            let row = (y * room) as usize;
            if let Some(pixel) = rows.get_mut(row).and_then(|row| row.get_mut(column)) {
                *pixel = self.palette.subtle;
            }
        }
        let (radius, middle) = (MOON.0 * height, MOON.1 * height);
        let span = width + 4.0 * radius;
        let centre = (0.7 * width + self.sky * width).rem_euclid(span) - 2.0 * radius;
        for (y, row) in rows.iter_mut().enumerate().take(horizon) {
            for (x, pixel) in row.iter_mut().enumerate() {
                let (dx, dy) = ((x as f64 + 0.5 - centre) / 2.0, y as f64 + 0.5 - middle);
                if dx * dx + dy * dy <= radius * radius {
                    *pixel = self.palette.text;
                }
            }
        }
    }

    /// The colour of each pixel, row by row. The road is laid from near to
    /// far, and each stretch fills only the rows above what is nearer, so
    /// a crest hides the road beyond it.
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
        self.draw_sky(&mut rows, horizon);
        let width = across as f64;
        let share = 1.0 / road_rows as f64;
        let eye = self.altitude(self.travelled);
        let (mut shift, mut drift) = (0.0, 0.0);
        let mut drawn = down;
        // The row, middle and half width of each stretch in view, by its
        // step from the horizon, for the posts to stand on.
        let mut stretches: Vec<Option<(usize, f64, f64)>> = vec![None; road_rows + 1];
        for step in (1..=road_rows).rev() {
            let near = step as f64 / road_rows as f64;
            let ahead = self.travelled + 1.0 / near;
            drift += self.curve(ahead) * share;
            shift += drift * share;
            let rise = self.altitude(ahead) - eye;
            let top = (horizon as f64 + step as f64 * (1.0 - rise))
                .round()
                .max(0.0) as usize;
            if top > drawn {
                continue;
            }
            let centre = width / 2.0 + shift * width * BEND;
            let half = width * ROAD / 2.0 * near;
            stretches[step] = Some((top, centre, half));
            let band = (ahead / STRIPE).floor() as i64 & 1 == 0;
            for row in &mut rows[top..drawn] {
                for (x, pixel) in row.iter_mut().enumerate() {
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
            drawn = top;
        }
        // The land meets the sky on a cell boundary, because a cell that
        // held both would hold more colours than a quadrant cell draws.
        if drawn % 2 == 1 && drawn < down {
            rows[drawn - 1] = rows[drawn].clone();
        }
        self.draw_posts(&mut rows, &stretches);
        self.draw_traffic(&mut rows, &stretches, horizon, eye);
        self.draw_car(&mut rows);
        rows
    }

    /// The posts on both sides of the road, from far to near, so a nearer
    /// post stands in front. A post on a stretch hidden behind a crest is
    /// hidden with it.
    fn draw_posts(&self, rows: &mut [Vec<Color>], stretches: &[Option<(usize, f64, f64)>]) {
        let road_rows = stretches.len() - 1;
        let (down, across) = (rows.len() as f64, rows.first().map_or(0, Vec::len) as f64);
        let first = (self.travelled + 1.0) / POST_GAP;
        let last = (self.travelled + road_rows as f64) / POST_GAP;
        for post in (first.ceil() as i64..=last.floor() as i64).rev() {
            let ahead = post as f64 * POST_GAP - self.travelled;
            let step = (road_rows as f64 / ahead).round() as usize;
            let Some(Some((ground, centre, half))) = stretches.get(step).copied() else {
                continue;
            };
            let near = step as f64 / road_rows as f64;
            let height = (POST.0 * down * near).round().max(1.0) as usize;
            let thick = (POST.1 * across * near).round().max(1.0) as usize;
            if (POST.2 - 1.0) * half < thick as f64 / 2.0 + CLEAR {
                continue;
            }
            // A post covers whole cells, from an even column and row to the
            // end of a cell, so no cell holds a post, grass of one band and
            // grass of the other.
            let thick = thick.div_ceil(2) * 2;
            let bottom = (ground | 1).min(rows.len() - 1);
            let top = ground.saturating_sub(height - 1) & !1;
            for side in [-1.0, 1.0] {
                let middle = centre + side * half * POST.2;
                let left = (middle - thick as f64 / 2.0).round() as i64 & !1;
                for x in left..left + thick as i64 {
                    let Ok(x) = usize::try_from(x) else {
                        continue;
                    };
                    for row in &mut rows[top..=bottom] {
                        if let Some(pixel) = row.get_mut(x) {
                            *pixel = self.palette.muted;
                        }
                    }
                }
            }
        }
    }

    /// The cars ahead in the left lane, from far to near, the same car as
    /// the one driven, shrunk with distance. A car on a stretch hidden
    /// behind a crest is hidden with it. Each covers whole cells, so no
    /// cell holds a car and two colours of the road behind it.
    fn draw_traffic(
        &self,
        rows: &mut [Vec<Color>],
        stretches: &[Option<(usize, f64, f64)>],
        horizon: usize,
        eye: f64,
    ) {
        let road_rows = stretches.len() - 1;
        let across = rows.first().map_or(0, Vec::len) as f64;
        let (wide, tall) = (CAR[0].len() as f64, CAR.len() as f64);
        for &car in self.traffic.iter().rev() {
            let near = 1.0 / (car - self.travelled);
            let step = road_rows as f64 * near;
            // A car nearer than the bottom row stands on no stretch in
            // view. The road has not turned yet there, so it is centred.
            let (ground, centre, half) = if step.round() as usize <= road_rows {
                let Some(Some(stretch)) = stretches.get(step.round() as usize).copied() else {
                    continue;
                };
                stretch
            } else {
                let rise = self.altitude(car) - eye;
                let ground = (horizon as f64 + step * (1.0 - rise)).round() as usize;
                (ground, across / 2.0, across * ROAD / 2.0 * near)
            };
            let cells_across = (wide * near / 2.0).ceil().max(1.0) as usize;
            let cells_down = (tall * near / 2.0).ceil().max(1.0) as usize;
            let middle = centre - half * LANE;
            let left = ((middle - cells_across as f64) / 2.0).floor() as i64 * 2;
            let top = (ground | 1) as i64 + 1 - 2 * cells_down as i64;
            for down in 0..cells_down {
                let line = CAR[((down as f64 + 0.5) * tall / cells_down as f64) as usize];
                for over in 0..cells_across {
                    let from = (over as f64 * wide / cells_across as f64) as usize;
                    let to = (((over + 1) as f64 * wide / cells_across as f64).ceil() as usize)
                        .max(from + 1);
                    let Some(letter) = line.as_bytes()[from..to]
                        .iter()
                        .min_by_key(|letter| SHOWN_FIRST.iter().position(|first| first == *letter))
                    else {
                        continue;
                    };
                    let colour = match *letter {
                        b'.' => continue,
                        _ if cells_down < DETAIL => self.palette.gold,
                        b'W' | b'T' => self.palette.base,
                        b'L' => self.palette.love,
                        b'P' => self.palette.text,
                        _ => self.palette.gold,
                    };
                    for y in 0..2 {
                        for x in 0..2 {
                            let (x, y) = (left + 2 * over as i64 + x, top + 2 * down as i64 + y);
                            let pixel = usize::try_from(y)
                                .ok()
                                .zip(usize::try_from(x).ok())
                                .and_then(|(y, x)| rows.get_mut(y)?.get_mut(x));
                            if let Some(pixel) = pixel {
                                *pixel = colour;
                            }
                        }
                    }
                }
            }
        }
    }

    /// The car at the bottom of the pane in the right lane. In a curve its
    /// roof leans two pixels toward it and its upper body one.
    fn draw_car(&self, rows: &mut [Vec<Color>]) {
        let (down, across) = (rows.len(), rows.first().map_or(0, Vec::len));
        let wide = CAR[0].len();
        if down < CAR.len() || across < wide {
            return;
        }
        let curve = self.curve(self.travelled + 2.0);
        let lean = if curve > 0.25 {
            1
        } else if curve < -0.25 {
            -1
        } else {
            0
        };
        let middle = across as f64 / 2.0 * (1.0 + ROAD * LANE);
        let left = middle.round() as i64 - wide as i64 / 2;
        for (line, pattern) in CAR.iter().enumerate() {
            let shift = match line {
                0..=2 => 2 * lean,
                3..=4 => lean,
                _ => 0,
            };
            let y = down - CAR.len() + line;
            for (offset, letter) in pattern.chars().enumerate() {
                let colour = match letter {
                    'B' => self.palette.iris,
                    'W' | 'T' => self.palette.base,
                    'L' => self.palette.love,
                    'P' => self.palette.text,
                    _ => continue,
                };
                let x = left + offset as i64 + shift;
                if let Some(pixel) = usize::try_from(x).ok().and_then(|x| rows[y].get_mut(x)) {
                    *pixel = colour;
                }
            }
        }
    }
}

impl Scene for Drive {
    fn tick(&mut self, area: Rect) {
        self.width = area.width;
        self.height = area.height;
        self.travelled += SPEED;
        self.sky -= self.curve(self.travelled) * SKY;
        let reach = self.travelled + f64::from(area.height) * 2.0 + EASE;
        self.lay(reach);
        let travelled = self.travelled;
        for car in &mut self.traffic {
            *car += SPEED * TRAFFIC;
        }
        // A car a quarter unit ahead stands far below the bottom row.
        self.traffic.retain(|car| car - travelled > 0.25);
        if travelled >= self.next_car {
            let down = usize::from(area.height) * 2;
            let road_rows = down.saturating_sub((down as f64 * HORIZON) as usize + 1);
            self.traffic.push(travelled + road_rows.max(1) as f64);
            self.next_car = travelled + self.rng.random_range(TRAFFIC_GAP.0..TRAFFIC_GAP.1);
        }
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
