#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::HashMap;
use std::ops::Range;

use hearth::octant::{self, CELL_HEIGHT, CELL_WIDTH, Canvas};
use hearth::theme::{self, Palette};
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::Color;

/// The canvas tests draw in Rose Pine. The base, love and foam are three
/// colours far apart.
const PALETTE: &Palette = &theme::ROSE_PINE;
const BASE: Color = PALETTE.base;
const INK: Color = PALETTE.love;
const OTHER: Color = PALETTE.foam;

/// The Unicode name of each glyph an octant cell can draw. Python's
/// `unicodedata` wrote the file, so the names do not come from the table
/// under test.
fn names() -> HashMap<char, &'static str> {
    include_str!("octant_names.txt")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let (point, name) = line.split_once(' ').unwrap();
            let point = u32::from_str_radix(point, 16).unwrap();
            (char::from_u32(point).unwrap(), name)
        })
        .collect()
}

/// The bits of the pixels in `rows` and `columns` of a cell.
fn area(rows: Range<usize>, columns: Range<usize>) -> u8 {
    rows.flat_map(|row| columns.clone().map(move |column| row * CELL_WIDTH + column))
        .fold(0, |bits, pixel| bits | 1 << pixel)
}

fn side(word: &str) -> Range<usize> {
    match word {
        "LEFT" => 0..1,
        "RIGHT" => 1..2,
        other => panic!("{other:?} is not a side"),
    }
}

fn rows(at: &str, count: usize) -> Range<usize> {
    match at {
        "UPPER" => 0..count,
        "LOWER" => CELL_HEIGHT - count..CELL_HEIGHT,
        "MIDDLE" => (CELL_HEIGHT - count) / 2..(CELL_HEIGHT + count) / 2,
        other => panic!("{other:?} is not a place in a cell"),
    }
}

/// The pixels a glyph's Unicode name describes, as the bits of a cell.
fn pixels(name: &str) -> u8 {
    if name == "SPACE" {
        return 0;
    }
    if let Some(digits) = name.strip_prefix("BLOCK OCTANT-") {
        return digits
            .chars()
            .map(|digit| digit.to_digit(10).unwrap() - 1)
            .fold(0, |bits, pixel| bits | 1 << pixel);
    }
    if let Some(quarters) = name.strip_prefix("QUADRANT ") {
        return quarters.split(" AND ").fold(0, |bits, quarter| {
            let (at, column) = quarter.split_once(' ').unwrap();
            bits | area(rows(at, CELL_HEIGHT / 2), side(column))
        });
    }
    let shape = name
        .strip_suffix(" BLOCK")
        .unwrap_or_else(|| panic!("{name:?} names no block"));
    let mut words: Vec<&str> = shape.split(' ').collect();
    let mut columns = 0..CELL_WIDTH;
    let mut scale = 1;
    if let Some(at) = words
        .iter()
        .position(|word| ["LEFT", "RIGHT"].contains(word))
    {
        columns = side(words.remove(at));
        // "LEFT HALF UPPER ONE QUARTER" takes a quarter of the left half's
        // height. "MIDDLE LEFT ONE QUARTER" takes a quarter of the cell, so
        // its height is a half.
        if words.get(at) == Some(&"HALF") {
            words.remove(at);
        } else {
            scale = 2;
        }
    }
    let rows = match words.as_slice() {
        [] | ["FULL"] => 0..CELL_HEIGHT,
        [at, fraction @ ..] => {
            let quarters = match fraction {
                ["ONE", "QUARTER"] => 1,
                ["HALF"] => 2,
                ["THREE", "QUARTERS"] => 3,
                other => panic!("{other:?} is not a fraction in {name:?}"),
            };
            rows(at, quarters * scale * CELL_HEIGHT / 4)
        }
    };
    area(rows, columns)
}

