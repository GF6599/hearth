use std::cmp::Ordering;
use std::f64::consts::TAU;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

use crate::quadrant;
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// The shore, where the towers stand and the water starts, as a share of
/// the pane's height from the top.
const SHORE: f64 = 0.7;
/// The least and the most width of a tower, as shares of the pane's width.
const TOWER_WIDTH: (f64, f64) = (0.04, 0.11);
/// The least and the most height of a far tower, as shares of the height
/// from the top of the pane to the shore.
const FAR: (f64, f64) = (0.45, 0.85);
/// The least and the most height of a near tower, in the same shares.
const NEAR: (f64, f64) = (0.2, 0.6);
/// The share of towers with an antenna, and its least and most height in
/// pixels.
const ANTENNA: (f64, usize, usize) = (0.35, 4, 8);
/// The ticks of one loop of the night, about three minutes, from the
/// evening to the late night and back.
const NIGHT: f64 = 3000.0;
/// The share of windows lit late at night and in the evening.
const LIT: (f64, f64) = (0.12, 0.7);
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
/// The pixels a train moves in a tick. Even, so a carriage always covers
/// whole cells.
const TRAIN_SPEED: i64 = 2;
/// The ticks from one train to the next, at the least and the most.
const TRAIN_GAP: (u64, u64) = (300, 900);
/// How many stars the sky holds, at any size of pane.
const STARS: usize = 90;
/// The moon's radius as a share of the pane's height, and its middle as
/// shares of the pane's width and height. A pixel is twice as tall as
/// wide, so the moon spans twice its radius in pixels across.
const MOON: (f64, f64, f64) = (0.06, 0.8, 0.14);

/// A tower's columns and its top row, in pixels. Every edge is even, so a
/// tower covers whole cells and no cell holds a wall, a window and the
/// sky.
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
}

