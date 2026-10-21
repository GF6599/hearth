use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::dither;
use crate::octant::{CELL_HEIGHT, CELL_WIDTH, Canvas};
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// Heat is kept finer than the colour steps, so the decay per row can be
/// small on a tall terminal and large on a short one, and the flames reach
/// about the same share of either.
pub const MAX_HEAT: u16 = 360;

/// Palette colours only, from cold to hot. A blend between two stops would
/// make a colour that the palette does not have.
const fn stops(palette: &Palette) -> [Color; 7] {
    [
        palette.base,
        palette.overlay,
        palette.muted,
        palette.love,
        palette.rose,
        palette.gold,
        palette.text,
    ]
}

/// The rows of pixels the heat climbs in a tick: half a cell.
const CLIMB: usize = 2;

/// The stop for one pixel. Between two stops, the share of pixels that take
/// the hotter one follows how far the heat is from the cooler one.
pub fn color(palette: &Palette, heat: u16, x: usize, row: usize) -> Color {
    let all = stops(palette);
    // A light theme's text is its darkest role, so its fire ends at gold.
    let stops = if palette.is_light() {
        &all[..all.len() - 1]
    } else {
        &all[..]
    };
    let heat = usize::from(heat.min(MAX_HEAT));
    let shade = heat * (stops.len() - 1) * dither::SHADES / usize::from(MAX_HEAT);
    let hotter = dither::lights(shade % dither::SHADES, x, row);
    stops[shade / dither::SHADES + usize::from(hotter)]
}

#[derive(Debug)]
pub struct Fire {
    rng: Rng,
    palette: &'static Palette,
    width: usize,
    rows: usize,
    heat: Vec<u16>,
}

impl Fire {
    pub fn new(rng: Rng, palette: &'static Palette) -> Self {
        Self {
            rng,
            palette,
            width: 0,
            rows: 0,
            heat: Vec::new(),
        }
    }

    /// The largest decay per heat row. Its mean puts the flames at about
    /// three fifths of the height.
    fn max_decay(&self) -> u16 {
        let cap = usize::from(MAX_HEAT) * 10 / (3 * self.rows.max(1));
        u16::try_from(cap.max(1)).unwrap_or(MAX_HEAT)
    }
}

impl Scene for Fire {
    fn tick(&mut self, area: Rect) {
        let width = usize::from(area.width) * CELL_WIDTH;
        let rows = usize::from(area.height) * CELL_HEIGHT;
        if (width, rows) != (self.width, self.rows) {
            self.width = width;
            self.rows = rows;
            self.heat = vec![0; width * rows];
        }
        if width == 0 || rows == 0 {
            return;
        }
        let bottom = (rows - 1) * width;
        self.heat[bottom..bottom + width].fill(MAX_HEAT);

        let max_decay = self.max_decay();
        let last_column = width - 1;
        for _ in 0..CLIMB {
            for y in 0..rows - 1 {
                for x in 0..width {
                    let source_x = match self.rng.random_range(0..3) {
                        0 => x.saturating_sub(1),
                        1 => x,
                        _ => (x + 1).min(last_column),
                    };
                    let source = self.heat[(y + 1) * width + source_x];
                    let decay = self.rng.random_range(0..=max_decay);
                    self.heat[y * width + x] = source.saturating_sub(decay);
                }
            }
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let mut canvas = Canvas::new(area.width, area.height, self.palette.base);
        for y in 0..self.rows.min(canvas.height()) {
            for x in 0..self.width.min(canvas.width()) {
                let heat = self.heat[y * self.width + x];
                canvas.set(x, y, color(self.palette, heat, x, y));
            }
        }
        canvas.render(area, buf);
    }
}
