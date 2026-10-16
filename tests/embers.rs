#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use hearth::scene::{Rng, Scene};
use hearth::scenes::embers::Embers;
use hearth::theme;
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 80, 24);

const HALF_BLOCK: &str = "▀";

fn ticked(seed: u64, area: Rect, ticks: usize) -> Embers {
    let mut scene = Embers::new(Rng::seed_from_u64(seed), &theme::ROSE_PINE);
    for _ in 0..ticks {
        scene.tick(area);
    }
    scene
}

fn render(scene: &Embers, area: Rect, canvas: Rect) -> Buffer {
    let mut buffer = Buffer::empty(canvas);
    scene.render(area, &mut buffer);
    buffer
}

/// The two pixels of every half-block cell, top first, by pixel row. A
/// cell that holds a glyph instead gives `None` for both.
fn pixels(buffer: &Buffer) -> Vec<Vec<Option<Color>>> {
    let area = buffer.area;
    let mut rows = vec![vec![None; usize::from(area.width)]; usize::from(area.height) * 2];
    for position in area.positions() {
        let cell = &buffer[position];
        if cell.symbol() == HALF_BLOCK {
            let x = usize::from(position.x - area.x);
            let upper = usize::from(position.y - area.y) * 2;
            rows[upper][x] = Some(cell.fg);
            rows[upper + 1][x] = Some(cell.bg);
        }
    }
    rows
}

const fn glow() -> [Color; 3] {
    [
        theme::ROSE_PINE.love,
        theme::ROSE_PINE.rose,
        theme::ROSE_PINE.gold,
    ]
}

/// Given: the scene on an eighty by twenty-four area.
/// When: it ticks once and renders.
/// Then: every half-block pixel in the top half is the base colour.
///
/// Why it matters: the fire sits low in a dark hearth, and a wall of colour
/// up the screen reads as the doom fire, not as logs on a grate.
#[test]
fn the_top_of_the_screen_stays_dark() {
    // Given, when:
    let shown = pixels(&render(&ticked(7, AREA, 1), AREA, AREA));

    // Then:
    for (y, row) in shown.iter().enumerate().take(shown.len() / 2) {
        for (x, pixel) in row.iter().enumerate() {
            if let Some(colour) = pixel {
                assert_eq!(*colour, theme::ROSE_PINE.base, "pixel ({x}, {y})");
            }
        }
    }
}

/// Given: the scene on an eighty by twenty-four area.
/// When: it ticks once and renders.
/// Then: the lowest pixel row holds stones in overlay with base gaps, a
/// grate bar in muted spans at least a third of the width just above
/// them, and wood sits above the grate.
///
/// Why it matters: the fire rests on something, so it reads as a
/// fireplace and not as a glow that floats.
#[test]
fn the_logs_rest_on_a_grate_over_hearth_stones() {
    // Given, when:
    let shown = pixels(&render(&ticked(7, AREA, 1), AREA, AREA));
    let palette = &theme::ROSE_PINE;
    let count = |row: &[Option<Color>], colour: Color| {
        row.iter().filter(|pixel| **pixel == Some(colour)).count()
    };

    // Then:
    let bottom = shown.last().unwrap();
    assert!(count(bottom, palette.overlay) > 0, "no stones");
    assert!(
        bottom
            .iter()
            .all(|pixel| *pixel == Some(palette.overlay) || *pixel == Some(palette.base)),
        "stones in another colour: {bottom:?}"
    );
    let grate = (0..shown.len())
        .rev()
        .find(|&y| count(&shown[y], palette.muted) >= shown[y].len() / 3)
        .expect("no grate bar");
    let wood_above = shown[..grate].iter().any(|row| {
        row.iter()
            .any(|pixel| pixel.is_some_and(|c| c != palette.base))
    });
    assert!(wood_above, "nothing above the grate at row {grate}");
}

/// Given: the scene on an eighty by twenty-four area.
/// When: it renders after one tick and again after forty.
/// Then: some pixels change, and every pixel that changes is a glow
/// colour (love, rose or gold) in both frames.
///
/// Why it matters: the glow is what makes the fire live, and a stone, the
/// grate or the wood that flickers reads as a fault.
#[test]
fn only_the_glow_changes_while_the_wood_holds_still() {
    // Given:
    let mut scene = ticked(7, AREA, 1);
    let before = pixels(&render(&scene, AREA, AREA));

    // When:
    for _ in 0..39 {
        scene.tick(AREA);
    }
    let after = pixels(&render(&scene, AREA, AREA));

    // Then:
    let mut changed = 0;
    for (y, (old, new)) in before.iter().zip(&after).enumerate() {
        for (x, (old, new)) in old.iter().zip(new).enumerate() {
            if let (Some(old), Some(new)) = (old, new)
                && old != new
            {
                changed += 1;
                assert!(
                    glow().contains(old) && glow().contains(new),
                    "pixel ({x}, {y}) went from {old:?} to {new:?}"
                );
            }
        }
    }
    assert!(changed > 0, "the glow held still");
}

/// Given: seeds 1 to 5.
/// When: each draws twice from the same seed, and once per seed.
/// Then: one seed draws the same fire both times, and not every seed
/// draws the same fire.
///
/// Why it matters: `--seed` replays a run, and `r` asks for a new stack of
/// logs.
#[test]
fn the_seed_chooses_the_stack_of_logs() {
    let frame = |seed| render(&ticked(seed, AREA, 1), AREA, AREA);

    // When, then:
    assert_eq!(frame(1), frame(1));
    let frames: Vec<Buffer> = (1..=5).map(frame).collect();
    assert!(frames.iter().any(|other| *other != frames[0]));
}

/// Given: areas from nothing to a few cells.
/// When: the scene ticks and renders into each, placed inside a larger
/// buffer.
/// Then: nothing panics, and no cell outside the area changes.
///
/// Why it matters: a split pane can be one cell high, and a scene that
/// draws past its pane paints over its neighbour.
#[test]
fn a_tiny_area_draws_nothing_outside_itself() {
    let canvas = Rect::new(0, 0, 20, 10);
    for (width, height) in [(0, 0), (1, 1), (5, 2), (12, 3)] {
        // Given:
        let area = Rect::new(4, 4, width, height);

        // When:
        let shown = render(&ticked(7, area, 3), area, canvas);

        // Then:
        let blank = Buffer::empty(canvas);
        for position in canvas.positions() {
            if !area.contains(position) {
                assert_eq!(shown[position], blank[position], "{area} at {position:?}");
            }
        }
    }
}
