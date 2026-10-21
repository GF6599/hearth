use std::cmp::Ordering;
use std::f64::consts::TAU;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::octant::{CELL_HEIGHT, CELL_WIDTH, Canvas};
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// The shore, where the towers stand and the water starts, as a share of
/// the pane's height from the top.
const SHORE: f64 = 0.7;
/// The least and the most width of a tower, as shares of the pane's width.
const TOWER_WIDTH: (f64, f64) = (0.04, 0.11);
/// The least and the most height of a far tower, as shares of the height
/// from the top of the pane to the shore.
const FAR: (f64, f64) = (0.5, 0.85);
/// The least and the most height of a near tower, in the same shares.
/// The least keeps a near roof above the train, which would hide it.
const NEAR: (f64, f64) = (0.35, 0.65);
/// The least pixel rows by which two towers side by side differ in height,
/// so their roofs make a skyline and not one block.
const STEP: usize = 2 * CELL_HEIGHT;
/// The share of near towers with a gap after them, and of far ones.
const GAPS: (f64, f64) = (0.5, 0.3);
/// The share of towers with an antenna, and its least and most height in
/// pixels.
const ANTENNA: (f64, usize, usize) = (0.35, 2 * CELL_HEIGHT, 4 * CELL_HEIGHT);
/// The ticks of one loop of the night, about three minutes, from the
/// evening to the late night and back.
const NIGHT: f64 = 3000.0;
/// The share of windows lit late at night and in the evening. Under a
/// quarter, so the lit windows stand out from the grid of dark ones.
const LIT: (f64, f64) = (0.04, 0.25);
/// The share of the lit windows in rose.
const WARM: f64 = 0.2;
/// The chance in a tick that a window turns toward the share the night
/// asks for, and the chance that one turns either way when none has to.
/// One window at most turns in a tick, so the city changes one by one.
const TURN: (f64, f64) = (0.6, 0.05);
/// An antenna light is on for this many ticks in each of its periods, and
/// the least and the most ticks of a period.
const BLINK: (u64, u64, u64) = (3, 14, 24);
/// The raised line's top row, as a share of the pane's height above the
/// shore.
const LINE: f64 = 0.14;
/// The pixels from one pillar under the line to the next.
const PILLAR_GAP: usize = 28;
/// A carriage's length and the gap after it, in pixels, and the least and
/// the most carriages in a train.
const CARRIAGE: (usize, usize) = (20, 2);
const CARRIAGES: (usize, usize) = (3, 5);
/// The pixels a train moves in a tick. Even, so at each tick a carriage
/// covers whole cells.
const TRAIN_SPEED: i64 = 2;
/// The ticks from one train to the next, at the least and the most.
const TRAIN_GAP: (u64, u64) = (300, 900);
/// How many stars the sky holds, at any size of pane.
const STARS: usize = 90;
/// The moon's radius as a share of the pane's height, and its middle as
/// shares of the pane's width and height. A pixel is about square.
const MOON: (f64, f64, f64) = (0.06, 0.8, 0.14);
/// The pixel rows of a rim, a window, a lamp and the water line: half a
/// cell.
const THIN: usize = CELL_HEIGHT / 2;
/// The shades of the sky from the top of the pane to the horizon, and how
/// far the horizon leans from the base to iris, as the city lights the
/// night above it.
const SKY_SHADES: usize = 12;
const HORIZON: f32 = 0.2;
/// The steps of the moon's glow, as of a coverage the canvas tells apart,
/// the most the glow leans the sky to the moon, and how far it reaches as
/// a share of the moon's radius.
const GLOW_STEPS: f32 = 16.0;
const GLOW: f64 = 0.3;
const GLOW_REACH: f64 = 2.4;
/// The shades of a wall from its left to its right, the side of the moon,
/// and how far the right leans to the rim of a near wall and to the near
/// wall from a far one.
const WALL_SHADES: usize = 6;
const NEAR_LIGHT: f32 = 0.2;
const FAR_LIGHT: f32 = 0.4;
/// How far a lit window warms the wall of its cell toward its own light.
const WINDOW_GLOW: f32 = 0.12;
/// How far the ends of a streak in the water lean to its light, and the
/// ticks for which a streak keeps to one of the two rows of its window.
const STREAK_END: f32 = 0.45;
const SHIMMER: u64 = 3;