/// One cell of `pixels`, in the order of the glyph's bits, as drawn.
fn draw(pixels: [Color; 8]) -> Cell {
    let mut canvas = Canvas::new(1, 1, PALETTE);
    for (pixel, colour) in pixels.into_iter().enumerate() {
        canvas.set(pixel % CELL_WIDTH, pixel / CELL_WIDTH, colour);
    }
    let area = Rect::new(0, 0, 1, 1);
    let mut buffer = Buffer::empty(area);
    canvas.render(area, &mut buffer);
    buffer[(0, 0)].clone()
}

/// The colour each pixel of a drawn cell shows on screen.
fn shown(cell: &Cell) -> [Color; 8] {
    let glyph = cell.symbol().chars().next().unwrap();
    let name = names()
        .get(&glyph)
        .copied()
        .unwrap_or_else(|| panic!("{glyph:?} is not an octant glyph"));
    let bits = pixels(name);
    std::array::from_fn(|pixel| {
        if bits >> pixel & 1 == 1 {
            cell.fg
        } else {
            cell.bg
        }
    })
}

/// Each pattern of pixels draws the glyph whose Unicode name describes it.
/// Given: every pattern of the eight pixels of a cell.
/// When: its glyph is looked up.
/// Then: the glyph's name, read as a shape, fills the same pixels.
///
/// Why it matters: a glyph that fills the wrong pixels draws every shape in
/// the scene with a notch or a stray block in it.
#[test]
fn each_pattern_draws_the_glyph_its_name_describes() {
    let names = names();
    for bits in 0..=u8::MAX {
        // When:
        let glyph = octant::glyph(bits);

        // Then:
        let name = names
            .get(&glyph)
            .unwrap_or_else(|| panic!("{bits:08b} drew {glyph:?}, which has no name"));
        assert_eq!(pixels(name), bits, "{bits:08b} drew {glyph:?}, {name}");
    }
}

/// Each glyph turns over to the glyph of the pixels it leaves out.
/// Given: the glyph of every pattern of the eight pixels of a cell.
/// When: its complement is looked up, and then the complement of that.
/// Then: the complement's name fills every pixel the glyph leaves out and
/// no other, and its complement is the glyph again.
///
/// Why it matters: a block drawn in the base turns over when the base
/// clears to the terminal's own background. A complement with a stray or a
/// missing pixel draws the picture with a hole or a blot in each such cell.
#[test]
fn each_glyph_turns_over_to_the_pixels_it_leaves_out() {
    let names = names();
    for bits in 0..=u8::MAX {
        // Given:
        let glyph = octant::glyph(bits).to_string();

        // When:
        let turned = octant::complement(&glyph)
            .unwrap_or_else(|| panic!("{bits:08b} drew {glyph:?}, which does not turn over"));
        let back = octant::complement(&turned.to_string());

        // Then:
        let name = names
            .get(&turned)
            .unwrap_or_else(|| panic!("{glyph:?} turned over to {turned:?}, which has no name"));
        assert_eq!(
            pixels(name),
            !bits,
            "{glyph:?} turned over to {turned:?}, {name}"
        );
        assert_eq!(back, glyph.chars().next(), "{turned:?} turned back");
    }
}

/// A glyph that is no block does not turn over.
/// Given: a letter, a braille dot, and two block glyphs in one symbol.
/// When: the complement of each is looked up.
/// Then: none has one.
///
/// Why it matters: a letter or a dot drawn in the base would turn into a
/// block that hides the picture around it.
#[test]
fn a_glyph_that_is_no_block_does_not_turn_over() {
    for glyph in ["a", "⠁", "▀▀"] {
        // When, then:
        assert_eq!(octant::complement(glyph), None, "{glyph:?}");
    }
}

/// A cell of two colours shows each pixel in its own colour.
/// Given: every pattern of eight pixels in the base and one ink, and every
/// pattern in two inks.
/// When: the cell is drawn.
/// Then: each pixel shows its own colour.
///
/// Why it matters: a cell that swaps its colours or its pixels draws every
/// edge in the scene ragged.
#[test]
fn each_pixel_shows_its_own_colour() {
    for (on, off) in [(INK, BASE), (INK, OTHER)] {
        for bits in 0..=u8::MAX {
            // Given:
            let pixels: [Color; 8] =
                std::array::from_fn(|pixel| if bits >> pixel & 1 == 1 { on } else { off });

            // When:
            let drawn = shown(&draw(pixels));

            // Then:
            assert_eq!(drawn, pixels, "{bits:08b} of {on:?} on {off:?}");
        }
    }
}

