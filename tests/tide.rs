#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use hearth::corpus::Corpus;
use hearth::scene::{Rng, Scene, SceneKind};
use hearth::scenes::tide::{DRY_TICKS, GRAINS, HIGH_TIDE, LOW_TIDE, SWASH, TIDE_TICKS, Tide};
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);
const PALETTE: &Palette = &theme::ROSE_PINE;

/// Each cell draws two by two pixels. A glyph's bits are its upper left,
/// upper right, lower left and lower right quarters, from the lowest bit,
/// and a set bit draws in the foreground.
const QUADRANTS: [&str; 16] = [
    " ", "▘", "▝", "▀", "▖", "▌", "▞", "▛", "▗", "▚", "▐", "▜", "▄", "▙", "▟", "█",
];

fn ticked(seed: u64, area: Rect, ticks: u32) -> Tide {
    let mut scene = Tide::new(Rng::seed_from_u64(seed), PALETTE);
    for _ in 0..ticks {
        scene.tick(area);
    }
    scene
}

fn render(scene: &Tide, area: Rect, canvas: Rect) -> Buffer {
    let mut buffer = Buffer::empty(canvas);
    scene.render(area, &mut buffer);
    buffer
}

/// The pixels of a render, by pixel row from the top and then by pixel
/// column from the left.
fn pixels(buffer: &Buffer, area: Rect) -> Vec<Vec<Color>> {
    let (width, rows) = (usize::from(area.width) * 2, usize::from(area.height) * 2);
    let mut grid = vec![vec![Color::Reset; width]; rows];
    for position in area.positions() {
        let cell = &buffer[position];
        let bits = QUADRANTS
            .iter()
            .position(|each| *each == cell.symbol())
            .unwrap_or_else(|| panic!("{:?} at {position:?}", cell.symbol()));
        for quarter in 0..4 {
            let colour = if bits & (1 << quarter) != 0 {
                cell.fg
            } else {
                cell.bg
            };
            let x = usize::from(position.x - area.x) * 2 + quarter % 2;
            let y = usize::from(position.y - area.y) * 2 + quarter / 2;
            grid[y][x] = colour;
        }
    }
    grid
}

fn is_sea(colour: Color) -> bool {
    colour == PALETTE.pine || colour == PALETTE.foam
}

/// How many pixels of each column, from the bottom, are sea.
fn waterlines(grid: &[Vec<Color>]) -> Vec<usize> {
    (0..grid[0].len())
        .map(|x| grid.iter().rev().take_while(|row| is_sea(row[x])).count())
        .collect()
}

fn mean(values: &[usize]) -> f32 {
    values.iter().sum::<usize>() as f32 / values.len() as f32
}

/// The sea comes in from the bottom and the sand lies above it.
/// Given: the scene at a hundred by thirty, at many times over a minute.
/// When: each column of pixels is read from the bottom up.
/// Then: the sea is one unbroken run from the bottom, no sea pixel lies
/// above it, and its top pixel is foam.
///
/// Why it matters: water above dry sand, or a gap in the sea, reads as a
/// puddle or a hole and not as a beach.
#[test]
fn the_sea_lies_below_the_sand_in_every_column() {
    let mut scene = ticked(7, AREA, 1);
    for tick in 0..1000 {
        scene.tick(AREA);
        if tick % 37 != 0 {
            continue;
        }

        // When:
        let grid = pixels(&render(&scene, AREA, AREA), AREA);

        // Then:
        for (x, line) in waterlines(&grid).into_iter().enumerate() {
            let rows = grid.len();
            assert!(line > 0, "tick {tick}: column {x} has no sea");
            for (y, row) in grid.iter().enumerate().take(rows - line) {
                assert!(
                    !is_sea(row[x]),
                    "tick {tick}: sea above the line at ({x}, {y})"
                );
            }
            assert_eq!(
                grid[rows - line][x],
                PALETTE.foam,
                "tick {tick}: column {x}"
            );
        }
    }
}

