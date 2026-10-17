#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use hearth::braille::{self, CELL_HEIGHT, CELL_WIDTH};
use hearth::clock::Clock;
use hearth::config::Background;
use hearth::corpus::Corpus;
use hearth::scene::{Rng, Scene, SceneKind, TICK_RATE};
use hearth::scenes::hourglass::{Hourglass, Lengths};
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);
const PALETTE: &Palette = &theme::ROSE_PINE;
const WORK: Duration = Lengths::DEFAULT.work;
const BREAK: Duration = Lengths::DEFAULT.r#break;
const LONG_BREAK: Duration = Lengths::DEFAULT.long_break;
/// Ticks with the clock held still, enough for the last grain to fall the
/// height of the glass and settle.
const SETTLE: u32 = 300;

/// A scene on a clock that the test moves by hand.
fn hourglass(seed: u64) -> (Hourglass, Rc<Cell<Duration>>) {
    hourglass_with(seed, Lengths::DEFAULT)
}

fn hourglass_with(seed: u64, lengths: Lengths) -> (Hourglass, Rc<Cell<Duration>>) {
    let now = Rc::new(Cell::new(Duration::ZERO));
    let hand = Rc::clone(&now);
    let clock = Clock::new(move || hand.get());
    (
        Hourglass::new(Rng::seed_from_u64(seed), PALETTE, clock, lengths),
        now,
    )
}

fn render(scene: &Hourglass, area: Rect, canvas: Rect) -> Buffer {
    let mut buffer = Buffer::empty(canvas);
    scene.render(area, &mut buffer);
    buffer
}

/// The lit braille dots of a render, in dots from the top left of the
/// area, each with the colour of its cell.
fn dots(buffer: &Buffer, area: Rect) -> Vec<((usize, usize), Color)> {
    let mut dots = Vec::new();
    for position in area.positions() {
        let cell = &buffer[position];
        let Some(code) = cell.symbol().chars().next().map(u32::from) else {
            continue;
        };
        if !(0x2801..=0x28ff).contains(&code) {
            continue;
        }
        for column in 0..CELL_WIDTH {
            for row in 0..CELL_HEIGHT {
                if (code - 0x2800) & braille::dot(column, row) != 0 {
                    let x = usize::from(position.x - area.x) * CELL_WIDTH + column;
                    let y = usize::from(position.y - area.y) * CELL_HEIGHT + row;
                    dots.push(((x, y), cell.fg));
                }
            }
        }
    }
    dots
}

/// The work sand above the neck and below it.
fn sand(buffer: &Buffer, area: Rect) -> (usize, usize) {
    sand_in(buffer, area, PALETTE.gold)
}

/// Ticks the scene with the clock moving at the tick rate, until the clock
/// reads `until`.
fn run(scene: &mut Hourglass, now: &Cell<Duration>, until: Duration, area: Rect) {
    while now.get() < until {
        now.set((now.get() + TICK_RATE).min(until));
        scene.tick(area);
    }
}

