#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use hearth::config::Background;
use hearth::corpus::Corpus;
use hearth::scene::{Rng, Scene, SceneKind};
use hearth::scenes::drive::Drive;
use hearth::scenes::hourglass::Lengths;
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
/// The quadrant glyphs, in the order of the quarters they set: upper left,
/// upper right, lower left and lower right from the lowest bit.
const QUADRANTS: [char; 16] = [
    ' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█',
];

fn render(scene: &Drive, area: Rect, canvas: Rect) -> Buffer {
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

/// The left and right ends of the road in each pixel row that has road, from
/// the top: the first and last pixel that is not grass or sky.
fn road(rows: &[Vec<Color>]) -> Vec<(usize, usize, usize)> {
    let off_road = |colour: &Color| *colour == PALETTE.pine || *colour == PALETTE.base;
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
/// and on average an end moves down. Where two ends are as near, the
/// nearer one is taken, which can pair an end with the one above it.
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
    let bottom = usize::from(AREA.height) * 2 - 1;
    scene.tick(AREA);
    let mut before = ends(&scene);
    let mut moves: Vec<isize> = Vec::new();

    // When:
    for _ in 0..400 {
        scene.tick(AREA);
        let after = ends(&scene);
        for end in before.iter().filter(|end| bottom - **end > 3) {
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
        down * 4 >= moves.len() * 3 && mean > 0.3,
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
/// Then: the name finds a scene, and its first frame shows the road and
/// the grass.
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
    assert!(rows.iter().flatten().any(|colour| *colour == PALETTE.pine));
}
