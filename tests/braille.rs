#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::HashSet;

use hearth::braille::{self, CELL_HEIGHT, CELL_WIDTH};

/// Each dot of a cell draws as the braille dot in its place.
/// Given: every dot of a cell, by column and row.
/// When: it alone is drawn.
/// Then: the glyph is the braille character with that dot raised, dots 1
/// to 3 and 7 down the left and 4 to 6 and 8 down the right.
///
/// Why it matters: a dot in the wrong place draws every line and every
/// bird in the scene bent or mirrored.
#[test]
fn each_dot_draws_in_its_own_place() {
    let expected = [
        ((0, 0), '⠁'),
        ((0, 1), '⠂'),
        ((0, 2), '⠄'),
        ((0, 3), '⡀'),
        ((1, 0), '⠈'),
        ((1, 1), '⠐'),
        ((1, 2), '⠠'),
        ((1, 3), '⢀'),
    ];
    for ((column, row), glyph) in expected {
        // When, then:
        assert_eq!(
            braille::glyph(braille::dot(column, row)),
            glyph,
            "dot ({column}, {row})"
        );
    }
}

/// Every set of dots in a cell draws a glyph of its own.
/// Given: every set of the eight dots of a cell.
/// When: each set is drawn.
/// Then: no two sets draw the same glyph, and every glyph is braille.
///
/// Why it matters: two sets that share a glyph lose a dot where they meet.
#[test]
fn every_set_of_dots_draws_its_own_glyph() {
    let dots: Vec<u32> = (0..CELL_WIDTH)
        .flat_map(|column| (0..CELL_HEIGHT).map(move |row| braille::dot(column, row)))
        .collect();
    let mut seen = HashSet::new();
    for set in 0u32..1 << dots.len() {
        // When:
        let bits = dots
            .iter()
            .enumerate()
            .filter(|(at, _)| set >> at & 1 == 1)
            .fold(0, |bits, (_, dot)| bits | dot);
        let glyph = braille::glyph(bits);

        // Then:
        assert!(
            ('\u{2800}'..='\u{28ff}').contains(&glyph),
            "{set:08b} drew {glyph:?}"
        );
        assert!(seen.insert(glyph), "{set:08b} drew {glyph:?} again");
    }
}
