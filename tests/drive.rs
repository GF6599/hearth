#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use hearth::config::Background;
use hearth::corpus::Corpus;
use hearth::octant::{self, CELL_HEIGHT, CELL_WIDTH};
use hearth::scene::{Rng, Scene, SceneKind, Settings, TICK_RATE};
use hearth::scenes::drive::Drive;
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);
const PALETTE: &Palette = &theme::ROSE_PINE;
/// Ticks enough for the track to run straights and curves both ways.
const RUN: u32 = 3000;

fn render(scene: &Drive, area: Rect, canvas: Rect) -> Buffer {
    let mut buffer = Buffer::empty(canvas);
    scene.render(area, &mut buffer);
    buffer
}

/// The colour of each pixel of the area, two across and four down to a
/// cell, row by row.
fn pixels(buffer: &Buffer, area: Rect) -> Vec<Vec<Color>> {
    let mut rows = vec![
        vec![Color::Reset; usize::from(area.width) * CELL_WIDTH];
        usize::from(area.height) * CELL_HEIGHT
    ];
    for position in area.positions() {
        let cell = &buffer[position];
        let bits = (0..=u8::MAX)
            .find(|bits| cell.symbol().chars().eq([octant::glyph(*bits)]))
            .unwrap_or(0);
        for pixel in 0..CELL_WIDTH * CELL_HEIGHT {
            let x = usize::from(position.x - area.x) * CELL_WIDTH + pixel % CELL_WIDTH;
            let y = usize::from(position.y - area.y) * CELL_HEIGHT + pixel / CELL_WIDTH;
            rows[y][x] = if bits & 1 << pixel != 0 {
                cell.fg
            } else {
                cell.bg
            };
        }
    }
    rows
}

/// The left and right ends of the road in each pixel row that has road, from
/// the top: the first and last pixel that is not land, sky or a post.
fn road(rows: &[Vec<Color>]) -> Vec<(usize, usize, usize)> {
    let off_road = |colour: &Color| [PALETTE.base, PALETTE.muted].contains(colour);
    rows.iter()
        .enumerate()
        .filter(|(_, row)| row.contains(&PALETTE.overlay))
        .filter_map(|(y, row)| {
            let left = row.iter().position(|colour| !off_road(colour))?;
            let right = row.iter().rposition(|colour| !off_road(colour))?;
            Some((y, left, right))
        })
        .collect()
}

/// The edges of the road step every quarter of a cell.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders over straights and curves.
/// Then: in most sampled frames, some pixel row of the road has its left
/// or its right edge in another column than the row above it, inside the
/// upper or the lower half of a cell.
///
/// Why it matters: road edges in steps of half a cell draw the bends and
/// the slopes in coarse stairs.
#[test]
fn the_road_edges_step_every_quarter_of_a_cell() {
    // Given:
    let mut scene = Drive::new(Rng::seed_from_u64(7), PALETTE);
    let (mut sampled, mut fine) = (0, 0);

    for tick in 0..RUN {
        // When:
        scene.tick(AREA);
        if tick % 25 != 0 {
            continue;
        }
        let rows = road(&pixels(&render(&scene, AREA, AREA), AREA));

        // Then:
        sampled += 1;
        fine += usize::from(rows.windows(2).any(|pair| {
            let ((upper, upper_left, upper_right), (lower, lower_left, lower_right)) =
                (pair[0], pair[1]);
            lower == upper + 1
                && upper % 2 == 0
                && (upper_left != lower_left || upper_right != lower_right)
        }));
    }
    assert!(
        fine * 2 > sampled,
        "the road edges stepped inside a half cell in {fine} of {sampled} frames"
    );
}

/// The road narrows from the bottom of the pane toward the horizon.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders over straights and curves.
/// Then: in every frame the road's width does not grow from one row to the
/// next going up, it is over half the pane wide at the bottom, and under a
/// fifth at its top.
///
/// Why it matters: a road that widens with distance breaks the view down
/// it, and the motion reads as a fault.
#[test]
fn the_road_narrows_toward_the_horizon() {
    // Given:
    let mut scene = Drive::new(Rng::seed_from_u64(7), PALETTE);
    let width = usize::from(AREA.width) * 2;

    for tick in 0..RUN {
        // When:
        scene.tick(AREA);
        if tick % 25 != 0 {
            continue;
        }
        let rows = road(&pixels(&render(&scene, AREA, AREA), AREA));

        // Then:
        let (_, bottom_left, bottom_right) = *rows.last().expect("road");
        let (_, top_left, top_right) = rows[0];
        assert!(
            bottom_right - bottom_left > width / 2,
            "tick {tick}: {rows:?}"
        );
        assert!(top_right - top_left < width / 5, "tick {tick}: {rows:?}");
        for pair in rows.windows(2) {
            let (above, below) = (pair[0].2 - pair[0].1, pair[1].2 - pair[1].1);
            assert!(above <= below + 2, "tick {tick}: {pair:?}");
        }
    }
}

