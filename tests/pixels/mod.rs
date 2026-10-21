use std::collections::HashMap;
use std::sync::OnceLock;

use hearth::octant;
use ratatui::buffer::Cell;
use ratatui::style::Color;

/// The colour each pixel of a cell shows, in the order of the glyph's bits,
/// or none for a cell that holds no octant glyph, such as a letter. A test
/// reads every cell of thousands of frames, so the lookup is built once and
/// a blank cell, the most common, skips it.
pub fn shown(cell: &Cell) -> Option<[Color; 8]> {
    static BITS: OnceLock<HashMap<char, u8>> = OnceLock::new();
    let bits = BITS.get_or_init(|| {
        (0..=u8::MAX)
            .map(|bits| (octant::glyph(bits), bits))
            .collect()
    });
    let bits = match cell.symbol() {
        " " => 0,
        symbol => {
            let mut glyphs = symbol.chars();
            let bits = *bits.get(&glyphs.next()?)?;
            if glyphs.next().is_some() {
                return None;
            }
            bits
        }
    };
    Some(std::array::from_fn(|pixel| {
        if bits >> pixel & 1 == 1 {
            cell.fg
        } else {
            cell.bg
        }
    }))
}
