#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pixels;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use hearth::config::Background;
use hearth::corpus::Corpus;
use hearth::octant::{self, CELL_HEIGHT, CELL_WIDTH};
use hearth::scene::{Rng, Scene, SceneKind, Settings};
use hearth::scenes::city::City;
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);
/// The pixels a streak in the water lies to either side of its window at
/// the most: a ripple slides a row by up to two cells, and a streak reaches
/// a pixel left and two right of its window.
const RIPPLE: usize = 2 * CELL_WIDTH + 2;
/// About three seconds of ticks.
const FEW_SECONDS: u32 = 50;
/// A loop of the night and a half, so a run holds an evening and a late
/// night from any start.
const NIGHT_AND_A_HALF: u32 = 4500;
/// Ticks enough for a few trains to cross.
const TRAINS: u32 = 4000;
/// The progress between two ticks that a glide test draws at.
const BETWEEN: [f64; 3] = [0.25, 0.5, 0.75];
const PALETTE: &Palette = &theme::ROSE_PINE;

fn render(scene: &City, area: Rect, canvas: Rect) -> Buffer {
    let mut buffer = Buffer::empty(canvas);
    scene.render(area, &mut buffer);
    buffer
}

/// The colour of each pixel of the area, two across and four down to a
/// cell, row by row.
/// A cell with another glyph shows its background.
fn pixels(buffer: &Buffer, area: Rect) -> Vec<Vec<Color>> {
    pixels::grid(buffer, area, |_, cell| {
        pixels::shown(cell).unwrap_or([cell.bg; pixels::PIXELS])
    })
}

/// The steps of coverage the canvas tells apart: it samples four by four
/// points of each pixel.
const COVERAGE_STEPS: usize = 16;

/// The roles the city draws, each shaded by mixes that lie nearer it than
/// any other role.
const ROLES: [Color; 10] = [
    PALETTE.base,
    PALETTE.surface,
    PALETTE.overlay,
    PALETTE.pine,
    PALETTE.muted,
    PALETTE.subtle,
    PALETTE.text,
    PALETTE.love,
    PALETTE.gold,
    PALETTE.rose,
];

/// Each mix of the sky of row `y` toward the moon, from the first step of
/// coverage to the moon itself.
fn moonlight(scene: &City, y: usize) -> Vec<Color> {
    (1..=COVERAGE_STEPS)
        .map(|step| {
            octant::mix(
                scene.sky(y),
                PALETTE.text,
                step as f32 / COVERAGE_STEPS as f32,
            )
        })
        .collect()
}

/// The role `colour` lies nearest.
fn nearest(colour: Color) -> Color {
    let channels = |colour: Color| match colour {
        Color::Rgb(r, g, b) => [r, g, b],
        other => panic!("{other:?} is not a colour of the theme"),
    };
    *ROLES
        .iter()
        .min_by_key(|role| {
            channels(**role)
                .iter()
                .zip(channels(colour))
                .map(|(a, b)| u32::from(a.abs_diff(b)).pow(2))
                .sum::<u32>()
        })
        .unwrap()
}

/// The role each pixel of a render shows, so the tests of the skyline read
/// a shaded wall as its wall and the sky as the sky. The sky of a row
/// reads as the base. A mix of the sky and the moon, the moon's rim and
/// its glow, reads as the moon from half way on and as the sky below. Any
/// other colour reads as the role it lies nearest.
fn frame(scene: &City, area: Rect) -> Vec<Vec<Color>> {
    let mut rows = pixels(&render(scene, area, area), area);
    let mut known: HashMap<Color, Color> = HashMap::new();
    for (y, row) in rows.iter_mut().enumerate() {
        let (sky, moon) = (scene.sky(y), moonlight(scene, y));
        for colour in row.iter_mut() {
            *colour = if *colour == sky {
                PALETTE.base
            } else if let Some(step) = moon.iter().position(|each| each == colour) {
                if (step + 1) * 2 >= COVERAGE_STEPS {
                    PALETTE.text
                } else {
                    PALETTE.base
                }
            } else {
                *known.entry(*colour).or_insert_with(|| nearest(*colour))
            };
        }
    }
    rows
}

