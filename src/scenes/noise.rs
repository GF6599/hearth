use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// Static. It exists to prove the loop, the seed and the keys with nothing
/// to draw.
#[derive(Debug)]
pub struct Noise {
    rng: Rng,
    palette: &'static Palette,
    width: u16,
    cells: Vec<char>,
}

const GLYPHS: &[char] = &[' ', ' ', ' ', '.', ':', '-', '=', '+', '*'];

/// The chance that a cell draws a new glyph in a tick. A screen drawn
/// again whole each tick flickers too fast to rest the eye.
const RENEW: f64 = 0.5;

impl Noise {
    pub fn new(rng: Rng, palette: &'static Palette) -> Self {
        Self {
            rng,
            palette,
            width: 0,
            cells: Vec::new(),
        }
    }
}

impl Scene for Noise {
    fn tick(&mut self, area: Rect) {
        let count = usize::try_from(area.area()).unwrap_or(0);
        let resized = self.width != area.width || self.cells.len() != count;
        self.width = area.width;
        if resized {
            self.cells = vec![' '; count];
        }
        for cell in &mut self.cells {
            if resized || self.rng.random_bool(RENEW) {
                *cell = GLYPHS[self.rng.random_range(0..GLYPHS.len())];
            }
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if self.width == 0 {
            return;
        }
        for (index, glyph) in self.cells.iter().enumerate() {
            let x = index % usize::from(self.width);
            let y = index / usize::from(self.width);
            let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) else {
                continue;
            };
            if x >= area.width || y >= area.height {
                continue;
            }
            buf[(area.x + x, area.y + y)]
                .set_char(*glyph)
                .set_fg(self.palette.text);
        }
    }
}