/// The sand leaves the top as the work phase runs, and piles in the bottom.
/// Given: a full glass at a hundred by thirty, on a clock the test moves.
/// When: the clock runs at the tick rate to half the work phase, then to
/// one tick before its end, when the glass turns, and the sand settles.
/// Then: half the grains have left the top at half the phase. Before the
/// end one grain is left in the top, the rest is in the bottom, and each
/// grain there rests on sand or glass below it and on both sides below it.
/// No grain is lost, and every grain lies between the walls of the glass.
///
/// Why it matters: a timer whose sand runs out early or late tells the
/// wrong time, and a grain that hangs in the air reads as a fault.
#[test]
fn the_sand_leaves_the_top_as_the_work_phase_runs() {
    // Given:
    let (mut scene, now) = hourglass(7);
    scene.tick(AREA);
    let (total, below) = sand(&render(&scene, AREA, AREA), AREA);
    assert!(total > 100, "{total} grains");
    assert_eq!(below, 0, "sand below the neck at the start");

    // When:
    let mut step = Duration::ZERO;
    while step < WORK / 2 {
        step += WORK / 40;
        run(&mut scene, &now, step, AREA);
        let buffer = render(&scene, AREA, AREA);
        let (top, bottom) = sand(&buffer, AREA);
        assert_eq!(top + bottom, total, "grains at {step:?}");
        for ((x, y), colour) in dots(&buffer, AREA) {
            if colour != PALETTE.gold {
                continue;
            }
            let row = u16::try_from(y / CELL_HEIGHT).unwrap();
            let column = u16::try_from(x / CELL_WIDTH).unwrap();
            let wall = |other: u16| buffer[(other, row)].fg == PALETTE.muted;
            assert!(
                (0..column).any(wall) && (column + 1..AREA.width).any(wall),
                "a grain at {:?} outside the glass at {step:?}",
                (x, y)
            );
        }
    }

    // Then:
    let (top, _) = sand(&render(&scene, AREA, AREA), AREA);
    assert!(
        top.abs_diff(total / 2) <= 2,
        "{top} of {total} grains in the top at half the phase"
    );

    // When:
    run(&mut scene, &now, WORK - TICK_RATE, AREA);
    for _ in 0..SETTLE {
        scene.tick(AREA);
    }

    // Then:
    let buffer = render(&scene, AREA, AREA);
    assert_eq!(sand(&buffer, AREA), (1, total - 1), "before the end");
    let neck = usize::from(AREA.height / 2) * CELL_HEIGHT;
    let lit = dots(&buffer, AREA);
    let held = |x: usize, y: usize| {
        let row = u16::try_from(y / CELL_HEIGHT).unwrap();
        let column = u16::try_from(x / CELL_WIDTH).unwrap();
        buffer[(column, row)].fg == PALETTE.muted
            || lit
                .iter()
                .any(|(dot, colour)| *dot == (x, y) && *colour == PALETTE.gold)
    };
    for ((x, y), colour) in &lit {
        if *colour == PALETTE.gold && *y >= neck {
            assert!(
                held(*x, y + 1) && held(x - 1, y + 1) && held(x + 1, y + 1),
                "a grain at {:?} does not rest",
                (x, y)
            );
        }
    }
}

/// A resize keeps the share of sand in the top.
/// Given: a glass a quarter through the work phase at a hundred by thirty.
/// When: the pane shrinks to sixty by twenty.
/// Then: the glass holds fewer grains, and the share in the top stays the
/// same to within a fiftieth.
///
/// Why it matters: a resize that refills or empties the glass would tell
/// the wrong time.
#[test]
fn a_resize_keeps_the_share_of_sand() {
    // Given:
    let (mut scene, now) = hourglass(7);
    run(&mut scene, &now, WORK / 4, AREA);
    for _ in 0..SETTLE {
        scene.tick(AREA);
    }
    let (top, bottom) = sand(&render(&scene, AREA, AREA), AREA);
    let share = top as f32 / (top + bottom) as f32;

    // When:
    let small = Rect::new(0, 0, 60, 20);
    scene.tick(small);

    // Then:
    let (small_top, small_bottom) = sand(&render(&scene, small, small), small);
    assert!(small_top + small_bottom < top + bottom);
    let small_share = small_top as f32 / (small_top + small_bottom) as f32;
    assert!(
        (small_share - share).abs() < 0.02,
        "{small_share} in the top after the resize, {share} before"
    );
}

