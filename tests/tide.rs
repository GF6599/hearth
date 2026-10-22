#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pixels;

use std::sync::Arc;

use hearth::config::Background;
use hearth::corpus::Corpus;
use hearth::octant::{CELL_HEIGHT, CELL_WIDTH};
use hearth::scene::{Rng, Scene, SceneKind, Settings};
use hearth::scenes::tide::{DRY_TICKS, HIGH_TIDE, LOW_TIDE, SWASH, TIDE_TICKS, Tide};
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

/// The red, green and blue of a colour the scene draws.
fn channels(colour: Color) -> (u8, u8, u8) {
    match colour {
        Color::Rgb(r, g, b) => (r, g, b),
        other => panic!("{other:?} is not a colour of the theme"),
    }
}

/// The scene shades each part with mixes of theme colours, so a test reads
/// a part by its hue and not by one exact colour. In Rose Pine the sea's
/// roles, pine and foam, hold more green than red, and so does every mix
/// of them. Sand, shells and pebbles hold more red than green.
fn is_sea(colour: Color) -> bool {
    let (r, g, _) = channels(colour);
    g > r
}

/// Sand is warm: red over green over blue, as gold and rose are.
fn is_sand(colour: Color) -> bool {
    let (r, g, b) = channels(colour);
    r > g && g >= b
}

/// A shell is pink: red over blue over green, as love is.
fn is_shell(colour: Color) -> bool {
    let (r, g, b) = channels(colour);
    r > b && b > g
}

/// The farthest a pebble lies from subtle, as a distance between red,
/// green and blue. The greys of Rose Pine share one hue, so a pebble is
/// read by its distance to its own role and not by its hue alone.
const PEBBLE_REACH: f32 = 45.0;

/// A pebble is grey: blue over red, and red over green, as subtle is, and
/// within `PEBBLE_REACH` of subtle.
fn is_pebble(colour: Color) -> bool {
    let (r, g, b) = channels(colour);
    b > r && r >= g && pixels::distance(colour, PALETTE.subtle).floor() <= PEBBLE_REACH
}

