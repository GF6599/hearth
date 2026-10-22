#![allow(
    dead_code,
    reason = "each test binary builds this module and uses only some of it"
)]

use std::collections::HashMap;
use std::sync::OnceLock;

use hearth::octant::{self, CELL_HEIGHT, CELL_WIDTH};
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

pub const PIXELS: usize = CELL_WIDTH * CELL_HEIGHT;

/// The bits of a cell's octant glyph, or none for a cell that holds another
/// glyph, such as a letter. A test reads every cell of thousands of frames,
/// so the lookup is built once and a blank cell, the most common, skips it.
pub fn bits(cell: &Cell) -> Option<u8> {
    static BITS: OnceLock<HashMap<char, u8>> = OnceLock::new();
    let symbol = cell.symbol();
    if symbol == " " {
        return Some(0);
    }
    let bits = BITS.get_or_init(|| {
        (0..=u8::MAX)
            .map(|bits| (octant::glyph(bits), bits))
            .collect()
    });
    let mut glyphs = symbol.chars();
    let bits = *bits.get(&glyphs.next()?)?;
    glyphs.next().is_none().then_some(bits)
}

/// The colour each pixel of a cell shows, in the order of the glyph's bits,
/// or none for a cell that holds no octant glyph.
pub fn shown(cell: &Cell) -> Option<[Color; PIXELS]> {
    let bits = bits(cell)?;
    let mut colours = [cell.bg; PIXELS];
    for (pixel, colour) in colours.iter_mut().enumerate() {
        if bits >> pixel & 1 == 1 {
            *colour = cell.fg;
        }
    }
    Some(colours)
}

/// Each pixel of the octant cells of `area` in a colour that `keep`
/// accepts, with its place as (column, row) in pixels from the top left of
/// the area, cell by cell. A cell with another glyph shows no pixels. The
/// filter runs inside the loop because a test reads thousands of frames
/// and an iterator chain per cell is slow in a debug build.
pub fn showing(
    buffer: &Buffer,
    area: Rect,
    keep: impl Fn(Color) -> bool,
) -> Vec<((usize, usize), Color)> {
    let mut kept = Vec::new();
    for position in area.positions() {
        let Some(colours) = shown(&buffer[position]) else {
            continue;
        };
        let (left, top) = corner(position, area);
        for (pixel, colour) in colours.into_iter().enumerate() {
            if keep(colour) {
                kept.push((
                    (left + pixel % CELL_WIDTH, top + pixel / CELL_WIDTH),
                    colour,
                ));
            }
        }
    }
    kept
}

/// The pixels of `area`, by pixel row from the top and then by pixel column
/// from the left. `read` gives the pixels of each cell in the order of the
/// glyph's bits, so a test chooses what a cell with another glyph shows.
pub fn grid<T: Clone + Default>(
    buffer: &Buffer,
    area: Rect,
    read: impl Fn(Position, &Cell) -> [T; PIXELS],
) -> Vec<Vec<T>> {
    let mut rows = vec![
        vec![T::default(); usize::from(area.width) * CELL_WIDTH];
        usize::from(area.height) * CELL_HEIGHT
    ];
    for position in area.positions() {
        let (left, top) = corner(position, area);
        for (pixel, value) in read(position, &buffer[position]).iter().enumerate() {
            rows[top + pixel / CELL_WIDTH][left + pixel % CELL_WIDTH] = value.clone();
        }
    }
    rows
}

/// The distance between two colours in RGB. A colour with no RGB value
/// fails the test, as no distance to it is true.
pub fn distance(one: Color, other: Color) -> f32 {
    match (one, other) {
        (Color::Rgb(r, g, b), Color::Rgb(s, h, c)) => [(r, s), (g, h), (b, c)]
            .into_iter()
            .map(|(one, other)| (f32::from(one) - f32::from(other)).powi(2))
            .sum::<f32>()
            .sqrt(),
        _ => panic!("{one:?} or {other:?} has no RGB value"),
    }
}

/// Whether `colour` is a mix of `one` and `other`: it lies on the way from
/// one to the other, to a step of rounding, and a twentieth of the way or
/// more from each, so a colour a step from either is no mix. A colour with
/// no RGB value, the terminal's own background, is no mix.
pub fn between(colour: Color, one: Color, other: Color) -> bool {
    let Color::Rgb(..) = colour else {
        return false;
    };
    let (span, near, far) = (
        distance(one, other),
        distance(one, colour),
        distance(colour, other),
    );
    near >= span / 20.0 && far >= span / 20.0 && near + far <= span + 2.0
}

/// The top left pixel of the cell at `position`, from the top left of
/// `area`.
fn corner(position: Position, area: Rect) -> (usize, usize) {
    (
        usize::from(position.x - area.x) * CELL_WIDTH,
        usize::from(position.y - area.y) * CELL_HEIGHT,
    )
}
