use std::collections::VecDeque;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::octant::{CELL_HEIGHT, CELL_WIDTH, Canvas};
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
/// The car from behind, a letter to a pixel across and to `CAR_LINE` pixels
/// down: body, window, tail light, plate and tyre. A dot is clear.
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
/// The pixel rows of each line of `CAR`. The car is drawn in pixels twice
/// as tall as wide, and a pixel is about square.
const CAR_LINE: usize = 2;
/// Where the car's middle runs, as a share of the road's half width right
/// of its middle.
const LANE: f64 = 0.45;
/// The speed of a car ahead, as a share of the car's speed.
const TRAFFIC: f64 = 0.25;
/// The track units the car covers before the first car ahead appears, at
/// the least and the most, so one shows within seconds of the start.
const FIRST_CAR: (f64, f64) = (1.0, 4.0);
/// The track units the car covers from one car ahead to the next, at the
/// least and the most.
const TRAFFIC_GAP: (f64, f64) = (15.0, 40.0);
/// How far ahead a car ahead appears, in track units, or at the far end of
/// the road when that is nearer. Further off it is one cell wide, so from
/// the far end of a tall road it would be a dot for most of its approach.
const AHEAD: f64 = 16.0;
/// The fewest cell rows at which a car ahead shows its window, lights and
/// plate. A smaller car is its outline in its body colour: its sampled
/// rows miss the roof, and a window with no roof reads as two ears.
const DETAIL: usize = 5;
/// The letters of the car from the one a cell of a car ahead shows first,
/// when its columns of the car hold more than one. The body comes before
/// the window, so the pillars beside the window stay at any size.
const SHOWN_FIRST: [u8; 6] = *b"LPBWT.";
/// The moon's radius and the height of its middle, as shares of the pane's
/// height. A pixel is half a cell across and a quarter of a cell down, so
/// about square.
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
    /// How far the scene has run from its last tick towards its next.
    progress: f64,
}