/// Given: the Rose Pine roles of each part of the beach, and the roles the
/// beach does not draw as a part.
/// When: each is read by the tests above.
/// Then: each role of a part reads as its part and as no other, and each
/// other role reads as no part.
///
/// Why it matters: the other tests read the beach by these hue tests, and a
/// hue test that reads a role wrong would pass a broken picture.
#[test]
fn each_role_reads_as_its_part_of_the_beach() {
    // Given:
    let parts = [is_sea, is_sand, is_shell, is_pebble];
    for (role, part) in [
        (PALETTE.pine, 0),
        (PALETTE.foam, 0),
        (PALETTE.gold, 1),
        (PALETTE.rose, 1),
        (PALETTE.love, 2),
        (PALETTE.subtle, 3),
        (PALETTE.base, usize::MAX),
        (PALETTE.surface, usize::MAX),
        (PALETTE.overlay, usize::MAX),
        (PALETTE.muted, usize::MAX),
        (PALETTE.text, usize::MAX),
        (PALETTE.iris, usize::MAX),
    ] {
        // When, then:
        for (index, reads) in parts.iter().enumerate() {
            assert_eq!(reads(role), index == part, "{role:?} as part {index}");
        }
    }
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

/// The most a channel of dry sand changes from one pixel to the next, so
/// the sand shades with no single grain that stands out.
const SAND_STEP: u8 = 12;

/// Dry sand is gold, shaded gently over the beach.
/// Given: the scene at a hundred by thirty.
/// When: the pixels above the highest reach of the sea are read.
/// Then: every pixel is sand, the sand shows four shades or more, and no
/// channel changes by more than `SAND_STEP` from a pixel to the one on its
/// right or under it.
///
/// Why it matters: single grains on one flat colour read as a speckle of
/// blocks, and flat sand reads as a painted wall and not as a beach.
#[test]
fn dry_sand_is_shaded_gently_with_no_speckle() {
    // Given:
    let scene = ticked(7, AREA, 200);

    // When:
    let grid = pixels(&render(&scene, AREA, AREA), AREA);
    let rows = grid.len();
    let dry = rows - ((HIGH_TIDE + SWASH) * rows as f32).ceil() as usize - 1;

    // Then:
    let mut shades: Vec<Color> = Vec::new();
    for y in 0..dry {
        for x in 0..grid[0].len() {
            let colour = grid[y][x];
            assert!(is_sand(colour), "{colour:?} at ({x}, {y})");
            if !shades.contains(&colour) {
                shades.push(colour);
            }
            for (other, at) in [
                (grid[y].get(x + 1), (x + 1, y)),
                (grid.get(y + 1).map(|row| &row[x]), (x, y + 1)),
            ] {
                let Some(other) = other.filter(|_| at.1 < dry) else {
                    continue;
                };
                let ((r, g, b), (r2, g2, b2)) = (channels(colour), channels(*other));
                let step = r.abs_diff(r2).max(g.abs_diff(g2)).max(b.abs_diff(b2));
                assert!(
                    step <= SAND_STEP,
                    "{colour:?} at ({x}, {y}) beside {other:?} at {at:?}"
                );
            }
        }
    }
    assert!(shades.len() >= 4, "dry sand in {shades:?}");
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

/// Wet sand is drawn darker than dry sand, and lightens as it dries.
/// Given: the scene at a hundred by thirty, over two minutes.
/// When: every sand pixel is read with its wetness, in four bands from dry
/// to just left by the sea.
/// Then: each band shows, and the mean brightness of each band is below
/// the band that is drier.
///
/// Why it matters: if the wet sand looks like the dry, the drying cannot
/// be seen and the tide leaves no trace, and wet sand that turns dry in one
/// step leaves a hard line where it should fade.
#[test]
fn wet_sand_is_darker_and_lightens_as_it_dries() {
    let mut scene = ticked(7, AREA, 0);
    let mut bands = [(0_u64, 0_u64); 4];
    for tick in 0..2000 {
        scene.tick(AREA);
        if tick % 25 != 0 {
            continue;
        }

        // When:
        let grid = pixels(&render(&scene, AREA, AREA), AREA);
        for (row, line) in grid.iter().enumerate() {
            for (x, colour) in line.iter().enumerate() {
                if !is_sand(*colour) {
                    continue;
                }
                let wetness = scene.wetness(x, row);
                let band = if wetness == 0 {
                    0
                } else {
                    1 + (3 * (wetness - 1) / DRY_TICKS) as usize
                };
                bands[band].0 += u64::from(theme::luma(*colour));
                bands[band].1 += 1;
            }
        }
    }

    // Then:
    let means: Vec<u64> = bands
        .iter()
        .map(|(sum, count)| {
            assert!(*count > 0, "a band of wetness never shows: {bands:?}");
            sum / count
        })
        .collect();
    assert!(
        means.windows(2).all(|pair| pair[0] > pair[1]),
        "mean brightness from dry to wet: {means:?}"
    );
}

/// The whole beach is sand, shells and pebbles, and none of it is the
/// background.
/// Given: the scene at a hundred by thirty, over two minutes.
/// When: every pixel above the sea is read, at the shore, around a shell
/// or a pebble, and on wet and dry sand.
/// Then: every pixel above the sea reads as sand, a shell or a pebble, so
/// none is the base colour.
///
/// Why it matters: a beach in the background colour reads as a night sky,
/// a strip of it at the shore reads as a gap between sand and sea, and a
/// colour of no part reads as a stain.
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
                    assert!(
                        is_sand(*colour) || is_shell_or_pebble(*colour),
                        "tick {tick}: {colour:?} on the beach at ({x}, {row}) is no part of it"
                    );
                }
            }
        }
    }
}

