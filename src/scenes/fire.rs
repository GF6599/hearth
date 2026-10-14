use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::scene::{Rng, Scene};
use crate::theme;

/// Heat is kept finer than the colour steps, so the decay per row can be
/// small on a tall terminal and large on a short one, and the flames reach
/// about the same share of either.
const MAX_HEAT: u16 = 360;

pub const LEVELS: u16 = 36;

const STOPS: [Color; 4] = [theme::BASE, theme::LOVE, theme::GOLD, theme::TEXT];

/// Draws a cell as two heat rows: the foreground is the upper one and the
/// background the lower one.
const HALF_BLOCK: char = '▀';

pub fn color(level: u16) -> Color {
    let level = level.min(LEVELS);
    let segments = u16::try_from(STOPS.len() - 1).unwrap_or(1);
    let span = LEVELS / segments;
    let index = (level / span).min(segments - 1);
    let within = level - index * span;
    mix(
        STOPS[usize::from(index)],
        STOPS[usize::from(index + 1)],
        within,
        span,
    )
}

fn mix(from: Color, to: Color, num: u16, den: u16) -> Color {
    let (Color::Rgb(r0, g0, b0), Color::Rgb(r1, g1, b1)) = (from, to) else {
        return to;
    };
    let channel = |a: u8, b: u8| {
        let a = i32::from(a);
        let b = i32::from(b);
        let blended = a + (b - a) * i32::from(num) / i32::from(den);
        u8::try_from(blended.clamp(0, 255)).unwrap_or(u8::MAX)
    };
    Color::Rgb(channel(r0, r1), channel(g0, g1), channel(b0, b1))
}

fn level(heat: u16) -> u16 {
    heat * LEVELS / MAX_HEAT
}

#[derive(Debug)]
pub struct Fire {
    rng: Rng,
    width: usize,
    rows: usize,
    heat: Vec<u16>,
}

impl Fire {
    pub fn new(rng: Rng) -> Self {
        Self {
            rng,
            width: 0,
            rows: 0,
            heat: Vec::new(),
        }
    }

    /// The largest decay per heat row. Its mean puts the flames at about
    /// three fifths of the height.
    fn max_decay(&self) -> u16 {
        let rows = u16::try_from(self.rows).unwrap_or(u16::MAX).max(1);
        (MAX_HEAT * 10 / (3 * rows)).max(1)
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
                    .set_fg(color(level(upper_heat)))
                    .set_bg(color(level(lower_heat)));
            }
        }
    }
}
