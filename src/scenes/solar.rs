use std::collections::BTreeMap;
use std::f32::consts::TAU;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

use crate::braille::{self, CELL_HEIGHT, CELL_WIDTH};
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// Each orbit's half width as a share of the outermost, Mercury to
/// Neptune. The real distances run from 0.4 to 30 times the Earth's, which
/// would put the inner four inside the sun at this scale, so the orbits are
/// spaced evenly instead.
const ORBITS: [f32; 8] = [0.2, 0.31, 0.43, 0.54, 0.66, 0.77, 0.89, 1.0];
/// Ticks for Mercury to go round, about eight seconds. The others take
/// longer by Kepler's third law on their shares, so Neptune takes about a
/// minute and a half.
const MERCURY_TICKS: f32 = 133.0;
/// The height of each orbit as a share of its width, for a view from above
/// at a slant.
const TILT: f32 = 0.5;
/// Dots between the outermost orbit and the edge of the pane.
const MARGIN: f32 = 2.0;
const SUN_RADIUS: f32 = 4.0;
/// Dots along an orbit from one drawn dot to the next.
const ORBIT_GAP: f32 = 3.0;

/// What a cell shows when more than one thing falls in it, from the
/// lowest. One braille cell holds one colour, so a cell shows the dots of
/// its highest layer alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Layer {
    Orbit,
    Planet,
    Sun,
}

#[derive(Debug)]
pub struct Solar {
    palette: &'static Palette,
    width: u16,
    height: u16,
    time: u32,
    /// Where each planet starts on its orbit, as an angle.
    phases: [f32; ORBITS.len()],
}

impl Solar {
    pub fn new(mut rng: Rng, palette: &'static Palette) -> Self {
        Self {
            palette,
            width: 0,
            height: 0,
            time: 0,
            phases: std::array::from_fn(|_| rng.random_range(0.0..TAU)),
        }
    }

    /// Each planet's middle, Mercury first, in dots from the top left of
    /// the pane.
    pub fn planets(&self) -> Vec<(f32, f32)> {
        let (centre, across) = self.frame();
        ORBITS
            .iter()
            .zip(self.phases)
            .map(|(share, phase)| {
                let period = MERCURY_TICKS * (share / ORBITS[0]).powf(1.5);
                let angle = phase + TAU * self.time as f32 / period;
                place(centre, across * share, angle)
            })
            .collect()
    }

    /// The middle of the pane and the half width of the outermost orbit, in
    /// dots. The orbit fits the width or the height, whichever is tighter.
    fn frame(&self) -> ((f32, f32), f32) {
        let width = f32::from(self.width) * CELL_WIDTH as f32;
        let height = f32::from(self.height) * CELL_HEIGHT as f32;
        let centre = ((width - 1.0) / 2.0, (height - 1.0) / 2.0);
        let across = (width / 2.0 - MARGIN).min((height / 2.0 - MARGIN) / TILT);
        (centre, across.max(0.0))
    }
}

/// The place at `angle` on the orbit `across` dots wide each side of the
/// centre. The angle grows the way the planets go, against the clock seen
/// from above, so a planet crosses the near side, the bottom of the pane,
/// from left to right.
fn place(centre: (f32, f32), across: f32, angle: f32) -> (f32, f32) {
    (
        centre.0 + across * angle.cos(),
        centre.1 - across * TILT * angle.sin(),
    )
}

impl Scene for Solar {
    fn tick(&mut self, area: Rect) {
        self.width = area.width;
        self.height = area.height;
        self.time = self.time.wrapping_add(1);
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let (centre, across) = self.frame();
        let mut cells: BTreeMap<(i32, i32), (Layer, u32, Color)> = BTreeMap::new();
        let mut plot = |x: i32, y: i32, layer: Layer, colour: Color| {
            if x < 0 || y < 0 {
                return;
            }
            let cell = (x / CELL_WIDTH as i32, y / CELL_HEIGHT as i32);
            let bit = braille::dot(
                (x % CELL_WIDTH as i32) as usize,
                (y % CELL_HEIGHT as i32) as usize,
            );
            let entry = cells.entry(cell).or_insert((layer, 0, colour));
            if layer > entry.0 {
                *entry = (layer, bit, colour);
            } else if layer == entry.0 {
                entry.1 |= bit;
            }
        };

        for share in ORBITS {
            let reach = across * share;
            let length = TAU * reach;
            let steps = (length / ORBIT_GAP).max(1.0) as usize;
            for step in 0..steps {
                let (x, y) = place(centre, reach, TAU * step as f32 / steps as f32);
                plot(
                    x.round() as i32,
                    y.round() as i32,
                    Layer::Orbit,
                    self.palette.muted,
                );
            }
        }
        for (x, y) in self.planets() {
            let (left, top) = ((x - 0.5).floor() as i32, (y - 0.5).floor() as i32);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                plot(left + dx, top + dy, Layer::Planet, self.palette.text);
            }
        }
        let reach = SUN_RADIUS.ceil() as i32;
        let (middle_x, middle_y) = (centre.0.round() as i32, centre.1.round() as i32);
        for y in middle_y - reach..=middle_y + reach {
            for x in middle_x - reach..=middle_x + reach {
                let (dx, dy) = (x as f32 - centre.0, y as f32 - centre.1);
                if dx * dx + dy * dy <= SUN_RADIUS * SUN_RADIUS {
                    plot(x, y, Layer::Sun, self.palette.gold);
                }
            }
        }

        for ((column, row), (_, dots, colour)) in cells {
            let (Ok(column), Ok(row)) = (u16::try_from(column), u16::try_from(row)) else {
                continue;
            };
            if column >= area.width || row >= area.height {
                continue;
            }
            buf[Position::new(area.x + column, area.y + row)]
                .set_char(braille::glyph(dots))
                .set_fg(colour);
        }
    }
}