/// A tower's columns and its top row, in pixels. Its sides are the edges
/// of cells and its top row is the top or the middle of a cell, so no cell
/// holds a wall, a window and the sky.
#[derive(Debug, Clone, Copy)]
struct Tower {
    left: usize,
    width: usize,
    top: usize,
    near: bool,
}

#[derive(Debug, Clone, Copy)]
struct Window {
    x: usize,
    y: usize,
    lit: bool,
    warm: bool,
    near: bool,
}

/// The column of an antenna, the row of its light at the top, and the row
/// of the roof it stands on.
#[derive(Debug, Clone, Copy)]
struct Antenna {
    x: usize,
    light: usize,
    roof: usize,
    near: bool,
    period: u64,
    offset: u64,
}

#[derive(Debug, Clone, Copy)]
struct Train {
    left: i64,
    forward: bool,
    carriages: usize,
}

impl Train {
    fn length(self) -> i64 {
        (self.carriages * (CARRIAGE.0 + CARRIAGE.1) - CARRIAGE.1) as i64
    }
}

#[derive(Debug)]
pub struct City {
    palette: &'static Palette,
    rng: Rng,
    width: u16,
    height: u16,
    towers: Vec<Tower>,
    /// The windows that show, the far ones a near tower hides left out.
    windows: Vec<Window>,
    antennas: Vec<Antenna>,
    /// Each star's place, as shares of the pane's width and of the sky's
    /// height.
    stars: Vec<(f64, f64)>,
    time: u64,
    /// Where in the loop of the night the scene starts, as a share of it.
    dusk: f64,
    train: Option<Train>,
    next_train: u64,
    /// How far the scene has run from its last tick towards its next.
    progress: f64,
    shades: Shades,
    /// The sky with its moon, laid once for each pane.
    backdrop: Canvas,
}

/// Every colour the city draws from more than one role, mixed once from
/// the palette, so a frame mixes no colour.
#[derive(Debug)]
struct Shades {
    /// The sky from the top of the pane to the horizon.
    sky: [Color; SKY_SHADES],
    /// A far wall and a near one, from left to right.
    walls: [[Color; WALL_SHADES]; 2],
    /// A near wall in the cell of a gold and of a rose window, from left
    /// to right.
    glows: [[Color; WALL_SHADES]; 2],
    /// The ends of a gold and of a rose streak on the open water, as the
    /// canvas blends them, so they leave a see-through base clear.
    ends: [Color; 2],
}

impl Shades {
    fn new(palette: &Palette) -> Self {
        let step = |shade: usize| shade as f32 / (WALL_SHADES - 1) as f32;
        let near = std::array::from_fn(|shade| {
            blended(
                palette,
                palette.overlay,
                palette.pine,
                NEAR_LIGHT * step(shade),
            )
        });
        let far = std::array::from_fn(|shade| {
            blended(
                palette,
                palette.surface,
                palette.overlay,
                FAR_LIGHT * step(shade),
            )
        });
        let sky = std::array::from_fn(|shade| {
            let share = shade as f32 / (SKY_SHADES - 1) as f32;
            blended(palette, palette.base, palette.iris, HORIZON * share)
        });
        Self {
            sky,
            ends: [palette.gold, palette.rose]
                .map(|light| blended(palette, palette.base, light, STREAK_END)),
            walls: [far, near],
            glows: [palette.gold, palette.rose]
                .map(|light| near.map(|wall| blended(palette, wall, light, WINDOW_GLOW))),
        }
    }
}

/// The colour `over` gives a pixel of `under` at `share`, as the canvas
/// blends it. A theme over the terminal's background has a see-through
/// base, and its surface too, and the canvas never mixes either, so the
/// sky and the far walls stay the user's own background.
fn blended(palette: &Palette, under: Color, over: Color, share: f32) -> Color {
    let mut pixel = Canvas::new(1, 1, palette);
    pixel.set(0, 0, under);
    pixel.blend(0, 0, over, share);
    pixel.get(0, 0).unwrap_or(under)
}

