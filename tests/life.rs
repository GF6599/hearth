#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pixels;

use std::collections::BTreeSet;

use hearth::octant;
use hearth::scene::{Rng, Scene};
use hearth::scenes::life::{HOLD_GENERATIONS, Life};
use hearth::theme;
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

fn render(scene: &Life, area: Rect) -> Buffer {
    let mut buffer = Buffer::empty(area);
    scene.render(area, &mut buffer);
    buffer
}

/// The live cells read back from a render, as (column, row) on the board,
/// where a terminal cell holds an octant glyph of two board columns and
/// four board rows, and a live cell shows in front of the base.
fn alive(buffer: &Buffer, area: Rect) -> BTreeSet<(u16, u16)> {
    pixels::showing(buffer, area, |colour| colour != theme::ROSE_PINE.base)
        .into_iter()
        .map(|((x, y), _)| (x as u16, y as u16))
        .collect()
}

/// The live cells of the board, as (column, row).
fn board(scene: &Life) -> BTreeSet<(usize, usize)> {
    scene.live_cells().collect()
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
    let expected: BTreeSet<(usize, usize)> = [(2, 1), (3, 2), (1, 3), (2, 3), (3, 3)].into();
    assert_eq!(board(&scene), expected);
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
    let horizontal: BTreeSet<(usize, usize)> = [(3, 4), (4, 4), (5, 4)].into();
    let vertical: BTreeSet<(usize, usize)> = [(4, 3), (4, 4), (4, 5)].into();

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
    let before = board(&held);
    assert!(before == horizontal || before == vertical, "{before:?}");
    let after = board(&reseeded);
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

/// The colour the pixel at (x, y) shows, with (0, 0) at the top left.
fn pixel(scene: &Life, area: Rect, (x, y): (usize, usize)) -> Color {
    let rows = pixels::grid(&render(scene, area), area, |_, cell| {
        pixels::shown(cell).unwrap_or_else(|| panic!("{:?} is not an octant glyph", cell.symbol()))
    });
    rows[y][x]
}

/// How far `colour` lies from `from` towards `to`, from 0 to 1, or none for
/// a colour that is no mix of the two. Either end reads exactly. A mix is
/// read back to within two in each channel, so the rounding of a mix to 8
/// bits does not count.
fn share(colour: Color, from: Color, to: Color) -> Option<f32> {
    const STEPS: u16 = 128;
    if colour == from {
        return Some(0.0);
    }
    if colour == to {
        return Some(1.0);
    }
    let Color::Rgb(r, g, b) = colour else {
        return None;
    };
    (0..=STEPS)
        .map(|step| f32::from(step) / f32::from(STEPS))
        .filter_map(|share| {
            let Color::Rgb(mr, mg, mb) = octant::mix(from, to, share) else {
                return None;
            };
            let gap = [r.abs_diff(mr), g.abs_diff(mg), b.abs_diff(mb)]
                .into_iter()
                .max()
                .unwrap_or(0);
            (gap <= 2).then_some((gap, share))
        })
        .min_by_key(|(gap, _)| *gap)
        .map(|(_, share)| share)
}

/// A block, which never changes, and one lone cell at (2, 2), which dies
/// in the first generation. Each sits alone in its terminal cell.
const LONE: &[&str] = &[
    "..........",
    "..........",
    "..#.......",
    "..........",
    "..........",
    "..........",
    "..........",
    "..........",
    "......##..",
    "......##..",
    "..........",
    "..........",
];

/// A cell that dies fades out over a few ticks.
/// Given: a lone cell beside a block that keeps the board alive.
/// When: the lone cell dies, and the board ticks on.
/// Then: for three ticks or more its pixel is a mix of its live colour and
/// the base, nearer the base each tick, and within ten ticks it is the
/// base.
///
/// Why it matters: a cell that drops to the base in one tick blinks out,
/// and a board of thousands of cells then reads as hard flicker. A fade
/// leaves a soft trail behind what moves.
#[test]
fn a_dead_cell_fades_to_the_base() {
    // Given:
    let area = Rect::new(0, 0, 5, 3);
    let palette = &theme::ROSE_PINE;
    let mut scene = Life::from_pattern(Rng::seed_from_u64(7), palette, LONE);
    let live = pixel(&scene, area, (2, 2));
    let mut shares = Vec::new();

    // When:
    for _ in 0..10 {
        scene.tick(area);
        let colour = pixel(&scene, area, (2, 2));
        shares.push(
            share(colour, palette.base, live)
                .unwrap_or_else(|| panic!("{colour:?} is no mix of the base and {live:?}")),
        );
    }

    // Then:
    let fading = shares.iter().take_while(|share| **share > 0.0).count();
    assert!(fading >= 3, "the cell fades for {fading} ticks: {shares:?}");
    assert!(
        shares[..fading].windows(2).all(|pair| pair[1] < pair[0]) && shares[0] < 1.0,
        "the fade does not fall each tick: {shares:?}"
    );
    assert!(
        shares[fading..].iter().all(|share| *share == 0.0),
        "the cell is not the base within ten ticks: {shares:?}"
    );
}

/// A fade glides between two ticks.
/// Given: the lone cell of the last test, dead for one tick.
/// When: the scene is half way to its next tick.
/// Then: the pixel lies between its colours at the two ticks.
///
/// Why it matters: the app draws four frames a tick. A fade that steps
/// once a tick shows each step four frames long.
#[test]
fn a_fade_glides_between_ticks() {
    // Given:
    let area = Rect::new(0, 0, 5, 3);
    let palette = &theme::ROSE_PINE;
    let mut scene = Life::from_pattern(Rng::seed_from_u64(7), palette, LONE);
    let live = pixel(&scene, area, (2, 2));
    let mut at = |scene: &Life| share(pixel(scene, area, (2, 2)), palette.base, live).unwrap();
    scene.tick(area);
    let first = at(&scene);

    // When:
    scene.set_progress(0.5);
    let half = at(&scene);

    // Then:
    scene.set_progress(0.0);
    scene.tick(area);
    let second = at(&scene);
    assert!(
        second < half && half < first,
        "{first} then {half} then {second}"
    );
}

/// A block never changes, so its cells age.
const BLOCK: &[&str] = &[
    "..........",
    "..........",
    "..........",
    "..........",
    "..........",
    "..........",
    "..........",
    "..........",
    "......##..",
    "......##..",
    "..........",
    "..........",
];

/// A live cell turns from the young colour to the settled colour as it
/// ages.
/// Given: a block of cells two generations old.
/// When: one generation passes, then ten more.
/// Then: after one, its pixels are a mix of rose and iris, part of the way
/// to iris, and after ten more they are iris.
///
/// Why it matters: the colour tells a birth from a cell that stays. A mix
/// by age shows how long each cell has lived at the grain of a pixel, so
/// a growing edge glows and a settled body rests.
#[test]
fn a_live_cell_turns_from_the_young_colour_to_the_settled_colour() {
    // Given:
    let area = Rect::new(0, 0, 5, 3);
    let palette = &theme::ROSE_PINE;
    let scene = Life::from_pattern(Rng::seed_from_u64(7), palette, BLOCK);

    // When:
    let scene = ticked(scene, area, 1);
    let young = pixel(&scene, area, (6, 8));
    let scene = ticked(scene, area, 10);
    let settled = pixel(&scene, area, (6, 8));

    // Then:
    let between = share(young, palette.rose, palette.iris)
        .unwrap_or_else(|| panic!("{young:?} is no mix of rose and iris"));
    assert!(
        0.0 < between && between < 1.0,
        "the young block is at {between}"
    );
    assert_eq!(settled, palette.iris);
}