/// Given: a glass that ticked on a hundred by thirty area.
/// When: it renders on a twenty by six area inside a larger canvas before
/// it ticks there, then ticks and renders on that area, on one cell, on a
/// row of no height, and on none.
/// Then: it draws without a panic, and no cell outside the area is touched.
#[test]
fn a_smaller_area_draws_only_inside_it() {
    // Given:
    let canvas = Rect::new(0, 0, 40, 12);
    let (mut scene, now) = hourglass(7);
    run(&mut scene, &now, WORK / 3, AREA);
    let inside_only = |buffer: &Buffer, area: Rect| {
        for position in canvas.positions() {
            if !area.contains(position) {
                assert_eq!(buffer[position].symbol(), " ", "glyph outside {area:?}");
                assert_eq!(buffer[position].fg, Color::Reset, "colour outside {area:?}");
            }
        }
    };

    // When, then:
    let first = Rect::new(3, 2, 20, 6);
    inside_only(&render(&scene, first, canvas), first);
    for area in [
        first,
        Rect::new(3, 2, 1, 1),
        Rect::new(3, 2, 5, 0),
        Rect::new(3, 2, 0, 0),
    ] {
        scene.tick(area);
        inside_only(&render(&scene, area, canvas), area);
    }
}

/// Given: the scene name `hourglass`.
/// When: it is looked up and built, then ticked and rendered.
/// Then: the name finds a scene, and its first frame shows the glass and
/// the sand.
///
/// Why it matters: a scene that is built but not listed cannot be chosen
/// from the command line, the picker or Tab.
#[test]
fn hourglass_is_a_scene_by_name() {
    // When:
    let kind = SceneKind::from_name("hourglass").expect("hourglass is listed");
    let mut scene = kind.build(
        7,
        PALETTE,
        Background::Theme,
        &Arc::new(Corpus::embedded()),
        Speed::DEFAULT,
    );
    scene.tick(AREA);

    // Then:
    let mut buffer = Buffer::empty(AREA);
    scene.render(AREA, &mut buffer);
    let lit = dots(&buffer, AREA);
    assert!(lit.iter().any(|(_, colour)| *colour == PALETTE.gold));
    assert!(lit.iter().any(|(_, colour)| *colour == PALETTE.muted));
}

/// The glass has round bulbs and stands in a frame.
/// Given: a glass at a hundred by thirty.
/// When: it renders.
/// Then: the glass is narrowest on the middle row. Each bulb is widest on a
/// row between its end and the neck, so it curves in at both ends. A rose
/// plate lies across the rows above and below the glass, wider than the
/// glass, and a rose post stands either side of every row of the glass.
///
/// Why it matters: two straight cones read as a diagram, not as an
/// hourglass.
#[test]
fn the_glass_has_round_bulbs_in_a_frame() {
    // Given:
    let (mut scene, _) = hourglass(7);
    scene.tick(AREA);

    // When:
    let buffer = render(&scene, AREA, AREA);

    // Then:
    let cells = |row: u16, colour: Color| -> Vec<u16> {
        (0..AREA.width)
            .filter(|&column| {
                buffer[(column, row)].fg == colour && buffer[(column, row)].symbol() != " "
            })
            .collect()
    };
    let glass: Vec<(u16, u16, u16)> = (0..AREA.height)
        .filter_map(|row| {
            let walls = cells(row, PALETTE.muted);
            Some((row, *walls.first()?, *walls.last()?))
        })
        .collect();
    let (first, last) = (glass.first().unwrap().0, glass.last().unwrap().0);
    let width = |row: u16| {
        glass
            .iter()
            .find(|(at, _, _)| *at == row)
            .map(|(_, left, right)| right - left)
            .unwrap()
    };
    let neck = AREA.height / 2;
    assert!(
        glass.iter().all(|(row, _, _)| width(*row) >= width(neck)),
        "the neck is not the narrowest row: {glass:?}"
    );
    for bulb in [first..neck, neck + 1..last + 1] {
        let widest = bulb.clone().map(width).max().unwrap();
        let ends = [bulb.start, bulb.end - 1];
        for end in ends {
            assert!(
                width(end) < widest,
                "row {end} is as wide as the bulb's widest row: {glass:?}"
            );
        }
    }
    let (outer_left, outer_right) = (
        glass.iter().map(|(_, left, _)| *left).min().unwrap(),
        glass.iter().map(|(_, _, right)| *right).max().unwrap(),
    );
    for plate in [first - 1, last + 1] {
        let rose = cells(plate, PALETTE.rose);
        assert!(
            rose.first().is_some_and(|left| *left < outer_left)
                && rose.last().is_some_and(|right| *right > outer_right),
            "no plate across row {plate}"
        );
    }
    for (row, left, right) in &glass {
        let rose = cells(*row, PALETTE.rose);
        assert!(
            rose.iter().any(|post| post < left) && rose.iter().any(|post| post > right),
            "no post beside row {row}"
        );
    }
}

