#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use hearth::scene::{Rng, Scene};
use hearth::scenes::bonsai::{Bonsai, HOLD_TICKS, POT_HEIGHT};
use hearth::theme;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use rand::SeedableRng;

const TREE_GLYPHS: &[&str] = &["/", "\\", "|", "~", "&", "_"];

fn scene(seed: u64) -> Bonsai {
    Bonsai::new(Rng::seed_from_u64(seed))
}

fn render(scene: &Bonsai, area: Rect, canvas: Rect) -> Buffer {
    let mut buffer = Buffer::empty(canvas);
    scene.render(area, &mut buffer);
    buffer
}

fn row_text(buffer: &Buffer, y: u16) -> String {
    (buffer.area.x..buffer.area.right())
        .map(|x| buffer[(x, y)].symbol().to_string())
        .collect()
}

/// Cells above the pot that hold a glyph.
fn tree_cells(buffer: &Buffer, area: Rect) -> usize {
    (area.y..area.bottom() - POT_HEIGHT)
        .flat_map(|y| (area.x..area.right()).map(move |x| (x, y)))
        .filter(|&(x, y)| buffer[(x, y)].symbol() != " ")
        .count()
}

/// Given: a scene on a thirty by ten area.
/// When: it ticks once, and then three hundred more times.
/// Then: the pot's top rule spans the bottom rows in the frame's runes, its
/// margins are equal within a cell, and no tree glyph ever lands on it.
///
/// Why it matters: a branch that grows down into the pot reads as a bug
/// in the one static part of the scene.
#[test]
fn the_pot_is_centred_and_never_overwritten() {
    // Given, when:
    let area = Rect::new(0, 0, 30, 10);
    let mut scene = scene(7);
    scene.tick(area);
    let first = render(&scene, area, area);

    // Then:
    let set = theme::BORDER.to_border_set();
    let top = row_text(&first, area.bottom() - POT_HEIGHT);
    let left = top.chars().take_while(|c| *c == ' ').count();
    let right = top.chars().rev().take_while(|c| *c == ' ').count();
    assert!(top.trim().starts_with(set.top_left), "{top:?}");
    assert!(top.trim().ends_with(set.top_right), "{top:?}");
    assert!(left.abs_diff(right) <= 1, "{top:?}");

    // And:
    for _ in 0..300 {
        scene.tick(area);
        let buffer = render(&scene, area, area);
        for y in area.bottom() - POT_HEIGHT..area.bottom() {
            let row = row_text(&buffer, y);
            assert!(
                !TREE_GLYPHS.iter().any(|glyph| row.contains(glyph)),
                "tree glyph on pot row {y}: {row:?}"
            );
        }
    }
}

/// Given: a scene on a forty by sixteen area.
/// When: it ticks eight hundred times, with a render after each.
/// Then: the tree's cell count never falls while it grows, reaches a peak,
/// holds there for the hold count, and then falls as a new tree starts.
#[test]
fn the_tree_grows_then_holds_then_starts_over() {
    // Given, when:
    let area = Rect::new(0, 0, 40, 16);
    let mut scene = scene(7);
    let counts: Vec<usize> = (0..800)
        .map(|_| {
            scene.tick(area);
            tree_cells(&render(&scene, area, area), area)
        })
        .collect();

    // Then:
    let peak = *counts.iter().max().unwrap();
    let first_peak = counts.iter().position(|&c| c == peak).unwrap();
    assert!(peak > counts[0], "the tree never grew: {counts:?}");
    assert!(
        counts[..first_peak].windows(2).all(|w| w[0] <= w[1]),
        "the tree shrank while growing: {counts:?}"
    );
    let held = counts[first_peak..].iter().take_while(|&&c| c == peak).count();
    assert!(held >= HOLD_TICKS, "held {held} ticks, expected {HOLD_TICKS}");
    assert!(
        counts[first_peak + held..].iter().any(|&c| c < peak),
        "no new tree after the hold"
    );
}

/// Given: a short and narrow area, one cell, and no cells, each inside a
/// larger canvas.
/// When: the scene ticks and renders a hundred times on each.
/// Then: no cell outside the area is touched.
#[test]
fn nothing_is_written_outside_a_small_area() {
    let canvas = Rect::new(0, 0, 12, 12);
    for area in [
        Rect::new(3, 3, 6, 4),
        Rect::new(3, 3, 1, 1),
        Rect::new(3, 3, 0, 0),
    ] {
        let mut scene = scene(7);
        for _ in 0..100 {
            scene.tick(area);
            let buffer = render(&scene, area, canvas);
            for position in canvas.positions() {
                if !area.contains(position) {
                    assert_eq!(buffer[position].symbol(), " ", "glyph outside {area:?}");
                    assert_eq!(buffer[position].fg, Color::Reset, "colour outside {area:?}");
                }
            }
        }
    }
}
