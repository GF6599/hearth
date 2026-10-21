#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pixels;

use std::sync::Arc;

use hearth::config::Background;
use hearth::corpus::Corpus;
use hearth::octant::{CELL_HEIGHT, CELL_WIDTH};
use hearth::scene::{Rng, Scene, SceneKind, Settings};
use hearth::scenes::tide::{DRY_TICKS, GRAINS, HIGH_TIDE, LOW_TIDE, SWASH, TIDE_TICKS, Tide};
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);
const PALETTE: &Palette = &theme::ROSE_PINE;

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
    pixels::grid(buffer, area, |position, cell| {
        pixels::shown(cell).unwrap_or_else(|| panic!("{:?} at {position:?}", cell.symbol()))
    })
}

/// The pixels of each cell of a render, row by row, with the top left pixel
/// of the cell.
fn cells(grid: &[Vec<Color>]) -> Vec<((usize, usize), Vec<Color>)> {
    (0..grid.len())
        .step_by(CELL_HEIGHT)
        .flat_map(|top| {
            (0..grid[0].len()).step_by(CELL_WIDTH).map(move |left| {
                let pixels = grid[top..top + CELL_HEIGHT]
                    .iter()
                    .flat_map(|row| &row[left..left + CELL_WIDTH])
                    .copied()
                    .collect();
                ((left, top), pixels)
            })
        })
        .collect()
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
    let rows = f32::from(AREA.height) * CELL_HEIGHT as f32;
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

/// Dry sand is gold, with a sparse, even scatter of rose grains.
/// Given: the scene at a hundred by thirty.
/// When: the pixels above the highest reach of the sea are read in tiles
/// of four by four.
/// Then: each tile holds exactly `GRAINS` rose pixels, and every other
/// pixel is gold.
///
/// Why it matters: grains on the background read as stars in a night sky
/// and not as a beach, and grains that clump or thin out read as a stain.
#[test]
fn dry_sand_is_gold_with_an_even_scatter_of_rose_grains() {
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
            let rose = tile.iter().filter(|each| **each == PALETTE.rose).count();
            let gold = tile.iter().filter(|each| **each == PALETTE.gold).count();
            assert_eq!(
                (rose, gold),
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
    let rows = usize::from(AREA.height) * CELL_HEIGHT;
    let columns = usize::from(AREA.width) * CELL_WIDTH;
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
/// Then: a muted pixel is always wet, a rose grain is always dry, and wet
/// sand shows on screen.
///
/// Why it matters: if the wet sand looks like the dry, the drying cannot
/// be seen, and the tide leaves no trace.
#[test]
fn wet_sand_is_muted_and_its_grains_hide() {
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
                if *colour == PALETTE.rose {
                    assert_eq!(wetness, 0, "tick {tick}: wet grain ({x}, {row})");
                }
            }
        }
    }
    assert!(muted_seen > 0, "no wet sand was ever drawn");
}

/// The whole beach is sand, and none of it is the background.
/// Given: the scene at a hundred by thirty, over two minutes.
/// When: every pixel above the sea is read, at the shore, around a shell
/// or a pebble, and on wet and dry sand.
/// Then: no pixel above the sea is the base colour.
///
/// Why it matters: a beach in the background colour reads as a night sky,
/// and a strip of it at the shore reads as a gap between sand and sea.
#[test]
fn no_part_of_the_beach_is_the_background() {
    let mut scene = ticked(7, AREA, 0);
    for tick in 0..2000 {
        scene.tick(AREA);
        if tick % 25 != 0 {
            continue;
        }

        // When:
        let grid = pixels(&render(&scene, AREA, AREA), AREA);
        let waterlines = waterlines(&grid);

        // Then:
        for (row, line) in grid.iter().enumerate() {
            for (x, colour) in line.iter().enumerate() {
                if grid.len() - row > waterlines[x] {
                    assert_ne!(
                        *colour, PALETTE.base,
                        "tick {tick}: background on the beach at ({x}, {row})"
                    );
                }
            }
        }
    }
}

fn is_shell_or_pebble(colour: Color) -> bool {
    colour == PALETTE.love || colour == PALETTE.subtle
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
    let rows = usize::from(AREA.height) * CELL_HEIGHT;
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
            for ((left, top), cell) in cells(&grid) {
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
            shown.push(count);
        }
        let (fewest, most) = (shown.iter().min().unwrap(), shown.iter().max().unwrap());
        assert!(most > fewest, "seed {seed}: always {most} on show");
        assert!(*most > 0, "seed {seed}: no shell ever shows");
    }
}