impl City {
    /// The colour of the sky on pixel row `y` of the pane, lighter toward
    /// the horizon a cell row at a time, and the base from the shore down.
    pub fn sky(&self, y: usize) -> Color {
        let shore = Self::shore(usize::from(self.height) * CELL_HEIGHT);
        if y >= shore {
            return self.palette.base;
        }
        let lines = (shore / CELL_HEIGHT).max(2) - 1;
        let shade = (y / CELL_HEIGHT * (SKY_SHADES - 1) + lines / 2) / lines;
        self.shades.sky[shade.min(SKY_SHADES - 1)]
    }

    pub fn new(mut rng: Rng, palette: &'static Palette) -> Self {
        let stars = (0..STARS)
            .map(|_| (rng.random_range(0.0..1.0), rng.random_range(0.0..1.0)))
            .collect();
        let dusk = rng.random_range(0.0..1.0);
        let next_train = rng.random_range(TRAIN_GAP.0 / 5..TRAIN_GAP.0);
        Self {
            palette,
            rng,
            width: 0,
            height: 0,
            towers: Vec::new(),
            windows: Vec::new(),
            antennas: Vec::new(),
            stars,
            time: 0,
            dusk,
            train: None,
            next_train,
            progress: 0.0,
            shades: Shades::new(palette),
            backdrop: Canvas::new(0, 0, palette),
        }
    }

    /// The share of windows the night asks to be lit now.
    fn share(&self) -> f64 {
        let turn = (self.time as f64 / NIGHT + self.dusk).fract();
        LIT.0 + (LIT.1 - LIT.0) * (0.5 + 0.5 * (TAU * turn).cos())
    }

    /// The top row of the raised line, or none on a pane too short for a
    /// train above it.
    fn line(down: usize, shore: usize) -> Option<usize> {
        let height = (down as f64 * LINE) as usize / CELL_HEIGHT * CELL_HEIGHT;
        let line = shore.checked_sub(height.max(3 * CELL_HEIGHT))?;
        (line >= 3 * CELL_HEIGHT).then_some(line)
    }

    fn shore(down: usize) -> usize {
        (down as f64 * SHORE) as usize / CELL_HEIGHT * CELL_HEIGHT
    }

    fn build(&mut self) {
        let across = usize::from(self.width) * CELL_WIDTH;
        let down = usize::from(self.height) * CELL_HEIGHT;
        let shore = Self::shore(down);
        self.towers.clear();
        self.windows.clear();
        self.antennas.clear();
        if across == 0 || shore < 2 * CELL_HEIGHT {
            return;
        }
        let far = self.layer(across, shore, FAR, false, 0);
        let start = CELL_WIDTH * self.rng.random_range(0..=3);
        let near = self.layer(across, shore, NEAR, true, start);
        self.towers = far.into_iter().chain(near).collect();
        let towers = self.towers.clone();
        for tower in &towers {
            if self.rng.random_bool(ANTENNA.0) {
                let height = self.rng.random_range(ANTENNA.1..=ANTENNA.2) & !1;
                if tower.top >= height + CELL_HEIGHT {
                    let period = self.rng.random_range(BLINK.1..=BLINK.2);
                    // The lamp ends at the bottom of a cell, so a lit lamp
                    // shares its cell with the sky and not with the mast.
                    let light = (tower.top - height) / CELL_HEIGHT * CELL_HEIGHT - THIN;
                    self.antennas.push(Antenna {
                        x: (tower.left + tower.width / 2) / CELL_WIDTH * CELL_WIDTH,
                        light,
                        roof: tower.top,
                        near: tower.near,
                        period,
                        offset: self.rng.random_range(0..period),
                    });
                }
            }
        }
        for tower in &towers {
            // The first and last cells of a tower hold no window, so the
            // rim at its right edge never shares a cell with one.
            for column in 1..(tower.width / CELL_WIDTH).saturating_sub(1) {
                let x = tower.left + 1 + CELL_WIDTH * column;
                let bottom = shore.saturating_sub(3 * THIN);
                for y in (tower.top + CELL_HEIGHT..bottom).step_by(CELL_HEIGHT) {
                    if x + 1 < across && (tower.near || !self.hidden(x, y)) {
                        self.windows.push(Window {
                            x,
                            y,
                            lit: self.rng.random_bool(self.share()),
                            warm: self.rng.random_bool(WARM),
                            near: tower.near,
                        });
                    }
                }
            }
        }
    }