/// The grains of one colour above the neck and below it. The neck is the
/// middle row of the pane.
fn sand_in(buffer: &Buffer, area: Rect, colour: Color) -> (usize, usize) {
    let neck = usize::from(area.height / 2) * CELL_HEIGHT;
    dots(buffer, area)
        .into_iter()
        .filter(|(_, lit)| *lit == colour)
        .fold((0, 0), |(top, bottom), ((_, y), _)| {
            if y < neck {
                (top + 1, bottom)
            } else {
                (top, bottom + 1)
            }
        })
}

/// The finished rounds the marks beside the glass show: full cells in iris.
fn rounds_done(buffer: &Buffer, area: Rect) -> usize {
    area.positions()
        .filter(|position| {
            buffer[*position].fg == PALETTE.iris && buffer[*position].symbol() == "\u{28ff}"
        })
        .count()
}

/// The glass turns over at the end of the work phase and fills with break
/// sand.
/// Given: a glass at a hundred by thirty near the end of the work phase.
/// When: the clock runs three seconds past the end.
/// Then: before the end the sand is gold, and after it every grain is
/// foam, and nearly all of it is back in the top.
///
/// Why it matters: a glass that does not turn stops keeping time, and sand
/// in the work colour during a break shows the wrong phase.
#[test]
fn the_glass_turns_at_the_end_of_a_phase_to_break_sand() {
    // Given:
    let (mut scene, now) = hourglass(7);
    scene.tick(AREA);
    let (total, _) = sand(&render(&scene, AREA, AREA), AREA);
    run(&mut scene, &now, WORK - Duration::from_secs(1), AREA);
    let (_, bottom) = sand(&render(&scene, AREA, AREA), AREA);
    assert!(
        bottom > total * 9 / 10,
        "{bottom} of {total} below before the end"
    );

    // When:
    run(&mut scene, &now, WORK + Duration::from_secs(3), AREA);

    // Then:
    let buffer = render(&scene, AREA, AREA);
    assert_eq!(
        sand_in(&buffer, AREA, PALETTE.gold),
        (0, 0),
        "work sand after the turn"
    );
    let (top, bottom) = sand_in(&buffer, AREA, PALETTE.foam);
    assert_eq!(top + bottom, total, "break sand after the turn");
    assert!(
        top > total * 95 / 100,
        "{top} of {total} in the top after the turn"
    );
}

/// The marks count the rounds, and the long break follows the fourth.
/// Given: a glass at a hundred by thirty.
/// When: the clock runs through four work rounds, their breaks, the long
/// break and into the next round, and the glass is read halfway through
/// several phases.
/// Then: the marks show the work rounds finished in the cycle, and the sand
/// is gold in work and foam in a break, also ten seconds after a short
/// break after the fourth round would have ended.
///
/// Why it matters: the long break is the point of counting rounds, and
/// marks that count wrong lose the cycle.
#[test]
fn the_marks_count_the_rounds_and_the_long_break_follows_the_fourth() {
    // Given:
    let (mut scene, now) = hourglass(7);
    let long_break = WORK * 4 + BREAK * 3;
    let checks = [
        (WORK / 2, 0, PALETTE.gold),
        (WORK + BREAK / 2, 1, PALETTE.foam),
        (WORK + BREAK + WORK / 2, 1, PALETTE.gold),
        (WORK * 3 + BREAK * 3 + WORK / 2, 3, PALETTE.gold),
        (
            long_break + BREAK + Duration::from_secs(10),
            4,
            PALETTE.foam,
        ),
        (long_break + LONG_BREAK + WORK / 2, 0, PALETTE.gold),
    ];

    for (at, done, colour) in checks {
        // When:
        run(&mut scene, &now, at, AREA);

        // Then:
        let buffer = render(&scene, AREA, AREA);
        assert_eq!(rounds_done(&buffer, AREA), done, "rounds done at {at:?}");
        let (top, bottom) = sand_in(&buffer, AREA, colour);
        assert!(top + bottom > 100, "no sand in its colour at {at:?}");
    }
}

