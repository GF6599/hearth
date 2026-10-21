//! Four pixels in one cell, drawn as a quadrant glyph and two colours.

use std::cmp::Reverse;

use ratatui::style::Color;

/// A glyph's bits are its upper left, upper right, lower left and lower
/// right quarters, from the lowest bit, and a set bit draws in the
/// foreground.
const QUADRANTS: [char; 16] = [
    ' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█',
];

/// The glyph, foreground and background for the pixels of one cell, in the
/// order of the glyph's bits. A glyph holds two colours, the two most
/// common, and a pixel of a third takes the foreground rather than `base`,
/// so a thin colour is not lost.
pub fn cell(pixels: [Color; 4], base: Color) -> (char, Color, Color) {
    let mut counts: Vec<(Color, usize)> = Vec::with_capacity(pixels.len());
    for colour in pixels {
        match counts.iter_mut().find(|(each, _)| *each == colour) {
            Some((_, count)) => *count += 1,
            None => counts.push((colour, 1)),
        }
    }
    counts.sort_by_key(|(_, count)| Reverse(*count));
    let first = counts.first().map_or(base, |(colour, _)| *colour);
    let Some(&(second, _)) = counts.get(1) else {
        return (QUADRANTS[0], first, first);
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
        .fold(0, |bits, (quarter, _)| bits | 1 << quarter);
    (QUADRANTS[bits], fore, back)
}