    /// Whether the cell of a near tower or of a near antenna holds a far
    /// window at row `y`. A window in such a cell would make three colours,
    /// more than a cell draws.
    fn hidden(&self, x: usize, y: usize) -> bool {
        let cell = y / CELL_HEIGHT * CELL_HEIGHT;
        let tower = self.towers.iter().any(|front| {
            front.near
                && (front.left..front.left + front.width).contains(&x)
                && cell + CELL_HEIGHT > front.top
        });
        let antenna = self.antennas.iter().any(|front| {
            front.near
                && x / CELL_WIDTH * CELL_WIDTH == front.x
                && (front.light / CELL_HEIGHT * CELL_HEIGHT..front.roof).contains(&y)
        });
        tower || antenna
    }

    /// One row of towers from `start` across the pane, each as tall as a
    /// share in `heights` of the sky over the shore.
    fn layer(
        &mut self,
        across: usize,
        shore: usize,
        heights: (f64, f64),
        near: bool,
        start: usize,
    ) -> Vec<Tower> {
        let least = ((across as f64 * TOWER_WIDTH.0) as usize).max(2 * CELL_WIDTH);
        let most = ((across as f64 * TOWER_WIDTH.1) as usize).max(least + CELL_WIDTH);
        let gap = if near { GAPS.0 } else { GAPS.1 };
        let mut towers: Vec<Tower> = Vec::new();
        let mut left = start;
        while left < across {
            let width = self.rng.random_range(least..=most) / CELL_WIDTH * CELL_WIDTH;
            let mut top = 0;
            for _ in 0..8 {
                let share = self.rng.random_range(heights.0..heights.1);
                let height = ((share * shore as f64) as usize / THIN * THIN).max(CELL_HEIGHT);
                top = shore - height;
                let beside = towers.last().filter(|last| last.left + last.width == left);
                if beside.is_none_or(|last| last.top.abs_diff(top) >= STEP) {
                    break;
                }
            }
            towers.push(Tower {
                left,
                width,
                top,
                near,
            });
            left += width
                + if self.rng.random_bool(gap) {
                    CELL_WIDTH
                } else {
                    0
                };
        }
        towers
    }

    /// Lays the sky and the moon of the pane. The moon has a soft rim and a
    /// glow that fades into the sky, each a mix of the sky toward the moon
    /// by a step of coverage.
    fn lay_backdrop(&mut self) {
        let mut canvas = Canvas::new(self.width, self.height, self.palette);
        let (across, down) = (canvas.width(), canvas.height());
        let shore = Self::shore(down);
        for y in 0..shore {
            let sky = self.sky(y);
            for x in 0..across {
                canvas.set(x, y, sky);
            }
        }
        let (width, height) = (across as f64, down as f64);
        let (radius, centre, middle) = (MOON.0 * height, MOON.1 * width, MOON.2 * height);
        // The canvas puts a pixel's middle on whole numbers, and the moon
        // puts it half a pixel in.
        let (x0, y0) = (centre - 0.5, middle - 0.5);
        let reach = radius * GLOW_REACH;
        // The disc on a canvas of its own gives the coverage of each pixel
        // of the moon's rim, read back by the colour each coverage blends.
        let mut disc = Canvas::new(self.width, self.height, self.palette);
        disc.disc((x0 as f32, y0 as f32), radius as f32, self.palette.text);
        let blends: Vec<Color> = (0..=GLOW_STEPS as usize)
            .map(|step| {
                let share = step as f32 / GLOW_STEPS;
                blended(self.palette, self.palette.base, self.palette.text, share)
            })
            .collect();
        // The glow takes one shade in each cell, from the distance of the
        // cell's middle, so a cell of glow holds one colour. A pixel of the
        // rim takes the more of its coverage and the glow, so the glow runs
        // into the moon with no dark ring between.
        for top in (0..shore).step_by(CELL_HEIGHT) {
            for left in (0..across).step_by(CELL_WIDTH) {
                let middle = (
                    left as f64 + (CELL_WIDTH - 1) as f64 / 2.0,
                    top as f64 + (CELL_HEIGHT - 1) as f64 / 2.0,
                );
                let distance = (middle.0 - x0).hypot(middle.1 - y0);
                if distance >= reach {
                    continue;
                }
                let fade = (1.0 - (distance - radius) / (reach - radius)).min(1.0);
                let glow = (GLOW * fade * fade * GLOW_STEPS as f64).round() as usize;
                for y in top..(top + CELL_HEIGHT).min(shore) {
                    for x in left..left + CELL_WIDTH {
                        let covered = disc
                            .get(x, y)
                            .and_then(|colour| blends.iter().rposition(|each| *each == colour))
                            .unwrap_or(0);
                        let share = covered.max(glow) as f32 / GLOW_STEPS;
                        canvas.blend(x, y, self.palette.text, share);
                    }
                }
            }
        }
        self.backdrop = canvas;
    }