/// The long break runs its own length.
/// Given: a glass at a hundred by thirty, run to just past the time a short
/// break after the fourth round would end.
/// When: it renders.
/// Then: the share of sand in the top is the share of the long break left.
///
/// Why it matters: a long break that drains at the pace of a short one
/// ends ten minutes early.
#[test]
fn the_long_break_drains_over_its_own_length() {
    // Given:
    let (mut scene, now) = hourglass(7);
    let into = BREAK + Duration::from_secs(10);
    run(&mut scene, &now, WORK * 4 + BREAK * 3 + into, AREA);

    // When:
    let buffer = render(&scene, AREA, AREA);

    // Then:
    let (top, bottom) = sand_in(&buffer, AREA, PALETTE.foam);
    let share = top as f32 / (top + bottom) as f32;
    let left = 1.0 - into.as_secs_f32() / LONG_BREAK.as_secs_f32();
    assert!(
        (share - left).abs() < 0.02,
        "{share} of the sand in the top, {left} of the long break left"
    );
}

/// The lengths set how long each phase runs.
/// Given: a glass whose work round lasts two minutes, its break one and its
/// long break three.
/// When: the clock runs three seconds past the end of the first round, and
/// three seconds past the end of the first break.
/// Then: the first time the sand is foam, and the second time it is gold
/// again with one round marked.
///
/// Why it matters: lengths set in the file that the glass ignores are a
/// setting the user saved for nothing.
#[test]
fn the_lengths_set_how_long_each_phase_runs() {
    // Given:
    let minute = Duration::from_secs(60);
    let lengths = Lengths {
        work: minute * 2,
        r#break: minute,
        long_break: minute * 3,
    };
    let (mut scene, now) = hourglass_with(7, lengths);
    let past = Duration::from_secs(3);

    for (at, colour, done) in [
        (lengths.work + past, PALETTE.foam, 0),
        (lengths.work + lengths.r#break + past, PALETTE.gold, 1),
    ] {
        // When:
        run(&mut scene, &now, at, AREA);

        // Then:
        let buffer = render(&scene, AREA, AREA);
        let (top, _) = sand_in(&buffer, AREA, colour);
        assert!(
            top > 100,
            "the top is not full of sand in its colour at {at:?}"
        );
        assert_eq!(rounds_done(&buffer, AREA), done, "rounds done at {at:?}");
    }
}

/// The time left names the phase, the round and the minutes and seconds.
/// Given: a glass on the default lengths.
/// When: the clock reads the start, a second into the first work round,
/// halfway through the first break, and the start of the long break.
/// Then: the time left reads "work 1/4, 25:00 left", "work 1/4, 24:59
/// left", "break 1/4, 2:30 left" and "long break, 15:00 left".
///
/// Why it matters: `+` and `-` show this on the hourglass, and a count
/// that rounds down shows a second less than is left.
#[test]
fn the_time_left_names_the_phase_and_the_round() {
    // Given:
    let (mut scene, now) = hourglass(7);
    let checks = [
        (Duration::ZERO, "work 1/4, 25:00 left"),
        (Duration::from_millis(1_000), "work 1/4, 24:59 left"),
        (WORK + BREAK / 2, "break 1/4, 2:30 left"),
        (WORK * 4 + BREAK * 3, "long break, 15:00 left"),
    ];

    for (at, expected) in checks {
        // When:
        run(&mut scene, &now, at, AREA);
        scene.tick(AREA);

        // Then:
        assert_eq!(scene.time_left().as_deref(), Some(expected), "at {at:?}");
    }
}
