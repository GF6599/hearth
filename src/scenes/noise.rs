use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::octant::Canvas;
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// Static. It exists to prove the loop, the seed and the keys with nothing
/// to draw.
#[derive(Debug)]
pub struct Noise {
    rng: Rng,
    palette: &'static Palette,
    size: (u16, u16),
    canvas: Canvas,
}

/// The levels a pixel draws from, each as likely as the next. A third of
/// them rest in the base, so the static does not fill the screen, and the
/// rest climb in pairs from muted to text, the palette's ramp away from
/// the base in a light theme as in a dark one.
fn levels(palette: &Palette) -> [Color; 9] {
    [
        palette.base,
        palette.base,
        palette.base,
        palette.muted,
        palette.muted,
        palette.subtle,
        palette.subtle,
        palette.text,
        palette.text,
    ]
}

/// The chance that a pixel draws a new level in a tick. A screen drawn
/// again whole each tick flickers too fast to rest the eye.
const RENEW: f64 = 0.5;

impl Noise {
    pub fn new(rng: Rng, palette: &'static Palette) -> Self {
        Self {
            rng,
            palette,
            size: (0, 0),
            canvas: Canvas::new(0, 0, palette.base),
        }
    }
}

impl Scene for Noise {
    fn tick(&mut self, area: Rect) {
        let resized = self.size != (area.width, area.height);
        if resized {
            self.size = (area.width, area.height);
            self.canvas = Canvas::new(area.width, area.height, self.palette.base);
        }
        let levels = levels(self.palette);
        for y in 0..self.canvas.height() {
            for x in 0..self.canvas.width() {
                if resized || self.rng.random_bool(RENEW) {
                    let level = levels[self.rng.random_range(0..levels.len())];
                    self.canvas.set(x, y, level);
                }
            }
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.canvas.render(area, buf);
    }
}