    /// Gives the sky left in each cell of the moon's glow that a tower or a
    /// mast partly covers one colour, the colour most of it shows. The rim
    /// and the glow of the moon shade the sky a pixel at a time, and a cell
    /// with two shades of it and a wall holds three colours, of which the
    /// canvas keeps the pair nearest its pixels, so a pixel of the wall
    /// could take the moon's light.
    fn settle_moonlight(&self, canvas: &mut Canvas) {
        let (across, down) = (canvas.width(), canvas.height());
        let (radius, centre, middle) = (
            MOON.0 * down as f64,
            MOON.1 * across as f64,
            MOON.2 * down as f64,
        );
        let reach = radius * GLOW_REACH;
        let cells = |from: f64, to: f64, size: usize, count: usize| {
            let first = (from.max(0.0) as usize) / size * size;
            (first..(to.max(0.0) as usize + 1).min(count)).step_by(size)
        };
        for top in cells(middle - reach, middle + reach, CELL_HEIGHT, down) {
            for left in cells(centre - reach, centre + reach, CELL_WIDTH, across) {
                if !Self::in_glow(left, top, across, down) {
                    continue;
                }
                let at = |pixel: usize| (left + pixel % CELL_WIDTH, top + pixel / CELL_WIDTH);
                let sky: Vec<(usize, usize, Color)> = (0..CELL_WIDTH * CELL_HEIGHT)
                    .map(at)
                    .filter_map(|(x, y)| {
                        let colour = canvas.get(x, y)?;
                        (self.backdrop.get(x, y) == Some(colour)).then_some((x, y, colour))
                    })
                    .collect();
                if sky.is_empty() || sky.len() == CELL_WIDTH * CELL_HEIGHT {
                    continue;
                }
                let most = sky
                    .iter()
                    .map(|(_, _, colour)| *colour)
                    .max_by_key(|colour| sky.iter().filter(|(_, _, other)| other == colour).count())
                    .unwrap_or(self.palette.base);
                for (x, y, _) in sky {
                    canvas.set(x, y, most);
                }
            }
        }
    }

    /// Whether the cell with its top left pixel at (left, top) lies in the
    /// glow of the moon.
    fn in_glow(left: usize, top: usize, across: usize, down: usize) -> bool {
        let (width, height) = (across as f64, down as f64);
        let (radius, centre, middle) = (MOON.0 * height, MOON.1 * width, MOON.2 * height);
        let x = centre.clamp(left as f64, (left + CELL_WIDTH) as f64);
        let y = middle.clamp(top as f64, (top + CELL_HEIGHT) as f64);
        (centre - x).hypot(middle - y) < radius * GLOW_REACH
    }

