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
        self.width = area.width;
        self.cells = (0..area.area())
            .map(|_| GLYPHS[self.rng.random_range(0..GLYPHS.len())])
            .collect();
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
