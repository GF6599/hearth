#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pixels;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use hearth::clock::Clock;
use hearth::config::Background;
use hearth::corpus::Corpus;
use hearth::octant::{CELL_HEIGHT, CELL_WIDTH};
use hearth::scene::{Rng, Scene, SceneKind, Settings, TICK_RATE};
use hearth::scenes::hourglass::{Hourglass, Lengths};
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);
const PALETTE: &Palette = &theme::ROSE_PINE;
const GLASS: Color = PALETTE.subtle;
const FRAME: Color = PALETTE.pine;
const WORK_SAND: Color = PALETTE.gold;
const BREAK_SAND: Color = PALETTE.foam;
const DONE: Color = PALETTE.iris;
const TO_DO: Color = PALETTE.muted;
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

/// The lit pixels of a render, those in any colour but the base, in pixels
/// from the top left of the area, each with its colour.
fn lit_pixels(buffer: &Buffer, area: Rect) -> Vec<((usize, usize), Color)> {
    pixels::showing(buffer, area, |colour| colour != PALETTE.base)
}

/// Whether a cell draws a pixel in `colour`.
fn shows(buffer: &Buffer, position: (u16, u16), colour: Color) -> bool {
    pixels::shown(&buffer[position]).is_some_and(|shown| shown.contains(&colour))
}

