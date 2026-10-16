#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use hearth::quadrant;
use ratatui::style::Color;

const BASE: Color = Color::Rgb(0x19, 0x17, 0x24);
const INK: Color = Color::Rgb(0xeb, 0x6f, 0x92);
const OTHER: Color = Color::Rgb(0x9c, 0xcf, 0xd8);

/// The quarters each glyph fills, upper left, upper right, lower left and
/// lower right, as the glyphs are drawn.
fn filled(glyph: char) -> [bool; 4] {
    match glyph {
        ' ' => [false, false, false, false],
        '▘' => [true, false, false, false],
        '▝' => [false, true, false, false],
        '▀' => [true, true, false, false],
        '▖' => [false, false, true, false],
        '▌' => [true, false, true, false],
        '▞' => [false, true, true, false],
        '▛' => [true, true, true, false],
        '▗' => [false, false, false, true],
        '▚' => [true, false, false, true],
        '▐' => [false, true, false, true],
        '▜' => [true, true, false, true],
        '▄' => [false, false, true, true],
        '▙' => [true, false, true, true],
        '▟' => [false, true, true, true],
        '█' => [true, true, true, true],
        other => panic!("{other:?} is not a quadrant glyph"),
    }
}

/// The colour each quarter of a cell shows on screen.
fn shown(pixels: [Color; 4]) -> [Color; 4] {
    let (glyph, fore, back) = quadrant::cell(pixels, BASE);
    filled(glyph).map(|fill| if fill { fore } else { back })
}

/// A cell of two colours shows each pixel in its own colour.
/// Given: every pattern of four pixels in the base and one ink, and every
/// pattern in two inks.
/// When: the cell is drawn.
/// Then: each quarter shows the colour of its pixel.
///
/// Why it matters: a glyph that fills the wrong quarter draws every shape
/// in the scene shifted or mirrored by half a cell.
#[test]
fn each_quarter_shows_its_own_pixel() {
    for (on, off) in [(INK, BASE), (INK, OTHER)] {
        for mask in 0..16 {
            // Given:
            let pixels: [Color; 4] =
                std::array::from_fn(|quarter| if mask >> quarter & 1 == 1 { on } else { off });

            // When:
            let drawn = shown(pixels);

            // Then:
            assert_eq!(drawn, pixels, "mask {mask:04b} of {on:?} on {off:?}");
        }
    }
}

/// A third colour in one cell is drawn as the ink, not the base.
/// Given: two pixels of the base, one of an ink and one of another ink.
/// When: the cell is drawn.
/// Then: the base pixels show the base, and the other two show an ink.
///
/// Why it matters: a thin trail of colour beside another would vanish into
/// the background where the two meet.
#[test]
fn a_third_colour_shows_as_ink() {
    // Given:
    let pixels = [BASE, INK, OTHER, BASE];

    // When:
    let drawn = shown(pixels);

    // Then:
    assert_eq!(drawn[0], BASE);
    assert_eq!(drawn[3], BASE);
    assert!(drawn[1] != BASE && drawn[2] != BASE, "{drawn:?}");
}