/// Given: each role the city draws.
/// When: it is read by the role it lies nearest.
/// Then: it reads as itself.
///
/// Why it matters: the tests of the skyline read each pixel by its role,
/// and a role read wrong would pass a broken picture.
#[test]
fn each_role_reads_as_itself() {
    // Given:
    for colour in ROLES {
        // When, then:
        assert_eq!(nearest(colour), colour, "{colour:?}");
    }
}

/// A wall, far or near, a dark window, or the rim of a near tower.
fn is_tower(colour: Color) -> bool {
    colour == PALETTE.overlay || colour == PALETTE.surface || colour == PALETTE.pine
}

fn is_window(colour: Color) -> bool {
    colour == PALETTE.gold || colour == PALETTE.rose
}

fn all_muted(row: &[Color]) -> bool {
    row.iter().all(|colour| *colour == PALETTE.muted)
}

/// The row the towers stand on, just above the water line. The water line
/// is the lowest run of rows that are all muted.
fn ground(rows: &[Vec<Color>]) -> usize {
    let mut line = rows
        .iter()
        .rposition(|row| all_muted(row))
        .expect("a water line");
    while line > 0 && all_muted(&rows[line - 1]) {
        line -= 1;
    }
    line - 1
}

/// The topmost tower row in each column above the water, or none for a
/// column with no tower.
fn tower_tops(rows: &[Vec<Color>]) -> Vec<Option<usize>> {
    let ground = ground(rows);
    (0..rows[0].len())
        .map(|x| rows[..=ground].iter().position(|row| is_tower(row[x])))
        .collect()
}

/// Whether the pixel at `x` in `row` has a near tower's wall on both sides.
fn in_near_wall(row: &[Color], x: usize) -> bool {
    x > 0 && x + 1 < row.len() && row[x - 1] == PALETTE.overlay && row[x + 1] == PALETTE.overlay
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
/// ground has a tower wall on both sides. Over fifty windows of the near
/// towers show, lit or dark, each counted at its top pixel.
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
        let mut shown = 0;
        for (y, row) in rows.iter().enumerate().take(ground + 1) {
            for (x, colour) in row.iter().enumerate() {
                let dark = *colour == PALETTE.surface && in_near_wall(row, x);
                let top = y > 0 && rows[y - 1][x] != *colour;
                shown += usize::from(top && (dark || is_window(*colour) && in_near_wall(row, x)));
                if !is_window(*colour) {
                    continue;
                }
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
        assert!(shown > 50, "seed {seed}: only {shown} near windows show");
    }
}

/// The stars and the moon are in the sky behind the towers.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders.
/// Then: every star and every pixel of the moon lies above the ground and
/// above the top of any tower in its column, and the moon shows with over
/// five cells of pixels.
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
        assert!(
            moon > 5 * CELL_WIDTH * CELL_HEIGHT,
            "seed {seed}: the moon has {moon} pixels"
        );
    }
}