    /// The stars, each only in a cell that nothing else touches, because a
    /// star beside a mast or the moon would make a third colour in its
    /// cell. The cell is judged by the shapes and not by the colours, as
    /// with the terminal's background a far wall reads as the sky.
    fn draw_stars(&self, canvas: &mut Canvas, shore: usize) {
        let (across, down) = (canvas.width(), canvas.height());
        let line = Self::line(down, shore).map_or(shore, |line| line - 2 * CELL_HEIGHT);
        let room = shore.saturating_sub(CELL_HEIGHT) as f64;
        for &(x, y) in &self.stars {
            let (column, row) = ((x * across as f64) as usize, (y * room) as usize);
            let (left, top) = (
                column / CELL_WIDTH * CELL_WIDTH,
                row / CELL_HEIGHT * CELL_HEIGHT,
            );
            let (right, bottom) = (left + CELL_WIDTH, top + CELL_HEIGHT);
            let towers = self.towers.iter().any(|tower| {
                left < tower.left + tower.width && tower.left < right && bottom > tower.top
            });
            let masts = self
                .antennas
                .iter()
                .any(|antenna| antenna.x == left && bottom > antenna.light && top < antenna.roof);
            let moon = Self::in_glow(left, top, across, down);
            if !towers && !masts && !moon && bottom <= line {
                canvas.set(column, row, self.palette.subtle);
            }
        }
    }

    /// The colour of `tower` at a pixel of it, without its windows. Each
    /// tower has a rim on its roof and on its right edge, the side of the
    /// moon, in a lighter tone than its wall, so towers side by side read
    /// apart.
    /// The wall lightens a cell column at a time toward the moon, so a cell
    /// keeps one shade of it.
    fn wall(&self, tower: &Tower, x: usize, y: usize) -> Color {
        let rim = if tower.near {
            self.palette.pine
        } else {
            self.palette.overlay
        };
        if y < tower.top + THIN || x == tower.left + tower.width - 1 {
            rim
        } else {
            self.shades.walls[usize::from(tower.near)][Self::wall_shade(tower, x)]
        }
    }

    fn wall_shade(tower: &Tower, x: usize) -> usize {
        let cells = (tower.width / CELL_WIDTH).max(2) - 1;
        (x - tower.left) / CELL_WIDTH * (WALL_SHADES - 1) / cells
    }

    /// The far towers and their antennas, then the near ones in front.
    fn draw_towers(&self, canvas: &mut Canvas, shore: usize) {
        for near in [false, true] {
            for tower in self.towers.iter().filter(|tower| tower.near == near) {
                for y in tower.top..shore {
                    for x in tower.left..tower.left + tower.width {
                        canvas.set(x, y, self.wall(tower, x, y));
                    }
                }
            }
            for antenna in self.antennas.iter().filter(|antenna| antenna.near == near) {
                for y in antenna.light + THIN..antenna.roof {
                    canvas.set(antenna.x, y, self.palette.muted);
                }
                // An unlit lamp keeps the mast's colour, so a far wall
                // behind it does not show through as it blinks.
                let on = (self.time + antenna.offset) % antenna.period < BLINK.0;
                let lamp = if on {
                    self.palette.love
                } else {
                    self.palette.muted
                };
                for y in antenna.light..antenna.light + THIN {
                    canvas.set(antenna.x, y, lamp);
                }
            }
        }
        for window in &self.windows {
            let colour = match (window.lit, window.warm, window.near) {
                (true, true, _) => self.palette.rose,
                (true, false, _) => self.palette.gold,
                (false, _, true) => self.palette.surface,
                (false, _, false) => continue,
            };
            // A lit near window warms the wall of its cell, so the cell
            // holds the window and its glow and no third colour.
            if window.lit && window.near {
                let tower = self.towers.iter().rev().find(|tower| {
                    tower.near && (tower.left..tower.left + tower.width).contains(&window.x)
                });
                if let Some(tower) = tower {
                    let glow = self.shades.glows[usize::from(window.warm)]
                        [Self::wall_shade(tower, window.x)];
                    let (left, top) = (
                        window.x / CELL_WIDTH * CELL_WIDTH,
                        window.y / CELL_HEIGHT * CELL_HEIGHT,
                    );
                    for y in top..top + CELL_HEIGHT {
                        for x in left..left + CELL_WIDTH {
                            canvas.set(x, y, glow);
                        }
                    }
                }
            }
            for y in window.y..window.y + THIN {
                canvas.set(window.x, y, colour);
            }
        }
    }