impl City {
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
        let line = shore.checked_sub(((down as f64 * LINE) as usize & !1).max(2))?;
        (line >= 6).then_some(line)
    }

    fn shore(down: usize) -> usize {
        (down as f64 * SHORE) as usize & !1
    }

    fn build(&mut self) {
        let across = usize::from(self.width) * 2;
        let down = usize::from(self.height) * 2;
        let shore = Self::shore(down);
        self.towers.clear();
        self.windows.clear();
        self.antennas.clear();
        if across == 0 || shore < 4 {
            return;
        }
        let far = self.layer(across, shore, FAR, false, 0);
        let start = 2 * self.rng.random_range(0..=3);
        let near = self.layer(across, shore, NEAR, true, start);
        self.towers = far.into_iter().chain(near).collect();
        let towers = self.towers.clone();
        for tower in &towers {
            if self.rng.random_bool(ANTENNA.0) {
                let height = self.rng.random_range(ANTENNA.1..=ANTENNA.2) & !1;
                if tower.top > height {
                    let period = self.rng.random_range(BLINK.1..=BLINK.2);
                    self.antennas.push(Antenna {
                        x: (tower.left + tower.width / 2) & !1,
                        light: tower.top - height - 1,
                        roof: tower.top,
                        near: tower.near,
                        period,
                        offset: self.rng.random_range(0..period),
                    });
                }
            }
        }
        for tower in &towers {
            for column in 0..(tower.width / 2).saturating_sub(1) {
                let x = tower.left + 1 + 2 * column;
                for y in (tower.top + 2..shore.saturating_sub(3)).step_by(2) {
                    if x + 1 < across && (tower.near || !self.hidden(x, y)) {
                        self.windows.push(Window {
                            x,
                            y,
                            lit: self.rng.random_bool(self.share()),
                            warm: self.rng.random_bool(WARM),
                        });
                    }
                }
            }
        }
    }

    /// Whether a near tower, or the cell of a near antenna, stands in front
    /// of a far window. A window in the cell of an antenna would make three
    /// colours, more than a quadrant cell draws.
    fn hidden(&self, x: usize, y: usize) -> bool {
        let tower = self.towers.iter().any(|front| {
            front.near && (front.left..front.left + front.width).contains(&x) && y >= front.top
        });
        let antenna = self
            .antennas
            .iter()
            .any(|front| front.near && x & !1 == front.x && (front.light..front.roof).contains(&y));
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
        let least = ((across as f64 * TOWER_WIDTH.0) as usize).max(4);
        let most = ((across as f64 * TOWER_WIDTH.1) as usize).max(least + 2);
        let mut towers = Vec::new();
        let mut left = start;
        while left < across {
            let width = self.rng.random_range(least..=most) & !1;
            let share = self.rng.random_range(heights.0..heights.1);
            let height = ((share * shore as f64) as usize & !1).max(2);
            towers.push(Tower {
                left,
                width,
                top: shore - height,
                near,
            });
            left += width + if self.rng.random_bool(0.3) { 2 } else { 0 };
        }
        towers
    }

    fn in_moon(x: usize, y: usize, across: usize, down: usize) -> bool {
        let (width, height) = (across as f64, down as f64);
        let (radius, centre, middle) = (MOON.0 * height, MOON.1 * width, MOON.2 * height);
        let (dx, dy) = ((x as f64 + 0.5 - centre) / 2.0, y as f64 + 0.5 - middle);
        dx * dx + dy * dy <= radius * radius
    }

    fn draw_moon(&self, rows: &mut [Vec<Color>], shore: usize) {
        let (across, down) = (rows.first().map_or(0, Vec::len), rows.len());
        for (y, row) in rows.iter_mut().enumerate().take(shore) {
            for (x, pixel) in row.iter_mut().enumerate() {
                if Self::in_moon(x, y, across, down) {
                    *pixel = self.palette.text;
                }
            }
        }
    }

    /// The stars, each only in a cell that nothing else touches, because a
    /// star beside a mast or the moon would make a third colour in its
    /// cell. The cell is judged by the shapes and not by the colours, as
    /// with the terminal's background a far wall reads as the sky.
    fn draw_stars(&self, rows: &mut [Vec<Color>], shore: usize) {
        let (across, down) = (rows.first().map_or(0, Vec::len), rows.len());
        let line = Self::line(down, shore).map_or(shore, |line| line - 4);
        let room = shore.saturating_sub(2) as f64;
        for &(x, y) in &self.stars {
            let (column, row) = ((x * across as f64) as usize, (y * room) as usize);
            let (left, top) = (column & !1, row & !1);
            let towers = self.towers.iter().any(|tower| {
                left < tower.left + tower.width && tower.left < left + 2 && top + 1 >= tower.top
            });
            let masts = self.antennas.iter().any(|antenna| {
                antenna.x & !1 == left && top + 1 >= antenna.light && top < antenna.roof
            });
            let moon =
                (top..top + 2).any(|y| (left..left + 2).any(|x| Self::in_moon(x, y, across, down)));
            if !towers && !masts && !moon && top + 1 < line {
                rows[row][column] = self.palette.subtle;
            }
        }
    }

    /// The far towers and their antennas, then the near ones in front.
    fn draw_towers(&self, rows: &mut [Vec<Color>], shore: usize) {
        for near in [false, true] {
            let wall = if near {
                self.palette.overlay
            } else {
                self.palette.surface
            };
            for tower in self.towers.iter().filter(|tower| tower.near == near) {
                for row in &mut rows[tower.top..shore] {
                    for pixel in row.iter_mut().skip(tower.left).take(tower.width) {
                        *pixel = wall;
                    }
                }
            }
            for antenna in self.antennas.iter().filter(|antenna| antenna.near == near) {
                for row in &mut rows[antenna.light + 1..antenna.roof] {
                    if let Some(pixel) = row.get_mut(antenna.x) {
                        *pixel = self.palette.muted;
                    }
                }
                // An unlit lamp keeps the mast's colour, so a far wall
                // behind it does not show through as it blinks.
                let on = (self.time + antenna.offset) % antenna.period < BLINK.0;
                if let Some(pixel) = rows[antenna.light].get_mut(antenna.x) {
                    *pixel = if on {
                        self.palette.love
                    } else {
                        self.palette.muted
                    };
                }
            }
        }
        for window in self.windows.iter().filter(|window| window.lit) {
            rows[window.y][window.x] = if window.warm {
                self.palette.rose
            } else {
                self.palette.gold
            };
        }
    }

    fn draw_line(&self, rows: &mut [Vec<Color>], line: usize, shore: usize) {
        let muted = self.palette.muted;
        for row in &mut rows[line..line + 2] {
            row.fill(muted);
        }
        for row in &mut rows[line + 2..shore] {
            for (x, pixel) in row.iter_mut().enumerate() {
                if x % PILLAR_GAP < 2 {
                    *pixel = muted;
                }
            }
        }
        let Some(train) = self.train else {
            return;
        };
        let car = (CARRIAGE.0 + CARRIAGE.1) as i64;
        for (down, row) in rows[line - 4..line].iter_mut().enumerate() {
            for (x, pixel) in row.iter_mut().enumerate() {
                let along = x as i64 - train.left;
                if along < 0 || along >= train.length() || along % car >= CARRIAGE.0 as i64 {
                    continue;
                }
                let inside = (2..CARRIAGE.0 as i64 - 2).contains(&(along % car));
                *pixel = if down == 1 && inside && along % 2 == 0 {
                    self.palette.gold
                } else {
                    muted
                };
            }
        }
    }

    /// The lit windows mirrored in the water, as far below the shore as
    /// they stand above it. Each row of the water slides to and fro by a
    /// ripple that grows with depth, and a band of rows that runs down the
    /// water breaks the reflection.
    fn draw_water(&self, rows: &mut [Vec<Color>], shore: usize) {
        let (sky, water) = rows.split_at_mut(shore);
        for (depth, row) in water.iter_mut().enumerate() {
            let Some(above) = shore.checked_sub(depth + 1) else {
                break;
            };
            if (depth as u64 / 2 + self.time / 8).is_multiple_of(4) {
                continue;
            }
            let reach = 1.0 + depth as f64 * 0.06;
            let wave = (self.time as f64 * 0.15 + depth as f64 * 0.7).sin();
            let ripple = (reach * wave).round() as i64;
            for (x, pixel) in row.iter_mut().enumerate() {
                let Some(colour) = usize::try_from(x as i64 - ripple)
                    .ok()
                    .and_then(|from| sky[above].get(from))
                else {
                    continue;
                };
                if *colour == self.palette.gold || *colour == self.palette.rose {
                    *pixel = *colour;
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
        let across = i64::from(self.width) * 2;
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

    fn pixels(&self) -> Vec<Vec<Color>> {
        let across = usize::from(self.width) * 2;
        let down = usize::from(self.height) * 2;
        let mut rows = vec![vec![self.palette.base; across]; down];
        let shore = Self::shore(down);
        if shore < 4 {
            return rows;
        }
        self.draw_moon(&mut rows, shore);
        self.draw_towers(&mut rows, shore);
        if let Some(line) = Self::line(down, shore) {
            self.draw_line(&mut rows, line, shore);
        }
        self.draw_stars(&mut rows, shore);
        self.draw_water(&mut rows, shore);
        rows
    }
}

impl Scene for City {
    fn tick(&mut self, area: Rect) {
        self.time += 1;
        if (area.width, area.height) != (self.width, self.height) {
            self.width = area.width;
            self.height = area.height;
            self.build();
        }
        self.turn_a_window();
        self.run_train();
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
