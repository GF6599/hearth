use std::collections::HashMap;
use std::sync::OnceLock;

use hearth::octant;
use ratatui::buffer::Cell;
use ratatui::style::Color;

/// The colour each pixel of a cell shows, in the order of the glyph's bits,
/// or none for a cell that holds no octant glyph, such as a letter. The
/// lookup is built once, because a test reads every cell of many frames.
pub fn shown(cell: &Cell) -> Option<[Color; 8]> {
    static BITS: OnceLock<HashMap<char, u8>> = OnceLock::new();
    let bits = BITS.get_or_init(|| {
        (0..=u8::MAX)
            .map(|bits| (octant::glyph(bits), bits))
            .collect()
    });
    let mut glyphs = cell.symbol().chars();
    let bits = *bits.get(&glyphs.next()?)?;
    if glyphs.next().is_some() {
        return None;
    }
    Some(std::array::from_fn(|pixel| {
        if bits >> pixel & 1 == 1 {
            cell.fg
        } else {
            cell.bg
        }
    }))
}
