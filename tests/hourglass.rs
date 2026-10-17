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
use hearth::scenes::hourglass::{Hourglass, WORK};
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);
const PALETTE: &Palette = &theme::ROSE_PINE;
/// Ticks with the clock held still, enough for the last grain to fall the
/// height of the glass and settle.
const SETTLE: u32 = 300;

/// A scene on a clock that the test moves by hand.
fn hourglass(seed: u64) -> (Hourglass, Rc<Cell<Duration>>) {
    let now = Rc::new(Cell::new(Duration::ZERO));
    let hand = Rc::clone(&now);
    let clock = Clock::new(move || hand.get());
    (
        Hourglass::new(Rng::seed_from_u64(seed), PALETTE, clock),
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

/// The grains above the neck and the grains below it. The neck is the
/// middle row of the pane.
fn sand(buffer: &Buffer, area: Rect) -> (usize, usize) {
    let neck = usize::from(area.height / 2) * CELL_HEIGHT;
    dots(buffer, area)
        .into_iter()
        .filter(|(_, colour)| *colour == PALETTE.gold)
        .fold((0, 0), |(top, bottom), ((_, y), _)| {
            if y < neck {
                (top + 1, bottom)
            } else {
                (top, bottom + 1)
            }
        })
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
/// its end, and the sand settles.
/// Then: half the grains have left the top at half the phase. At the end
/// the top is empty, every grain is in the bottom, and each grain rests on
/// sand or glass below it and on both sides below it. No grain is lost,
/// and every grain lies between the walls of the glass.
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
    run(&mut scene, &now, WORK, AREA);
    for _ in 0..SETTLE {
        scene.tick(AREA);
    }

    // Then:
    let buffer = render(&scene, AREA, AREA);
    assert_eq!(sand(&buffer, AREA), (0, total), "at the end");
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
        if *colour == PALETTE.gold {
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
