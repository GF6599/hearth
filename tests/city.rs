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
/// The pixels a ripple moves a reflection to either side, at the most.
const RIPPLE: usize = 3;
/// About three seconds of ticks.
const FEW_SECONDS: u32 = 50;
/// A loop of the night and a half, so a run holds an evening and a late
/// night from any start.
const NIGHT_AND_A_HALF: u32 = 4500;
/// Ticks enough for a few trains to cross.
const TRAINS: u32 = 4000;
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
        // A near tower's antenna can stand in front of a far tower.
        assert!(
            (top..=ground).all(|y| is_tower(rows[y][x])
                || is_window(rows[y][x])
                || rows[y][x] == PALETTE.muted
                || rows[y][x] == PALETTE.love),
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
                // A near tower's antenna can stand in front of a far wall.
                let wall = |colour: Color| is_tower(colour) || colour == PALETTE.muted;
                let walls = x > 0 && x + 1 < row.len() && wall(row[x - 1]) && wall(row[x + 1]);
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

/// The water under the city mirrors the lit windows, broken by ripples.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders over a few seconds.
/// Then: in every frame each lit pixel in the water is a lit window colour,
/// and the row as far above the ground as it is below holds that colour
/// within `RIPPLE` pixels to either side. The water shows over half of the
/// lit windows whose mirror falls in the pane, and it changes in most
/// frames.
///
/// Why it matters: water that does not mirror the city reads as a flat
/// band, a reflection of something that is not there reads as a fault,
/// and water that never moves reads as a second city upside down.
#[test]
fn the_water_mirrors_the_lit_windows_through_ripples() {
    // Given:
    let mut scene = City::new(Rng::seed_from_u64(7), PALETTE);
    let (mut changed, mut before) = (0, None);

    for _ in 0..FEW_SECONDS {
        // When:
        scene.tick(AREA);
        let rows = frame(&scene, AREA);

        // Then:
        let shore = ground(&rows) + 1;
        let (mut shown, mut windows) = (0, 0);
        for y in shore..rows.len().min(2 * shore) {
            let above = &rows[2 * shore - 1 - y];
            for (x, &water) in rows[y].iter().enumerate() {
                let near = x.saturating_sub(RIPPLE)..(x + RIPPLE + 1).min(above.len());
                if water != PALETTE.base {
                    assert!(
                        is_window(water) && above[near].contains(&water),
                        "{:?} in the water mirrors no lit window",
                        (x, y)
                    );
                    shown += 1;
                }
                windows += usize::from(is_window(above[x]));
            }
        }
        assert!(
            shown * 2 > windows,
            "the water shows {shown} of {windows} lit windows"
        );
        let water = rows[shore..].to_vec();
        if before.as_ref().is_some_and(|before| *before != water) {
            changed += 1;
        }
        before = Some(water);
    }
    assert!(
        changed * 2 > FEW_SECONDS,
        "the water changed in {changed} of {FEW_SECONDS} frames"
    );
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

/// The first row of the raised line, the top row that is all its colour.
fn rail(rows: &[Vec<Color>]) -> usize {
    rows.iter()
        .position(|row| row.iter().all(|colour| *colour == PALETTE.muted))
        .expect("a raised line")
}

/// Whether a row of the city lies outside the rows a train passes on and
/// the line it runs on.
fn off_the_line(y: usize, rail: usize) -> bool {
    y + 4 < rail || y > rail + 1
}

/// Windows light and go dark one at a time as the night passes.
/// Given: the scene at a hundred by thirty.
/// When: it ticks over a loop of the night and a half.
/// Then: off the line of the train, the towers and the places of the
/// windows stay the same, at most one window changes from a frame to the
/// next, and the count of lit windows both rises and falls by over a
/// quarter of its most.
///
/// Why it matters: a city whose windows never change reads as a picture,
/// and one whose windows flicker all at once reads as a fault.
#[test]
fn windows_light_and_go_dark_one_by_one_as_the_night_passes() {
    // Given:
    let mut scene = City::new(Rng::seed_from_u64(7), PALETTE);
    let mut counts = Vec::new();
    let mut before: Option<Vec<(usize, usize, Color)>> = None;

    for tick in 0..NIGHT_AND_A_HALF {
        // When:
        scene.tick(AREA);
        let rows = frame(&scene, AREA);

        // Then:
        let (rail, shore) = (rail(&rows), ground(&rows) + 1);
        let city: Vec<(usize, usize, Color)> = rows[..shore]
            .iter()
            .enumerate()
            .filter(|(y, _)| off_the_line(*y, rail))
            .flat_map(|(y, row)| {
                row.iter()
                    .enumerate()
                    .filter(|(_, colour)| is_tower(**colour) || is_window(**colour))
                    .map(move |(x, colour)| (x, y, *colour))
            })
            .collect();
        counts.push(
            city.iter()
                .filter(|(_, _, colour)| is_window(*colour))
                .count(),
        );
        if let Some(before) = &before {
            let places = |city: &[(usize, usize, Color)]| -> Vec<(usize, usize)> {
                city.iter().map(|(x, y, _)| (*x, *y)).collect()
            };
            assert_eq!(
                places(before),
                places(&city),
                "tick {tick}: the skyline moved"
            );
            let changes = before.iter().zip(&city).filter(|(a, b)| a != b).count();
            assert!(
                changes <= 1,
                "tick {tick}: {changes} windows changed at once"
            );
        }
        before = Some(city);
    }
    let most = *counts.iter().max().unwrap();
    let (mut lowest, mut highest, mut rise, mut fall) = (usize::MAX, 0, 0, 0);
    for &count in &counts {
        lowest = lowest.min(count);
        highest = highest.max(count);
        rise = rise.max(count - lowest);
        fall = fall.max(highest - count);
    }
    assert!(
        rise * 4 > most && fall * 4 > most,
        "the lit windows rose by {rise} and fell by {fall} of {most}"
    );
}

/// The lights on the antennas blink.
/// Given: the scene at a hundred by thirty.
/// When: it ticks over a few seconds.
/// Then: the antenna lights stand in at least three places, and each place
/// is lit in some frames and dark in others.
///
/// Why it matters: a warning light that never blinks reads as a red
/// pixel stuck on the screen.
#[test]
fn the_antenna_lights_blink() {
    // Given:
    let mut scene = City::new(Rng::seed_from_u64(7), PALETTE);
    let mut lit: Vec<((usize, usize), u32)> = Vec::new();

    for _ in 0..FEW_SECONDS {
        // When:
        scene.tick(AREA);
        let rows = frame(&scene, AREA);
        for (y, row) in rows.iter().enumerate() {
            for (x, colour) in row.iter().enumerate() {
                if *colour != PALETTE.love {
                    continue;
                }
                match lit.iter_mut().find(|(place, _)| *place == (x, y)) {
                    Some((_, frames)) => *frames += 1,
                    None => lit.push(((x, y), 1)),
                }
            }
        }
    }

    // Then:
    assert!(lit.len() >= 3, "lights at {lit:?}");
    for (place, frames) in lit {
        assert!(
            frames < FEW_SECONDS,
            "the light at {place:?} is lit in every frame"
        );
    }
}

/// A lit train crosses the city on its raised line now and then.
/// Given: the scene at a hundred by thirty.
/// When: it ticks long enough for a few trains.
/// Then: each train, a run of six or more pixels of its body on the row
/// above the line, comes in at one side of the pane, moves the same way in
/// every frame, and leaves at the other side. No such run shows on any
/// row above the train's rows, and more than one train crosses.
///
/// Why it matters: a train that jumps, turns back or stops halfway reads
/// as a fault, and one that leaves its line reads as a crash.
#[test]
fn a_train_crosses_on_its_line_now_and_then() {
    // Given:
    let mut scene = City::new(Rng::seed_from_u64(7), PALETTE);
    let across = usize::from(AREA.width) * 2;
    let runs = |row: &[Color]| -> Option<(usize, usize)> {
        let mut found: Option<(usize, usize)> = None;
        let mut start = None;
        for (x, colour) in row.iter().chain([&Color::Reset]).enumerate() {
            match (start, *colour == PALETTE.muted) {
                (None, true) => start = Some(x),
                (Some(from), false) => {
                    if x - from >= 6 {
                        found = Some(found.map_or((from, x - 1), |(left, _)| (left, x - 1)));
                    }
                    start = None;
                }
                _ => {}
            }
        }
        found
    };
    let mut crossed = 0;
    // The edge a train came in at, and its extent in the frame before.
    let mut train: Option<(bool, (usize, usize))> = None;

    for tick in 0..TRAINS {
        // When:
        scene.tick(AREA);
        let rows = frame(&scene, AREA);

        // Then:
        let rail = rail(&rows);
        for (y, row) in rows.iter().enumerate().take(rail - 4) {
            assert!(runs(row).is_none(), "tick {tick}: a train body on row {y}");
        }
        let now = runs(&rows[rail - 1]);
        train = match (train, now) {
            (None, None) => None,
            (None, Some((left, right))) => {
                assert!(
                    left == 0 || right == across - 1,
                    "tick {tick}: a train came in at {left} to {right}"
                );
                Some((left == 0, (left, right)))
            }
            (Some((from_left, (left, right))), Some((now_left, now_right))) => {
                let forward = if from_left {
                    now_left >= left && now_right >= right
                } else {
                    now_left <= left && now_right <= right
                };
                assert!(
                    forward,
                    "tick {tick}: the train moved from {left}..{right} to {now_left}..{now_right}"
                );
                Some((from_left, (now_left, now_right)))
            }
            (Some((from_left, (left, right))), None) => {
                let other_side = if from_left {
                    right == across - 1
                } else {
                    left == 0
                };
                assert!(other_side, "tick {tick}: a train left at {left} to {right}");
                crossed += 1;
                None
            }
        };
    }
    assert!(crossed >= 2, "{crossed} trains crossed");
}