/// The work sand above the neck and below it.
fn sand(buffer: &Buffer, area: Rect) -> (usize, usize) {
    sand_in(buffer, area, WORK_SAND)
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
        for ((x, y), colour) in lit_pixels(&buffer, AREA) {
            if colour != WORK_SAND {
                continue;
            }
            let row = u16::try_from(y / CELL_HEIGHT).unwrap();
            let column = u16::try_from(x / CELL_WIDTH).unwrap();
            let wall = |other: u16| shows(&buffer, (other, row), GLASS);
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
    let lit = lit_pixels(&buffer, AREA);
    let held = |x: usize, y: usize| {
        let row = u16::try_from(y / CELL_HEIGHT).unwrap();
        let column = u16::try_from(x / CELL_WIDTH).unwrap();
        shows(&buffer, (column, row), GLASS)
            || lit
                .iter()
                .any(|(pixel, colour)| *pixel == (x, y) && *colour == WORK_SAND)
    };
    for ((x, y), colour) in &lit {
        if *colour == WORK_SAND && *y >= neck {
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
        Settings::DEFAULT,
    );
    scene.tick(AREA);

    // Then:
    let mut buffer = Buffer::empty(AREA);
    scene.render(AREA, &mut buffer);
    let lit = lit_pixels(&buffer, AREA);
    assert!(lit.iter().any(|(_, colour)| *colour == WORK_SAND));
    assert!(lit.iter().any(|(_, colour)| *colour == GLASS));
}

/// The glass has round bulbs and stands in a frame.
/// Given: a glass at a hundred by thirty.
/// When: it renders.
/// Then: the glass is narrowest on the middle row. Each bulb is widest on a
/// row between its end and the neck, so it curves in at both ends. A frame
/// plate lies across the rows above and below the glass, wider than the
/// glass, and a frame post stands either side of every row of the glass.
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
    // The hint's words can share the glass colour, and only the pixels of
    // the canvas draw the glass. The posts are glyphs and not pixels.
    let cells = |row: u16, colour: Color| -> Vec<u16> {
        (0..AREA.width)
            .filter(|&column| {
                let cell = &buffer[(column, row)];
                match pixels::shown(cell) {
                    Some(shown) => shown.contains(&colour),
                    None => cell.fg == colour && colour != GLASS,
                }
            })
            .collect()
    };
    let glass: Vec<(u16, u16, u16)> = (0..AREA.height)
        .filter_map(|row| {
            let walls = cells(row, GLASS);
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
        let frame = cells(plate, FRAME);
        assert!(
            frame.first().is_some_and(|left| *left < outer_left)
                && frame.last().is_some_and(|right| *right > outer_right),
            "no plate across row {plate}"
        );
    }
    for (row, left, right) in &glass {
        let frame = cells(*row, FRAME);
        assert!(
            frame.iter().any(|post| post < left) && frame.iter().any(|post| post > right),
            "no post beside row {row}"
        );
    }
}

/// The grains of one colour above the neck and below it. The neck is the
/// middle row of the pane.
fn sand_in(buffer: &Buffer, area: Rect, colour: Color) -> (usize, usize) {
    let neck = usize::from(area.height / 2) * CELL_HEIGHT;
    lit_pixels(buffer, area)
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

/// The finished rounds the marks beside the glass show: cells solid in the
/// colour of a finished round.
fn rounds_done(buffer: &Buffer, area: Rect) -> usize {
    area.positions()
        .filter(|position| pixels::shown(&buffer[*position]) == Some([DONE; 8]))
        .count()
}

/// The rounds to come that the marks show: cells whose middle two rows of
/// pixels are in the colour of a round to come, with the base above and
/// below.
fn rounds_to_do(buffer: &Buffer, area: Rect) -> usize {
    let (base, mark) = (PALETTE.base, TO_DO);
    area.positions()
        .filter(|position| {
            pixels::shown(&buffer[*position])
                == Some([base, base, mark, mark, mark, mark, base, base])
        })
        .count()
}

/// The glass turns over at the end of the work phase and fills with break
/// sand.
/// Given: a glass at a hundred by thirty near the end of the work phase.
/// When: the clock runs three seconds past the end.
/// Then: before the end the sand is in the work colour, and after it every
/// grain is in the break colour, and nearly all of it is back in the top.
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
        sand_in(&buffer, AREA, WORK_SAND),
        (0, 0),
        "work sand after the turn"
    );
    let (top, bottom) = sand_in(&buffer, AREA, BREAK_SAND);
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
/// Then: the marks show the work rounds finished in the cycle as solid
/// cells and the rest of the four as smaller blocks, and the sand is in the
/// work colour in work and the break colour in a break, also ten seconds
/// after a short break after the fourth round would have ended.
///
/// Why it matters: the long break is the point of counting rounds, and
/// marks that count wrong lose the cycle.
#[test]
fn the_marks_count_the_rounds_and_the_long_break_follows_the_fourth() {
    // Given:
    let (mut scene, now) = hourglass(7);
    let long_break = WORK * 4 + BREAK * 3;
    let checks = [
        (WORK / 2, 0, WORK_SAND),
        (WORK + BREAK / 2, 1, BREAK_SAND),
        (WORK + BREAK + WORK / 2, 1, WORK_SAND),
        (WORK * 3 + BREAK * 3 + WORK / 2, 3, WORK_SAND),
        (long_break + BREAK + Duration::from_secs(10), 4, BREAK_SAND),
        (long_break + LONG_BREAK + WORK / 2, 0, WORK_SAND),
    ];

    for (at, done, colour) in checks {
        // When:
        run(&mut scene, &now, at, AREA);

        // Then:
        let buffer = render(&scene, AREA, AREA);
        assert_eq!(rounds_done(&buffer, AREA), done, "rounds done at {at:?}");
        assert_eq!(
            rounds_to_do(&buffer, AREA),
            4 - done,
            "rounds to come at {at:?}"
        );
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
    let (top, bottom) = sand_in(&buffer, AREA, BREAK_SAND);
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
/// Then: the first time the sand is in the break colour, and the second
/// time it is in the work colour again, with the first round marked both times.
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
        (lengths.work + past, BREAK_SAND, 1),
        (lengths.work + lengths.r#break + past, WORK_SAND, 1),
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
/// When: the clock reads the start, half a second and a second and a half
/// into the first work round, halfway through the first break, and the
/// start of the long break.
/// Then: the time left reads "work 1/4, 25:00 left" twice, "work 1/4,
/// 24:59 left", "break 1/4, 2:30 left" and "long break, 15:00 left".
///
/// Why it matters: `+` and `-` show this on the hourglass, and a count
/// that rounds down shows a second less than is left.
#[test]
fn the_time_left_names_the_phase_and_the_round() {
    // Given:
    let (mut scene, now) = hourglass(7);
    let checks = [
        (Duration::ZERO, "work 1/4, 25:00 left"),
        (Duration::from_millis(500), "work 1/4, 25:00 left"),
        (Duration::from_millis(1_500), "work 1/4, 24:59 left"),
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

fn colour_of(buffer: &Buffer, area: Rect, glyph: &str) -> Color {
    area.positions()
        .find(|position| buffer[*position].symbol() == glyph)
        .map(|position| buffer[position].fg)
        .unwrap_or_else(|| panic!("no {glyph} drawn"))
}

/// The colour of the glass's walls and of its sand. On each row between
/// the two posts the outermost cell with a lit pixel is wall. Near a cap or
/// the neck the wall spans more cells of a row, so the sand is the colour
/// most cells between the walls show apart from the wall's, or the wall's
/// when no other shows.
fn glass_and_sand(buffer: &Buffer, area: Rect) -> (Color, Color) {
    let lit = |position: (u16, u16)| -> Vec<Color> {
        let mut colours: Vec<Color> = pixels::shown(&buffer[position])
            .into_iter()
            .flatten()
            .filter(|colour| *colour != PALETTE.base)
            .collect();
        colours.dedup();
        colours
    };
    let mut glass = None;
    let mut inner: Vec<Color> = Vec::new();
    for row in area.top()..area.bottom() {
        let posts: Vec<u16> = (area.left()..area.right())
            .filter(|&column| buffer[(column, row)].symbol() == "\u{2503}")
            .collect();
        let [left, right] = posts[..] else {
            continue;
        };
        let inside: Vec<u16> = (left + 1..right)
            .filter(|&column| !lit((column, row)).is_empty())
            .collect();
        let (Some(first), Some(last)) = (inside.first(), inside.last()) else {
            continue;
        };
        glass.get_or_insert_with(|| lit((*first, row))[0]);
        inner.extend(
            inside
                .iter()
                .filter(|column| *column != first && *column != last)
                .flat_map(|column| lit((*column, row))),
        );
    }
    let glass = glass.expect("no glass");
    let sand = inner
        .iter()
        .filter(|colour| **colour != glass)
        .max_by_key(|colour| inner.iter().filter(|other| other == colour).count())
        .copied()
        .unwrap_or(glass);
    (glass, sand)
}

/// Each part of the hourglass reads in its own colour, and the glass is not
/// a dim grey.
/// Given: a glass at a hundred by thirty, halfway through the first work
/// round, then halfway through the first break.
/// When: it renders.
/// Then: the walls are in none of the theme's dim greys (base, surface,
/// overlay or muted). The frame, the walls, the work sand and the
/// break sand take four colours that differ from each other.
///
/// Why it matters: dim grey walls fade into the background, so the glass reads
/// as a few loose pixels, and two parts in one colour run together.
#[test]
fn each_part_reads_in_its_own_colour_and_the_glass_is_not_dim() {
    // Given:
    let (mut scene, now) = hourglass(7);
    run(&mut scene, &now, WORK / 2, AREA);

    // When:
    let work = render(&scene, AREA, AREA);
    run(&mut scene, &now, WORK + BREAK / 2, AREA);
    let rest = render(&scene, AREA, AREA);

    // Then:
    let frame = colour_of(&work, AREA, "\u{2503}");
    let (glass, work_sand) = glass_and_sand(&work, AREA);
    let (_, break_sand) = glass_and_sand(&rest, AREA);
    let greys = [
        PALETTE.base,
        PALETTE.surface,
        PALETTE.overlay,
        PALETTE.muted,
    ];
    assert!(
        !greys.contains(&glass),
        "the glass is a dim grey: {glass:?}"
    );
    let parts = [frame, glass, work_sand, break_sand];
    for (index, part) in parts.iter().enumerate() {
        assert!(
            !parts[index + 1..].contains(part),
            "two parts share a colour: frame, glass, work sand, break sand {parts:?}"
        );
    }
}

fn text_of(buffer: &Buffer, area: Rect, row: u16) -> String {
    (area.left()..area.right())
        .map(|column| buffer[(column, area.y + row)].symbol())
        .collect()
}

/// A hint under the glass names the keys.
/// Given: a glass at a hundred by thirty, then the same glass on a pane
/// thirty cells wide.
/// When: each ticks and renders.
/// Then: on the wide pane a row under the frame's lower plate reads "r
/// start the cycle again" and "+ time left", centred under the frame to
/// within two cells. The narrow pane has no room for the hint, so no row
/// holds it, and the frame still stands.
///
/// Why it matters: `r` starts the cycle again and `+` shows the time left
/// on the hourglass, and with nothing on screen to say so a user cannot
/// find either.
#[test]
fn a_hint_under_the_glass_names_the_keys() {
    // Given:
    let (mut scene, _) = hourglass(7);
    let narrow = Rect::new(0, 0, 30, 30);

    // When:
    scene.tick(AREA);
    let wide = render(&scene, AREA, AREA);

    // Then:
    let plate = (0..AREA.height)
        .rev()
        .find(|&row| text_of(&wide, AREA, row).contains('\u{2580}'))
        .expect("no lower plate");
    let frame: Vec<u16> = (0..AREA.width)
        .filter(|&column| wide[(column, plate)].symbol() == "\u{2580}")
        .collect();
    let middle = f32::from(frame[0] + frame[frame.len() - 1]) / 2.0;
    let hint = (plate + 1..AREA.height)
        .find(|&row| text_of(&wide, AREA, row).contains("time left"))
        .unwrap_or_else(|| panic!("no hint under the plate at row {plate}"));
    let text = text_of(&wide, AREA, hint);
    assert!(
        text.contains("r start the cycle again") && text.contains("+ time left"),
        "the hint reads {text:?}"
    );
    let first = text.find('r').unwrap() as f32;
    let last = (text.trim_end().chars().count() - 1) as f32;
    assert!(
        ((first + last) / 2.0 - middle).abs() <= 2.0,
        "the hint spans {first} to {last}, the frame is centred on {middle}"
    );

    // When:
    scene.tick(narrow);
    let small = render(&scene, narrow, narrow);

    // Then:
    assert!(
        (0..narrow.height).all(|row| !text_of(&small, narrow, row).contains("time")),
        "a hint on a pane too narrow for it"
    );
    assert_eq!(colour_of(&small, narrow, "\u{2503}"), FRAME);
}

fn outline(buffer: &Buffer, area: Rect) -> Vec<(usize, usize)> {
    lit_pixels(buffer, area)
        .into_iter()
        .filter(|(_, colour)| *colour == GLASS)
        .map(|(pixel, _)| pixel)
        .collect()
}

fn rises_then_falls(values: &[usize]) -> bool {
    let peak = values
        .iter()
        .enumerate()
        .max_by_key(|(index, value)| (**value, usize::MAX - index))
        .map_or(0, |(index, _)| index);
    values[..=peak].windows(2).all(|pair| pair[0] <= pair[1])
        && values[peak..].windows(2).all(|pair| pair[0] >= pair[1])
}

/// The glass is one smooth outline with a clear neck.
/// Given: a glass at a hundred by thirty.
/// When: it renders.
/// Then: every row of pixels from the top cap to the bottom cap holds a wall
/// on both sides of the middle, and the pixels of the outline touch each
/// other, diagonals included, as one line. From the neck out to each cap
/// the outline only widens and then only narrows. The neck is at most a
/// fifth as wide as the widest row, and no wall comes within a pixel of the
/// middle between the caps, so the neck stays open.
///
/// Why it matters: an outline drawn on every other row, or one that steps
/// at each cell, reads as loose pixels and not as glass, and a neck with no
/// clear channel reads as a gap in the glass.
#[test]
fn the_glass_is_one_smooth_outline_with_a_clear_neck() {
    // Given:
    let (mut scene, _) = hourglass(7);
    scene.tick(AREA);

    // When:
    let glass = outline(&render(&scene, AREA, AREA), AREA);

    // Then:
    let (top, bottom) = (
        glass.iter().map(|(_, y)| *y).min().unwrap(),
        glass.iter().map(|(_, y)| *y).max().unwrap(),
    );
    let (left, right) = (
        glass.iter().map(|(x, _)| *x).min().unwrap(),
        glass.iter().map(|(x, _)| *x).max().unwrap(),
    );
    let middle = (left + right) as f32 / 2.0;
    let width = |y: usize| {
        let row: Vec<usize> = glass
            .iter()
            .filter(|(_, at)| *at == y)
            .map(|(x, _)| *x)
            .collect();
        let (Some(first), Some(last)) = (row.iter().min(), row.iter().max()) else {
            panic!("no wall on row {y}");
        };
        assert!(
            (*first as f32) < middle && (*last as f32) > middle,
            "row {y} has a wall on one side only"
        );
        last - first
    };
    let widths: Vec<usize> = (top..=bottom).map(width).collect();

    let mut open = vec![glass[0]];
    let mut seen = vec![glass[0]];
    while let Some((x, y)) = open.pop() {
        for &(other_x, other_y) in &glass {
            if x.abs_diff(other_x) <= 1
                && y.abs_diff(other_y) <= 1
                && !seen.contains(&(other_x, other_y))
            {
                seen.push((other_x, other_y));
                open.push((other_x, other_y));
            }
        }
    }
    assert_eq!(seen.len(), glass.len(), "the outline breaks into pieces");

    let inner = &widths[1..widths.len() - 1];
    let neck = 1 + inner
        .iter()
        .enumerate()
        .min_by_key(|(_, width)| **width)
        .map(|(index, _)| index)
        .unwrap();
    let up: Vec<usize> = widths[..=neck].iter().rev().copied().collect();
    let down = &widths[neck..];
    assert!(
        rises_then_falls(&up) && rises_then_falls(down),
        "the outline does not widen and then narrow from the neck: {widths:?}"
    );
    let widest = *widths.iter().max().unwrap();
    assert!(
        widths[neck] * 5 <= widest,
        "the neck is {} pixels wide and the widest row {widest}",
        widths[neck]
    );
    for &(x, y) in &glass {
        assert!(
            y == top || y == bottom || (x as f32 - middle).abs() > 1.0,
            "a wall at {:?} closes the neck",
            (x, y)
        );
    }
}
