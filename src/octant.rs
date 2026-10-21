//! Eight pixels in one cell, two across and four down, drawn as an octant
//! glyph and two colours.

use std::cmp::Reverse;

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

/// The glyph, foreground and background for the pixels of one cell, in the
/// order of the glyph's bits. A glyph holds two colours, the two most
/// common, and a pixel of a third takes the foreground rather than `base`,
/// so a thin colour is not lost. Of two colours with the same count, the
/// one with the first pixel goes first, so a seed draws the same frame
/// each time.
fn cell(pixels: [Color; 8], base: Color) -> (char, Color, Color) {
    let mut counts = [(base, 0); 8];
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

/// The pixels of a frame, `CELL_WIDTH` across and `CELL_HEIGHT` down in
/// each cell, that draw into a buffer one glyph to a cell.
#[derive(Debug, Clone)]
pub struct Canvas {
    columns: u16,
    rows: u16,
    base: Color,
    pixels: Vec<Color>,
}

impl Canvas {
    /// A canvas of `columns` by `rows` cells with every pixel in the base
    /// of `palette`. The cells keep the base behind them where they can, so
    /// it clears to the terminal's own background.
    pub fn new(columns: u16, rows: u16, palette: &Palette) -> Self {
        let size = usize::from(columns) * CELL_WIDTH * usize::from(rows) * CELL_HEIGHT;
        Self {
            columns,
            rows,
            base: palette.base,
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

    /// Draws each cell into `buf` from the top left of `area`. Cells past
    /// the area are not drawn.
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
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
                let (glyph, fore, back) = cell(pixels, self.base);
                buf[Position::new(area.x + column, area.y + row)]
                    .set_char(glyph)
                    .set_fg(fore)
                    .set_bg(back);
            }
        }
    }
}