/// The road runs straight and bends both ways as the track curves.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders over the track.
/// Then: some frames show the road straight ahead, with its centre in the
/// same column from the bottom to the top, some bend it to the left and
/// some to the right, and in a bend the centre moves one way only from the
/// bottom up. At the bottom the road is always in the middle of the pane.
///
/// Why it matters: a road that never curves reads as a treadmill, and one
/// that wiggles both ways in one bend reads as a fault.
#[test]
fn the_road_bends_with_the_track_both_ways() {
    // Given:
    let mut scene = Drive::new(Rng::seed_from_u64(7), PALETTE);
    let middle = f32::from(AREA.width);
    let (mut straight, mut left, mut right) = (0, 0, 0);

    for tick in 0..RUN * 2 {
        // When:
        scene.tick(AREA);
        if tick % 10 != 0 {
            continue;
        }
        let rows = road(&pixels(&render(&scene, AREA, AREA), AREA));

        // Then:
        let centres: Vec<f32> = rows
            .iter()
            .rev()
            .map(|(_, left, right)| (*left + *right) as f32 / 2.0)
            .collect();
        assert!(
            (centres[0] - middle).abs() <= 2.0,
            "tick {tick}: {centres:?}"
        );
        let bend = centres[centres.len() - 1] - centres[0];
        if bend.abs() <= 2.0 {
            straight += 1;
            continue;
        }
        for pair in centres.windows(2) {
            assert!(
                (pair[1] - pair[0]) * bend.signum() >= -2.0,
                "tick {tick}: the bend turns back {centres:?}"
            );
        }
        if bend < -10.0 {
            left += 1;
        } else if bend > 10.0 {
            right += 1;
        }
    }
    assert!(
        straight > 10 && left > 10 && right > 10,
        "{straight} straight, {left} left, {right} right"
    );
}

