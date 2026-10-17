//! Eight dots in one cell, drawn as a braille glyph.

/// A braille cell is two dots wide and four tall.
pub const CELL_WIDTH: usize = 2;
pub const CELL_HEIGHT: usize = 4;

const BLANK: u32 = 0x2800;

/// The bit of the dot at (column, row) of a cell. Dots 7 and 8, the bottom
/// row, came to the standard after the six dots of the two columns, so they
/// take the two high bits.
pub fn dot(column: usize, row: usize) -> u32 {
    match (column, row) {
        (0, 3) => 1 << 6,
        (_, 3) => 1 << 7,
        (0, row) => 1 << row,
        (_, row) => 1 << (row + 3),
    }
}

/// The glyph that draws `dots`, a set of bits from `dot`.
pub fn glyph(dots: u32) -> char {
    char::from_u32(BLANK + (dots & 0xff)).unwrap_or(' ')
}
