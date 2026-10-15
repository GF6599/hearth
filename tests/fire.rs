#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use hearth::scene::{Rng, Scene};
use hearth::scenes::fire::{self, Fire, LEVELS};
use hearth::theme;
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

fn scene(seed: u64) -> Fire {
    Fire::new(Rng::seed_from_u64(seed))
}

fn render(scene: &Fire, area: Rect, canvas: Rect) -> Buffer {
    let mut buffer = Buffer::empty(canvas);
    scene.render(area, &mut buffer);
    buffer
}

/// The agreed ramp, from no heat to full heat.
const RAMP: [Color; 7] = [
    theme::BASE,
    theme::OVERLAY,
    theme::MUTED,
    theme::LOVE,
    theme::ROSE,
    theme::GOLD,
    theme::TEXT,
];

const FULL: usize = RAMP.len() - 1;

const PALETTE: [Color; 12] = [
    theme::BASE,
    theme::SURFACE,
    theme::OVERLAY,
    theme::MUTED,
    theme::SUBTLE,
    theme::TEXT,
    theme::LOVE,
    theme::GOLD,
    theme::ROSE,
    theme::PINE,
    theme::FOAM,
    theme::IRIS,
];

fn band_of(colour: Color) -> usize {
    RAMP.iter()
        .position(|stop| *stop == colour)
        .unwrap_or_else(|| panic!("{colour:?} is not on the ramp"))
}

/// The heat grid read back from a render as bands of the ramp: the
/// foreground of a cell is the upper heat row and the background the lower
/// one, so the grid has twice the rows of the area.
fn bands(buffer: &Buffer, area: Rect) -> Vec<Vec<usize>> {
    let mut rows = Vec::with_capacity(usize::from(area.height) * 2);
    for y in area.rows() {
        let upper: Vec<usize> = y
            .columns()
            .map(|cell| band_of(buffer[(cell.x, cell.y)].fg))
            .collect();
        let lower: Vec<usize> = y
            .columns()
            .map(|cell| band_of(buffer[(cell.x, cell.y)].bg))
            .collect();
        rows.push(upper);
        rows.push(lower);
    }
    rows
}

/// Given: the ramp.
/// When: each heat level from none to full is read.
/// Then: the colours climb through base, overlay, muted, love, rose, gold
/// and text, in that order, with each stop shown.
///
/// Why it matters: a blend between two stops makes a colour that Rose Pine
/// does not have, and a skipped stop leaves a jump in the flame.
#[test]
fn the_ramp_climbs_through_the_seven_stops() {
    // When:
    let mut shown: Vec<Color> = (0..=LEVELS).map(fire::color).collect();
    shown.dedup();

    // Then:
    assert_eq!(shown, RAMP);
}

/// Given: a scene on a forty by twenty area.
/// When: it ticks thirty times and renders.
/// Then: every cell's foreground and background is a Rose Pine colour.
///
/// Why it matters: one colour outside the palette reads as a fault on a
/// surface where every other cell is Rose Pine.
#[test]
fn every_cell_is_a_rose_pine_colour() {
    // Given:
    let area = Rect::new(0, 0, 40, 20);
    let mut scene = scene(7);

    // When:
    for _ in 0..30 {
        scene.tick(area);
    }
    let buffer = render(&scene, area, area);

    // Then:
    for position in area.positions() {
        let cell = &buffer[position];
        for colour in [cell.fg, cell.bg] {
            assert!(PALETTE.contains(&colour), "{colour:?} at {position:?}");
        }
    }
}

/// Given: a scene on a twelve by six area.
/// When: it ticks and renders twenty times.
/// Then: on every frame the lower half of every bottom cell is at full
/// heat.
///
/// Why it matters: the bottom row is the fuel. A tick that decays it puts
/// the fire out within seconds.
#[test]
fn the_bottom_row_stays_at_full_heat() {
    let area = Rect::new(0, 0, 12, 6);
    let mut scene = scene(7);
    for tick in 0..20 {
        scene.tick(area);
        let grid = bands(&render(&scene, area, area), area);
        let bottom = grid.last().unwrap();
        assert!(
            bottom.iter().all(|&band| band == FULL),
            "tick {tick}: bottom row {bottom:?}"
        );
    }
}

/// Given: a scene on a sixteen by eight area.
/// When: it ticks forty times, with a render after each.
/// Then: every cell's heat is at most the hottest of the three cells below
/// it on the frame before, and a cell above three cold cells is cold.
///
/// Why it matters: heat that appears from nowhere, or climbs, reads as
/// sparks in the sky instead of a fire.
#[test]
fn heat_only_comes_from_below() {
    // Given:
    let area = Rect::new(0, 0, 16, 8);
    let mut scene = scene(7);
    scene.tick(area);
    let mut previous = bands(&render(&scene, area, area), area);

    // When:
    for tick in 0..40 {
        scene.tick(area);
        let current = bands(&render(&scene, area, area), area);

        // Then:
        for (y, row) in current.iter().enumerate().take(current.len() - 1) {
            let below = &previous[y + 1];
            for (x, &heat) in row.iter().enumerate() {
                let hottest_below = (x.saturating_sub(1)..=(x + 1).min(below.len() - 1))
                    .map(|sx| below[sx])
                    .max()
                    .unwrap();
                assert!(
                    heat <= hottest_below,
                    "tick {tick}: heat {heat} at ({x}, {y}) above at most {hottest_below}"
                );
            }
        }
        previous = current;
    }
}

/// Given: a scene that ticked on a twenty by ten area.
/// When: it ticks and renders on a five by three area inside a larger
/// canvas, then on one cell, then on none.
/// Then: it draws without a panic, the bottom row is at full heat, and no
/// cell outside the area is touched.
#[test]
fn a_smaller_area_drops_the_old_buffer() {
    // Given:
    let canvas = Rect::new(0, 0, 10, 10);
    let mut scene = scene(7);
    for _ in 0..5 {
        scene.tick(Rect::new(0, 0, 20, 10));
    }

    // When, then:
    for area in [
        Rect::new(2, 2, 5, 3),
        Rect::new(2, 2, 1, 1),
        Rect::new(2, 2, 0, 0),
    ] {
        scene.tick(area);
        let buffer = render(&scene, area, canvas);
        for position in canvas.positions() {
            if !area.contains(position) {
                assert_eq!(buffer[position].symbol(), " ", "glyph outside {area:?}");
                assert_eq!(buffer[position].bg, Color::Reset, "colour outside {area:?}");
            }
        }
        if let Some(bottom) = bands(&buffer, area).last() {
            assert!(bottom.iter().all(|&band| band == FULL), "{bottom:?}");
        }
    }
}