/// The tide rises and falls over minutes, within its bounds.
/// Given: the scene of three seeds, over one whole tide.
/// When: the waterline of every column is read every fifty ticks.
/// Then: no waterline is below the low tide or above the high tide and the
/// highest run-up, and the mean waterline rises and falls by at least half
/// the range between low and high tide.
///
/// Why it matters: a tide that stays put, or a wave that runs off the top
/// of the pane, loses the slow change that makes the scene a tide.
#[test]
fn the_tide_rises_and_falls_between_its_bounds() {
    let rows = f32::from(AREA.height) * 2.0;
    let (lowest, highest) = (LOW_TIDE * rows - 1.0, (HIGH_TIDE + SWASH) * rows + 1.0);
    for seed in [1, 7, 42] {
        let mut scene = ticked(seed, AREA, 1);
        let mut means = Vec::new();
        for tick in 0..TIDE_TICKS {
            scene.tick(AREA);
            if tick % 50 != 0 {
                continue;
            }

            // When:
            let lines = waterlines(&pixels(&render(&scene, AREA, AREA), AREA));

            // Then:
            for (x, line) in lines.iter().enumerate() {
                let line = *line as f32;
                assert!(
                    (lowest..=highest).contains(&line),
                    "seed {seed}, tick {tick}: column {x} at {line} of {rows}"
                );
            }
            means.push(mean(&lines));
        }
        let (low, high) = means
            .iter()
            .fold((f32::MAX, f32::MIN), |(low, high), each| {
                (low.min(*each), high.max(*each))
            });
        assert!(
            high - low >= (HIGH_TIDE - LOW_TIDE) / 2.0 * rows,
            "seed {seed}: the mean waterline moved from {low} to {high}"
        );
    }
}

/// Dry sand is a sparse, even scatter of gold grains.
/// Given: the scene at a hundred by thirty.
/// When: the pixels above the highest reach of the sea are read in tiles
/// of four by four.
/// Then: each tile holds exactly `GRAINS` gold pixels, and every other
/// pixel is the base.
///
/// Why it matters: flat gold over half the screen is loud, and grains that
/// clump or thin out read as a stain and not as sand.
#[test]
fn dry_sand_is_an_even_scatter_of_gold_grains() {
    // Given:
    let scene = ticked(7, AREA, 200);

    // When:
    let grid = pixels(&render(&scene, AREA, AREA), AREA);
    let rows = grid.len();
    let dry = rows - ((HIGH_TIDE + SWASH) * rows as f32).ceil() as usize - 1;

    // Then:
    for top in (0..dry / 4 * 4).step_by(4) {
        for left in (0..grid[0].len()).step_by(4) {
            let tile: Vec<Color> = (0..16)
                .map(|at| grid[top + at / 4][left + at % 4])
                .collect();
            let gold = tile.iter().filter(|each| **each == PALETTE.gold).count();
            let base = tile.iter().filter(|each| **each == PALETTE.base).count();
            assert_eq!(
                (gold, base),
                (GRAINS, 16 - GRAINS),
                "tile at ({left}, {top})"
            );
        }
    }
}

/// Sand that the water leaves stays wet for a while, then dries.
/// Given: the scene at a hundred by thirty, rendered every tick for longer
/// than sand takes to dry.
/// When: the wetness of every pixel is read after each tick.
/// Then: a pixel under the sea is fully wet, a pixel the sea left is
/// `DRY_TICKS` less the ticks since it left, and a pixel the sea never
/// reached is dry.
///
/// Why it matters: sand that never dries leaves a dark band where the tide
/// once was, and sand that turns wet with no water reads as a stain.
#[test]
fn sand_the_sea_leaves_dries_in_a_fixed_time() {
    let mut scene = ticked(7, AREA, 0);
    let rows = usize::from(AREA.height) * 2;
    let columns = usize::from(AREA.width) * 2;
    let mut last_sea: Vec<Option<u32>> = vec![None; rows * columns];
    for tick in 0..DRY_TICKS * 3 {
        scene.tick(AREA);
        let grid = pixels(&render(&scene, AREA, AREA), AREA);
        for (row, line) in grid.iter().enumerate() {
            for (x, colour) in line.iter().enumerate() {
                // When:
                if is_sea(*colour) {
                    last_sea[row * columns + x] = Some(tick);
                }
                let wetness = scene.wetness(x, row);

                // Then:
                let expected = last_sea[row * columns + x]
                    .map_or(0, |left| DRY_TICKS.saturating_sub(tick - left));
                assert_eq!(wetness, expected, "tick {tick}: pixel ({x}, {row})");
            }
        }
    }
}

/// Wet sand is drawn darker than dry sand, and its grains hide.
/// Given: the scene at a hundred by thirty, over two minutes.
/// When: every sand pixel is read with its wetness.
/// Then: a muted pixel is always wet, a gold grain is always dry, and wet
/// sand shows on screen.
///
/// Why it matters: if the wet sand looks like the dry, the drying cannot
/// be seen, and the tide leaves no trace.
#[test]
fn wet_sand_is_muted_and_dry_sand_is_gold() {
    let mut scene = ticked(7, AREA, 0);
    let mut muted_seen = 0;
    for tick in 0..2000 {
        scene.tick(AREA);
        if tick % 25 != 0 {
            continue;
        }

        // When:
        let grid = pixels(&render(&scene, AREA, AREA), AREA);

        // Then:
        for (row, line) in grid.iter().enumerate() {
            for (x, colour) in line.iter().enumerate() {
                let wetness = scene.wetness(x, row);
                if *colour == PALETTE.muted {
                    assert!(wetness > 0, "tick {tick}: dry muted pixel ({x}, {row})");
                    muted_seen += 1;
                }
                if *colour == PALETTE.gold {
                    assert_eq!(wetness, 0, "tick {tick}: wet grain ({x}, {row})");
                }
            }
        }
    }
    assert!(muted_seen > 0, "no wet sand was ever drawn");
}

