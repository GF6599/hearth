#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use hearth::scene::{Rng, Scene};
use hearth::scenes::matrix::{self, HEAD, Matrix};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;
use unicode_width::UnicodeWidthChar;

fn scene(seed: u64) -> Matrix {
    Matrix::new(Rng::seed_from_u64(seed))
}

fn ticked(seed: u64, area: Rect, ticks: usize) -> Matrix {
    let mut scene = scene(seed);
    for _ in 0..ticks {
        scene.tick(area);
    }
    scene
}

fn render(scene: &Matrix, area: Rect, canvas: Rect) -> Buffer {
    let mut buffer = Buffer::empty(canvas);
    scene.render(area, &mut buffer);
    buffer
}

fn cells(buffer: &Buffer) -> impl Iterator<Item = (Position, &ratatui::buffer::Cell)> {
    buffer
        .area
        .positions()
        .map(|position| (position, &buffer[position]))
}

/// The row of the head in each column, or none where no head shows.
fn heads(buffer: &Buffer) -> Vec<Option<u16>> {
    (0..buffer.area.width)
        .map(|x| {
            let rows: Vec<u16> = (0..buffer.area.height)
                .filter(|&y| buffer[(x, y)].fg == HEAD)
                .collect();
            assert!(rows.len() <= 1, "column {x} shows {} heads", rows.len());
            rows.first().copied()
        })
        .collect()
}

/// Given: the glyph table.
/// When: each glyph is measured.
/// Then: every one is one cell wide.
///
/// Why it matters: a two-cell glyph pushes the rest of its row one cell
/// right, which tears every column to its right.
#[test]
fn every_glyph_in_the_table_is_one_cell_wide() {
    for glyph in matrix::glyphs() {
        assert_eq!(glyph.width(), Some(1), "{glyph:?} is not one cell");
    }
}

/// Given: a scene ticked forty times on a small area.
/// When: it renders.
/// Then: it drew something, and every drawn cell is a glyph from the table.
///
/// Why it matters: the width test above covers the render only when the
/// render draws from the table it measured.
#[test]
fn every_drawn_cell_comes_from_the_glyph_table() {
    // Given, when:
    let area = Rect::new(0, 0, 12, 10);
    let buffer = render(&ticked(7, area, 40), area, area);
    let table: Vec<char> = matrix::glyphs().collect();

    // Then:
    let drawn: Vec<char> = cells(&buffer)
        .filter(|(_, cell)| cell.symbol() != " ")
        .map(|(_, cell)| cell.symbol().chars().next().unwrap())
        .collect();
    assert!(!drawn.is_empty());
    for glyph in drawn {
        assert!(table.contains(&glyph), "{glyph:?} is not in the table");
    }
}

/// Given: a scene on an eight by sixteen area.
/// When: it ticks sixty times, with a render after each.
/// Then: between two frames that both show a column's head, the head stays
/// or falls one row, never more and never up, and every column fell.
#[test]
fn a_head_falls_one_row_at_a_time() {
    // Given:
    let area = Rect::new(0, 0, 8, 16);
    let mut scene = ticked(7, area, 1);
    let mut previous = heads(&render(&scene, area, area));
    let mut fell = vec![false; usize::from(area.width)];

    // When:
    for _ in 0..60 {
        scene.tick(area);
        let current = heads(&render(&scene, area, area));

        // Then:
        for (x, (before, after)) in previous.iter().zip(&current).enumerate() {
            if let (Some(before), Some(after)) = (before, after) {
                assert!(
                    *after == *before || *after == before + 1,
                    "column {x} head went from row {before} to row {after}"
                );
                fell[x] |= after > before;
            }
        }
        previous = current;
    }
    assert!(
        fell.iter().all(|&moved| moved),
        "a column never fell: {fell:?}"
    );
}

/// Given: areas of one column, one row, one cell and no cells, inside a
/// larger canvas.
/// When: the scene ticks and renders fifty times on each.
/// Then: no cell outside the area is touched, and each non-empty area
/// gets at least one glyph.
#[test]
fn nothing_is_written_outside_a_narrow_area() {
    let canvas = Rect::new(0, 0, 10, 10);
    for area in [
        Rect::new(3, 3, 1, 6),
        Rect::new(3, 3, 6, 1),
        Rect::new(3, 3, 1, 1),
        Rect::new(3, 3, 0, 0),
    ] {
        // Given:
        let mut scene = scene(7);
        let mut drew = false;

        // When:
        for _ in 0..50 {
            scene.tick(area);
            let buffer = render(&scene, area, canvas);

            // Then:
            for (position, cell) in cells(&buffer) {
                if area.contains(position) {
                    drew |= cell.symbol() != " ";
                } else {
                    assert_eq!(cell.symbol(), " ", "glyph outside {area:?} at {position:?}");
                    assert_eq!(
                        cell.fg,
                        Color::Reset,
                        "colour outside {area:?} at {position:?}"
                    );
                }
            }
        }
        assert_eq!(drew, area.area() > 0, "drew={drew} on {area:?}");
    }
}