    fn draw_line(&self, canvas: &mut Canvas, line: usize, shore: usize) {
        let muted = self.palette.muted;
        let across = canvas.width();
        for y in line..line + CELL_HEIGHT {
            for x in 0..across {
                canvas.set(x, y, muted);
            }
        }
        for y in line + CELL_HEIGHT..shore {
            for x in (0..across).filter(|x| x % PILLAR_GAP < CELL_WIDTH) {
                canvas.set(x, y, muted);
            }
        }
        let Some(train) = self.train else {
            return;
        };
        // The train moves on a whole pixel at a time towards where the next
        // tick puts it, so it glides at an even pace at any rate.
        let glide = (TRAIN_SPEED as f64 * self.progress).floor() as i64;
        let left = train.left + if train.forward { glide } else { -glide };
        let car = (CARRIAGE.0 + CARRIAGE.1) as i64;
        let carries = |x: usize| {
            let along = x as i64 - left;
            (0..train.length()).contains(&along) && along % car < CARRIAGE.0 as i64
        };
        let top = line - 2 * CELL_HEIGHT;
        for y in top..line {
            for x in (0..across).filter(|x| carries(*x)) {
                let along = x as i64 - left;
                let inside = (2..CARRIAGE.0 as i64 - 2).contains(&(along % car));
                let window = (y - top) / THIN == 1;
                let colour = if window && inside && along % 2 == 0 {
                    self.palette.gold
                } else {
                    muted
                };
                canvas.set(x, y, colour);
            }
        }
        // Between ticks a carriage can cover half a cell at each end. The
        // other half shows the wall behind without its window, so the cell
        // holds two colours and the end of the carriage stays straight.
        for x in (0..across).step_by(CELL_WIDTH) {
            let behind = match (carries(x), carries(x + 1)) {
                (true, false) => x + 1,
                (false, true) => x,
                _ => continue,
            };
            for y in top..line {
                canvas.set(behind, y, self.behind(behind, y));
            }
        }
    }

    /// The colour of the nearest tower at a pixel, without its windows, or
    /// the sky where no tower stands. The near towers come last, so the
    /// first found from the end stands in front.
    fn behind(&self, x: usize, y: usize) -> Color {
        self.towers
            .iter()
            .rev()
            .find(|tower| (tower.left..tower.left + tower.width).contains(&x) && y >= tower.top)
            .map_or_else(
                || self.backdrop.get(x, y).unwrap_or(self.palette.base),
                |tower| self.wall(tower, x, y),
            )
    }