/// The stripes move toward the car.
/// Given: the scene at a hundred by thirty.
/// When: it ticks, and the rows where the colour at the road's centre
/// changes, the ends of the stripes, are read in the lower half of the road
/// in each frame.
/// Then: three ends in four are at the same row or lower in the next frame,
/// and on average an end moves down by over a sixth of a cell. Where two
/// ends are as near, the nearer one is taken, which can pair an end with
/// the one above it.
///
/// Why it matters: stripes that run up the screen show the car reversing.
#[test]
fn the_stripes_move_toward_the_car() {
    // Given:
    let mut scene = Drive::new(Rng::seed_from_u64(7), PALETTE);
    let ends = |scene: &Drive| -> Vec<usize> {
        let rows = pixels(&render(scene, AREA, AREA), AREA);
        let road = road(&rows);
        let lower = &road[road.len() / 2..];
        let centre: Vec<(usize, Color)> = lower
            .iter()
            .map(|(y, left, right)| (*y, rows[*y][(left + right) / 2]))
            .collect();
        centre
            .windows(2)
            .filter(|pair| pair[0].1 != pair[1].1)
            .map(|pair| pair[1].0)
            .collect()
    };
    let bottom = usize::from(AREA.height) * CELL_HEIGHT - 1;
    scene.tick(AREA);
    let mut before = ends(&scene);
    let mut moves: Vec<isize> = Vec::new();

    // When:
    for _ in 0..400 {
        scene.tick(AREA);
        let after = ends(&scene);
        for end in before
            .iter()
            .filter(|end| bottom - **end >= 2 * CELL_HEIGHT)
        {
            if let Some(next) = after.iter().min_by_key(|next| next.abs_diff(*end)) {
                moves.push(*next as isize - *end as isize);
            }
        }
        before = after;
    }

    // Then:
    let down = moves.iter().filter(|step| **step >= 0).count();
    let mean = moves.iter().sum::<isize>() as f32 / moves.len() as f32;
    assert!(
        down * 4 >= moves.len() * 3 && mean > 0.15 * CELL_HEIGHT as f32,
        "{down} of {} ends at the same row or lower, {mean} rows on average",
        moves.len()
    );
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
    let mut scene = Drive::new(Rng::seed_from_u64(7), PALETTE);
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

/// Given: the scene name `drive`.
/// When: it is looked up and built, then ticked and rendered.
/// Then: the name finds a scene, and its first frame shows the road with
/// land beside it on the bottom row.
///
/// Why it matters: a scene that is built but not listed cannot be chosen
/// from the command line, the picker or Tab.
#[test]
fn drive_is_a_scene_by_name() {
    // When:
    let kind = SceneKind::from_name("drive").expect("drive is listed");
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
    let bottom = rows.last().unwrap();
    assert_eq!(bottom[0], PALETTE.base, "no land left of the road");
}

/// The row of the topmost road pixel, the far end of the road in view.
fn road_top(rows: &[Vec<Color>]) -> usize {
    road(rows).first().expect("road").0
}

/// The moon and stars stay above the road, and the sky slides against a
/// curve.
/// Given: the scene at a hundred by thirty.
/// When: it ticks over straights and curves both ways.
/// Then: every star, and every pixel of the moon, lies above the road's top
/// row, the moon shows in some frames with over five cells of pixels, and
/// the moon moves left in a right bend and right in a left bend.
///
/// Why it matters: a moon on the land reads as a fault, and a sky that
/// stays put in a curve takes away the turn.
#[test]
fn the_moon_and_stars_stay_above_the_road_and_slide_against_a_curve() {
    // Given:
    let mut scene = Drive::new(Rng::seed_from_u64(7), PALETTE);
    let width = usize::from(AREA.width) * 2;
    let (mut shown, mut in_right, mut in_left) = (0, 0.0_f32, 0.0_f32);
    let mut last: Option<f32> = None;

    for tick in 0..RUN * 2 {
        // When:
        scene.tick(AREA);
        if tick % 3 != 0 {
            continue;
        }
        let rows = pixels(&render(&scene, AREA, AREA), AREA);
        let top = road_top(&rows);

        // Then:
        let mut moon = Vec::new();
        for (y, row) in rows.iter().enumerate() {
            if row.contains(&PALETTE.overlay) {
                continue;
            }
            for (x, colour) in row.iter().enumerate() {
                if *colour == PALETTE.text || *colour == PALETTE.subtle {
                    assert!(
                        y < top,
                        "tick {tick}: sky at {:?} below the road top {top}",
                        (x, y)
                    );
                }
                if *colour == PALETTE.text {
                    moon.push(x as f32);
                }
            }
        }
        let centre = (moon.len() > 5 * CELL_WIDTH * CELL_HEIGHT)
            .then(|| moon.iter().sum::<f32>() / moon.len() as f32);
        let bend = bend(&rows);
        if let (Some(now), Some(before)) = (centre, last) {
            shown += 1;
            let step = now - before;
            if step.abs() < width as f32 / 4.0 {
                if bend > 10.0 {
                    in_right += step;
                } else if bend < -10.0 {
                    in_left += step;
                }
            }
        }
        last = centre;
    }
    assert!(shown > 100, "the moon showed in {shown} frames");
    assert!(
        in_right < -5.0 && in_left > 5.0,
        "the moon moved {in_right} in right bends and {in_left} in left bends"
    );
}

/// The road rises and falls over hills.
/// Given: the scene at a hundred by thirty.
/// When: it ticks over the track.
/// Then: the road's top row moves over at least two cells, and the road
/// always reaches the bottom row.
///
/// Why it matters: a road that never leaves the flat reads as a picture
/// slid down the screen, and a gap under the road reads as a fault.
#[test]
fn the_road_rises_and_falls_over_hills() {
    // Given:
    let mut scene = Drive::new(Rng::seed_from_u64(7), PALETTE);
    let bottom = usize::from(AREA.height) * CELL_HEIGHT - 1;
    let (mut highest, mut lowest) = (usize::MAX, 0);

    for tick in 0..RUN {
        // When:
        scene.tick(AREA);
        let rows = pixels(&render(&scene, AREA, AREA), AREA);

        // Then:
        let top = road_top(&rows);
        highest = highest.min(top);
        lowest = lowest.max(top);
        assert_eq!(road(&rows).last().unwrap().0, bottom, "tick {tick}");
    }
    assert!(
        lowest - highest >= 2 * CELL_HEIGHT,
        "the road's top only moved from {highest} to {lowest}"
    );
}

/// The bend of the road in a frame: how far its centre moves from the
/// bottom row to the top, in pixels, positive to the right.
fn bend(rows: &[Vec<Color>]) -> f32 {
    let road = road(rows);
    let middle = |(_, left, right): (usize, usize, usize)| (left + right) as f32 / 2.0;
    middle(road[0]) - middle(*road.last().unwrap())
}

/// The car keeps to its lane at the bottom and leans into a curve.
/// Given: the scene at a hundred by thirty.
/// When: it ticks over straights and curves both ways.
/// Then: the car, in iris, lies in the bottom fifth of the pane, inside
/// the road and right of its middle. On a straight its top is over its
/// bottom, and on average its top leans right in right bends and left in
/// left bends.
///
/// Why it matters: a car off the road reads as a crash, and a car that
/// stays upright in a curve takes away the turn.
#[test]
fn the_car_keeps_its_lane_and_leans_into_a_curve() {
    // Given:
    let mut scene = Drive::new(Rng::seed_from_u64(7), PALETTE);
    let height = usize::from(AREA.height) * CELL_HEIGHT;
    let (mut right, mut left) = (Vec::new(), Vec::new());

    for tick in 0..RUN * 2 {
        // When:
        scene.tick(AREA);
        if tick % 5 != 0 {
            continue;
        }
        let rows = pixels(&render(&scene, AREA, AREA), AREA);

        // Then:
        let car: Vec<(usize, usize)> = rows
            .iter()
            .enumerate()
            .flat_map(|(y, row)| {
                row.iter()
                    .enumerate()
                    .filter(|(_, colour)| **colour == PALETTE.iris)
                    .map(move |(x, _)| (x, y))
            })
            .collect();
        assert!(!car.is_empty(), "tick {tick}: no car");
        let road = road(&rows);
        for &(x, y) in &car {
            assert!(
                y >= height * 4 / 5,
                "tick {tick}: car at {:?} above the bottom fifth",
                (x, y)
            );
            let (_, left_edge, right_edge) =
                road.iter().find(|(row, _, _)| *row == y).copied().unwrap();
            let middle = (left_edge + right_edge) as f32 / 2.0;
            assert!(
                x >= left_edge && x <= right_edge && x as f32 > middle,
                "tick {tick}: car at {:?} not in the right lane of {:?}",
                (x, y),
                (left_edge, right_edge)
            );
        }
        let centre_of = |y: usize| {
            let xs: Vec<usize> = car
                .iter()
                .filter(|(_, at)| *at == y)
                .map(|(x, _)| *x)
                .collect();
            xs.iter().sum::<usize>() as f32 / xs.len() as f32
        };
        let (top, bottom) = (
            car.iter().map(|(_, y)| *y).min().unwrap(),
            car.iter().map(|(_, y)| *y).max().unwrap(),
        );
        let lean = centre_of(top) - centre_of(bottom);
        let bend = bend(&rows);
        if bend.abs() <= 2.0 {
            assert!(
                lean.abs() <= 0.5,
                "tick {tick}: the car leans {lean} on a straight"
            );
        } else if bend > 10.0 {
            right.push(lean);
        } else if bend < -10.0 {
            left.push(lean);
        }
    }
    let mean = |leans: &[f32]| leans.iter().sum::<f32>() / leans.len() as f32;
    assert!(
        mean(&right) > 0.5 && mean(&left) < -0.5,
        "the car leans {} in right bends and {} in left bends",
        mean(&right),
        mean(&left)
    );
}

/// The posts beside the road grow as they come near and move outward.
/// Given: the scene at a hundred by thirty.
/// When: it ticks over the track.
/// Then: posts, upright runs of muted pixels, stand in every frame on both
/// sides of the road. A post's height is the shorter of the pair at its
/// row, because in a sharp bend a post can touch a farther one in its
/// column. On each side a post two cells or more lower on the screen is no
/// shorter by over half a cell and no nearer the road's middle than one
/// above it, and when four cells or more lie between the lowest and the
/// highest, the lowest is taller and further out. The lowest post on the
/// right moves in most sampled frames. Posts nearer than two cells are at
/// about one distance, and whole pixels can order them either way.
///
/// Why it matters: posts that do not grow and spread as they come near
/// stop reading as passing by, and the speed goes with them.
#[test]
fn the_posts_grow_as_they_come_near_and_move_outward() {
    // Given:
    let mut scene = Drive::new(Rng::seed_from_u64(7), PALETTE);
    let mut lowest: Vec<(usize, usize)> = Vec::new();

    for tick in 0..RUN {
        // When:
        scene.tick(AREA);
        if tick % 7 != 0 {
            continue;
        }
        let rows = pixels(&render(&scene, AREA, AREA), AREA);

        // Then:
        let mut posts: Vec<(usize, usize, usize)> = Vec::new();
        for x in 0..rows[0].len() {
            let mut run = 0;
            for (y, row) in rows.iter().enumerate() {
                if row[x] == PALETTE.muted {
                    run += 1;
                } else if run > 0 {
                    posts.push((x, y - 1, run));
                    run = 0;
                }
            }
            if run > 0 {
                posts.push((x, rows.len() - 1, run));
            }
        }
        let road = road(&rows);
        let middle_at = |y: usize| {
            road.iter()
                .find(|(row, _, _)| *row == y)
                .map(|(_, left, right)| (left + right) as f32 / 2.0)
        };
        // A post that touches a farther post in its column reads as one tall
        // run, so a post's height is the shorter of its pair's.
        let height_at = |bottom: usize| {
            posts
                .iter()
                .filter(|(_, other, _)| *other == bottom)
                .map(|(_, _, height)| *height)
                .min()
                .unwrap()
        };
        let mut sides: [Vec<(usize, usize, f32)>; 2] = [Vec::new(), Vec::new()];
        for &(x, bottom, _) in &posts {
            let height = height_at(bottom);
            let Some(middle) = middle_at(bottom) else {
                continue;
            };
            let out = x as f32 - middle;
            sides[usize::from(out > 0.0)].push((bottom, height, out.abs()));
        }
        for side in &mut sides {
            assert!(!side.is_empty(), "tick {tick}: no post on a side");
            side.sort_by_key(|(bottom, _, _)| *bottom);
            for pair in side.windows(2) {
                let ((upper, upper_height, upper_out), (lower, lower_height, lower_out)) =
                    (pair[0], pair[1]);
                if lower >= upper + 2 * CELL_HEIGHT {
                    assert!(
                        lower_height + CELL_HEIGHT / 2 >= upper_height
                            && lower_out + 1.0 >= upper_out,
                        "tick {tick}: a post at row {lower} is smaller or nearer the middle than one at row {upper}"
                    );
                }
            }
        }
        for side in &sides {
            let (high, low) = (side[0], side[side.len() - 1]);
            if low.0 >= high.0 + 4 * CELL_HEIGHT {
                assert!(
                    low.1 > high.1 && low.2 > high.2 + 4.0,
                    "tick {tick}: the post at row {} is no taller or no further out than the one at row {}",
                    low.0,
                    high.0
                );
            }
        }
        let low = sides[1]
            .last()
            .map(|(bottom, height, _)| (*bottom, *height))
            .unwrap();
        lowest.push(low);
    }
    let moves = lowest.windows(2).filter(|pair| pair[0] != pair[1]).count();
    assert!(
        moves * 2 > lowest.len(),
        "the lowest post moved in {moves} of {} frames",
        lowest.len()
    );
}

/// The top, bottom, left and right pixel of a car ahead.
type CarAhead = (usize, usize, usize, usize);

/// Each car ahead in a frame, the highest first. A car ahead is a group of
/// gold pixels that touch, diagonals included, because a gap in rows cannot
/// tell a car's window from the road between two cars a cell apart.
fn cars_ahead(rows: &[Vec<Color>]) -> Vec<CarAhead> {
    let mut left: Vec<(usize, usize)> = rows
        .iter()
        .enumerate()
        .flat_map(|(y, row)| {
            row.iter()
                .enumerate()
                .filter(|(_, colour)| **colour == PALETTE.gold)
                .map(move |(x, _)| (x, y))
        })
        .collect();
    let mut cars = Vec::new();
    while let Some(start) = left.pop() {
        let mut car = (start.1, start.1, start.0, start.0);
        let mut open = vec![start];
        while let Some((x, y)) = open.pop() {
            car = (car.0.min(y), car.1.max(y), car.2.min(x), car.3.max(x));
            let (touching, rest): (Vec<_>, Vec<_>) =
                left.into_iter().partition(|(other_x, other_y)| {
                    x.abs_diff(*other_x) <= 1 && y.abs_diff(*other_y) <= 1
                });
            open.extend(touching);
            left = rest;
        }
        cars.push(car);
    }
    cars.sort_unstable();
    cars
}

/// Slower cars ahead in the other lane come near and drop behind.
/// Given: the scene at a hundred by thirty.
/// When: it ticks over the track, and each car ahead, in gold, is followed
/// from frame to frame.
/// Then: every pixel of a car two cells wide or more lies on the road and
/// left of its middle, at one row of the car's lowest cell. A car covers
/// whole cells, so its lowest cell can reach rows past its own stretch of
/// road, and at a crest those rows are a nearer stretch with another
/// middle. A car one cell wide is far off, where many stretches share a
/// row, and the row shows only the nearest. While a car is all in view it
/// never narrows by more than a cell from one frame to the next. Several
/// cars come from the far road at a cell or two wide, more than five cells
/// above the bottom, and leave through the bottom row, wider than they
/// came.
///
/// Why it matters: a car ahead that drifts into the car's lane reads as a
/// crash, and one that shrinks or vanishes mid-road reads as a fault.
#[test]
fn slower_cars_ahead_come_near_and_drop_behind_in_the_other_lane() {
    // Given:
    let mut scene = Drive::new(Rng::seed_from_u64(7), PALETTE);
    let bottom = usize::from(AREA.height) * CELL_HEIGHT - 1;
    let width = |car: CarAhead| car.3 - car.2 + 1;
    // The first and the last sighting of each car followed so far.
    let mut followed: Vec<(CarAhead, CarAhead)> = Vec::new();
    let mut passed = 0;

    for tick in 0..RUN * 2 {
        // When:
        scene.tick(AREA);
        let rows = pixels(&render(&scene, AREA, AREA), AREA);
        let cars = cars_ahead(&rows);

        // Then:
        let road = road(&rows);
        for &(top, lowest, left, right) in cars.iter().filter(|car| width(**car) > 2) {
            let gold: Vec<usize> = rows[top..=lowest]
                .iter()
                .flat_map(|row| (left..=right).filter(|x| row[*x] == PALETTE.gold))
                .collect();
            let lanes: Vec<(usize, usize)> = road
                .iter()
                .filter(|(row, _, _)| *row / CELL_HEIGHT == lowest / CELL_HEIGHT)
                .map(|(_, left_edge, right_edge)| (*left_edge, *right_edge))
                .collect();
            assert!(
                lanes.iter().any(|(left_edge, right_edge)| {
                    let middle = (left_edge + right_edge) as f32 / 2.0;
                    gold.iter().all(|x| x >= left_edge && (*x as f32) < middle)
                }),
                "tick {tick}: car ahead at columns {:?} in rows {top} to {lowest} not in the left lane of {lanes:?}",
                (gold.iter().min(), gold.iter().max())
            );
        }
        // Each car takes the nearest one followed from the frame before,
        // closest pairs first, within three cells each way.
        let gap = |a: CarAhead, b: CarAhead| {
            a.0.abs_diff(b.0) + a.1.abs_diff(b.1) + a.2.abs_diff(b.2) + a.3.abs_diff(b.3)
        };
        let mut pairs: Vec<(usize, usize, usize)> = Vec::new();
        for (index, car) in cars.iter().enumerate() {
            for (before, (_, last)) in followed.iter().enumerate() {
                let (down, across) = (3 * CELL_HEIGHT, 3 * CELL_WIDTH);
                if car.0 <= last.1 + down
                    && last.0 <= car.1 + down
                    && car.2 <= last.3 + across
                    && last.2 <= car.3 + across
                {
                    pairs.push((gap(*car, *last), index, before));
                }
            }
        }
        pairs.sort_unstable();
        let mut taken: Vec<Option<usize>> = vec![None; cars.len()];
        let mut ended = vec![true; followed.len()];
        for (_, index, before) in pairs {
            if taken[index].is_none() && ended[before] {
                taken[index] = Some(before);
                ended[before] = false;
            }
        }
        let mut next = Vec::new();
        for (&car, before) in cars.iter().zip(taken) {
            match before {
                Some(before) => {
                    let (first, last) = followed[before];
                    if last.1 + 2 * CELL_HEIGHT < bottom && car.1 + 2 * CELL_HEIGHT < bottom {
                        assert!(
                            width(car) + 2 >= width(last),
                            "tick {tick}: a car ahead narrowed from {last:?} to {car:?}"
                        );
                    }
                    next.push((first, car));
                }
                None => next.push((car, car)),
            }
        }
        let gone = followed
            .iter()
            .zip(ended)
            .filter_map(|(track, ended)| ended.then_some(*track));
        for (first, last) in gone {
            if last.1 == bottom && first.1 + 5 * CELL_HEIGHT < bottom && width(first) <= 4 {
                assert!(
                    width(last) > width(first) + 8,
                    "tick {tick}: a car ahead left at {last:?} no wider than it came at {first:?}"
                );
                passed += 1;
            }
        }
        followed = next;
    }
    assert!(
        passed >= 3,
        "{passed} cars ahead came near and dropped behind"
    );
}

/// The land beside the road holds still while the car drives.
/// Given: the scene at a hundred by thirty.
/// When: it ticks over the track, and each pixel beside the road that is
/// in a colour the land could take in two frames in a row is compared.
/// Then: under one such pixel in a hundred changes its colour from a frame
/// to the next.
///
/// Why it matters: bands of grass that flash past across the whole width
/// make the drive hard to watch, and the edge stripes, the dashes and the
/// posts already give the speed.
#[test]
fn the_land_beside_the_road_holds_still() {
    // Given:
    let mut scene = Drive::new(Rng::seed_from_u64(7), PALETTE);
    let land = |colour: Color| [PALETTE.base, PALETTE.surface, PALETTE.pine].contains(&colour);
    scene.tick(AREA);
    let mut before = pixels(&render(&scene, AREA, AREA), AREA);
    let (mut changed, mut compared) = (0_usize, 0_usize);

    for _ in 0..RUN {
        // When:
        scene.tick(AREA);
        let after = pixels(&render(&scene, AREA, AREA), AREA);
        for &(y, left, right) in &road(&after) {
            for x in (0..left).chain(right + 1..after[y].len()) {
                let (was, now) = (before[y][x], after[y][x]);
                if land(was) && land(now) {
                    compared += 1;
                    changed += usize::from(was != now);
                }
            }
        }
        before = after;
    }

    // Then:
    assert!(
        changed * 100 < compared,
        "{changed} of {compared} land pixels changed colour"
    );
}

/// A car ahead comes into view soon after the start.
/// Given: the scene at a hundred by thirty, at twenty seeds.
/// When: it ticks for five seconds of the app's ticks.
/// Then: at every seed, a car ahead in gold shows in some frame.
///
/// Why it matters: a viewer who watches for a minute and sees no other car
/// misses the traffic of the design.
#[test]
fn a_car_ahead_shows_within_five_seconds() {
    let ticks = 5_000 / TICK_RATE.as_millis();
    for seed in 0..20 {
        // Given:
        let mut scene = Drive::new(Rng::seed_from_u64(seed), PALETTE);
        let mut seen = false;

        // When:
        for _ in 0..ticks {
            scene.tick(AREA);
            let rows = pixels(&render(&scene, AREA, AREA), AREA);
            seen |= !cars_ahead(&rows).is_empty();
        }

        // Then:
        assert!(seen, "seed {seed}: no car ahead in {ticks} ticks");
    }
}

/// A car ahead reads as a car for most of the time it is in view.
/// Given: the scene at a hundred by thirty.
/// When: it ticks over the track, and the widest car ahead is read in each
/// frame that shows one.
/// Then: in two frames in three or more, that car is wider than one cell.
///
/// Why it matters: a car that is one gold cell for most of its approach
/// reads as a star, not as a car.
#[test]
fn a_car_ahead_is_wider_than_a_cell_for_most_of_its_approach() {
    // Given:
    let mut scene = Drive::new(Rng::seed_from_u64(7), PALETTE);
    let (mut shown, mut wide) = (0, 0);

    for _ in 0..RUN * 2 {
        // When:
        scene.tick(AREA);
        let rows = pixels(&render(&scene, AREA, AREA), AREA);
        let widest = cars_ahead(&rows).iter().map(|car| car.3 - car.2 + 1).max();

        // Then:
        if let Some(width) = widest {
            shown += 1;
            wide += usize::from(width > 2);
        }
    }
    assert!(shown > 0, "no car ahead in view");
    assert!(
        wide * 3 >= shown * 2,
        "a car ahead was wider than a cell in {wide} of {shown} frames"
    );
}
