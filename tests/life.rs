#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeSet;

use hearth::braille;
use hearth::scene::{Rng, Scene};
use hearth::scenes::life::{HOLD_GENERATIONS, Life};
use hearth::theme;
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

fn render(scene: &Life, area: Rect) -> Buffer {
    let mut buffer = Buffer::empty(area);
    scene.render(area, &mut buffer);
    buffer
}

/// The live cells read back from a render, as (column, row) on the board,
/// where a terminal cell holds a braille glyph of two board columns and
/// four board rows.
fn alive(buffer: &Buffer, area: Rect) -> BTreeSet<(u16, u16)> {
    let mut cells = BTreeSet::new();
    for position in area.positions() {
        let glyph = buffer[position].symbol().chars().next().unwrap_or(' ');
        let dots = u32::from(glyph).wrapping_sub(0x2800);
        if dots > 0xff {
            continue;
        }
        let (x, y) = ((position.x - area.x) * 2, (position.y - area.y) * 4);
        for column in 0..2 {
            for row in 0..4 {
                if dots & braille::dot(usize::from(column), usize::from(row)) != 0 {
                    cells.insert((x + column, y + row));
                }
            }
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
    "..........",
    "..........",
];

/// Given: a glider at the top left of a ten by twelve board.
/// When: four generations pass.
/// Then: the same five cells sit one column right and one row down.
///
/// Why it matters: the glider is the smallest pattern that exercises every
/// rule, and a wrong neighbour count breaks it within a generation.
#[test]
fn a_glider_moves_one_cell_in_four_generations() {
    // Given:
    let area = Rect::new(0, 0, 5, 3);
    let scene = Life::from_pattern(Rng::seed_from_u64(7), &theme::ROSE_PINE, GLIDER);

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
    "..........",
    "..........",
];

/// Given: a blinker alone on a ten by twelve board.
/// When: the hold count of generations passes, then one more.
/// Then: the board is still a blinker at the hold count, and is something
/// else one generation later.
///
/// Why it matters: a board that settles into an oscillator looks frozen,
/// and the restart is what keeps the scene moving.
#[test]
fn a_blinker_reseeds_after_the_hold_count() {
    // Given:
    let area = Rect::new(0, 0, 5, 3);
    let horizontal: BTreeSet<(u16, u16)> = [(3, 4), (4, 4), (5, 4)].into();
    let vertical: BTreeSet<(u16, u16)> = [(4, 3), (4, 4), (4, 5)].into();

    // When:
    let held = ticked(
        Life::from_pattern(Rng::seed_from_u64(7), &theme::ROSE_PINE, BLINKER),
        area,
        HOLD_GENERATIONS,
    );
    let reseeded = ticked(
        Life::from_pattern(Rng::seed_from_u64(7), &theme::ROSE_PINE, BLINKER),
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
    let area = Rect::new(0, 0, 5, 3);
    let dead = vec![".........."; 12];

    // When:
    let scene = ticked(
        Life::from_pattern(Rng::seed_from_u64(7), &theme::ROSE_PINE, &dead),
        area,
        1,
    );

    // Then:
    let buffer = render(&scene, area);
    assert!(!alive(&buffer, area).is_empty());
    assert!(
        area.positions()
            .any(|p| buffer[p].fg == theme::ROSE_PINE.rose)
    );
}

/// Each terminal cell draws two board columns and four board rows in
/// braille, and takes the young colour when most of its live cells were
/// just born.
/// Given: a blinker on a ten by twelve board in a five by three area.
/// When: one generation turns it from across to down.
/// Then: the cell that holds only a new end of the blinker is in the young
/// colour, and the cell that holds the old middle and the other new end is
/// in the settled colour.
///
/// Why it matters: a board four times as fine reads as more detail only if
/// the colour still tells a birth from a cell that stays.
#[test]
fn a_cell_takes_the_colour_of_most_of_its_live_cells() {
    // Given:
    let area = Rect::new(0, 0, 5, 3);
    let scene = Life::from_pattern(Rng::seed_from_u64(7), &theme::ROSE_PINE, BLINKER);

    // When:
    let scene = ticked(scene, area, 1);

    // Then:
    let buffer = render(&scene, area);
    let vertical: BTreeSet<(u16, u16)> = [(4, 3), (4, 4), (4, 5)].into();
    assert_eq!(alive(&buffer, area), vertical);
    assert_eq!(buffer[(2, 0)].fg, theme::ROSE_PINE.rose);
    assert_eq!(buffer[(2, 1)].fg, theme::ROSE_PINE.iris);
}