/// A cell of the base and one ink keeps the base behind.
/// Given: every pattern of eight pixels in the base and one ink, with at
/// least one pixel of the base.
/// When: the cell is drawn.
/// Then: its background is the base.
///
/// Why it matters: with the terminal's own background the base clears. An
/// ink behind a cell that is mostly ink would fill the base pixels with
/// ink, or paint a block where the terminal should show.
#[test]
fn the_base_stays_behind_one_ink() {
    for bits in 0..u8::MAX {
        // Given:
        let pixels: [Color; 8] =
            std::array::from_fn(|pixel| if bits >> pixel & 1 == 1 { INK } else { BASE });

        // When:
        let cell = draw(pixels);

        // Then:
        assert_eq!(cell.bg, BASE, "{bits:08b}");
    }
}

/// A third colour in one cell is drawn as the ink, not the base.
/// Given: four pixels of the base, two of an ink and two of another ink.
/// When: the cell is drawn.
/// Then: the base pixels show the base, and the other four show an ink.
///
/// Why it matters: a thin trail of colour beside another would vanish into
/// the background where the two meet.
#[test]
fn a_third_colour_shows_as_ink() {
    // Given:
    let pixels = [BASE, INK, OTHER, BASE, BASE, INK, OTHER, BASE];

    // When:
    let drawn = shown(&draw(pixels));

    // Then:
    for (pixel, (colour, shows)) in pixels.into_iter().zip(drawn).enumerate() {
        assert_eq!(colour == BASE, shows == BASE, "pixel {pixel}: {drawn:?}");
    }
}

/// Two colours of equal count go in front by the order of their pixels.
/// Given: a cell of two inks, four pixels each, the upper half of one and
/// the lower half of the other, and the same cell with the inks swapped.
/// When: each cell is drawn.
/// Then: the ink of the upper half is the foreground of each.
///
/// Why it matters: a tie broken by chance draws a seeded replay with other
/// glyphs than the first run.
#[test]
fn a_tie_goes_to_the_first_pixel() {
    for (upper, lower) in [(INK, OTHER), (OTHER, INK)] {
        // Given:
        let pixels: [Color; 8] = std::array::from_fn(|pixel| if pixel < 4 { upper } else { lower });

        // When:
        let cell = draw(pixels);

        // Then:
        assert_eq!(
            (cell.symbol(), cell.fg, cell.bg),
            ("▀", upper, lower),
            "{upper:?} over {lower:?}"
        );
    }
}

/// The canvas covers its cells two pixels across and four down, and starts
/// in the base.
/// Given: a canvas of three columns and two rows.
/// When: it is measured and read.
/// Then: it is six pixels wide and eight tall, each pixel is the base, and
/// a pixel past an edge reads as none.
///
/// Why it matters: a scene sizes its shapes by the pixels across and down.
/// A wrong size draws them stretched or cut off.
#[test]
fn a_canvas_covers_its_cells_in_the_base() {
    // Given:
    let canvas = Canvas::new(3, 2, PALETTE);

    // When, then:
    assert_eq!((canvas.width(), canvas.height()), (6, 8));
    for y in 0..8 {
        for x in 0..6 {
            assert_eq!(canvas.get(x, y), Some(BASE), "({x}, {y})");
        }
    }
    assert_eq!(canvas.get(6, 0), None);
    assert_eq!(canvas.get(0, 8), None);
}

