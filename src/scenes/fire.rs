use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::dither;
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

/// Draws a cell as two heat rows: the foreground is the upper one and the
/// background the lower one.
const HALF_BLOCK: char = '▀';

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
        let width = usize::from(area.width);
        let rows = usize::from(area.height) * 2;
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

    fn render(&self, area: Rect, buf: &mut Buffer) {
        for y in 0..area.height {
            let upper = usize::from(y) * 2;
            let lower = upper + 1;
            if lower >= self.rows {
                break;
            }
            for x in 0..area.width {
                let column = usize::from(x);
                if column >= self.width {
                    break;
                }
                let upper_heat = self.heat[upper * self.width + column];
                let lower_heat = self.heat[lower * self.width + column];
                buf[(area.x + x, area.y + y)]
                    .set_char(HALF_BLOCK)
                    .set_fg(color(self.palette, upper_heat, column, upper))
                    .set_bg(color(self.palette, lower_heat, column, lower));
            }
        }
    }
}
