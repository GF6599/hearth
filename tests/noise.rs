#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use hearth::octant::{self, CELL_HEIGHT, CELL_WIDTH};
use hearth::scene::{Rng, Scene};
use hearth::scenes::noise::Noise;
use hearth::theme;
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);

fn render(scene: &Noise) -> Buffer {
    let mut buffer = Buffer::empty(AREA);
    scene.render(AREA, &mut buffer);
    buffer
}

/// The pixels of an octant glyph, as its bits, or none for another glyph.
fn bits(symbol: &str) -> Option<u8> {
    (0..=u8::MAX).find(|&bits| octant::glyph(bits).to_string() == symbol)
}

/// The colour each pixel shows, cell by cell: the foreground where the
/// glyph sets the pixel, else the background.
fn pixels(buffer: &Buffer) -> Vec<Color> {
    buffer
        .area
        .positions()
        .flat_map(|position| {
            let cell = &buffer[position];
            let bits = bits(cell.symbol())
                .unwrap_or_else(|| panic!("{:?} is not an octant glyph", cell.symbol()));
            (0..CELL_WIDTH * CELL_HEIGHT).map(move |pixel| {
                if bits & 1 << pixel == 0 {
                    cell.bg
                } else {
                    cell.fg
                }
            })
        })
        .collect()
}

/// Given: the scene on a hundred by thirty area.
/// When: it ticks once and renders.
/// Then: every cell is an octant glyph in base, muted, subtle or text, and
/// muted, subtle and text each show.
///
/// Why it matters: the level of the noise shows as a ramp of palette
/// colours from the base to the text, as the glyphs went from a blank to a
/// star. A colour off the palette clashes with the theme.
#[test]
fn noise_draws_its_levels_in_a_ramp_of_palette_colours() {
    // Given:
    let palette = &theme::ROSE_PINE;
    let mut scene = Noise::new(Rng::seed_from_u64(7), palette);

    // When:
    scene.tick(AREA);
    let shown = pixels(&render(&scene));

    // Then:
    let ramp = [palette.base, palette.muted, palette.subtle, palette.text];
    let stray = shown.iter().find(|colour| !ramp.contains(colour));
    assert_eq!(stray, None, "a pixel is off the ramp");
    for level in &ramp[1..] {
        assert!(shown.contains(level), "no pixel shows {level:?}");
    }
}

/// Given: the scene on a hundred by thirty area.
/// When: it ticks once and renders.
/// Then: nine cells in ten or more show two colours in one octant glyph.
///
/// Why it matters: the grain of the noise is a pixel, a quarter of a cell
/// tall. A cell in one colour throughout is the old grain of a whole cell.
#[test]
fn noise_varies_within_a_cell() {
    // Given:
    let mut scene = Noise::new(Rng::seed_from_u64(7), &theme::ROSE_PINE);

    // When:
    scene.tick(AREA);
    let buffer = render(&scene);

    // Then:
    let split = buffer
        .area
        .positions()
        .filter(|&position| {
            let cell = &buffer[position];
            let bits = bits(cell.symbol());
            cell.fg != cell.bg && bits.is_some_and(|bits| bits != 0 && bits != u8::MAX)
        })
        .count();
    let cells = usize::try_from(AREA.area()).unwrap();
    assert!(split * 10 >= cells * 9, "{split} of {cells} cells split");
}

/// Noise changes some of its pixels each tick, not all of them.
/// Given: the scene on a hundred by thirty area, ticked once.
/// When: it ticks fifty times more.
/// Then: from one tick to the next, between a quarter and a little over
/// half of the pixels show another colour, on average.
///
/// Why it matters: a screen that draws every pixel again each tick flickers
/// too fast to rest the eye.
#[test]
fn noise_changes_some_of_its_pixels_each_tick() {
    // Given:
    let mut scene = Noise::new(Rng::seed_from_u64(7), &theme::ROSE_PINE);
    scene.tick(AREA);
    let mut before = pixels(&render(&scene));
    let mut shares = Vec::new();

    for _ in 0..50 {
        // When:
        scene.tick(AREA);
        let after = pixels(&render(&scene));
        let changed = before.iter().zip(&after).filter(|(a, b)| a != b).count();
        shares.push(changed as f64 / after.len() as f64);
        before = after;
    }

    // Then:
    let mean = shares.iter().sum::<f64>() / shares.len() as f64;
    assert!(
        (0.25..0.55).contains(&mean),
        "{mean} of the pixels changed a tick"
    );
}
