#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use hearth::config::Background;
use hearth::corpus::Corpus;
use hearth::scene::{Rng, Scene, SceneKind};
use hearth::scenes::city::City;
use hearth::scenes::hourglass::Lengths;
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);
const PALETTE: &Palette = &theme::ROSE_PINE;
/// The quadrant glyphs, in the order of the quarters they set: upper left,
/// upper right, lower left and lower right from the lowest bit.
const QUADRANTS: [char; 16] = [
    ' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█',
];

fn render(scene: &City, area: Rect, canvas: Rect) -> Buffer {
    let mut buffer = Buffer::empty(canvas);
    scene.render(area, &mut buffer);
    buffer
}

/// The colour of each pixel of the area, two by two to a cell, row by row.
fn pixels(buffer: &Buffer, area: Rect) -> Vec<Vec<Color>> {
    let mut rows =
        vec![vec![Color::Reset; usize::from(area.width) * 2]; usize::from(area.height) * 2];
    for position in area.positions() {
        let cell = &buffer[position];
        let glyph = cell.symbol().chars().next().unwrap_or(' ');
        let bits = QUADRANTS
            .iter()
            .position(|each| *each == glyph)
            .unwrap_or(0);
        for quarter in 0..4 {
            let x = usize::from(position.x - area.x) * 2 + quarter % 2;
            let y = usize::from(position.y - area.y) * 2 + quarter / 2;
            rows[y][x] = if bits & 1 << quarter != 0 {
                cell.fg
            } else {
                cell.bg
            };
        }
    }
    rows
}

fn frame(scene: &City, area: Rect) -> Vec<Vec<Color>> {
    pixels(&render(scene, area, area), area)
}

fn is_tower(colour: Color) -> bool {
    colour == PALETTE.overlay || colour == PALETTE.surface
}

fn is_window(colour: Color) -> bool {
    colour == PALETTE.gold || colour == PALETTE.rose
}

/// The lowest row that holds a tower, the row the towers stand on.
fn ground(rows: &[Vec<Color>]) -> usize {
    rows.iter()
        .rposition(|row| row.iter().any(|colour| is_tower(*colour)))
        .expect("towers")
}

/// The topmost tower row in each column, or none for a column with no
/// tower.
fn tower_tops(rows: &[Vec<Color>]) -> Vec<Option<usize>> {
    (0..rows[0].len())
        .map(|x| rows.iter().position(|row| is_tower(row[x])))
        .collect()
}

/// Asserts that the skyline fits the pane: the towers stand on one ground
/// row across most of the width, every tower reaches that row, and every
/// tower has sky above it.
fn assert_skyline_fits(rows: &[Vec<Color>], context: &str) {
    let ground = ground(rows);
    let across = rows[0].len();
    let standing = (0..across).filter(|x| is_tower(rows[ground][*x])).count();
    assert!(
        standing * 5 >= across * 4,
        "{context}: towers stand on {standing} of {across} columns"
    );
    for (x, top) in tower_tops(rows).into_iter().enumerate() {
        let Some(top) = top else { continue };
        assert!(
            top >= rows.len() / 10,
            "{context}: the tower at column {x} reaches row {top}"
        );
        assert!(
            (top..=ground).all(|y| is_tower(rows[y][x]) || is_window(rows[y][x])),
            "{context}: the tower at column {x} does not stand on the ground"
        );
    }
}

/// The towers stand on the ground across the pane, under the top, with
/// lit windows only inside them.
/// Given: the scene at a hundred by thirty, from three seeds.
/// When: it ticks and renders.
/// Then: towers stand on one ground row across most of the width, each
/// reaches that row and has sky above it, and every lit window above the
/// ground has a tower wall on both sides.
///
/// Why it matters: a window in the sky or a tower cut off by the top of
/// the pane breaks the picture of a city.
#[test]
fn windows_light_only_inside_towers_that_fit_the_pane() {
    for seed in [7, 8, 9] {
        // Given:
        let mut scene = City::new(Rng::seed_from_u64(seed), PALETTE);

        // When:
        scene.tick(AREA);
        let rows = frame(&scene, AREA);

        // Then:
        assert_skyline_fits(&rows, &format!("seed {seed}"));
        let ground = ground(&rows);
        let mut lit = 0;
        for (y, row) in rows.iter().enumerate().take(ground + 1) {
            for (x, colour) in row.iter().enumerate() {
                if !is_window(*colour) {
                    continue;
                }
                lit += 1;
                let walls =
                    x > 0 && x + 1 < row.len() && is_tower(row[x - 1]) && is_tower(row[x + 1]);
                assert!(
                    walls,
                    "seed {seed}: a lit window at {:?} outside a tower",
                    (x, y)
                );
            }
        }
        assert!(lit > 50, "seed {seed}: only {lit} lit windows");
    }
}

