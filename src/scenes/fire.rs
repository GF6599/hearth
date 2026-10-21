use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::octant::{self, CELL_HEIGHT, CELL_WIDTH, Canvas};
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// Heat is kept finer than the colour steps, so the decay per row can be
/// small on a tall terminal and large on a short one, and the flames reach
/// about the same share of either.
pub const MAX_HEAT: u16 = 360;

/// The stops of the heat from cold to hot, each a role of the theme. The
/// shades between two stops are their mix. The base is not a stop: the
/// coldest heat is the first stop over the base by a share of the pixel,
/// so the canvas keeps that edge hard over a see-through base.
fn stops(palette: &Palette) -> Vec<Color> {
    let mut stops = vec![
        palette.overlay,
        palette.muted,
        palette.love,
        palette.rose,
        palette.gold,
        palette.text,
    ];
    // A light theme's text is its darkest role, so its fire ends at gold.
    if palette.is_light() {
        stops.pop();
    }
    stops
}

/// The rows of pixels the heat climbs in a tick: half a cell.
const CLIMB: usize = 2;

/// The shades from one stop to the next. The eye sees no step between two
/// mixes of theme colours a sixteenth apart, and a frame of few colours
/// costs the canvas less to pair into cells.
const SHADES: u32 = 16;

/// The colour of a heat and the share of the pixel it covers. From no heat
/// to the first stop the first stop covers more of the pixel, and above it
/// the colour runs from stop to stop and covers the whole pixel. The heats
/// share out evenly, and each stop's own heat gives the stop exactly.
fn flame(stops: &[Color], heat: u16) -> (Color, f32) {
    let max = u32::from(MAX_HEAT);
    let scaled = u32::from(heat.min(MAX_HEAT)) * stops.len() as u32;
    let (stop, share) = (
        (scaled / max) as usize,
        (scaled % max * SHADES / max) as f32 / SHADES as f32,
    );
    match stop {
        0 => (stops[0], share),
        _ if stop >= stops.len() => (stops[stops.len() - 1], 1.0),
        _ => (octant::mix(stops[stop - 1], stops[stop], share), 1.0),
    }
}

/// The colour a pixel of `heat` draws: the base at no heat, each stop at
/// its own heat, and the mix of the two nearest stops between them. The
/// canvas blends the colour over the base, so a see-through base keeps the
/// top of the fire hard.
pub fn color(palette: &Palette, heat: u16) -> Color {
    shade(palette, &stops(palette), heat)
}

fn shade(palette: &Palette, stops: &[Color], heat: u16) -> Color {
    let (colour, coverage) = flame(stops, heat);
    let mut canvas = Canvas::new(1, 1, palette);
    canvas.blend(0, 0, colour, coverage);
    canvas.get(0, 0).unwrap_or(palette.base)
}

#[derive(Debug)]
pub struct Fire {
    rng: Rng,
    palette: &'static Palette,
    width: usize,
    rows: usize,
    heat: Vec<u16>,
    /// The colour each heat draws over the base, read once, because a frame
    /// draws every pixel and a mix is slow.
    shades: Vec<Color>,
}

impl Fire {
    pub fn new(rng: Rng, palette: &'static Palette) -> Self {
        Self {
            rng,
            palette,
            width: 0,
            rows: 0,
            heat: Vec::new(),
            shades: {
                let stops = stops(palette);
                (0..=MAX_HEAT)
                    .map(|heat| shade(palette, &stops, heat))
                    .collect()
            },
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
        let mut canvas = Canvas::new(area.width, area.height, self.palette);
        for y in 0..self.rows.min(canvas.height()) {
            for x in 0..self.width.min(canvas.width()) {
                let heat = self.heat[y * self.width + x];
                canvas.set(x, y, self.shades[usize::from(heat.min(MAX_HEAT))]);
            }
        }
        canvas.render(area, buf);
    }
}
