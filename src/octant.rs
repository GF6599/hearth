//! Eight pixels in one cell, two across and four down, drawn as an octant
//! glyph and two colours.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::ops::Range;
use std::sync::LazyLock;

use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

use crate::theme::Palette;

/// An octant cell is two pixels wide and four tall.
pub const CELL_WIDTH: usize = 2;
pub const CELL_HEIGHT: usize = 4;

/// The patterns that Unicode draws with an older block glyph, so the
/// octant block leaves them out. The octant block holds every other
/// pattern in the order of its bits.
const BLOCKS: [(u8, u32); 26] = [
    (0x00, 0x0020),
    (0x01, 0x1cea8),
    (0x02, 0x1ceab),
    (0x03, 0x1fb82),
    (0x05, 0x2598),
    (0x0a, 0x259d),
    (0x0f, 0x2580),
    (0x14, 0x1fbe6),
    (0x28, 0x1fbe7),
    (0x3f, 0x1fb85),
    (0x40, 0x1cea3),
    (0x50, 0x2596),
    (0x55, 0x258c),
    (0x5a, 0x259e),
    (0x5f, 0x259b),
    (0x80, 0x1cea0),
    (0xa0, 0x2597),
    (0xa5, 0x259a),
    (0xaa, 0x2590),
    (0xaf, 0x259c),
    (0xc0, 0x2582),
    (0xf0, 0x2584),
    (0xf5, 0x2599),
    (0xfa, 0x259f),
    (0xfc, 0x2586),
    (0xff, 0x2588),
];

const OCTANTS: u32 = 0x1cd00;

const GLYPHS: [u32; 256] = glyphs();

const fn glyphs() -> [u32; 256] {
    let mut glyphs = [0; 256];
    let mut next = OCTANTS;
    let mut bits = 0;
    while bits < glyphs.len() {
        let mut block = 0;
        while block < BLOCKS.len() && BLOCKS[block].0 as usize != bits {
            block += 1;
        }
        glyphs[bits] = if block < BLOCKS.len() {
            BLOCKS[block].1
        } else {
            next += 1;
            next - 1
        };
        bits += 1;
    }
    glyphs
}

/// The glyph that fills the pixels of `bits`. The pixel at `row` from the
/// top and `column` from the left of a cell is bit
/// `row * CELL_WIDTH + column`, and a set bit draws in the foreground.
pub fn glyph(bits: u8) -> char {
    char::from_u32(GLYPHS[usize::from(bits)]).unwrap_or(' ')
}

/// The glyph that fills the pixels `glyph` leaves out, or none for a glyph
/// that is not in the table. With its colours swapped it draws the same
/// cell.
pub fn complement(glyph: &str) -> Option<char> {
    let index = GLYPHS
        .iter()
        .position(|point| glyph.chars().map(u32::from).eq([*point]))?;
    let bits = u8::try_from(index).ok()?;
    Some(self::glyph(!bits))
}

/// Gives a cell in `base` the terminal's own background. A block glyph
/// drawn in `base` turns over, so the other colour stays in front and the
/// picture holds.
pub fn clear(cell: &mut Cell, base: Color) {
    let turned = (cell.fg == base)
        .then(|| complement(cell.symbol()))
        .flatten();
    if let Some(glyph) = turned {
        let front = if cell.bg == base {
            Color::Reset
        } else {
            cell.bg
        };
        let symbol = if front == Color::Reset { ' ' } else { glyph };
        cell.set_char(symbol).set_fg(front).set_bg(Color::Reset);
    } else if cell.bg == base {
        cell.set_bg(Color::Reset);
    }
}

/// Each colour of `pixels` once, in the order of its first pixel, with the
/// count of its pixels, and how many colours there are.
fn tally(pixels: [Color; 8]) -> ([(Color, u8); 8], usize) {
    let mut counts = [(Color::Reset, 0); 8];
    let mut colours = 0;
    for colour in pixels {
        match counts[..colours]
            .iter_mut()
            .find(|(each, _)| *each == colour)
        {
            Some((_, count)) => *count += 1,
            None => {
                counts[colours] = (colour, 1);
                colours += 1;
            }
        }
    }
    (counts, colours)
}