/// Where the parts that move stand at a moment between two ticks.
#[derive(Debug, Clone, Copy)]
struct Moment {
    travelled: f64,
    sky: f64,
    /// How far each car ahead has come since the last tick.
    traffic: f64,
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
            progress: 0.0,
        };
        drive.next_car = drive.rng.random_range(FIRST_CAR.0..FIRST_CAR.1);
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

    /// Each part that moves, the share `progress` of the way to where the
    /// next tick puts it. The parts glide from their places and not across
    /// the screen, so a star or a stripe that comes round again at the
    /// other side draws no streak.
    fn moment(&self) -> Moment {
        Moment {
            travelled: self.travelled + SPEED * self.progress,
            // The next tick turns the sky by the curve where it puts the car.
            sky: self.sky - self.curve(self.travelled + SPEED) * SKY * self.progress,
            traffic: SPEED * TRAFFIC * self.progress,
        }
    }

    /// The stars, then the moon, in the sky above `horizon`. Both slide
    /// with the sky and come round again at the other side.
    fn draw_sky(&self, canvas: &mut Canvas, horizon: usize, sky: f64) {
        let width = canvas.width() as f64;
        let height = canvas.height() as f64;
        let room = horizon.saturating_sub(CELL_HEIGHT) as f64;
        for &(x, y) in &self.stars {
            let column = ((x + sky).rem_euclid(1.0) * width) as usize;
            let row = (y * room) as usize;
            canvas.set(column, row, self.palette.subtle);
        }
        let (radius, middle) = (MOON.0 * height, MOON.1 * height);
        let span = width + 2.0 * radius;
        let centre = (0.7 * width + sky * width).rem_euclid(span) - radius;
        for y in 0..horizon.min(canvas.height()) {
            for x in 0..canvas.width() {
                let (dx, dy) = (x as f64 + 0.5 - centre, y as f64 + 0.5 - middle);
                if dx * dx + dy * dy <= radius * radius {
                    canvas.set(x, y, self.palette.text);
                }
            }
        }
    }

    /// The colour of each pixel, row by row. The road is laid from near to
    /// far, and each stretch fills only the rows above what is nearer, so
    /// a crest hides the road beyond it.
    fn pixels(&self) -> Canvas {
        let palette = self.palette;
        let mut canvas = Canvas::new(self.width, self.height, palette.base);
        let (across, down) = (canvas.width(), canvas.height());
        let horizon = (down as f64 * HORIZON) as usize;
        let road_rows = down.saturating_sub(horizon + 1);
        if road_rows == 0 {
            return canvas;
        }
        let now = self.moment();
        self.draw_sky(&mut canvas, horizon, now.sky);
        let width = across as f64;
        let share = 1.0 / road_rows as f64;
        let eye = self.altitude(now.travelled);
        let (mut shift, mut drift) = (0.0, 0.0);
        let mut drawn = down;
        // The row, middle and half width of each stretch in view, by its
        // step from the horizon, for the posts to stand on.
        let mut stretches: Vec<Option<(usize, f64, f64)>> = vec![None; road_rows + 1];
        let mut shown: Option<(f64, f64)> = None;
        for step in (1..=road_rows).rev() {
            let near = step as f64 / road_rows as f64;
            let ahead = now.travelled + 1.0 / near;
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
            // A stretch whose top meets the nearer road's top paints no row,
            // so what stands on it takes the road that row shows. Its own
            // middle can lie pixels apart from it on a climb far ahead.
            let (road_centre, road_half) = match shown {
                Some(road) if top == drawn => road,
                _ => (centre, half),
            };
            stretches[step] = Some((top, road_centre, road_half));
            if top < drawn {
                shown = Some((centre, half));
            }
            let band = (ahead / STRIPE).floor() as i64 & 1 == 0;
            for y in top..drawn {
                for x in 0..across {
                    let off = (x as f64 + 0.5 - centre).abs();
                    let colour = if off < half * DASH && band {
                        palette.text
                    } else if off < half * (1.0 - RUMBLE) {
                        palette.overlay
                    } else if off < half {
                        if band { palette.love } else { palette.text }
                    } else {
                        palette.base
                    };
                    canvas.set(x, y, colour);
                }
            }
            drawn = top;
        }
        // The sky in the cell of the road's top row is clear, because a cell
        // that held a star or the moon and the road would hold more colours
        // than a cell draws.
        if drawn < down {
            for y in drawn / CELL_HEIGHT * CELL_HEIGHT..drawn {
                for x in 0..across {
                    canvas.set(x, y, palette.base);
                }
            }
        }
        self.draw_posts(&mut canvas, &stretches, now.travelled);
        self.draw_traffic(&mut canvas, &stretches, horizon, eye, now);
        self.draw_car(&mut canvas, now.travelled);
        canvas
    }

    /// The posts on both sides of the road, from far to near, so a nearer
    /// post stands in front. A post on a stretch hidden behind a crest is
    /// hidden with it.
    fn draw_posts(
        &self,
        canvas: &mut Canvas,
        stretches: &[Option<(usize, f64, f64)>],
        travelled: f64,
    ) {
        let road_rows = stretches.len() - 1;
        let (down, across) = (canvas.height() as f64, canvas.width() as f64);
        let first = (travelled + 1.0) / POST_GAP;
        let last = (travelled + road_rows as f64) / POST_GAP;
        for post in (first.ceil() as i64..=last.floor() as i64).rev() {
            let ahead = post as f64 * POST_GAP - travelled;
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
            // A post covers whole cells, from the first column and row of a
            // cell to the end of one, so no cell holds a post and two colours
            // of what stands behind it.
            let thick = thick.div_ceil(CELL_WIDTH) * CELL_WIDTH;
            let bottom = ground / CELL_HEIGHT * CELL_HEIGHT + CELL_HEIGHT - 1;
            let top = ground.saturating_sub(height - 1) / CELL_HEIGHT * CELL_HEIGHT;
            for side in [-1.0, 1.0] {
                let middle = centre + side * half * POST.2;
                let left = ((middle - thick as f64 / 2.0).round() as i64)
                    .div_euclid(CELL_WIDTH as i64)
                    * CELL_WIDTH as i64;
                for x in left..left + thick as i64 {
                    let Ok(x) = usize::try_from(x) else {
                        continue;
                    };
                    for y in top..=bottom {
                        canvas.set(x, y, self.palette.muted);
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
        canvas: &mut Canvas,
        stretches: &[Option<(usize, f64, f64)>],
        horizon: usize,
        eye: f64,
        now: Moment,
    ) {
        let road_rows = stretches.len() - 1;
        let across = canvas.width() as f64;
        let (wide, lines) = (CAR[0].len() as f64, CAR.len() as f64);
        let tall = lines * CAR_LINE as f64;
        for &car in self.traffic.iter().rev() {
            let car = car + now.traffic;
            let near = 1.0 / (car - now.travelled);
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
            let (cell_width, cell_height) = (CELL_WIDTH as i64, CELL_HEIGHT as i64);
            let cells_across = (wide * near / CELL_WIDTH as f64).ceil().max(1.0) as usize;
            let cells_down = (tall * near / CELL_HEIGHT as f64).ceil().max(1.0) as usize;
            let middle = centre - half * LANE;
            let left =
                ((middle - cells_across as f64) / CELL_WIDTH as f64).floor() as i64 * cell_width;
            let top = (ground as i64 / cell_height + 1 - cells_down as i64) * cell_height;
            for down in 0..cells_down {
                let line = CAR[((down as f64 + 0.5) * lines / cells_down as f64) as usize];
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
                    for y in 0..cell_height {
                        for x in 0..cell_width {
                            let (x, y) = (
                                left + cell_width * over as i64 + x,
                                top + cell_height * down as i64 + y,
                            );
                            if let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) {
                                canvas.set(x, y, colour);
                            }
                        }
                    }
                }
            }
        }
    }

    /// The car at the bottom of the pane in the right lane. In a curve its
    /// roof leans two pixels toward it and its upper body one.
    fn draw_car(&self, canvas: &mut Canvas, travelled: f64) {
        let (down, across) = (canvas.height(), canvas.width());
        let (wide, tall) = (CAR[0].len(), CAR.len() * CAR_LINE);
        if down < tall || across < wide {
            return;
        }
        let curve = self.curve(travelled + 2.0);
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
            let y = down - tall + line * CAR_LINE;
            for (offset, letter) in pattern.chars().enumerate() {
                let colour = match letter {
                    'B' => self.palette.iris,
                    'W' | 'T' => self.palette.base,
                    'L' => self.palette.love,
                    'P' => self.palette.text,
                    _ => continue,
                };
                let Ok(x) = usize::try_from(left + offset as i64 + shift) else {
                    continue;
                };
                for y in y..y + CAR_LINE {
                    canvas.set(x, y, colour);
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
        let reach = self.travelled + f64::from(area.height) * CELL_HEIGHT as f64 + EASE;
        self.lay(reach);
        let travelled = self.travelled;
        for car in &mut self.traffic {
            *car += SPEED * TRAFFIC;
        }
        // A car a quarter unit ahead stands far below the bottom row.
        self.traffic.retain(|car| car - travelled > 0.25);
        if travelled >= self.next_car {
            let down = usize::from(area.height) * CELL_HEIGHT;
            let road_rows = down.saturating_sub((down as f64 * HORIZON) as usize + 1);
            self.traffic
                .push(travelled + (road_rows.max(1) as f64).min(AHEAD));
            self.next_car = travelled + self.rng.random_range(TRAFFIC_GAP.0..TRAFFIC_GAP.1);
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.pixels().render(area, buf);
    }

    fn set_progress(&mut self, progress: f64) {
        self.progress = progress;
    }
}
