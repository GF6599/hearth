#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pixels;

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
    draw_on(PALETTE, pixels)
}

/// One cell of `pixels` on a canvas of `palette`, as drawn.
fn draw_on(palette: &Palette, pixels: [Color; 8]) -> Cell {
    let mut canvas = Canvas::new(1, 1, palette);
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

/// A cell of more than two colours draws each pixel in the nearer colour
/// of the pair that lies nearest all its pixels.
/// Given: a cell that shades from the base to the ink, with three pixels
/// of each and one pixel each of the mixes at three tenths and seven
/// tenths, and a cell that shades from the ink to foam, with three pixels
/// of each and two of their mix at six tenths.
/// When: each cell is drawn.
/// Then: the first cell shows the base where its colours are nearer the
/// base and the ink where they are nearer the ink, with the base behind.
/// The second shows the ink in its three ink pixels and foam in the rest.
///
/// Why it matters: a gradient puts three or more colours in a cell. The
/// rule before gave every colour past the two most common to the
/// foreground, so a gradient drew a band of the foreground colour through
/// colours nearer the background. Each pixel in the nearer colour keeps
/// the shade even across the cell.
#[test]
fn a_cell_of_more_colours_takes_the_pair_nearest_its_pixels() {
    // Given:
    let low = octant::mix(BASE, INK, 0.3);
    let high = octant::mix(BASE, INK, 0.7);
    let dusk = [BASE, BASE, BASE, low, high, INK, INK, INK];
    let middle = octant::mix(INK, OTHER, 0.6);
    let sea = [INK, INK, INK, middle, middle, OTHER, OTHER, OTHER];

    // When:
    let dusk_cell = draw(dusk);
    let sea_cell = draw(sea);

    // Then:
    assert_eq!(
        shown(&dusk_cell),
        [BASE, BASE, BASE, BASE, INK, INK, INK, INK]
    );
    assert_eq!(dusk_cell.bg, BASE);
    assert_eq!(
        shown(&sea_cell),
        [INK, INK, INK, OTHER, OTHER, OTHER, OTHER, OTHER]
    );
}

/// A cell keeps the base behind when the base is one of its pair.
/// Given: a cell of one pixel of the base, three of a colour near the
/// ink, the mix of the ink and foam at three tenths, and four of the ink.
/// When: the cell is drawn.
/// Then: the base pixel shows the base, the other seven show the ink, and
/// the base is behind.
///
/// Why it matters: with the terminal's own background the base clears.
/// The rule before put the two most common colours in the cell, so the
/// base pixel took the ink and hid the terminal. The base lies far from
/// both other colours, so the pair nearest the pixels holds it.
#[test]
fn a_cell_keeps_the_base_behind_in_its_pair() {
    // Given:
    let near = octant::mix(INK, OTHER, 0.3);
    let pixels = [BASE, near, near, near, INK, INK, INK, INK];

    // When:
    let cell = draw(pixels);

    // Then:
    assert_eq!(shown(&cell), [BASE, INK, INK, INK, INK, INK, INK, INK]);
    assert_eq!(cell.bg, BASE);
}

/// Of two pairs that lie as near a cell's pixels, the pair whose colours
/// come first in the cell wins.
/// Given: four pixels of the base, two of the ink and two of rose, which
/// lies near the ink, once with the ink first and once with rose first.
/// When: each cell is drawn.
/// Then: the colour that comes first of the ink and rose fills all four
/// of their pixels, with the base behind.
///
/// Why it matters: the base with the ink and the base with rose lie
/// exactly as near these pixels. A tie broken by chance draws a seeded
/// replay with other colours than the first run.
#[test]
fn a_tie_between_two_pairs_goes_to_the_first_colours() {
    for (first, second) in [(INK, PALETTE.rose), (PALETTE.rose, INK)] {
        // Given:
        let pixels = [first, second, BASE, BASE, first, second, BASE, BASE];

        // When:
        let cell = draw(pixels);

        // Then:
        let expected = pixels.map(|colour| if colour == BASE { BASE } else { first });
        assert_eq!(shown(&cell), expected, "{first:?} first");
        assert_eq!(cell.bg, BASE, "{first:?} first");
    }
}

/// A cell draws the same pair on a see-through base as on a coloured one.
/// Given: a cell of four pixels of the base, two of the overlay, which
/// lies near the base, and two of the ink, once on a base that is a colour
/// and once on a see-through base.
/// When: each cell is drawn.
/// Then: both show the base in the base and overlay pixels and the ink in
/// the ink pixels, with the base behind.
///
/// Why it matters: the terminal's own background changes no picture, only
/// what lies behind it. A cell that chose its pair by the kind of base
/// would draw other colours on each background.
#[test]
fn a_cell_draws_the_same_pair_on_a_see_through_base() {
    // Given:
    let pixels = [
        BASE,
        BASE,
        BASE,
        BASE,
        PALETTE.overlay,
        PALETTE.overlay,
        INK,
        INK,
    ];

    for palette in [PALETTE, SEE_THROUGH] {
        // When:
        let cell = draw_on(palette, pixels);

        // Then:
        let at = format!("see-through: {}", palette.see_through);
        assert_eq!(
            shown(&cell),
            [BASE, BASE, BASE, BASE, BASE, BASE, INK, INK],
            "{at}"
        );
        assert_eq!(cell.bg, BASE, "{at}");
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

/// The colours of Rose Pine, each role once.
fn rose_pine() -> [Color; 12] {
    let palette = PALETTE;
    [
        palette.base,
        palette.surface,
        palette.overlay,
        palette.muted,
        palette.subtle,
        palette.text,
        palette.love,
        palette.gold,
        palette.rose,
        palette.pine,
        palette.foam,
        palette.iris,
    ]
}

/// A mix at either end is that end's colour exactly.
/// Given: every pair of Rose Pine's colours.
/// When: each pair mixes at 0 and at 1, and past each end.
/// Then: 0 and below give the first colour, and 1 and above give the
/// second, to the last bit.
///
/// Why it matters: a shape at full coverage must draw its own theme
/// colour. A mix one step off at an end draws a colour that no theme has,
/// and a two colour cell then holds three.
#[test]
fn a_mix_at_either_end_is_that_colour() {
    for from in rose_pine() {
        for to in rose_pine() {
            // When, then:
            assert_eq!(octant::mix(from, to, 0.0), from, "{from:?} to {to:?} at 0");
            assert_eq!(
                octant::mix(from, to, -0.5),
                from,
                "{from:?} to {to:?} below 0"
            );
            assert_eq!(octant::mix(from, to, 1.0), to, "{from:?} to {to:?} at 1");
            assert_eq!(octant::mix(from, to, 1.5), to, "{from:?} to {to:?} above 1");
        }
    }
}

/// A mix between two colours is the point between them in OKLab.
/// Given: black and white, love and foam, and the base and love.
/// When: each pair mixes at a share between its ends.
/// Then: each mix is the colour that coloraide 8.13, a colour library apart
/// from this crate, gives for the same mix in OKLab, rounded to 8 bits.
///
/// Why it matters: a mix in sRGB makes the middle of a gradient too dark
/// and too grey. OKLab keeps each step of a shade an even step to the eye.
#[test]
fn a_mix_is_the_point_between_in_oklab() {
    let cases = [
        (
            Color::Rgb(0, 0, 0),
            Color::Rgb(255, 255, 255),
            0.5,
            Color::Rgb(99, 99, 99),
        ),
        (PALETTE.love, PALETTE.foam, 0.5, Color::Rgb(206, 163, 181)),
        (PALETTE.base, PALETTE.love, 0.25, Color::Rgb(72, 44, 61)),
    ];
    for (from, to, share, expected) in cases {
        // When, then:
        assert_eq!(
            octant::mix(from, to, share),
            expected,
            "{from:?} to {to:?} at {share}"
        );
    }
}

/// A colour that is no RGB value takes the nearer end of a mix.
/// Given: the terminal's own background and an ink.
/// When: they mix at shares below a half, at a half, and above it.
/// Then: below a half the mix is the first colour, and from a half on it
/// is the second.
///
/// Why it matters: the terminal's own background has no value to mix
/// with. A mix that guessed one would paint a colour the user's terminal
/// does not show.
#[test]
fn a_colour_with_no_value_takes_the_nearer_end() {
    for (from, to) in [(Color::Reset, INK), (INK, Color::Reset)] {
        // When, then:
        assert_eq!(octant::mix(from, to, 0.25), from, "{from:?} to {to:?}");
        assert_eq!(octant::mix(from, to, 0.5), to, "{from:?} to {to:?}");
        assert_eq!(octant::mix(from, to, 0.75), to, "{from:?} to {to:?}");
    }
}

/// A ramp passes through each stop at its place, and mixes between two.
/// Given: a ramp of the base, love and gold.
/// When: it is read at 0, a quarter, a half and 1, and past each end.
/// Then: 0 and below give the base, a half gives love, 1 and above give
/// gold, and a quarter gives the mix of the base and love at a half.
///
/// Why it matters: a fire or a sky shades from heat or height through
/// several theme colours. A ramp that missed a stop would draw a colour
/// that is not in the theme where the scene asks for one that is.
#[test]
fn a_ramp_passes_through_each_stop() {
    // Given:
    let stops = [PALETTE.base, PALETTE.love, PALETTE.gold];

    // When, then:
    assert_eq!(octant::ramp(&stops, 0.0), PALETTE.base);
    assert_eq!(octant::ramp(&stops, -1.0), PALETTE.base);
    assert_eq!(octant::ramp(&stops, 0.5), PALETTE.love);
    assert_eq!(octant::ramp(&stops, 1.0), PALETTE.gold);
    assert_eq!(octant::ramp(&stops, 2.0), PALETTE.gold);
    assert_eq!(
        octant::ramp(&stops, 0.25),
        octant::mix(PALETTE.base, PALETTE.love, 0.5)
    );
}

/// The share of the way from `from` to `to` whose mix is `colour`, to the
/// nearest 1/256, or none for a colour off the line between them.
fn share_of(from: Color, to: Color, colour: Color) -> Option<f32> {
    pixels::share_on(colour, from, to, 256, 0)
}

/// Each row of `canvas` from `rows`, pixel by pixel.
fn canvas_rows(canvas: &Canvas, rows: Range<usize>) -> Vec<Vec<Color>> {
    rows.map(|y| {
        (0..canvas.width())
            .map(|x| canvas.get(x, y).unwrap())
            .collect()
    })
    .collect()
}

/// A colour blends into a pixel by its coverage.
/// Given: a canvas in the base with one pixel of ink.
/// When: foam blends into pixels of the base at coverage 0, a quarter and
/// 1, into the ink at a half, and into a pixel past an edge.
/// Then: coverage 0 leaves the base, a quarter gives the mix of the base
/// and foam at a quarter, 1 gives foam, and a half on the ink gives the mix
/// of the ink and foam at a half. No other pixel changes.
///
/// Why it matters: every soft edge is a blend. A blend that ignored the
/// colour already there would draw a halo of the base round a shape that
/// lies over another.
#[test]
fn a_colour_blends_into_a_pixel_by_its_coverage() {
    // Given:
    let mut canvas = Canvas::new(2, 1, PALETTE);
    canvas.set(3, 3, INK);

    // When:
    canvas.blend(0, 0, OTHER, 0.0);
    canvas.blend(1, 0, OTHER, 0.25);
    canvas.blend(2, 0, OTHER, 1.0);
    canvas.blend(3, 3, OTHER, 0.5);
    canvas.blend(4, 0, OTHER, 1.0);

    // Then:
    let mut expected = vec![vec![BASE; 4]; 4];
    expected[0][1] = octant::mix(BASE, OTHER, 0.25);
    expected[0][2] = OTHER;
    expected[3][3] = octant::mix(INK, OTHER, 0.5);
    assert_eq!(canvas_rows(&canvas, 0..4), expected);
}

/// A disc fills its inside, and blends its edge by the share of each
/// pixel it covers.
/// Given: a canvas in the base.
/// When: a disc of ink with radius 2.5 is drawn round the centre of pixel
/// (5, 6).
/// Then: the pixels whose whole square lies inside show the ink, the
/// pixels whose whole square lies outside show the base, the pixel at
/// (7, 8) on the edge shows a mix of the two strictly between them, and
/// the shares of all pixels add up to the disc's area within half a pixel.
///
/// Why it matters: a disc drawn whole pixel by pixel has a ragged rim. A
/// soft rim that covered too much or too little would make a moon or a
/// bubble grow or shrink as it moves.
#[test]
fn a_disc_blends_its_edge_by_coverage() {
    // Given:
    let mut canvas = Canvas::new(6, 3, PALETTE);
    let (centre, radius) = ((5.0, 6.0), 2.5);

    // When:
    canvas.disc(centre, radius, INK);

    // Then:
    let mut area = 0.0;
    for y in 0..canvas.height() {
        for x in 0..canvas.width() {
            let colour = canvas.get(x, y).unwrap();
            let share = share_of(BASE, INK, colour)
                .unwrap_or_else(|| panic!("({x}, {y}) is {colour:?}, no mix of base and ink"));
            area += share;
            let (dx, dy) = (x as f32 - centre.0, y as f32 - centre.1);
            let far = (dx.abs() + 0.5).hypot(dy.abs() + 0.5);
            let near = (dx.abs() - 0.5).max(0.0).hypot((dy.abs() - 0.5).max(0.0));
            if far <= radius {
                assert_eq!(colour, INK, "({x}, {y}) lies inside");
            } else if near >= radius {
                assert_eq!(colour, BASE, "({x}, {y}) lies outside");
            }
        }
    }
    let edge = canvas.get(7, 8).unwrap();
    assert_ne!(edge, INK, "the edge at (7, 8)");
    assert_ne!(edge, BASE, "the edge at (7, 8)");
    let disc = std::f32::consts::PI * radius * radius;
    assert!(
        (area - disc).abs() < 0.5,
        "the shares add up to {area}, not {disc}"
    );
}

/// A line covers its width, and a pixel it half covers shows the half mix.
/// Given: a canvas in the base.
/// When: a line of ink two pixels wide runs across from the centre of
/// pixel (1, 2) to the centre of pixel (10, 2).
/// Then: away from its ends, row 2 shows the ink, rows 1 and 3, half under
/// the line, show the mix of the base and the ink at a half, and rows 0
/// and 4 show the base.
///
/// Why it matters: a constellation line or a wire drawn one pixel at a
/// time steps like a stair. A soft line that came out too wide or too
/// thin would change weight as it turns.
#[test]
fn a_line_covers_its_width() {
    // Given:
    let mut canvas = Canvas::new(6, 2, PALETTE);

    // When:
    canvas.line((1.0, 2.0), (10.0, 2.0), 2.0, INK);

    // Then:
    let half = octant::mix(BASE, INK, 0.5);
    for x in 3..9 {
        let column: Vec<Color> = (0..5).map(|y| canvas.get(x, y).unwrap()).collect();
        assert_eq!(column, [BASE, half, INK, half, BASE], "column {x}");
    }
}

/// A line at a slant blends each pixel it crosses in part.
/// Given: a canvas in the base.
/// When: a line of ink one pixel wide runs from the centre of pixel (0, 0)
/// to the centre of pixel (9, 3).
/// Then: some pixel shows a mix strictly between the base and the ink,
/// and every pixel shows the base, the ink, or a mix of the two.
///
/// Why it matters: a slanted line is where a hard line steps the most.
#[test]
fn a_slanted_line_blends_the_pixels_it_crosses() {
    // Given:
    let mut canvas = Canvas::new(5, 1, PALETTE);

    // When:
    canvas.line((0.0, 0.0), (9.0, 3.0), 1.0, INK);

    // Then:
    let mut soft = 0;
    for y in 0..canvas.height() {
        for x in 0..canvas.width() {
            let colour = canvas.get(x, y).unwrap();
            let share = share_of(BASE, INK, colour)
                .unwrap_or_else(|| panic!("({x}, {y}) is {colour:?}, no mix of base and ink"));
            if share > 0.0 && share < 1.0 {
                soft += 1;
            }
        }
    }
    assert!(soft > 0, "no pixel blends");
}

/// A polygon fills its inside, and a pixel its edge halves shows the half
/// mix.
/// Given: a canvas in the base.
/// When: a polygon of ink is drawn with its left side on the left edge of
/// the canvas, its right side through the centres of column 4, its top on
/// the top edge, and its bottom side on the line between rows 2 and 3.
/// Then: columns 0 to 3 of rows 0 to 2 show the ink, column 4 of those
/// rows shows the mix of the base and the ink at a half, and every other
/// pixel shows the base.
///
/// Why it matters: a roof, a hill or a wave crest is a polygon. Its edge
/// must blend by the share each pixel lies inside.
#[test]
fn a_polygon_fills_its_inside_and_blends_its_edge() {
    // Given:
    let mut canvas = Canvas::new(4, 2, PALETTE);

    // When:
    canvas.polygon(&[(-0.5, -0.5), (4.0, -0.5), (4.0, 2.5), (-0.5, 2.5)], INK);

    // Then:
    let half = octant::mix(BASE, INK, 0.5);
    for y in 0..canvas.height() {
        for x in 0..canvas.width() {
            let expected = match (x, y) {
                (0..4, 0..3) => INK,
                (4, 0..3) => half,
                _ => BASE,
            };
            assert_eq!(canvas.get(x, y), Some(expected), "({x}, {y})");
        }
    }
}

/// A gradient starts on its top colour and ends on its bottom colour.
/// Given: a canvas in the base.
/// When: a gradient from the ink to foam fills rows 2 to 9.
/// Then: each pixel of row 2 is the ink, each of row 9 is foam, each of
/// row 5 is the mix three sevenths of the way, and rows outside the span
/// keep the base.
///
/// Why it matters: a sky or a sea shades between two theme colours. Its
/// end rows must be the theme colours themselves, or the gradient meets
/// the next band of the scene with a seam.
#[test]
fn a_gradient_runs_from_its_top_colour_to_its_bottom_colour() {
    // Given:
    let mut canvas = Canvas::new(2, 3, PALETTE);

    // When:
    canvas.gradient(2..10, INK, OTHER);

    // Then:
    let rows = canvas_rows(&canvas, 0..12);
    let width = canvas.width();
    assert_eq!(rows[2], vec![INK; width]);
    assert_eq!(rows[9], vec![OTHER; width]);
    assert_eq!(rows[5], vec![octant::mix(INK, OTHER, 3.0 / 7.0); width]);
    for y in [0, 1, 10, 11] {
        assert_eq!(rows[y], vec![BASE; width], "row {y}");
    }
}

/// Rose Pine for the terminal's own background, whose base shows the
/// terminal through.
const SEE_THROUGH: &Palette = &Palette {
    see_through: true,
    ..theme::ROSE_PINE
};

/// Over a see-through base an edge stays hard, and over another colour it
/// blends.
/// Given: a canvas whose base is see-through, with foam over its right
/// half.
/// When: a disc of ink is drawn across both halves, a line two pixels wide
/// runs over the base with rows half under it, and the base blends into a
/// pixel of foam at a quarter and at three quarters.
/// Then: each pixel over the base is the base or the ink, never a mix, and
/// a pixel half under the line is the ink. Over the foam the disc's edge
/// shows a mix of foam and the ink. The base at a quarter leaves the foam,
/// and at three quarters replaces it.
///
/// Why it matters: with the terminal's own background the base clears to
/// the terminal. A mix of the base and an ink is an opaque colour, so a
/// soft edge on the base would draw a dark or light ring over the
/// terminal round each shape.
#[test]
fn an_edge_over_a_see_through_base_stays_hard() {
    // Given:
    let mut canvas = Canvas::new(8, 4, SEE_THROUGH);
    let half = canvas.width() / 2;
    for y in 0..canvas.height() {
        for x in half..canvas.width() {
            canvas.set(x, y, OTHER);
        }
    }
    let foam = |x: usize| x >= half;

    // When:
    canvas.disc((8.0, 8.0), 5.3, INK);
    canvas.line((0.0, 13.0), (5.0, 13.0), 2.0, INK);
    canvas.blend(12, 0, BASE, 0.25);
    canvas.blend(13, 0, BASE, 0.75);

    // Then:
    let mut soft = 0;
    for y in 0..canvas.height() {
        for x in 0..canvas.width() {
            let colour = canvas.get(x, y).unwrap();
            if (x, y) == (12, 0) {
                assert_eq!(colour, OTHER, "the base at a quarter over foam");
            } else if (x, y) == (13, 0) {
                assert_eq!(colour, BASE, "the base at three quarters over foam");
            } else if !foam(x) {
                assert!(colour == BASE || colour == INK, "({x}, {y}) is {colour:?}");
            } else if colour != OTHER && colour != INK {
                assert!(
                    share_of(OTHER, INK, colour).is_some(),
                    "({x}, {y}) is {colour:?}"
                );
                soft += 1;
            }
        }
    }
    assert!(soft > 0, "no pixel over foam blends");
    for x in 0..4 {
        let column: Vec<Color> = (11..16).map(|y| canvas.get(x, y).unwrap()).collect();
        assert_eq!(
            column,
            [BASE, INK, INK, INK, BASE],
            "column {x} of the line"
        );
    }
}

/// A gradient from a see-through base turns at its middle row.
/// Given: a canvas whose base is see-through.
/// When: a gradient from the base to the ink fills rows 0 to 7.
/// Then: rows 0 to 3 show the base, and rows 4 to 7 show the ink.
///
/// Why it matters: a sky that fades into the terminal's own background
/// would paint a band of opaque mixes over the terminal.
#[test]
fn a_gradient_from_a_see_through_base_turns_at_its_middle() {
    // Given:
    let mut canvas = Canvas::new(1, 2, SEE_THROUGH);

    // When:
    canvas.gradient(0..8, BASE, INK);

    // Then:
    let colours: Vec<Color> = (0..8).map(|y| canvas.get(0, y).unwrap()).collect();
    assert_eq!(colours, [BASE, BASE, BASE, BASE, INK, INK, INK, INK]);
}

/// One colour blends as a pixel of the canvas blends it.
/// Given: the opaque and the see-through Rose Pine, and each pair of the
/// base, an ink and foam, one under the other.
/// When: the upper colour blends over the lower at shares from none to
/// whole, as one colour and as a pixel of a canvas.
/// Then: the colour is the canvas's pixel each time, and over a
/// see-through base it is one of the two colours and never a mix.
///
/// Why it matters: a scene that works out a shade ahead of time, such as a
/// ramp of heat or a fade, must keep the hard edge a canvas keeps, or it
/// paints a mix of the base over the terminal's own background.
#[test]
fn a_colour_blends_as_a_pixel_of_the_canvas() {
    // Given:
    for palette in [PALETTE, SEE_THROUGH] {
        let colours = [palette.base, INK, OTHER];
        for under in colours {
            for over in colours {
                for step in 0..=16 {
                    let share = step as f32 / 16.0;

                    // When:
                    let colour = octant::blend(palette, under, over, share);
                    let mut pixel = Canvas::new(1, 1, palette);
                    pixel.set(0, 0, under);
                    pixel.blend(0, 0, over, share);

                    // Then:
                    let at = format!("{over:?} over {under:?} at {share}");
                    assert_eq!(Some(colour), pixel.get(0, 0), "{at}");
                    if palette.see_through && (under == palette.base || over == palette.base) {
                        assert!(colour == under || colour == over, "{at} is {colour:?}");
                    }
                }
            }
        }
    }
}

/// A place on the screen shows what the last canvas drew there.
/// Given: the same pixels on Rose Pine and on Dawn, whose base differs, and
/// other pixels on Rose Pine.
/// When: each canvas draws in turn to one place, and then the first again.
/// Then: the place shows each time the cell that canvas draws at a place no
/// canvas drew before, and the two bases draw two different cells.
///
/// Why it matters: a frame draws each place again, often with the same
/// pixels. A cell kept from an earlier frame that showed after its pixels
/// or its base changed would leave a stale block in the picture.
#[test]
fn a_place_shows_what_its_last_canvas_drew() {
    // Given:
    let dawn = &theme::ROSE_PINE_DAWN;
    let mostly_base = [BASE, BASE, BASE, INK, INK, BASE, BASE, INK];
    let halves = [OTHER, OTHER, OTHER, OTHER, INK, INK, INK, INK];
    let draws = [
        (PALETTE, mostly_base),
        (dawn, mostly_base),
        (PALETTE, halves),
        (PALETTE, mostly_base),
    ];
    let canvas = |palette: &Palette, pixels: [Color; 8]| {
        let mut canvas = Canvas::new(1, 1, palette);
        for (pixel, colour) in pixels.into_iter().enumerate() {
            canvas.set(pixel % CELL_WIDTH, pixel / CELL_WIDTH, colour);
        }
        canvas
    };
    let screen = Rect::new(0, 0, 1 + draws.len() as u16, 1);
    let mut buffer = Buffer::empty(screen);

    let mut shown = Vec::new();
    for (fresh, (palette, pixels)) in draws.into_iter().enumerate() {
        // When:
        canvas(palette, pixels).render(Rect::new(0, 0, 1, 1), &mut buffer);
        canvas(palette, pixels).render(Rect::new(1 + fresh as u16, 0, 1, 1), &mut buffer);

        // Then:
        assert_eq!(
            buffer[(0, 0)],
            buffer[(1 + fresh as u16, 0)],
            "draw {fresh}"
        );
        shown.push(buffer[(0, 0)].clone());
    }
    assert_ne!(shown[0], shown[1], "Rose Pine and Dawn draw the same cell");
}