    /// The water line at the shore, and from a cell under the shore the near
    /// towers mirrored dim, as far below the shore's cell as they stand
    /// above it. The far towers are left out of the mirror, so the near
    /// roofs show their shapes in it. A lit window mirrors as a streak four
    /// pixels wide, with an end on each side that fades into the open water,
    /// on one of the two rows of its window, a row that turns now and then,
    /// so the light shimmers in thin lines. Each cell row of the water slides
    /// by whole cells with a ripple that grows with depth, so no cell holds a
    /// streak, the dim city and the water, and gaps that run along the rows
    /// break the streaks.
    fn draw_water(&self, canvas: &mut Canvas, shore: usize) {
        let (across, down) = (canvas.width(), canvas.height());
        for y in shore..shore + THIN {
            for x in 0..across {
                canvas.set(x, y, self.palette.muted);
            }
        }
        let water = shore + CELL_HEIGHT;
        for y in water..down {
            let Some(above) = (shore + water - 1).checked_sub(y) else {
                break;
            };
            let cell = y / CELL_HEIGHT;
            let cells_down = (cell - shore / CELL_HEIGHT) as f64;
            let reach = (0.4 + cells_down * 0.2).min(2.0);
            let wave = (self.time as f64 * 0.12 + cell as f64 * 0.9).sin();
            let ripple = CELL_WIDTH as i64 * (reach * wave).round() as i64;
            for x in 0..across {
                let Ok(from) = usize::try_from(x as i64 - ripple) else {
                    continue;
                };
                let near = self.towers.iter().any(|tower| {
                    tower.near
                        && (tower.left..tower.left + tower.width).contains(&from)
                        && above >= tower.top
                });
                if near {
                    canvas.set(x, y, self.palette.surface);
                }
            }
            for x in 0..across {
                let Some(colour) = canvas.get(x, above) else {
                    continue;
                };
                let lit = colour == self.palette.gold || colour == self.palette.rose;
                let gap = (x / 4 + cell * 3 + (self.time / 5) as usize).is_multiple_of(4);
                let shimmer = (y + (self.time / SHIMMER) as usize).is_multiple_of(2);
                if !lit || gap || !shimmer {
                    continue;
                }
                let at = x as i64 + ripple;
                for spread in -1..=2 {
                    if let Ok(x) = usize::try_from(at + spread) {
                        canvas.set(x, y, colour);
                    }
                }
                let end = self.shades.ends[usize::from(colour == self.palette.rose)];
                for x in [at - 2, at + 3] {
                    if let Ok(x) = usize::try_from(x)
                        && canvas.get(x, y) == Some(self.palette.base)
                    {
                        canvas.set(x, y, end);
                    }
                }
            }
        }
    }

    fn turn_a_window(&mut self) {
        let target = (self.share() * self.windows.len() as f64).round() as usize;
        let lit = self.windows.iter().filter(|window| window.lit).count();
        let on = match lit.cmp(&target) {
            Ordering::Less if self.rng.random_bool(TURN.0) => Some(true),
            Ordering::Greater if self.rng.random_bool(TURN.0) => Some(false),
            _ if self.rng.random_bool(TURN.1) => Some(self.rng.random_bool(0.5)),
            _ => None,
        };
        let Some(on) = on else {
            return;
        };
        let turnable: Vec<usize> = (0..self.windows.len())
            .filter(|index| self.windows[*index].lit != on)
            .collect();
        if !turnable.is_empty() {
            let index = turnable[self.rng.random_range(0..turnable.len())];
            self.windows[index].lit = on;
        }
    }

    fn run_train(&mut self) {
        let across = i64::from(self.width) * CELL_WIDTH as i64;
        if let Some(train) = &mut self.train {
            train.left += if train.forward {
                TRAIN_SPEED
            } else {
                -TRAIN_SPEED
            };
            let gone = if train.forward {
                train.left >= across
            } else {
                train.left + train.length() <= 0
            };
            if gone {
                self.train = None;
                self.next_train = self.time + self.rng.random_range(TRAIN_GAP.0..TRAIN_GAP.1);
            }
        } else if self.time >= self.next_train {
            let forward = self.rng.random_bool(0.5);
            let carriages = self.rng.random_range(CARRIAGES.0..=CARRIAGES.1);
            let mut train = Train {
                left: across,
                forward,
                carriages,
            };
            if forward {
                train.left = -train.length();
            }
            self.train = Some(train);
        }
    }

    fn pixels(&self) -> Canvas {
        let mut canvas = self.backdrop.clone();
        let down = canvas.height();
        let shore = Self::shore(down);
        if shore < 2 * CELL_HEIGHT {
            return canvas;
        }
        self.draw_towers(&mut canvas, shore);
        self.settle_moonlight(&mut canvas);
        if let Some(line) = Self::line(down, shore) {
            self.draw_line(&mut canvas, line, shore);
        }
        self.draw_stars(&mut canvas, shore);
        self.draw_water(&mut canvas, shore);
        canvas
    }
}

impl Scene for City {
    fn tick(&mut self, area: Rect) {
        self.time += 1;
        if (area.width, area.height) != (self.width, self.height) {
            self.width = area.width;
            self.height = area.height;
            self.build();
            self.lay_backdrop();
        }
        self.turn_a_window();
        self.run_train();
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.pixels().render(area, buf);
    }

    fn set_progress(&mut self, progress: f64) {
        self.progress = progress;
    }
}