/// `pixels` in two colours of `counts`, the colours of the cell: the pair
/// whose total OKLab distance from every pixel to the nearer of the two is
/// least, with each pixel in the nearer. Of two pairs with the same total,
/// the pair whose colours come first wins, and a pixel as near both takes
/// the first, so a seed draws the same frame each time. A colour with no
/// RGB value lies infinitely far from every other.
fn nearest_pair(pixels: [Color; 8], counts: &[(Color, u8)], cache: &mut Labs) -> [Color; 8] {
    let labs: [Option<Lab>; 8] =
        std::array::from_fn(|index| counts.get(index).and_then(|(colour, _)| cache.of(*colour)));
    // Row `one` holds what each colour of the cell costs to draw in colour
    // `one`: its distance to it times its pixels, and 0 past the colours
    // the cell holds. A pair then costs the smaller of its two rows in each
    // place, so the search over pairs needs no multiplication.
    let mut costs = [[0.0_f32; 8]; 8];
    for one in 0..counts.len() {
        for other in one + 1..counts.len() {
            let gap = match (labs[one], labs[other]) {
                (Some(Lab(a)), Some(Lab(b))) => {
                    let squares: f32 = (0..3).map(|axis| (a[axis] - b[axis]).powi(2)).sum();
                    squares.sqrt()
                }
                _ => f32::INFINITY,
            };
            costs[one][other] = f32::from(counts[other].1) * gap;
            costs[other][one] = f32::from(counts[one].1) * gap;
        }
    }
    let mut best = (0, 1);
    let mut least = f32::INFINITY;
    for one in 0..counts.len() {
        for other in one + 1..counts.len() {
            let total: f32 = (0..8)
                .map(|each| costs[one][each].min(costs[other][each]))
                .sum();
            if total < least {
                (best, least) = ((one, other), total);
            }
        }
    }
    let (one, other) = best;
    pixels.map(|colour| {
        let each = counts
            .iter()
            .position(|(held, _)| *held == colour)
            .unwrap_or(one);
        if costs[one][each] <= costs[other][each] {
            counts[one].0
        } else {
            counts[other].0
        }
    })
}

/// The glyph, foreground and background for the pixels of one cell, in the
/// order of the glyph's bits. A glyph holds two colours. A cell of more
/// first takes the pair of its colours that lies nearest its pixels, with
/// each pixel in the nearer of the two. Of the two colours, the more common
/// goes in front, but `base` stays behind, so it can clear to the
/// terminal's own background. Of two colours with the same count, the one
/// with the first pixel goes first, so a seed draws the same frame each
/// time.
fn cell(pixels: [Color; 8], base: Color, cache: &mut Labs) -> (char, Color, Color) {
    let (mut counts, mut colours) = tally(pixels);
    let pixels = if colours > 2 {
        let paired = nearest_pair(pixels, &counts[..colours], cache);
        (counts, colours) = tally(paired);
        paired
    } else {
        pixels
    };
    let counts = &mut counts[..colours];
    counts.sort_by_key(|(_, count)| Reverse(*count));
    let first = counts.first().map_or(base, |(colour, _)| *colour);
    let Some(&(second, _)) = counts.get(1) else {
        return (glyph(0), first, first);
    };
    let (fore, back) = if first == base {
        (second, first)
    } else {
        (first, second)
    };
    let bits = pixels
        .iter()
        .enumerate()
        .filter(|(_, colour)| **colour != back)
        .fold(0, |bits, (pixel, _)| bits | 1 << pixel);
    (glyph(bits), fore, back)
}

/// A colour in OKLab, Björn Ottosson's space where an even step in each
/// coordinate looks an even step to the eye.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Lab([f32; 3]);

/// Each 8 bit sRGB channel as linear light, so a cell reads each colour
/// with three table reads and not three powers.
static LINEAR: LazyLock<[f32; 256]> = LazyLock::new(|| {
    std::array::from_fn(|channel| {
        let value = channel as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    })
});

