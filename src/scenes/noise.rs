use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::octant::{self, Canvas};
use crate::scene::{Cells, Rng, Scene};
use crate::theme::Palette;

/// Soft static: a field of gradient noise that drifts across the screen,
/// shaded from the base to the text.
#[derive(Debug)]
pub struct Noise {
    palette: &'static Palette,
    size: (u16, u16),
    canvas: Canvas,
    lattice: Lattice,
    shades: [Color; SHADES],
    ticks: u64,
    cells: Cells,
}

/// The shades the field takes, from the base to the text. Neighbouring
/// pixels then share a shade, so most cells hold one or two colours and
/// cost little to draw. About six shades to a stop keep each step too small to
/// see as a band.
const SHADES: usize = 24;

/// The share of the field that rests in the base, so the static does not
/// fill the screen.
const REST: f32 = 0.3;

/// The pixels between two points of the coarse lattice: the size of a
/// cloud.
const SCALE: f64 = 30.0;

/// Each octave of the field: its size against `SCALE`, its weight, and
/// the pixels it drifts each tick across and down. The octaves drift
/// apart, so the clouds change shape as they move and do not slide by as
/// one picture.
const OCTAVES: [(f64, f32, (f64, f64)); 2] = [(1.0, 1.0, (0.10, 0.04)), (0.5, 0.5, (-0.07, 0.11))];

/// How far the summed octaves reach from 0 either way, near enough that
/// the field spans the whole ramp and rarely clips.
const REACH: f32 = 0.9;

/// The lattice repeats after this many points either way, so an offset
/// wraps at it and keeps its precision on a long run.
const PERIOD: usize = 256;

const DIAGONAL: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// Eight directions, evenly round, for the gradient at each lattice point.
const GRADIENTS: [(f32, f32); 8] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    (DIAGONAL, DIAGONAL),
    (-DIAGONAL, DIAGONAL),
    (DIAGONAL, -DIAGONAL),
    (-DIAGONAL, -DIAGONAL),
];

/// A seeded order of the lattice points, which picks the gradient at each,
/// so a seed draws the same clouds each time.
#[derive(Debug)]
struct Lattice([u8; PERIOD]);

impl Lattice {
    fn new(rng: &mut Rng) -> Self {
        let mut order: [u8; PERIOD] = std::array::from_fn(|index| index as u8);
        for index in (1..PERIOD).rev() {
            order.swap(index, rng.random_range(0..=index));
        }
        Self(order)
    }

    fn gradient(&self, x: usize, y: usize, dx: f32, dy: f32) -> f32 {
        let hash = self.0[(usize::from(self.0[x % PERIOD]) + y) % PERIOD];
        let (gx, gy) = GRADIENTS[usize::from(hash) % GRADIENTS.len()];
        gx * dx + gy * dy
    }

    /// Gradient noise at a place in lattice points, from about -1 to 1.
    fn at(&self, across: Axis, down: Axis) -> f32 {
        let (left, top) = (across.point, down.point);
        let (dx, dy) = (across.offset, down.offset);
        let corners = [
            self.gradient(left, top, dx, dy),
            self.gradient(left + 1, top, dx - 1.0, dy),
            self.gradient(left, top + 1, dx, dy - 1.0),
            self.gradient(left + 1, top + 1, dx - 1.0, dy - 1.0),
        ];
        let upper = corners[0] + (corners[1] - corners[0]) * across.eased;
        let lower = corners[2] + (corners[3] - corners[2]) * across.eased;
        upper + (lower - upper) * down.eased
    }
}

/// A place along one side of the lattice: the lattice point before it, how
/// far past that point it lies, and that share eased by `fade`. The
/// pixels of a column share their place across, and the pixels of a row
/// their place down, so a tick works out each place once for each column
/// or row, not once for each pixel.
#[derive(Debug, Clone, Copy)]
struct Axis {
    point: usize,
    offset: f32,
    eased: f32,
}

impl Axis {
    /// The place `pixel` pixels along, on a lattice whose points lie
    /// `scale` pixels apart, moved by `drift` points. The drift is 0 or
    /// more, so the place is too.
    fn new(pixel: usize, scale: f64, drift: f64) -> Self {
        let place = pixel as f64 / scale + drift;
        let point = place.floor();
        let offset = (place - point) as f32;
        Self {
            point: point as usize,
            offset,
            eased: fade(offset),
        }
    }
}

/// Ken Perlin's curve, whose slope and bend are 0 at each lattice point,
/// so the field shows no crease along the lattice.
fn fade(share: f32) -> f32 {
    share * share * share * (share * (share * 6.0 - 15.0) + 10.0)
}

/// Each shade from the base to the text. The first stop of the ramp from
/// the base, overlay, blends in by coverage as a canvas blends it, so on a
/// see-through base its edge stays hard. The rest are mixes of theme
/// colours along overlay, muted, subtle and text.
fn shades(palette: &Palette) -> [Color; SHADES] {
    let stops = [palette.overlay, palette.muted, palette.subtle, palette.text];
    std::array::from_fn(|shade| {
        let place = shade as f32 / (SHADES - 1) as f32 * stops.len() as f32;
        let colour = octant::ramp(&stops, (place - 1.0) / (stops.len() - 1) as f32);
        octant::blend(palette, palette.base, colour, place)
    })
}

impl Noise {
    pub fn new(mut rng: Rng, palette: &'static Palette) -> Self {
        Self {
            palette,
            size: (0, 0),
            canvas: Canvas::new(0, 0, palette),
            lattice: Lattice::new(&mut rng),
            shades: shades(palette),
            ticks: 0,
            cells: Cells::default(),
        }
    }

    /// The shade of the summed octaves `field`.
    fn shade(&self, field: f32) -> Color {
        let level = (field / REACH + 1.0) / 2.0;
        let lit = ((level - REST) / (1.0 - REST)).clamp(0.0, 1.0);
        self.shades[((lit * SHADES as f32) as usize).min(SHADES - 1)]
    }
}

impl Scene for Noise {
    fn tick(&mut self, area: Rect) {
        if self.size != (area.width, area.height) {
            self.size = (area.width, area.height);
            self.canvas = Canvas::new(area.width, area.height, self.palette);
        }
        self.ticks += 1;
        let (width, height) = (self.canvas.width(), self.canvas.height());
        // Each place runs against the drift, so the clouds move with it,
        // and starts a period in, so no place falls below 0.
        let starts = OCTAVES.map(|(size, _, (across, down))| {
            let scale = SCALE * size;
            let wrap = |speed: f64| (self.ticks as f64 * speed / scale).rem_euclid(PERIOD as f64);
            (
                scale,
                PERIOD as f64 - wrap(across),
                PERIOD as f64 - wrap(down),
            )
        });
        let columns = starts.map(|(scale, left, _)| {
            (0..width)
                .map(|x| Axis::new(x, scale, left))
                .collect::<Vec<_>>()
        });
        let mut field = vec![0.0; width];
        for y in 0..height {
            field.fill(0.0);
            for ((columns, (scale, _, top)), (_, weight, _)) in
                columns.iter().zip(starts).zip(OCTAVES)
            {
                let down = Axis::new(y, scale, top);
                for (sum, across) in field.iter_mut().zip(columns) {
                    *sum += weight * self.lattice.at(*across, down);
                }
            }
            for (x, sum) in field.iter().enumerate() {
                self.canvas.set(x, y, self.shade(*sum));
            }
        }
        self.cells
            .draw(area, |cells| self.canvas.render(area, cells));
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.cells
            .render(area, buf, |buf| self.canvas.render(area, buf));
    }
}