fn is_shell_or_pebble(colour: Color) -> bool {
    colour == PALETTE.rose || colour == PALETTE.subtle
}

/// Shells and pebbles lie where the sea reaches, and the sea hides them.
/// Given: the scene of three seeds, over one whole tide.
/// When: each cell of the render is read every fifty ticks.
/// Then: no cell shows a shell or a pebble beside the sea, none lies
/// where the sea can never reach or never leaves, and the number on show
/// changes as the tide comes and goes.
///
/// Why it matters: a shell drawn in the sea floats, one above the highest
/// wave is never covered, and ones that never hide show no tide at all.
#[test]
fn shells_and_pebbles_show_where_the_sea_has_gone() {
    let rows = usize::from(AREA.height) * 2;
    let lowest = rows - (LOW_TIDE * rows as f32).floor() as usize;
    let highest = rows - ((HIGH_TIDE + SWASH) * rows as f32).ceil() as usize - 1;
    for seed in [1, 7, 42] {
        let mut scene = ticked(seed, AREA, 1);
        let mut shown = Vec::new();
        for tick in 0..TIDE_TICKS {
            scene.tick(AREA);
            if tick % 50 != 0 {
                continue;
            }

            // When:
            let grid = pixels(&render(&scene, AREA, AREA), AREA);

            // Then:
            let mut count = 0;
            for top in (0..rows).step_by(2) {
                for left in (0..grid[0].len()).step_by(2) {
                    let cell = [
                        grid[top][left],
                        grid[top][left + 1],
                        grid[top + 1][left],
                        grid[top + 1][left + 1],
                    ];
                    if !cell.iter().any(|each| is_shell_or_pebble(*each)) {
                        continue;
                    }
                    count += 1;
                    assert!(
                        !cell.iter().any(|each| is_sea(*each)),
                        "seed {seed}, tick {tick}: a shell beside the sea at ({left}, {top})"
                    );
                    assert!(
                        (highest..lowest).contains(&top),
                        "seed {seed}, tick {tick}: a shell out of the sea's reach at ({left}, {top})"
                    );
                }
            }
            shown.push(count);
        }
        let (fewest, most) = (shown.iter().min().unwrap(), shown.iter().max().unwrap());
        assert!(most > fewest, "seed {seed}: always {most} on show");
        assert!(*most > 0, "seed {seed}: no shell ever shows");
    }
}

/// Given: a scene that ticked on a hundred by thirty area.
/// When: it ticks and renders on a twenty by six area inside a larger
/// canvas, then on one cell, then on a row of no height, then on none.
/// Then: it draws without a panic, and no cell outside the area is touched.
#[test]
fn a_smaller_area_draws_only_inside_it() {
    // Given:
    let canvas = Rect::new(0, 0, 40, 12);
    let mut scene = ticked(7, AREA, 40);

    // When, then:
    for area in [
        Rect::new(3, 2, 20, 6),
        Rect::new(3, 2, 1, 1),
        Rect::new(3, 2, 5, 0),
        Rect::new(3, 2, 0, 0),
    ] {
        scene.tick(area);
        let buffer = render(&scene, area, canvas);
        for position in canvas.positions() {
            if !area.contains(position) {
                assert_eq!(buffer[position].symbol(), " ", "glyph outside {area:?}");
                assert_eq!(buffer[position].bg, Color::Reset, "colour outside {area:?}");
            }
        }
    }
}

/// Given: the scene name `tide`.
/// When: it is looked up and built, then ticked and rendered.
/// Then: the name finds a scene, and its first frame already shows the sea.
///
/// Why it matters: a scene that is built but not listed cannot be chosen
/// from the command line, the picker or Tab.
#[test]
fn tide_is_a_scene_by_name() {
    // When:
    let kind = SceneKind::from_name("tide").expect("tide is listed");
    let mut scene = kind.build(7, PALETTE, &Arc::new(Corpus::embedded()), Speed::DEFAULT);
    scene.tick(AREA);

    // Then:
    let mut buffer = Buffer::empty(AREA);
    scene.render(AREA, &mut buffer);
    let grid = pixels(&buffer, AREA);
    assert!(waterlines(&grid).iter().all(|line| *line > 0));
}