/// A pixel set on the canvas reads back, and one past an edge is dropped.
/// Given: a canvas of one cell.
/// When: one pixel inside it and one past each edge are set.
/// Then: the inside pixel reads back in its colour, and no other changes.
///
/// Why it matters: a scene draws shapes that cross the edge of the screen.
/// A pixel past the edge that wrapped to the next row would draw a stray
/// dot at the far side.
#[test]
fn a_pixel_past_an_edge_is_dropped() {
    // Given:
    let mut canvas = Canvas::new(1, 1, PALETTE);

    // When:
    canvas.set(1, 2, INK);
    canvas.set(2, 0, OTHER);
    canvas.set(0, 4, OTHER);

    // Then:
    for y in 0..4 {
        for x in 0..2 {
            let expected = if (x, y) == (1, 2) { INK } else { BASE };
            assert_eq!(canvas.get(x, y), Some(expected), "({x}, {y})");
        }
    }
}

/// Each cell of the canvas lands at its own place in the area, and no
/// cell lands past it.
/// Given: a canvas of three columns and three rows, with an ink pixel in
/// its first column and one in its last.
/// When: it is drawn into an area of two columns and two rows at (4, 2) of
/// a larger buffer.
/// Then: the ink of the first column shows at its place in the area, and
/// no cell outside the area changes.
///
/// Why it matters: a canvas sized before the terminal shrank must not draw
/// past its pane into the next one.
#[test]
fn each_cell_lands_in_its_place_in_the_area() {
    // Given:
    let mut canvas = Canvas::new(3, 3, PALETTE);
    canvas.set(1, 4, INK);
    canvas.set(5, 7, INK);
    let mut buffer = Buffer::empty(Rect::new(0, 0, 10, 6));

    // When:
    canvas.render(Rect::new(4, 2, 2, 2), &mut buffer);

    // Then:
    let mut ink = [BASE; 8];
    ink[1] = INK;
    for y in 0..6 {
        for x in 0..10 {
            let cell = &buffer[(x, y)];
            if !((4..6).contains(&x) && (2..4).contains(&y)) {
                assert_eq!(cell, &Cell::default(), "({x}, {y}) is outside the area");
            } else if (x, y) == (4, 3) {
                assert_eq!(shown(cell), ink, "({x}, {y})");
            } else {
                assert_eq!(shown(cell), [BASE; 8], "({x}, {y})");
            }
        }
    }
}

/// What lies on a pixel in the settle tests, from the lowest layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Liquid {
    Water,
    Foam,
}

impl octant::Layer for Liquid {
    fn joins(self, top: Self) -> bool {
        self == Liquid::Water && top == Liquid::Foam
    }
}

/// The same layers, with no layer that joins another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Apart(Liquid);

impl octant::Layer for Apart {}

/// A layer that joins the layer in front takes its colour, and one that
/// does not takes the colour behind.
/// Given: a cell whose top row is foam, whose middle rows are water, and
/// whose bottom row is empty, once with water that joins the foam and once
/// with water that does not.
/// When: each cell settles.
/// Then: the joined water shows the foam's colour, the other water shows
/// the empty colour, and the empty row shows the empty colour in both.
///
/// Why it matters: water that drops to the bed behind a band of foam
/// draws a gap through its lane.
#[test]
fn a_layer_that_joins_the_front_takes_its_colour() {
    // Given:
    let layer = |pixel: usize| match pixel / CELL_WIDTH {
        0 => Some(Liquid::Foam),
        1 | 2 => Some(Liquid::Water),
        _ => None,
    };
    let colour = |liquid: Liquid| if liquid == Liquid::Foam { INK } else { OTHER };
    let joined: [Option<(Liquid, Color)>; 8] =
        std::array::from_fn(|pixel| layer(pixel).map(|liquid| (liquid, colour(liquid))));
    let apart = joined.map(|pixel| pixel.map(|(liquid, colour)| (Apart(liquid), colour)));

    // When:
    let joined = octant::settle(joined, BASE);
    let apart = octant::settle(apart, BASE);

    // Then:
    let expected = |water: Color| -> [Color; 8] {
        std::array::from_fn(|pixel| match layer(pixel) {
            Some(Liquid::Foam) => INK,
            Some(Liquid::Water) => water,
            None => BASE,
        })
    };
    assert_eq!(joined, expected(INK));
    assert_eq!(apart, expected(BASE));
}
