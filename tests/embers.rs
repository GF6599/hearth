#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use hearth::scene::{Rng, Scene};
use hearth::scenes::embers::{Embers, SETTLE_TICKS};
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
/// Then: the lowest pixel row holds stones in muted with overlay mortar, a
/// grate bar in subtle spans at least a third of the width above them, and
/// wood sits above the grate.
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
    assert!(count(bottom, palette.muted) > 0, "no stones");
    assert!(
        bottom
            .iter()
            .all(|pixel| *pixel == Some(palette.muted) || *pixel == Some(palette.overlay)),
        "stones in another colour: {bottom:?}"
    );
    let grate = (0..shown.len())
        .rev()
        .find(|&y| count(&shown[y], palette.subtle) >= shown[y].len() / 3)
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

/// The cells that hold a spark, as `(x, y, glyph, fg, bg)`.
fn sparks(buffer: &Buffer) -> Vec<(u16, u16, String, Color, Color)> {
    buffer
        .area
        .positions()
        .filter_map(|position| {
            let cell = &buffer[position];
            ["•", "·", "."].contains(&cell.symbol()).then(|| {
                (
                    position.x,
                    position.y,
                    cell.symbol().to_string(),
                    cell.fg,
                    cell.bg,
                )
            })
        })
        .collect()
}

/// Given: the scene on an eighty by twenty-four area.
/// When: it runs for four hundred ticks, with a frame after each.
/// Then: every spark is `•` in gold, `·` in rose or `.` in muted, on the
/// base colour, in a cell above the grate, and some spark reaches the top
/// half of the screen.
///
/// Why it matters: the glyph shrinks and the colour cools as a spark rises,
/// and a spark below the grate or one that never climbs breaks the picture
/// of heat going up a chimney.
#[test]
fn sparks_rise_above_the_grate_and_cool_as_they_shrink() {
    // Given:
    let mut scene = ticked(7, AREA, 1);
    let palette = &theme::ROSE_PINE;
    let grate_cell = AREA.height - 2;
    let mut seen = 0;
    let mut high = false;

    for _ in 0..400 {
        // When:
        scene.tick(AREA);
        let shown = render(&scene, AREA, AREA);

        // Then:
        for (x, y, glyph, fg, bg) in sparks(&shown) {
            let colour = match glyph.as_str() {
                "•" => palette.gold,
                "·" => palette.rose,
                _ => palette.muted,
            };
            assert_eq!((fg, bg), (colour, palette.base), "{glyph} at ({x}, {y})");
            assert!(
                y < grate_cell,
                "{glyph} at ({x}, {y}) is not above the grate"
            );
            seen += 1;
            high |= y < AREA.height / 2;
        }
    }
    assert!(seen > 0, "no spark");
    assert!(high, "no spark reached the top half");
}

/// Given: the scene on an eighty by twenty-four area.
/// When: it runs past the longest wait before a settle, counting the young
/// sparks, the gold `•`, in each frame.
/// Then: the count reaches a burst's level, and first does so no sooner
/// than the shortest wait.
///
/// Why it matters: a settle is the rare event that makes the fire feel
/// alive, and one that comes at once or never is no event at all.
#[test]
fn a_log_settles_with_a_burst_of_sparks_after_a_while() {
    // Given:
    let mut scene = ticked(7, AREA, 1);
    let last = usize::try_from(*SETTLE_TICKS.end()).unwrap() + 60;

    // When:
    let young: Vec<usize> = (0..last)
        .map(|_| {
            scene.tick(AREA);
            let shown = sparks(&render(&scene, AREA, AREA));
            shown
                .iter()
                .filter(|(.., glyph, _, _)| glyph == "•")
                .count()
        })
        .collect();

    // Then:
    let first = young
        .iter()
        .position(|&count| count >= BURST_YOUNG)
        .unwrap_or_else(|| panic!("no burst: {young:?}"));
    let soonest = usize::try_from(*SETTLE_TICKS.start()).unwrap();
    assert!(first >= soonest, "a burst at tick {first}: {young:?}");
}

/// Young sparks on screen that only a burst reaches. Over forty seeds the
/// usual flow showed at most ten and a burst at least eighteen.
const BURST_YOUNG: usize = 14;