/// The water under the city reads as water.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders over a few seconds.
/// Then: in every frame a water line runs under the ground, and the water
/// starts a cell under the shore. Under the middle of every near tower the
/// first row of the water is the dim tower colour or a streak, as the
/// towers mirror in it. Every run of lit pixels in the water is two pixels
/// wide or more, unless it meets the pane's edge, and each lit pixel has a
/// lit window of its colour within `RIPPLE` pixels in the row as far above
/// the shore's cell as it is below. Over half of the lit windows whose
/// mirror falls in the pane show in the water, on the row of their mirror
/// or the row beside it, and the water changes in most frames.
///
/// Why it matters: lit pixels on a dark band do not read as a reflection,
/// a reflection of something that is not there reads as a fault, and water
/// that never moves reads as a second city upside down.
#[test]
fn the_water_mirrors_the_city_in_streaks_that_ripple() {
    // Given:
    let mut scene = City::new(Rng::seed_from_u64(7), PALETTE);
    let (mut changed, mut before) = (0, None);

    for tick in 0..FEW_SECONDS {
        // When:
        scene.tick(AREA);
        let rows = frame(&scene, AREA);

        // Then:
        let ground = ground(&rows);
        let shore = ground + 1;
        let water = shore + CELL_HEIGHT;
        let across = rows[0].len();
        for x in 4..across - 4 {
            if rows[ground][x - 4..=x + 4]
                .iter()
                .all(|colour| *colour == PALETTE.overlay)
            {
                let first = rows[water][x];
                assert!(
                    first == PALETTE.surface || is_window(first),
                    "tick {tick}: {first:?} in the water under the near tower at column {x}"
                );
            }
        }
        let (mut shown, mut windows) = (0, 0);
        for y in water..rows.len().min(shore + water) {
            let above = &rows[shore + water - 1 - y];
            let mut run = 0;
            for (x, &water) in rows[y].iter().enumerate() {
                if is_window(water) {
                    let near = x.saturating_sub(RIPPLE)..(x + RIPPLE + 1).min(across);
                    assert!(
                        above[near].contains(&water),
                        "tick {tick}: {:?} in the water mirrors no lit window",
                        (x, y)
                    );
                    run += 1;
                } else {
                    assert!(
                        run != 1 || x == 1,
                        "tick {tick}: a lit pixel alone at {:?}",
                        (x - 1, y)
                    );
                    run = 0;
                }
            }
            // A window is two pixel rows tall, and its streak shows on one
            // of the two rows of its mirror.
            let mirror = y.saturating_sub(1)..(y + 2).min(rows.len());
            for (x, &window) in above.iter().enumerate() {
                if is_window(window) {
                    windows += 1;
                    let near = x.saturating_sub(RIPPLE)..(x + RIPPLE + 1).min(across);
                    shown += usize::from(
                        rows[mirror.clone()]
                            .iter()
                            .any(|row| row[near.clone()].contains(&window)),
                    );
                }
            }
        }
        assert!(
            shown * 2 > windows,
            "tick {tick}: the water shows {shown} of {windows} lit windows"
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

/// Each near tower reads as its own shape.
/// Given: the scene at a hundred by thirty, from three seeds.
/// When: it ticks and renders.
/// Then: on the ground row each near tower, a run of three or more pixels of
/// its wall, ends in a rim at its right, or a pillar of the raised line in
/// front of the rim, before the next tower or the pane's edge. Its roof, the
/// first rim above the ground in its middle column, is there, and two near
/// towers that touch differ in height by two cells or more.
///
/// Why it matters: towers of one colour side by side melt into one block,
/// and the skyline stops reading as a city.
#[test]
fn each_near_tower_has_a_rim_and_its_own_roof_line() {
    for seed in [7, 8, 9] {
        // Given:
        let mut scene = City::new(Rng::seed_from_u64(seed), PALETTE);

        // When:
        scene.tick(AREA);
        let rows = frame(&scene, AREA);

        // Then:
        let ground = ground(&rows);
        let row = &rows[ground];
        let mut towers: Vec<(usize, usize, usize)> = Vec::new();
        let mut x = 0;
        while x < row.len() {
            if row[x] != PALETTE.overlay {
                x += 1;
                continue;
            }
            let start = x;
            while x < row.len() && row[x] == PALETTE.overlay {
                x += 1;
            }
            if x - start < 3 {
                continue;
            }
            // A pillar of the raised line can stand in front of the rim.
            assert!(
                x == row.len() || row[x] == PALETTE.pine || row[x] == PALETTE.muted,
                "seed {seed}: the near tower at {start}..{x} ends in {:?}",
                row[x]
            );
            let middle = (start + x) / 2;
            let roof = (0..ground)
                .rev()
                .find(|y| rows[*y][middle] == PALETTE.pine)
                .unwrap_or_else(|| panic!("seed {seed}: no roof over column {middle}"));
            towers.push((start, x, roof));
        }
        assert!(towers.len() >= 4, "seed {seed}: near towers {towers:?}");
        for pair in towers.windows(2) {
            let ((_, rim, first), (start, _, second)) = (pair[0], pair[1]);
            if start == rim + 1 {
                assert!(
                    first.abs_diff(second) >= 2 * CELL_HEIGHT,
                    "seed {seed}: touching towers at {rim} with roofs at {first} and {second}"
                );
            }
        }
    }
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

/// The moon and the roofs are drawn at a quarter of a cell.
/// Given: the scene at a hundred by thirty, from three seeds.
/// When: it ticks and renders.
/// Then: some pixel row of the moon starts or ends in another column than
/// the row above it, inside the upper or the lower half of a cell, and
/// some tower has its top row in the middle of a cell.
///
/// Why it matters: a moon in steps of half a cell has a jagged rim, and
/// roofs at whole cells give the skyline a third of the heights it can
/// have.
#[test]
fn the_moon_and_the_roofs_are_drawn_at_a_quarter_of_a_cell() {
    for seed in [7, 8, 9] {
        // Given:
        let mut scene = City::new(Rng::seed_from_u64(seed), PALETTE);

        // When:
        scene.tick(AREA);
        let rows = frame(&scene, AREA);

        // Then:
        let rim = |row: &Vec<Color>| {
            let left = row.iter().position(|colour| *colour == PALETTE.text)?;
            let right = row.iter().rposition(|colour| *colour == PALETTE.text)?;
            Some((left, right))
        };
        let round = (0..rows.len()).step_by(2).any(|y| {
            let (upper, lower) = (rim(&rows[y]), rim(&rows[y + 1]));
            upper.is_some() && lower.is_some() && upper != lower
        });
        assert!(round, "seed {seed}: the moon steps by half cells");
        let tops = tower_tops(&rows);
        assert!(
            tops.iter()
                .flatten()
                .any(|top| top % CELL_HEIGHT == CELL_HEIGHT / 2),
            "seed {seed}: every roof is at the top of a cell"
        );
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
        Settings::DEFAULT,
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

/// Whether a row of the city lies outside the rows a train passes on, two
/// cells, and the line it runs on, one cell.
fn off_the_line(y: usize, rail: usize) -> bool {
    y + 2 * CELL_HEIGHT < rail || y >= rail + CELL_HEIGHT
}

/// Each pixel of a tower or a window above the water and off the train's
/// rows, with its colour.
fn skyline(rows: &[Vec<Color>]) -> Vec<(usize, usize, Color)> {
    let (rail, shore) = (rail(rows), ground(rows) + 1);
    rows[..shore]
        .iter()
        .enumerate()
        .filter(|(y, _)| off_the_line(*y, rail))
        .flat_map(|(y, row)| {
            row.iter()
                .enumerate()
                .filter(|(_, colour)| is_tower(**colour) || is_window(**colour))
                .map(move |(x, colour)| (x, y, *colour))
        })
        .collect()
}

fn places(city: &[(usize, usize, Color)]) -> Vec<(usize, usize)> {
    city.iter().map(|(x, y, _)| (*x, *y)).collect()
}

/// A far window that turns never changes the shape of what stands in front
/// of it.
/// Given: the scene at a hundred by thirty, from seeds 4 and 69, whose
/// skylines put the lamp of a near antenna beside a far window.
/// When: it ticks over a few seconds.
/// Then: the places of the towers and windows stay the same from a frame
/// to the next.
///
/// Why it matters: a cell draws two colours, so a lamp in the cell of a
/// far window and its wall takes the wall's colour while the window is
/// lit, and the wall seems to grow a pixel each time the window turns.
#[test]
fn a_far_window_never_shares_a_cell_with_a_lamp() {
    for seed in [4, 69] {
        // Given:
        let mut scene = City::new(Rng::seed_from_u64(seed), PALETTE);
        let mut before = None;

        for tick in 0..FEW_SECONDS {
            // When:
            scene.tick(AREA);
            let now = places(&skyline(&frame(&scene, AREA)));

            // Then:
            if let Some(before) = &before {
                assert_eq!(*before, now, "seed {seed} tick {tick}: the skyline moved");
            }
            before = Some(now);
        }
    }
}

/// Windows light and go dark one at a time as the night passes.
/// Given: the scene at a hundred by thirty.
/// When: it ticks over a loop of the night and a half.
/// Then: off the line of the train, the towers and the places of the
/// windows stay the same, the pixels that change from a frame to the next
/// lie in one cell at most, as one window does, and the count of lit
/// window pixels both rises and falls by over a quarter of its most. Of the windows of the near towers, never more
/// than three in ten are lit, and at some point fewer than one in twelve.
///
/// Why it matters: a city whose windows never change reads as a picture,
/// one whose windows flicker all at once reads as a fault, and one lit
/// window in two reads as a wall of dots.
#[test]
fn windows_light_and_go_dark_one_by_one_as_the_night_passes() {
    // Given:
    let mut scene = City::new(Rng::seed_from_u64(7), PALETTE);
    let mut counts = Vec::new();
    let mut shares: Vec<f64> = Vec::new();
    let mut before: Option<Vec<(usize, usize, Color)>> = None;

    for tick in 0..NIGHT_AND_A_HALF {
        // When:
        scene.tick(AREA);
        let rows = frame(&scene, AREA);

        // Then:
        let (rail, shore) = (rail(&rows), ground(&rows) + 1);
        let city = skyline(&rows);
        counts.push(
            city.iter()
                .filter(|(_, _, colour)| is_window(*colour))
                .count(),
        );
        let (mut lit, mut dark) = (0, 0);
        for (y, row) in rows.iter().enumerate().take(shore) {
            for (x, colour) in row.iter().enumerate() {
                if off_the_line(y, rail) && in_near_wall(row, x) {
                    lit += usize::from(is_window(*colour));
                    dark += usize::from(*colour == PALETTE.surface);
                }
            }
        }
        shares.push(lit as f64 / (lit + dark) as f64);
        if let Some(before) = &before {
            assert_eq!(
                places(before),
                places(&city),
                "tick {tick}: the skyline moved"
            );
            let mut changes: Vec<(usize, usize)> = before
                .iter()
                .zip(&city)
                .filter(|(a, b)| a != b)
                .map(|((x, y, _), _)| (x / CELL_WIDTH, y / CELL_HEIGHT))
                .collect();
            changes.dedup();
            assert!(
                changes.len() <= 1,
                "tick {tick}: windows changed at once in cells {changes:?}"
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
    let (least, highest) = shares
        .iter()
        .fold((1.0_f64, 0.0_f64), |(least, highest), share| {
            (least.min(*share), highest.max(*share))
        });
    assert!(
        highest <= 0.3 && least < 1.0 / 12.0,
        "the near towers lit from {least} to {highest} of their windows"
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
/// Then: each train, a run of two or more pixels of its body on the row
/// above the line, comes in at one side of the pane, and leaves at the
/// other side. Its front end moves toward that side in every frame until
/// it first gets there. Either end at an edge can step back for a frame,
/// when the gap between two carriages is at that edge. No run of four or
/// more shows on any row above the train's rows, and more than one train
/// crosses.
///
/// Why it matters: a train that jumps, turns back or stops halfway reads
/// as a fault, and one that leaves its line reads as a crash.
#[test]
fn a_train_crosses_on_its_line_now_and_then() {
    // Given:
    let mut scene = City::new(Rng::seed_from_u64(7), PALETTE);
    let across = usize::from(AREA.width) * 2;
    let runs = |row: &[Color], least: usize| -> Option<(usize, usize)> {
        let mut found: Option<(usize, usize)> = None;
        let mut start = None;
        for (x, colour) in row.iter().chain([&Color::Reset]).enumerate() {
            match (start, *colour == PALETTE.muted) {
                (None, true) => start = Some(x),
                (Some(from), false) => {
                    if x - from >= least {
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
        for (y, row) in rows.iter().enumerate().take(rail - 2 * CELL_HEIGHT) {
            // A near mast in one cell with a far tower's rim and the sky
            // can draw two pixels of the mast's colour.
            assert!(
                runs(row, 4).is_none(),
                "tick {tick}: a train body on row {y}"
            );
        }
        let now = runs(&rows[rail - 1], 2);
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
                    now_right > right || right == across - 1
                } else {
                    now_left < left || left == 0
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

/// The leftmost and the rightmost pixel of the train on its lowest row, or
/// none when no train shows.
fn train_ends(rows: &[Vec<Color>]) -> Option<(usize, usize)> {
    let row = &rows[rail(rows) - 1];
    let left = row.iter().position(|colour| *colour == PALETTE.muted)?;
    let right = row.iter().rposition(|colour| *colour == PALETTE.muted)?;
    Some((left, right))
}

/// Asserts that each column of the train's rows holds the train in all of
/// them, in its body or its windows, or no pixel of its body.
fn assert_train_upright(rows: &[Vec<Color>], context: &str) {
    let rail = rail(rows);
    let train = &rows[rail - 2 * CELL_HEIGHT..rail];
    for x in 0..rows[0].len() {
        let column: Vec<Color> = train.iter().map(|row| row[x]).collect();
        if column.contains(&PALETTE.muted) {
            assert!(
                column
                    .iter()
                    .all(|colour| *colour == PALETTE.muted || *colour == PALETTE.gold),
                "{context}: column {x} of the train holds {column:?}"
            );
        }
    }
}

/// The train glides a pixel at a time between ticks.
/// Given: the scene at a hundred by thirty, and a second scene from the
/// same seed that is never told its progress.
/// When: both tick until the first train has crossed, and before each tick
/// the first renders at no progress and at a quarter, a half and three
/// quarters of the way to the next.
/// Then: at no progress the first draws the frame of the second. Where an
/// end of the train is inside the pane at two ticks in a row, at each
/// progress it lies between its places at the two ticks, and strictly
/// between them from a half on, over a hundred times. In every frame each
/// column of the train's rows holds the train in all of them or none of its
/// body, so the ends of the carriages stand straight.
///
/// Why it matters: a train that moves a cell once a tick steps visibly at a
/// half or a quarter of the speed, and a notch at the end of a carriage
/// reads as a fault.
#[test]
fn the_train_glides_between_ticks() {
    // Given:
    let mut scene = City::new(Rng::seed_from_u64(7), PALETTE);
    let mut plain = City::new(Rng::seed_from_u64(7), PALETTE);
    let across = usize::from(AREA.width) * CELL_WIDTH;
    let inside = |end: usize| end > 0 && end + 1 < across;
    type Ends = Option<(usize, usize)>;
    // The train's ends at the last tick, and at each progress after it.
    let mut last: Option<(Ends, Vec<Ends>)> = None;
    let (mut seen, mut glided) = (false, 0);

    for tick in 0..TRAINS {
        // When:
        scene.tick(AREA);
        plain.tick(AREA);
        scene.set_progress(0.0);
        let rows = frame(&scene, AREA);
        let now = train_ends(&rows);

        // Then:
        assert_eq!(rows, frame(&plain, AREA), "tick {tick}");
        if let Some((Some(before), between)) = &last
            && let Some(after) = now
        {
            for (progress, ends) in BETWEEN.iter().zip(between) {
                let (left, right) = ends.expect("a train between ticks");
                let strictly = *progress >= 0.5;
                for (end, before, after) in [(left, before.0, after.0), (right, before.1, after.1)]
                {
                    if !inside(before) || !inside(after) {
                        continue;
                    }
                    glided += usize::from(strictly);
                    let (low, high) = (before.min(after), before.max(after));
                    assert!(
                        if strictly {
                            low < end && end < high
                        } else {
                            low <= end && end <= high
                        },
                        "tick {tick}: an end at {end} at progress {progress}, from {before} to {after}"
                    );
                }
            }
        }
        let between = BETWEEN
            .iter()
            .map(|progress| {
                scene.set_progress(*progress);
                let rows = frame(&scene, AREA);
                assert_train_upright(&rows, &format!("tick {tick} at progress {progress}"));
                train_ends(&rows)
            })
            .collect();
        seen |= now.is_some();
        if seen && now.is_none() {
            break;
        }
        last = Some((now, between));
    }
    assert!(glided > 100, "the ends of the train glided {glided} times");
}

/// The colour most pixels of `row` show.
fn most(row: &[Color]) -> Color {
    *row.iter()
        .max_by_key(|colour| row.iter().filter(|other| other == colour).count())
        .unwrap()
}

/// The sky lightens from the top of the pane toward the horizon.
/// Given: the scene at a hundred by thirty, from three seeds.
/// When: it ticks and renders.
/// Then: down the top fifth of the pane, above the towers, the colour most
/// of each row shows never darkens, and it takes three shades or more.
///
/// Why it matters: a sky of one flat colour reads as a backdrop and not as
/// a night sky that glows over the lights of a city.
#[test]
fn the_sky_lightens_toward_the_horizon() {
    for seed in [7, 8, 9] {
        // Given:
        let mut scene = City::new(Rng::seed_from_u64(seed), PALETTE);

        // When:
        scene.tick(AREA);
        let rows = pixels(&render(&scene, AREA, AREA), AREA);

        // Then:
        let sky: Vec<Color> = rows[..rows.len() / 5].iter().map(|row| most(row)).collect();
        assert!(
            sky.windows(2)
                .all(|pair| theme::luma(pair[1]) >= theme::luma(pair[0])),
            "seed {seed}: the sky from the top down is {sky:?}"
        );
        let mut shades = sky.clone();
        shades.dedup();
        assert!(shades.len() >= 3, "seed {seed}: the sky in {shades:?}");
    }
}

/// The moon has a soft rim and a glow round it.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders.
/// Then: over two cells of pixels show a mix of the sky and the moon, part
/// of the way from one to the other.
///
/// Why it matters: a moon cut out in hard steps reads as a block of pixels
/// and not as a round light in the sky.
#[test]
fn the_moon_has_a_soft_rim_and_a_glow() {
    // Given:
    let mut scene = City::new(Rng::seed_from_u64(7), PALETTE);

    // When:
    scene.tick(AREA);
    let rows = pixels(&render(&scene, AREA, AREA), AREA);

    // Then:
    let soft: usize = rows
        .iter()
        .enumerate()
        .map(|(y, row)| {
            let moon = moonlight(&scene, y);
            let part = &moon[..COVERAGE_STEPS - 1];
            row.iter().filter(|colour| part.contains(colour)).count()
        })
        .sum();
    assert!(
        soft > 2 * CELL_WIDTH * CELL_HEIGHT,
        "{soft} pixels mix the sky and the moon"
    );
}

/// The reflection shimmers in streaks one pixel row tall.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders over a few seconds.
/// Then: no lit pixel in the water has a lit pixel straight above it.
///
/// Why it matters: streaks two rows tall stack into blocks, and the water
/// reads as a second grid of windows and not as light on ripples.
#[test]
fn the_reflection_streaks_are_one_pixel_row_tall() {
    // Given:
    let mut scene = City::new(Rng::seed_from_u64(7), PALETTE);

    for tick in 0..FEW_SECONDS {
        // When:
        scene.tick(AREA);
        let rows = frame(&scene, AREA);

        // Then:
        let water = ground(&rows) + 1 + CELL_HEIGHT;
        for y in water + 1..rows.len() {
            for (x, (below, above)) in rows[y].iter().zip(&rows[y - 1]).enumerate() {
                assert!(
                    !(is_window(*below) && is_window(*above)),
                    "tick {tick}: a streak two rows tall at {:?}",
                    (x, y)
                );
            }
        }
    }
}

/// Each near tower is lit from its right, the side of the moon.
/// Given: the scene at a hundred by thirty, from three seeds.
/// When: it ticks and renders.
/// Then: on the ground row each near tower, a run of three or more pixels
/// that read as its wall from its left side to its rim, is darker at its
/// left end than at its right.
///
/// Why it matters: a wall of one flat colour reads as a cut-out and not as
/// a tower that stands in the light of the moon.
#[test]
fn each_near_tower_is_lit_from_the_side_of_the_moon() {
    for seed in [7, 8, 9] {
        // Given:
        let mut scene = City::new(Rng::seed_from_u64(seed), PALETTE);

        // When:
        scene.tick(AREA);
        let raw = pixels(&render(&scene, AREA, AREA), AREA);
        let rows = frame(&scene, AREA);

        // Then:
        let ground = ground(&rows);
        let row = &rows[ground];
        let mut x = 0;
        let mut towers = 0;
        while x < row.len() {
            if row[x] != PALETTE.overlay {
                x += 1;
                continue;
            }
            let start = x;
            while x < row.len() && row[x] == PALETTE.overlay {
                x += 1;
            }
            // A pillar of the raised line can stand in front of a wall and
            // cut it, so only a run from a wall's left side to its rim
            // holds the whole of its light.
            let whole = (start == 0 || row[start - 1] != PALETTE.muted)
                && x < row.len()
                && row[x] == PALETTE.pine;
            if x - start < 3 || !whole {
                continue;
            }
            towers += 1;
            let (left, right) = (raw[ground][start], raw[ground][x - 1]);
            assert!(
                theme::luma(left) < theme::luma(right),
                "seed {seed}: the near tower at {start}..{x} runs from {left:?} to {right:?}"
            );
        }
        assert!(towers >= 3, "seed {seed}: {towers} near towers");
    }
}

/// A lit window warms the wall beside it.
/// Given: the scene at a hundred by thirty, from three seeds.
/// When: it ticks and renders.
/// Then: for most lit windows of the near towers, the wall on their left is
/// brighter than the wall on the left of a dark window in the same column.
///
/// Why it matters: a lit window with no light round it reads as a dot
/// painted on the wall and not as a room with its lamp on.
#[test]
fn a_lit_window_warms_the_wall_beside_it() {
    for seed in [7, 8, 9] {
        // Given:
        let mut scene = City::new(Rng::seed_from_u64(seed), PALETTE);

        // When:
        scene.tick(AREA);
        let raw = pixels(&render(&scene, AREA, AREA), AREA);
        let rows = frame(&scene, AREA);

        // Then:
        let ground = ground(&rows);
        let (mut pairs, mut warm) = (0, 0);
        for y in 0..ground {
            for x in 1..rows[y].len() - 1 {
                if !is_window(rows[y][x]) || !in_near_wall(&rows[y], x) {
                    continue;
                }
                let dark = (0..ground).find(|other| {
                    rows[*other][x] == PALETTE.surface && in_near_wall(&rows[*other], x)
                });
                let Some(dark) = dark else {
                    continue;
                };
                pairs += 1;
                warm += usize::from(theme::luma(raw[y][x - 1]) > theme::luma(raw[dark][x - 1]));
            }
        }
        assert!(pairs > 0, "seed {seed}: no lit window beside a dark one");
        assert!(
            warm * 2 > pairs,
            "seed {seed}: {warm} of {pairs} lit windows warm their wall"
        );
    }
}

/// Each colour within a channel step of `colour`.
fn around(colour: Color) -> Vec<(u8, u8, u8)> {
    let Color::Rgb(r, g, b) = colour else {
        return Vec::new();
    };
    (0..27)
        .map(|at| {
            let step = |channel: u8, by: u8| channel.saturating_add(by).saturating_sub(1);
            (step(r, at % 3), step(g, at / 3 % 3), step(b, at / 9))
        })
        .collect()
}

/// Every colour within a channel step of a mix strictly between `base` and
/// one of `colours`, sampled finely along each mix, less the colours within
/// a step of the base or of one of `colours` themselves.
fn base_mixes(base: Color, colours: &[Color]) -> HashSet<(u8, u8, u8)> {
    let mut near: HashSet<(u8, u8, u8)> = colours
        .iter()
        .flat_map(|colour| {
            (1..200).flat_map(move |step| around(octant::mix(base, *colour, step as f32 / 200.0)))
        })
        .collect();
    for end in colours.iter().chain([&base]) {
        for each in around(*end) {
            near.remove(&each);
        }
    }
    near
}

/// Over the terminal's background the city mixes no colour with the base.
/// Given: the scene in flat Rose Pine, whose base and surface are the
/// terminal's own background, from three seeds.
/// When: it ticks and renders over a few seconds.
/// Then: no pixel lies on a mix from the base toward overlay, pine, gold,
/// rose, love or text, the colours the city draws.
///
/// Why it matters: a mix with a see-through base is an opaque colour, and
/// it shows as a dark smear over the user's own background.
#[test]
fn over_the_terminal_background_nothing_mixes_with_the_base() {
    let palette = &theme::FLAT_THEMES[0];
    let drawn = [
        palette.overlay,
        palette.pine,
        palette.gold,
        palette.rose,
        palette.love,
        palette.text,
    ];
    let mixes = base_mixes(palette.base, &drawn);
    for seed in [7, 8, 9] {
        // Given:
        let mut scene = City::new(Rng::seed_from_u64(seed), palette);
        for tick in 0..FEW_SECONDS {
            // When:
            scene.tick(AREA);
            let rows = pixels(&render(&scene, AREA, AREA), AREA);

            // Then:
            for (y, row) in rows.iter().enumerate() {
                for (x, colour) in row.iter().enumerate() {
                    if let Color::Rgb(r, g, b) = *colour {
                        assert!(
                            !mixes.contains(&(r, g, b)),
                            "seed {seed}, tick {tick}: {colour:?} at {:?} mixes with the base",
                            (x, y)
                        );
                    }
                }
            }
        }
    }
}
