#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::HashMap;

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

/// The heat level behind each colour of the ramp, which must be distinct
/// for the read-back below to be exact.
fn level_of_colour() -> HashMap<Color, u16> {
    let mut map = HashMap::new();
    for level in 0..=LEVELS {
        let colour = fire::color(level);
        assert!(
            map.insert(colour, level).is_none(),
            "two levels share {colour:?}"
        );
    }
    map
}

/// The heat grid read back from a render: the foreground of a cell is the
/// upper heat row and the background the lower one, so the grid has twice
/// the rows of the area.
fn levels(buffer: &Buffer, area: Rect) -> Vec<Vec<u16>> {
    let map = level_of_colour();
    let mut rows = Vec::with_capacity(usize::from(area.height) * 2);
    for y in area.rows() {
        let upper: Vec<u16> = y
            .columns()
            .map(|cell| map[&buffer[(cell.x, cell.y)].fg])
            .collect();
        let lower: Vec<u16> = y
            .columns()
            .map(|cell| map[&buffer[(cell.x, cell.y)].bg])
            .collect();
        rows.push(upper);
        rows.push(lower);
    }
    rows
}

/// Given: the ramp.
/// When: its two ends are read.
/// Then: no heat is the base colour and full heat is the text colour.
#[test]
fn the_ramp_runs_from_base_to_text() {
    assert_eq!(fire::color(0), theme::BASE);
    assert_eq!(fire::color(LEVELS), theme::TEXT);
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
        let grid = levels(&render(&scene, area, area), area);
        let bottom = grid.last().unwrap();
        assert!(
            bottom.iter().all(|&level| level == LEVELS),
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
    let mut previous = levels(&render(&scene, area, area), area);

    // When:
    for tick in 0..40 {
        scene.tick(area);
        let current = levels(&render(&scene, area, area), area);

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
        if let Some(bottom) = levels(&buffer, area).last() {
            assert!(bottom.iter().all(|&level| level == LEVELS), "{bottom:?}");
        }
    }
}