fn encode(linear: f32) -> u8 {
    let value = if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

impl Lab {
    /// The colour in OKLab, or none for a colour with no RGB value, such as
    /// the terminal's own background.
    fn of(colour: Color) -> Option<Self> {
        let Color::Rgb(r, g, b) = colour else {
            return None;
        };
        let [r, g, b] = [r, g, b].map(|channel| LINEAR[usize::from(channel)]);
        let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
        let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
        let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
        Some(Self([
            0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
            1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
            0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
        ]))
    }

    fn rgb(self) -> Color {
        let Self([lightness, a, b]) = self;
        let l = (lightness + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
        let m = (lightness - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
        let s = (lightness - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
        Color::Rgb(
            encode(4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s),
            encode(-1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s),
            encode(-0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s),
        )
    }

    fn towards(self, other: Self, share: f32) -> Self {
        Self(std::array::from_fn(|axis| {
            self.0[axis] + (other.0[axis] - self.0[axis]) * share
        }))
    }
}

/// The colours a render has read in OKLab, so a frame takes the cube roots
/// of each colour once and not once for each cell. A colour whose slot
/// another holds is read again, so the cache needs no heap.
#[derive(Debug)]
struct Labs([Option<(Color, Lab)>; 256]);

impl Labs {
    fn new() -> Self {
        Self([None; 256])
    }

    fn of(&mut self, colour: Color) -> Option<Lab> {
        let Color::Rgb(r, g, b) = colour else {
            return None;
        };
        let slot = usize::from(r ^ g.rotate_left(3) ^ b.rotate_left(5));
        match self.0[slot] {
            Some((held, lab)) if held == colour => Some(lab),
            _ => {
                let lab = Lab::of(colour)?;
                self.0[slot] = Some((colour, lab));
                Some(lab)
            }
        }
    }
}

/// The colour `share` of the way from `from` to `to` in OKLab, so each step
/// of a shade looks as large as the last. A share of 0 or less gives `from`
/// and 1 or more gives `to`, each exactly. A colour with no RGB value, such
/// as `Color::Reset`, has nothing to mix with, so the mix is the nearer end:
/// `from` below a half and `to` from a half on.
pub fn mix(from: Color, to: Color, share: f32) -> Color {
    if share <= 0.0 {
        return from;
    }
    if share >= 1.0 {
        return to;
    }
    match (Lab::of(from), Lab::of(to)) {
        (Some(near), Some(far)) => near.towards(far, share).rgb(),
        _ if share < 0.5 => from,
        _ => to,
    }
}

/// The colour `over` gives a pixel of `under` at `share`, as a canvas of
/// `palette` blends it. Where the palette's base is see-through and either
/// colour is the base, the colour is the nearer end and never a mix, so a
/// shade worked out ahead of time keeps the canvas's hard edge.
pub fn blend(palette: &Palette, under: Color, over: Color, share: f32) -> Color {
    blend_on(palette.base, palette.see_through, under, over, share)
}

fn blend_on(base: Color, see_through: bool, under: Color, over: Color, share: f32) -> Color {
    if see_through && (under == base || over == base) {
        if share >= 0.5 { over } else { under }
    } else {
        mix(under, over, share)
    }
}

/// The colour at `at` along `stops`, with the first stop at 0, the last at
/// 1, and the rest evenly between. Between two stops the colour is their
/// mix. A place past either end takes that end's stop, and no stops give
/// `Color::Reset`.
pub fn ramp(stops: &[Color], at: f32) -> Color {
    let Some(last) = stops.len().checked_sub(1) else {
        return Color::Reset;
    };
    if last == 0 {
        return stops[0];
    }
    let place = at.clamp(0.0, 1.0) * last as f32;
    let below = (place as usize).min(last - 1);
    mix(stops[below], stops[below + 1], place - below as f32)
}

/// What can lie on a pixel, ordered from the lowest layer to the highest.
pub trait Layer: Ord + Copy {
    /// Whether a pixel of this layer shows the colour of `top`, the highest
    /// layer in its cell, rather than the colour behind.
    fn joins(self, _top: Self) -> bool {
        false
    }
}

/// The colour of each pixel of one cell, in the order of the glyph's bits,
/// from the layer and the colour that lie on it. A cell shows two colours:
/// the colour of its highest layer in front, and behind it `empty` where
/// any pixel is empty, or else the colour most of the rest takes, the
/// first pixel's of two with the same count. A pixel of a third colour
/// takes the colour behind, so an empty pixel never draws in an ink. A
/// pixel whose layer joins the highest takes the colour in front instead.
pub fn settle<L: Layer>(pixels: [Option<(L, Color)>; 8], empty: Color) -> [Color; 8] {
    let Some((top, front)) = pixels
        .iter()
        .flatten()
        .max_by_key(|(layer, _)| *layer)
        .copied()
    else {
        return [empty; 8];
    };
    let colours = pixels.map(|pixel| pixel.map_or(empty, |(_, colour)| colour));
    let behind = if pixels.contains(&None) {
        empty
    } else {
        colours
            .iter()
            .filter(|colour| **colour != front)
            .max_by_key(|colour| {
                let count = colours.iter().filter(|other| other == colour).count();
                let first = colours.iter().position(|other| other == *colour);
                (count, Reverse(first))
            })
            .copied()
            .unwrap_or(empty)
    };
    pixels.map(|pixel| match pixel {
        Some((_, colour)) if colour == front => front,
        Some((layer, _)) if layer.joins(top) => front,
        _ => behind,
    })
}

/// The points across each side of a pixel that sample its coverage. Four
/// give 17 steps of coverage, enough that the step between two is not seen
/// in a mix of two theme colours.
const SAMPLES: usize = 4;

/// Where each sample lies from the centre of its pixel, across or down.
const OFFSETS: [f32; SAMPLES] = [-0.375, -0.125, 0.125, 0.375];

/// The furthest a sample lies from the centre of its pixel.
const REACH: f32 = 0.375 * std::f32::consts::SQRT_2;

/// Every pixel along one side that a shape from `low` to `high` can touch,
/// cut to the `count` pixels of the canvas, so a shape past an edge draws
/// only the part inside.
fn span(low: f32, high: f32, count: usize) -> Range<usize> {
    let start = (low - 1.0).floor().max(0.0) as usize;
    let end = (high + 2.0).ceil().max(0.0) as usize;
    start.min(count)..end.min(count)
}

/// The distance from `point` to the nearest point of the segment from
/// `from` to `to`.
fn distance(point: (f32, f32), from: (f32, f32), to: (f32, f32)) -> f32 {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = dx * dx + dy * dy;
    // A segment of no length is a point, and the share along it would be
    // 0 / 0.
    let along = if length == 0.0 {
        0.0
    } else {
        (((point.0 - from.0) * dx + (point.1 - from.1) * dy) / length).clamp(0.0, 1.0)
    };
    (point.0 - from.0 - along * dx).hypot(point.1 - from.1 - along * dy)
}

/// The pixels of a frame, `CELL_WIDTH` across and `CELL_HEIGHT` down in
/// each cell, that draw into a buffer one glyph to a cell.
#[derive(Debug, Clone)]
pub struct Canvas {
    columns: u16,
    rows: u16,
    base: Color,
    see_through: bool,
    pixels: Vec<Color>,
}

impl Canvas {
    /// A canvas of `columns` by `rows` cells with every pixel in the base
    /// of `palette`. The cells keep the base behind them where they can, so
    /// it clears to the terminal's own background. Where the palette's base
    /// is see-through, the canvas never mixes a colour with the base.
    pub fn new(columns: u16, rows: u16, palette: &Palette) -> Self {
        let size = usize::from(columns) * CELL_WIDTH * usize::from(rows) * CELL_HEIGHT;
        Self {
            columns,
            rows,
            base: palette.base,
            see_through: palette.see_through,
            pixels: vec![palette.base; size],
        }
    }

    /// The pixels across.
    pub fn width(&self) -> usize {
        usize::from(self.columns) * CELL_WIDTH
    }

    /// The pixels down.
    pub fn height(&self) -> usize {
        usize::from(self.rows) * CELL_HEIGHT
    }

    fn index(&self, x: usize, y: usize) -> Option<usize> {
        (x < self.width() && y < self.height()).then(|| y * self.width() + x)
    }

    /// The colour of the pixel at (x, y), or none past an edge.
    pub fn get(&self, x: usize, y: usize) -> Option<Color> {
        self.index(x, y).map(|index| self.pixels[index])
    }

    /// Paints the pixel at (x, y). A pixel past an edge is dropped, so a
    /// shape can cross the edge of the frame.
    pub fn set(&mut self, x: usize, y: usize, colour: Color) {
        if let Some(index) = self.index(x, y) {
            self.pixels[index] = colour;
        }
    }

    /// Mixes `colour` into the pixel at (x, y) by `coverage`, the share of
    /// the pixel a shape covers, from 0 to 1. Where the base is
    /// see-through and the pixel or `colour` is the base, the pixel takes
    /// `colour` from a coverage of a half on and keeps its own colour
    /// below, so the edge stays hard. A pixel past an edge is dropped.
    pub fn blend(&mut self, x: usize, y: usize, colour: Color, coverage: f32) {
        if let Some(index) = self.index(x, y) {
            self.pixels[index] = blend_on(
                self.base,
                self.see_through,
                self.pixels[index],
                colour,
                coverage,
            );
        }
    }

    /// Fills the disc of `radius` round `centre`. As for every shape, a
    /// point is in pixels, and the centre of the pixel at (x, y) lies at
    /// (x, y), so a shape on whole numbers centres on whole pixels.
    pub fn disc(&mut self, centre: (f32, f32), radius: f32, colour: Color) {
        let (x, y) = centre;
        let bounds = [x - radius, y - radius, x + radius, y + radius];
        self.fill(bounds, colour, |px, py| (px - x).hypot(py - y) - radius);
    }

    /// Draws a line `width` pixels wide from `from` to `to`, with round
    /// ends, so lines that meet at a point join without a notch.
    pub fn line(&mut self, from: (f32, f32), to: (f32, f32), width: f32, colour: Color) {
        let half = width / 2.0;
        let bounds = [
            from.0.min(to.0) - half,
            from.1.min(to.1) - half,
            from.0.max(to.0) + half,
            from.1.max(to.1) + half,
        ];
        self.fill(bounds, colour, |x, y| distance((x, y), from, to) - half);
    }

    /// Fills the polygon with `corners` in order, the last joined to the
    /// first. Where its sides cross, a point is inside when a ray from it
    /// crosses the sides an odd number of times.
    pub fn polygon(&mut self, corners: &[(f32, f32)], colour: Color) {
        let bounds = corners.iter().fold(
            [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ],
            |[left, top, right, bottom], &(x, y)| {
                [left.min(x), top.min(y), right.max(x), bottom.max(y)]
            },
        );
        let sides = || corners.iter().zip(corners.iter().cycle().skip(1));
        self.fill(bounds, colour, |x, y| {
            let mut inside = false;
            let mut nearest = f32::INFINITY;
            for (&from, &to) in sides() {
                nearest = nearest.min(distance((x, y), from, to));
                if (from.1 > y) != (to.1 > y)
                    && x < from.0 + (y - from.1) / (to.1 - from.1) * (to.0 - from.0)
                {
                    inside = !inside;
                }
            }
            if inside { -nearest } else { nearest }
        });
    }

    /// Fills each row of `rows` across the canvas with a mix from `top` in
    /// the first row to `bottom` in the last. Rows past the bottom edge are
    /// dropped, and the rows left keep the colours of the whole span. From
    /// a see-through base, the rows turn from one colour to the other at
    /// the middle.
    pub fn gradient(&mut self, rows: Range<usize>, top: Color, bottom: Color) {
        let steps = rows.len().saturating_sub(1).max(1) as f32;
        let width = self.width();
        for y in rows.start..rows.end.min(self.height()) {
            let colour = blend_on(
                self.base,
                self.see_through,
                top,
                bottom,
                (y - rows.start) as f32 / steps,
            );
            self.pixels[y * width..(y + 1) * width].fill(colour);
        }
    }

    /// Blends `colour` into each pixel by its coverage of the shape whose
    /// signed distance is `edge`: how far a point lies outside the shape,
    /// and below 0 inside. `bounds` holds the shape: left, top, right and
    /// bottom.
    ///
    /// The coverage of a pixel is the share of `SAMPLES` by `SAMPLES`
    /// points evenly spread over it that lie inside. Each shape takes the
    /// same points, so a frame draws the same each time. A pixel whose
    /// centre lies further than `REACH` from the edge has every point on
    /// one side, so it needs one distance and no samples.
    fn fill(&mut self, bounds: [f32; 4], colour: Color, edge: impl Fn(f32, f32) -> f32) {
        let [left, top, right, bottom] = bounds;
        for y in span(top, bottom, self.height()) {
            for x in span(left, right, self.width()) {
                let (cx, cy) = (x as f32, y as f32);
                let centre = edge(cx, cy);
                let coverage = if centre <= -REACH {
                    1.0
                } else if centre >= REACH {
                    continue;
                } else {
                    let inside = OFFSETS
                        .iter()
                        .flat_map(|dy| OFFSETS.iter().map(move |dx| (dx, dy)))
                        .filter(|(dx, dy)| edge(cx + *dx, cy + *dy) < 0.0)
                        .count();
                    inside as f32 / (SAMPLES * SAMPLES) as f32
                };
                self.blend(x, y, colour, coverage);
            }
        }
    }

    /// Draws each cell into `buf` from the top left of `area`. Cells past
    /// the area are not drawn.
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let mut cache = Labs::new();
        for row in 0..self.rows.min(area.height) {
            for column in 0..self.columns.min(area.width) {
                let (left, top) = (
                    usize::from(column) * CELL_WIDTH,
                    usize::from(row) * CELL_HEIGHT,
                );
                let pixels = std::array::from_fn(|pixel| {
                    let (x, y) = (left + pixel % CELL_WIDTH, top + pixel / CELL_WIDTH);
                    self.pixels[y * self.width() + x]
                });
                let place = Position::new(area.x + column, area.y + row);
                let (glyph, fore, back) = SHOWN.with_borrow_mut(|shown| {
                    let kept = shown.at(place);
                    match kept {
                        Some(drawn) if drawn.pixels == pixels && drawn.base == self.base => {
                            drawn.cell
                        }
                        _ => {
                            let cell = cell(pixels, self.base, &mut cache);
                            *kept = Some(Drawn {
                                pixels,
                                base: self.base,
                                cell,
                            });
                            cell
                        }
                    }
                });
                buf[place].set_char(glyph).set_fg(fore).set_bg(back);
            }
        }
    }
}

thread_local! {
    /// The cell each place on the screen last drew, by the pixels and the
    /// base it drew them for. A frame draws every place again, mostly with
    /// the same pixels, and the pair search for a cell of three colours or
    /// more is about half the cost of drawing the fire.
    static SHOWN: RefCell<Shown> = RefCell::new(Shown::default());
}

#[derive(Debug, Default)]
struct Shown {
    width: usize,
    height: usize,
    drawn: Vec<Option<Drawn>>,
}

#[derive(Debug, Clone, Copy)]
struct Drawn {
    pixels: [Color; CELL_WIDTH * CELL_HEIGHT],
    base: Color,
    cell: (char, Color, Color),
}

impl Shown {
    /// The cell kept for `place`. A place past the screen kept so far
    /// grows it, as when the terminal grows, and keeps every cell.
    fn at(&mut self, place: Position) -> &mut Option<Drawn> {
        let (x, y) = (usize::from(place.x), usize::from(place.y));
        if x >= self.width || y >= self.height {
            let (width, height) = (self.width.max(x + 1), self.height.max(y + 1));
            let mut drawn = vec![None; width * height];
            for row in 0..self.height {
                drawn[row * width..row * width + self.width]
                    .copy_from_slice(&self.drawn[row * self.width..(row + 1) * self.width]);
            }
            (self.width, self.height, self.drawn) = (width, height, drawn);
        }
        &mut self.drawn[y * self.width + x]
    }
}
