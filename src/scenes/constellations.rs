use std::ops::RangeInclusive;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::scene::{Rng, Scene};
use crate::theme::Palette;

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
pub struct Constellations {
    rng: Rng,
    palette: &'static Palette,
    width: u16,
    height: u16,
    stars: Vec<Star>,
}

impl Constellations {
    pub fn new(rng: Rng, palette: &'static Palette) -> Self {
        Self {
            rng,
            palette,
            width: 0,
            height: 0,
            stars: Vec::new(),
        }
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
}

impl Scene for Constellations {
    fn tick(&mut self, area: Rect) {
        if (area.width, area.height) != (self.width, self.height) {
            self.width = area.width;
            self.height = area.height;
            self.scatter();
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
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        for star in &self.stars {
            if star.x >= area.width || star.y >= area.height {
                continue;
            }
            let brightness = star.shown();
            buf[(area.x + star.x, area.y + star.y)]
                .set_char(GLYPHS[brightness])
                .set_fg(self.role(brightness));
        }
    }
}
