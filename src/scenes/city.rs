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
/// The share of windows lit, and the share of the lit ones in rose.
const LIT: f64 = 0.45;
const WARM: f64 = 0.2;
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
}

impl City {
    pub fn new(mut rng: Rng, palette: &'static Palette) -> Self {
        let stars = (0..STARS)
            .map(|_| (rng.random_range(0.0..1.0), rng.random_range(0.0..1.0)))
            .collect();
        Self {
            palette,
            rng,
            width: 0,
            height: 0,
            towers: Vec::new(),
            windows: Vec::new(),
            antennas: Vec::new(),
            stars,
        }
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
                    self.antennas.push(Antenna {
                        x: (tower.left + tower.width / 2) & !1,
                        light: tower.top - height - 1,
                        roof: tower.top,
                        near: tower.near,
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
                            lit: self.rng.random_bool(LIT),
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

    fn draw_sky(&self, rows: &mut [Vec<Color>], shore: usize) {
        let across = rows.first().map_or(0, Vec::len);
        let (width, height) = (across as f64, rows.len() as f64);
        let room = shore.saturating_sub(2) as f64;
        for &(x, y) in &self.stars {
            let (column, row) = ((x * width) as usize, (y * room) as usize);
            if let Some(pixel) = rows.get_mut(row).and_then(|row| row.get_mut(column)) {
                *pixel = self.palette.subtle;
            }
        }
        let (radius, centre, middle) = (MOON.0 * height, MOON.1 * width, MOON.2 * height);
        for (y, row) in rows.iter_mut().enumerate().take(shore) {
            for (x, pixel) in row.iter_mut().enumerate() {
                let (dx, dy) = ((x as f64 + 0.5 - centre) / 2.0, y as f64 + 0.5 - middle);
                if dx * dx + dy * dy <= radius * radius {
                    *pixel = self.palette.text;
                }
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
                if let Some(pixel) = rows[antenna.light].get_mut(antenna.x) {
                    *pixel = self.palette.love;
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

    /// The lit windows mirrored in the water, as far below the shore as
    /// they stand above it.
    fn draw_water(&self, rows: &mut [Vec<Color>], shore: usize) {
        let (sky, water) = rows.split_at_mut(shore);
        for (depth, row) in water.iter_mut().enumerate() {
            let Some(above) = shore.checked_sub(depth + 1) else {
                break;
            };
            for (pixel, colour) in row.iter_mut().zip(&sky[above]) {
                if *colour == self.palette.gold || *colour == self.palette.rose {
                    *pixel = *colour;
                }
            }
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
        self.draw_sky(&mut rows, shore);
        self.draw_towers(&mut rows, shore);
        self.draw_water(&mut rows, shore);
        rows
    }
}

impl Scene for City {
    fn tick(&mut self, area: Rect) {
        if (area.width, area.height) != (self.width, self.height) {
            self.width = area.width;
            self.height = area.height;
            self.build();
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
