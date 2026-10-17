#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use hearth::config::Background;
use hearth::corpus::Corpus;
use hearth::scene::{Rng, Scene, SceneKind};
use hearth::scenes::hourglass::Lengths;
use hearth::scenes::study::{LEAST, PICTURE, Study, WINDOW};
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);
const PALETTE: &Palette = &theme::ROSE_PINE;

fn render(scene: &Study, area: Rect, canvas: Rect) -> Buffer {
    let mut buffer = Buffer::empty(canvas);
    scene.render(area, &mut buffer);
    buffer
}

/// The colour of each pixel of the area, one across and two down to a
/// cell, row by row.
fn pixels(buffer: &Buffer, area: Rect) -> Vec<Vec<Color>> {
    let mut rows = vec![vec![Color::Reset; usize::from(area.width)]; usize::from(area.height) * 2];
    for position in area.positions() {
        let cell = &buffer[position];
        let (upper, lower) = match cell.symbol() {
            "▀" => (cell.fg, cell.bg),
            "▄" => (cell.bg, cell.fg),
            "█" => (cell.fg, cell.fg),
            _ => (cell.bg, cell.bg),
        };
        let x = usize::from(position.x - area.x);
        let y = usize::from(position.y - area.y) * 2;
        rows[y][x] = upper;
        rows[y + 1][x] = lower;
    }
    rows
}

fn view(scene: &Study, width: u16, height: u16) -> Vec<Vec<Color>> {
    let area = Rect::new(0, 0, width, height);
    pixels(&render(scene, area, area), area)
}

/// Where a view of `size` pixels starts on a pane of `cells`, in pixels:
/// half the room left over, which is less than none when the view is cut.
fn offset(cells: (u16, u16), size: (usize, usize)) -> (isize, isize) {
    (
        (cells.0 as isize - size.0 as isize) / 2,
        (cells.1 as isize * 2 - size.1 as isize) / 2,
    )
}

/// Asserts that `small` shows the pixels of `large` from `shift` on.
fn shows_from(small: &[Vec<Color>], large: &[Vec<Color>], shift: (isize, isize)) {
    for (y, row) in small.iter().enumerate() {
        for (x, colour) in row.iter().enumerate() {
            let (from_x, from_y) = (x as isize + shift.0, y as isize + shift.1);
            assert_eq!(
                *colour,
                large[from_y as usize][from_x as usize],
                "pixel {:?} against {:?}",
                (x, y),
                (from_x, from_y)
            );
        }
    }
}

fn scene() -> Study {
    let mut scene = Study::new(Rng::seed_from_u64(7), PALETTE);
    scene.tick(AREA);
    scene
}

/// The room is one picture that stays the same.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders over twenty seconds.
/// Then: every frame is the same as the first.
///
/// Why it matters: a still picture that shifts or flickers by itself reads
/// as a fault, not as calm.
#[test]
fn the_room_stands_still_from_tick_to_tick() {
    // Given:
    let mut scene = scene();
    let first = render(&scene, AREA, AREA);

    for tick in 0..330 {
        // When:
        scene.tick(AREA);

        // Then:
        assert_eq!(render(&scene, AREA, AREA), first, "tick {tick}");
    }
}

/// The picture is centred in a pane larger than it.
/// Given: the scene on a pane of a hundred by thirty, and on one ten cells
/// wider and three rows taller.
/// When: both render.
/// Then: the larger pane shows the same pixels five cells further right
/// and three pixels further down, so the picture keeps the middle of each.
/// The picture holds at least eight colours of the theme.
///
/// Why it matters: a picture stuck at a corner of a large terminal reads
/// as a fault.
#[test]
fn the_picture_is_centred_in_a_larger_pane() {
    // Given:
    let scene = scene();
    assert!(AREA.width as usize >= PICTURE.0 && AREA.height as usize * 2 >= PICTURE.1);

    // When:
    let (smaller, larger) = (view(&scene, 100, 30), view(&scene, 110, 33));

    // Then:
    shows_from(&smaller, &larger, (5, 3));
    let (left, top) = offset((100, 30), PICTURE);
    let mut colours: Vec<Color> = Vec::new();
    for row in &smaller[top as usize..top as usize + PICTURE.1] {
        for colour in &row[left as usize..left as usize + PICTURE.0] {
            if !colours.contains(colour) {
                colours.push(*colour);
            }
        }
    }
    assert!(colours.len() >= 8, "{} colours: {colours:?}", colours.len());
}

/// A pane smaller than the picture shows its middle.
/// Given: the scene on a pane at the least size or more but narrower and
/// shorter than the picture, and on a hundred by thirty.
/// When: both render.
/// Then: the smaller pane shows the middle of what the larger one shows.
///
/// Why it matters: a picture cut from one corner loses the desk or the
/// window, and its middle holds both.
#[test]
fn a_smaller_pane_shows_the_middle_of_the_picture() {
    // Given:
    let scene = scene();
    let small = (56, 18);
    assert!(small.0 >= LEAST.0 && small.1 >= LEAST.1);
    assert!((small.0 as usize) < PICTURE.0 && (small.1 as usize) * 2 < PICTURE.1);

    // When:
    let (smaller, larger) = (view(&scene, small.0, small.1), view(&scene, 100, 30));

    // Then:
    let (large_left, large_top) = offset((100, 30), PICTURE);
    let (small_left, small_top) = offset(small, PICTURE);
    shows_from(
        &smaller,
        &larger,
        (large_left - small_left, large_top - small_top),
    );
}

/// Below the least size only the window shows.
/// Given: the scene on a pane under the least size, and on a hundred by
/// thirty.
/// When: both render.
/// Then: the small pane shows the window part of the picture, the window,
/// the cat and the sill, centred on it, and not the middle of the room.
///
/// Why it matters: a room cut to a small pane is a strip of desk that
/// reads as nothing, and the rainy window still reads as the scene.
#[test]
fn below_the_least_size_only_the_window_shows() {
    // Given:
    let scene = scene();
    let small = (30, 11);
    assert!(small.0 < LEAST.0 || small.1 < LEAST.1);

    // When:
    let (smaller, larger) = (view(&scene, small.0, small.1), view(&scene, 100, 30));

    // Then:
    let (large_left, large_top) = offset((100, 30), PICTURE);
    let (small_left, small_top) = offset(small, WINDOW);
    shows_from(
        &smaller,
        &larger,
        (large_left - small_left, large_top - small_top),
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
    let mut scene = scene();
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

/// Given: the scene name `study`.
/// When: it is looked up and built, then ticked and rendered.
/// Then: the name finds a scene, and its first frame shows the night sky
/// in the window and the cat.
///
/// Why it matters: a scene that is built but not listed cannot be chosen
/// from the command line, the picker or Tab.
#[test]
fn study_is_a_scene_by_name() {
    // When:
    let kind = SceneKind::from_name("study").expect("study is listed");
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
    assert!(rows.iter().flatten().any(|colour| *colour == PALETTE.pine));
    assert!(rows.iter().flatten().any(|colour| *colour == PALETTE.text));
}