fn is_shell_or_pebble(colour: Color) -> bool {
    is_shell(colour) || is_pebble(colour)
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
                if !cell.iter().any(|each| is_shell(*each)) {
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

/// The sea darkens from its edge down to the bottom of the pane.
/// Given: the scene of three seeds, over one whole tide.
/// When: each column of the sea is read from its top pixel down, every
/// fifty ticks.
/// Then: no pixel is brighter than the one above it, and over the whole
/// tide the sea shows eight shades or more.
///
/// Why it matters: a sea of one flat colour reads as a painted band, and a
/// lighter pixel under a darker one reads as a stain in the water.
#[test]
fn the_sea_darkens_with_depth() {
    for seed in [1, 7, 42] {
        let mut scene = ticked(seed, AREA, 1);
        let mut shades: Vec<Color> = Vec::new();
        for tick in 0..TIDE_TICKS {
            scene.tick(AREA);
            if tick % 50 != 0 {
                continue;
            }

            // When:
            let grid = pixels(&render(&scene, AREA, AREA), AREA);

            // Then:
            for (x, line) in waterlines(&grid).into_iter().enumerate() {
                let sea: Vec<Color> = grid[grid.len() - line..].iter().map(|row| row[x]).collect();
                for (depth, pair) in sea.windows(2).enumerate() {
                    assert!(
                        theme::luma(pair[1]) <= theme::luma(pair[0]),
                        "seed {seed}, tick {tick}: column {x} lightens from {:?} to {:?} at depth {depth}",
                        pair[0],
                        pair[1]
                    );
                }
                for colour in sea {
                    if !shades.contains(&colour) {
                        shades.push(colour);
                    }
                }
            }
        }
        assert!(shades.len() >= 8, "seed {seed}: the sea in {shades:?}");
    }
}

/// The foam at the edge of the sea fades into the water under it.
/// Given: the scene of three seeds, over one whole tide.
/// When: the top two cells of pixels of each column of the sea are read,
/// every fifty ticks.
/// Then: in most of those columns they hold three colours or more, foam at
/// the top and mixes of foam and the water under it.
///
/// Why it matters: a band of foam that ends in a hard line on the water
/// reads as a stripe of paint and not as froth on a wave.
#[test]
fn the_foam_fades_into_the_sea() {
    for seed in [1, 7, 42] {
        let mut scene = ticked(seed, AREA, 1);
        let (mut soft, mut columns) = (0, 0);
        for tick in 0..TIDE_TICKS {
            scene.tick(AREA);
            if tick % 50 != 0 {
                continue;
            }

            // When:
            let grid = pixels(&render(&scene, AREA, AREA), AREA);
            for (x, line) in waterlines(&grid).into_iter().enumerate() {
                if line < 2 * CELL_HEIGHT {
                    continue;
                }
                let top = grid.len() - line;
                let mut edge: Vec<Color> = Vec::new();
                for row in &grid[top..top + 2 * CELL_HEIGHT] {
                    if !edge.contains(&row[x]) {
                        edge.push(row[x]);
                    }
                }
                columns += 1;
                soft += usize::from(edge.len() >= 3);
            }
        }

        // Then:
        assert!(
            soft * 2 > columns,
            "seed {seed}: {soft} of {columns} columns fade from the foam"
        );
    }
}

/// Each shell and pebble has a shade of its own.
/// Given: the scene of three seeds, over one whole tide.
/// When: the colours of the shells and of the pebbles are read every fifty
/// ticks.
/// Then: the shells show three colours or more, and so do the pebbles.
///
/// Why it matters: shells all in one flat colour read as a pattern stamped
/// on the sand and not as things that lie on a beach.
#[test]
fn shells_and_pebbles_are_shaded() {
    for seed in [1, 7, 42] {
        let mut scene = ticked(seed, AREA, 1);
        let (mut shells, mut pebbles): (Vec<Color>, Vec<Color>) = (Vec::new(), Vec::new());
        for tick in 0..TIDE_TICKS {
            scene.tick(AREA);
            if tick % 50 != 0 {
                continue;
            }

            // When:
            let grid = pixels(&render(&scene, AREA, AREA), AREA);
            for colour in grid.into_iter().flatten() {
                let seen = if is_shell(colour) {
                    &mut shells
                } else if is_pebble(colour) {
                    &mut pebbles
                } else {
                    continue;
                };
                if !seen.contains(&colour) {
                    seen.push(colour);
                }
            }
        }

        // Then:
        assert!(shells.len() >= 3, "seed {seed}: shells in {shells:?}");
        assert!(pebbles.len() >= 3, "seed {seed}: pebbles in {pebbles:?}");
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

/// Over the terminal's background the beach mixes no colour with the base.
/// Given: the scene in flat Rose Pine, whose base and surface are the
/// terminal's own background.
/// When: it ticks and renders over two minutes.
/// Then: no pixel lies on a mix from the base toward pine, foam, gold, rose
/// or love, the colours the beach draws.
///
/// Why it matters: a mix with a see-through base is an opaque colour that
/// darkens toward the theme's base, which the user does not see, so deep
/// water and wet sand show a colour that is not on their screen.
#[test]
fn over_the_terminal_background_nothing_mixes_with_the_base() {
    // Given:
    let palette = &theme::FLAT_THEMES[0];
    let drawn = [
        palette.pine,
        palette.foam,
        palette.gold,
        palette.rose,
        palette.love,
    ];
    // Deep water leans pine toward overlay, and overlay lies near the base,
    // so the last tenth of each mix reads as its colour.
    let mixes = pixels::base_mixes(palette.base, &drawn, 1..180);
    let mut scene = Tide::new(Rng::seed_from_u64(7), palette);
    for tick in 0..2000 {
        scene.tick(AREA);
        if tick % 100 != 0 {
            continue;
        }

        // When:
        let grid = pixels(&render(&scene, AREA, AREA), AREA);

        // Then:
        for (y, row) in grid.iter().enumerate() {
            for (x, colour) in row.iter().enumerate() {
                assert!(
                    !mixes.contains(&channels(*colour)),
                    "tick {tick}: {colour:?} at {:?} mixes with the base",
                    (x, y)
                );
            }
        }
    }
}