/// The stars and the moon are in the sky behind the towers.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders.
/// Then: every star and every pixel of the moon lies above the ground and
/// above the top of any tower in its column, and the moon shows.
///
/// Why it matters: a star on a wall or a moon in the water reads as a
/// fault.
#[test]
fn the_stars_and_moon_stay_in_the_sky() {
    for seed in [7, 8, 9] {
        // Given:
        let mut scene = City::new(Rng::seed_from_u64(seed), PALETTE);

        // When:
        scene.tick(AREA);
        let rows = frame(&scene, AREA);

        // Then:
        let ground = ground(&rows);
        let tops = tower_tops(&rows);
        let mut moon = 0;
        for (y, row) in rows.iter().enumerate() {
            for (x, colour) in row.iter().enumerate() {
                if *colour != PALETTE.subtle && *colour != PALETTE.text {
                    continue;
                }
                assert!(
                    y < ground && tops[x].is_none_or(|top| y < top),
                    "seed {seed}: sky at {:?} below a tower top {:?}",
                    (x, y),
                    tops[x]
                );
                moon += usize::from(*colour == PALETTE.text);
            }
        }
        assert!(moon > 20, "seed {seed}: the moon has {moon} pixels");
    }
}

/// The water under the city mirrors the lit windows.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders.
/// Then: every lit pixel in the water is a lit window colour, the same as
/// the pixel as far above the ground as it is below, and every lit window
/// whose mirror falls in the pane shows there.
///
/// Why it matters: water that does not mirror the city reads as a flat
/// band, and a reflection of something that is not there reads as a
/// fault.
#[test]
fn the_water_mirrors_the_lit_windows() {
    // Given:
    let mut scene = City::new(Rng::seed_from_u64(7), PALETTE);

    // When:
    scene.tick(AREA);
    let rows = frame(&scene, AREA);

    // Then:
    let shore = ground(&rows) + 1;
    let mut mirrored = 0;
    for y in shore..rows.len().min(2 * shore) {
        let above = 2 * shore - 1 - y;
        for x in 0..rows[y].len() {
            let (water, window) = (rows[y][x], rows[above][x]);
            if water != PALETTE.base {
                assert!(
                    is_window(water) && water == window,
                    "{:?} in the water mirrors {window:?}",
                    (x, y)
                );
                mirrored += 1;
            }
            if is_window(window) {
                assert_eq!(water, window, "no mirror at {:?}", (x, y));
            }
        }
    }
    assert!(mirrored > 10, "only {mirrored} lit windows in the water");
}

/// The skyline is built for each pane it is given.
/// Given: one scene.
/// When: it ticks and renders on panes of several sizes in turn.
/// Then: on each, the skyline fits the pane as at a hundred by thirty.
///
/// Why it matters: a skyline laid for one size and cropped or stretched on
/// another leaves a gap or cuts the towers off.
#[test]
fn the_skyline_is_built_again_for_each_pane() {
    // Given:
    let mut scene = City::new(Rng::seed_from_u64(7), PALETTE);

    for (width, height) in [(100, 30), (60, 20), (200, 50), (30, 10), (100, 30)] {
        // When:
        let area = Rect::new(0, 0, width, height);
        scene.tick(area);
        let rows = frame(&scene, area);

        // Then:
        assert_skyline_fits(&rows, &format!("{width} by {height}"));
    }
}

/// Given: a scene that ticked on a hundred by thirty area.
/// When: it renders on a twenty by six area inside a larger canvas before
/// it ticks there, then ticks and renders on that area, on one cell, on a
/// row of no height, and on none.
/// Then: it draws without a panic, and no cell outside the area is touched.
#[test]
fn a_smaller_area_draws_only_inside_it() {
    // Given:
    let canvas = Rect::new(0, 0, 40, 12);
    let mut scene = City::new(Rng::seed_from_u64(7), PALETTE);
    for _ in 0..40 {
        scene.tick(AREA);
    }
    let inside_only = |buffer: &Buffer, area: Rect| {
        for position in canvas.positions() {
            if !area.contains(position) {
                assert_eq!(buffer[position].symbol(), " ", "glyph outside {area:?}");
                assert_eq!(buffer[position].fg, Color::Reset, "colour outside {area:?}");
                assert_eq!(buffer[position].bg, Color::Reset, "colour outside {area:?}");
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

/// Given: the scene name `city`.
/// When: it is looked up and built, then ticked and rendered.
/// Then: the name finds a scene, and its first frame shows towers and lit
/// windows.
///
/// Why it matters: a scene that is built but not listed cannot be chosen
/// from the command line, the picker or Tab.
#[test]
fn city_is_a_scene_by_name() {
    // When:
    let kind = SceneKind::from_name("city").expect("city is listed");
    let mut scene = kind.build(
        7,
        PALETTE,
        Background::Theme,
        &Arc::new(Corpus::embedded()),
        Speed::DEFAULT,
        Lengths::DEFAULT,
    );
    scene.tick(AREA);

    // Then:
    let mut buffer = Buffer::empty(AREA);
    scene.render(AREA, &mut buffer);
    let rows = pixels(&buffer, AREA);
    assert!(
        rows.iter()
            .flatten()
            .any(|colour| *colour == PALETTE.overlay)
    );
    assert!(rows.iter().flatten().any(|colour| *colour == PALETTE.gold));
}
