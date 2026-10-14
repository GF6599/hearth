#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeSet;

use hearth::scene::{Rng, Scene};
use hearth::scenes::life::{HOLD_GENERATIONS, Life, OLD, YOUNG};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

fn render(scene: &Life, area: Rect) -> Buffer {
    let mut buffer = Buffer::empty(area);
    scene.render(area, &mut buffer);
    buffer
}

/// The live cells read back from a render, as (column, row) on the board,
/// where a terminal row holds two board rows: the upper one in the
/// foreground and the lower one in the background.
fn alive(buffer: &Buffer, area: Rect) -> BTreeSet<(u16, u16)> {
    let mut cells = BTreeSet::new();
    for position in area.positions() {
        let cell = &buffer[position];
        let x = position.x - area.x;
        let upper = (position.y - area.y) * 2;
        if cell.fg == YOUNG || cell.fg == OLD {
            cells.insert((x, upper));
        }
        if cell.bg == YOUNG || cell.bg == OLD {
            cells.insert((x, upper + 1));
        }
    }
    cells
}

fn ticked(mut scene: Life, area: Rect, generations: usize) -> Life {
    for _ in 0..generations {
        scene.tick(area);
    }
    scene
}

const GLIDER: &[&str] = &[
    ".#........",
    "..#.......",
    "###.......",
    "..........",
    "..........",
    "..........",
    "..........",
    "..........",
    "..........",
    "..........",
];

/// Given: a glider at the top left of a ten by ten board.
/// When: four generations pass.
/// Then: the same five cells sit one column right and one row down.
///
/// Why it matters: the glider is the smallest pattern that exercises every
/// rule, and a wrong neighbour count breaks it within a generation.
#[test]
fn a_glider_moves_one_cell_in_four_generations() {
    // Given:
    let area = Rect::new(0, 0, 10, 5);
    let scene = Life::from_pattern(Rng::seed_from_u64(7), GLIDER);

    // When:
    let scene = ticked(scene, area, 4);

    // Then:
    let expected: BTreeSet<(u16, u16)> = [(2, 1), (3, 2), (1, 3), (2, 3), (3, 3)].into();
    assert_eq!(alive(&render(&scene, area), area), expected);
}

const BLINKER: &[&str] = &[
    "..........",
    "..........",
    "..........",
    "..........",
    "...###....",
    "..........",
    "..........",
    "..........",
    "..........",
    "..........",
];

/// Given: a blinker alone on a ten by ten board.
/// When: the hold count of generations passes, then one more.
/// Then: the board is still a blinker at the hold count, and is something
/// else one generation later.
///
/// Why it matters: a board that settles into an oscillator looks frozen,
/// and the restart is what keeps the scene moving.
#[test]
fn a_blinker_reseeds_after_the_hold_count() {
    // Given:
    let area = Rect::new(0, 0, 10, 5);
    let horizontal: BTreeSet<(u16, u16)> = [(3, 4), (4, 4), (5, 4)].into();
    let vertical: BTreeSet<(u16, u16)> = [(4, 3), (4, 4), (4, 5)].into();

    // When:
    let held = ticked(
        Life::from_pattern(Rng::seed_from_u64(7), BLINKER),
        area,
        HOLD_GENERATIONS,
    );
    let reseeded = ticked(
        Life::from_pattern(Rng::seed_from_u64(7), BLINKER),
        area,
        HOLD_GENERATIONS + 1,
    );

    // Then:
    let before = alive(&render(&held, area), area);
    assert!(before == horizontal || before == vertical, "{before:?}");
    let after = alive(&render(&reseeded, area), area);
    assert!(after != horizontal && after != vertical, "{after:?}");
}

/// Given: a board with no live cell.
/// When: one generation passes.
/// Then: live cells show, and a cell that was just born is in the young
/// colour.
///
/// Why it matters: a dead board is stable, so without its own rule it
/// would sit black for the whole hold count.
#[test]
fn a_dead_board_reseeds_at_once() {
    // Given:
    let area = Rect::new(0, 0, 10, 5);
    let dead = vec![".........."; 10];

    // When:
    let scene = ticked(Life::from_pattern(Rng::seed_from_u64(7), &dead), area, 1);

    // Then:
    let buffer = render(&scene, area);
    assert!(!alive(&buffer, area).is_empty());
    assert!(
        area.positions()
            .any(|p| buffer[p].fg == YOUNG || buffer[p].bg == YOUNG)
    );
}