/// The edge of the sea moves up and down the sand by a quarter of a cell.
/// Given: the scene of three seeds, over one whole tide.
/// When: the waterline of every column is read every fifty ticks.
/// Then: some waterlines end inside the upper or the lower half of a
/// cell, a pixel row from its middle or its edge.
///
/// Why it matters: a waterline in steps of half a cell moves in jumps as
/// the tide turns, where the beach should change smoothly.
#[test]
fn the_waterline_moves_by_a_quarter_of_a_cell() {
    for seed in [1, 7, 42] {
        let mut scene = ticked(seed, AREA, 1);
        let mut odd = 0;
        for tick in 0..TIDE_TICKS {
            scene.tick(AREA);
            if tick % 50 != 0 {
                continue;
            }

            // When:
            let lines = waterlines(&pixels(&render(&scene, AREA, AREA), AREA));

            // Then:
            odd += lines.iter().filter(|line| *line % 2 == 1).count();
        }
        assert!(odd > 0, "seed {seed}: every waterline ends on a half cell");
    }
}

/// A shell has a shape that a grid of half cells cannot draw.
/// Given: the scene of three seeds, over one whole tide.
/// When: each cell that shows a shell is read every fifty ticks.
/// Then: some shell shows one colour in a pixel row and another in the
/// row under it, inside the upper or the lower half of its cell.
///
/// Why it matters: a shell of half cells is a square block, and it reads
/// as a stray glyph rather than a shell on the sand.
#[test]
fn a_shell_is_drawn_at_a_quarter_of_a_cell() {
    for seed in [1, 7, 42] {
        let mut scene = ticked(seed, AREA, 1);
        let mut fine = 0;
        for tick in 0..TIDE_TICKS {
            scene.tick(AREA);
            if tick % 50 != 0 {
                continue;
            }

            // When:
            let grid = pixels(&render(&scene, AREA, AREA), AREA);

            // Then:
            for (_, cell) in cells(&grid) {
                if !cell.contains(&PALETTE.love) {
                    continue;
                }
                let rows: Vec<&[Color]> = cell.chunks(CELL_WIDTH).collect();
                fine += usize::from(rows[0] != rows[1] || rows[2] != rows[3]);
            }
        }
        assert!(
            fine > 0,
            "seed {seed}: every shell is a block of half cells"
        );
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
    let grid = pixels(&buffer, AREA);
    assert!(waterlines(&grid).iter().all(|line| *line > 0));
}

/// A pane tall enough that the edge of a wave runs over many pixel rows in
/// one tick.
const TALL: Rect = Rect::new(0, 0, 8, 600);
/// The pixel rows a waterline must move in one tick for a quarter of that
/// move to show as at least one pixel row.
const LEAP: usize = 6;

/// The edge of a wave glides over the sand between two ticks.
/// Given: the scene on a pane eight cells wide and six hundred tall, at the
/// first tick after which a waterline moves by `LEAP` pixel rows or more.
/// When: it renders a quarter, a half and three quarters of the way to the
/// next tick, and again at no progress.
/// Then: at no progress the frame is the frame of a scene that was never
/// given progress. In each column that moves by `LEAP` or more, the
/// waterline at each progress lies strictly between its lines at the two
/// ticks, in the order of the progress.
///
/// Why it matters: at a slow rate a scene ticks once in eight or sixteen
/// frames, and a waterline that waits for the tick moves in jumps.
#[test]
fn the_waterline_glides_between_two_ticks() {
    // Given:
    let mut scene = ticked(7, TALL, 1);
    let mut next = ticked(7, TALL, 2);
    let lines = |scene: &Tide| waterlines(&pixels(&render(scene, TALL, TALL), TALL));
    let (mut from, mut to) = (lines(&scene), lines(&next));
    for _ in 0..TIDE_TICKS {
        if from.iter().zip(&to).any(|(a, b)| a.abs_diff(*b) >= LEAP) {
            break;
        }
        scene.tick(TALL);
        next.tick(TALL);
        (from, to) = (to, lines(&next));
    }
    let moving: Vec<usize> = (0..from.len())
        .filter(|x| from[*x].abs_diff(to[*x]) >= LEAP)
        .collect();
    assert!(
        !moving.is_empty(),
        "no waterline moves {LEAP} rows in a tick"
    );
    let still = render(&scene, TALL, TALL);

    // When:
    let gliding: Vec<Vec<usize>> = [0.25, 0.5, 0.75]
        .into_iter()
        .map(|progress| {
            scene.set_progress(progress);
            lines(&scene)
        })
        .collect();
    scene.set_progress(0.0);

    // Then:
    assert_eq!(
        render(&scene, TALL, TALL),
        still,
        "the frame at no progress"
    );
    for x in moving {
        let path: Vec<usize> = std::iter::once(from[x])
            .chain(gliding.iter().map(|lines| lines[x]))
            .chain(std::iter::once(to[x]))
            .collect();
        let order = from[x].cmp(&to[x]);
        assert!(
            path.windows(2).all(|pair| pair[0].cmp(&pair[1]) == order),
            "column {x}: waterline {path:?} from one tick to the next"
        );
    }
}
