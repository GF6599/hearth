#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use hearth::config::Background;
use hearth::corpus::Corpus;
use hearth::scene::{Rng, Scene, SceneKind};
use hearth::scenes::hourglass::Lengths;
use hearth::scenes::study::{LAYERS, LEAST, PICTURE, Study, WINDOW};
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

/// A pane exactly the picture's size, so a pixel of the pane is the pixel
/// of the picture at the same place.
const FITTED: Rect = Rect::new(0, 0, PICTURE.0 as u16, PICTURE.1 as u16 / 2);
/// Ticks enough for every layer to move a few times, about two minutes.
const RUN: u32 = 2000;

fn layer_at(x: usize, y: usize) -> Option<&'static str> {
    LAYERS
        .iter()
        .find(|(_, (left, top, wide, tall))| {
            (*left..left + wide).contains(&x) && (*top..top + tall).contains(&y)
        })
        .map(|(name, _)| *name)
}

/// For each tick of a run, the layers whose pixels changed from the frame
/// before. Every changed pixel must lie in a layer's area.
fn changes(run: u32) -> Vec<Vec<&'static str>> {
    let mut scene = scene();
    let mut before = view(&scene, FITTED.width, FITTED.height);
    let mut ticks = Vec::new();
    for tick in 0..run {
        scene.tick(FITTED);
        let after = view(&scene, FITTED.width, FITTED.height);
        let mut moved: Vec<&'static str> = Vec::new();
        for (y, (old, new)) in before.iter().zip(&after).enumerate() {
            for (x, (old, new)) in old.iter().zip(new).enumerate() {
                if old != new {
                    let layer = layer_at(x, y).unwrap_or_else(|| {
                        panic!(
                            "tick {tick}: pixel {:?} changed outside every layer",
                            (x, y)
                        )
                    });
                    if !moved.contains(&layer) {
                        moved.push(layer);
                    }
                }
            }
        }
        ticks.push(moved);
        before = after;
    }
    ticks
}

/// Each layer moves, and only inside its own area.
/// Given: the scene on a pane the size of the picture.
/// When: it ticks and renders over about two minutes.
/// Then: the layers' areas lie inside the picture and do not overlap,
/// every pixel that changes from a frame to the next lies in one of them,
/// and each of the rain, the steam, the lamp, the tail and the screen
/// changes at least once.
///
/// Why it matters: a pixel that moves outside its layer is the still room
/// breaking up, and a layer that never moves is a still picture with a
/// name.
#[test]
fn each_layer_moves_only_inside_its_own_area() {
    // Given:
    for (index, (name, (left, top, wide, tall))) in LAYERS.iter().enumerate() {
        assert!(
            left + wide <= PICTURE.0 && top + tall <= PICTURE.1,
            "{name}"
        );
        for (other, (x, y, w, h)) in &LAYERS[index + 1..] {
            let apart = left + wide <= *x || x + w <= *left || top + tall <= *y || y + h <= *top;
            assert!(apart, "{name} overlaps {other}");
        }
    }

    // When:
    let ticks = changes(RUN);

    // Then:
    for name in ["rain", "steam", "lamp", "tail", "screen"] {
        assert!(
            ticks.iter().any(|moved| moved.contains(&name)),
            "{name} never moved"
        );
    }
}

/// Each layer keeps its own pace.
/// Given: the scene on a pane the size of the picture.
/// When: it ticks over about two minutes.
/// Then: the rain moves in most ticks, and the lamp, the tail and the
/// screen rest in most ticks.
///
/// Why it matters: a lamp that flickers all the time or a cat that never
/// stops its tail reads as a fault, and the calm is in the slow layers.
#[test]
fn each_layer_keeps_its_own_pace() {
    // When:
    let ticks = changes(RUN);

    // Then:
    let share = |name: &str| {
        ticks.iter().filter(|moved| moved.contains(&name)).count() as f32 / ticks.len() as f32
    };
    assert!(
        share("rain") > 0.5,
        "the rain moved in {} of ticks",
        share("rain")
    );
    for name in ["lamp", "tail", "screen"] {
        assert!(
            share(name) < 0.25,
            "{name} moved in {} of ticks",
            share(name)
        );
    }
}

/// The rain runs down the glass behind the cat.
/// Given: the scene on a pane the size of the picture.
/// When: it ticks, and the lowest pixel of each run of rain, in foam on
/// the glass, is followed from a frame to the next.
/// Then: three in four stay in their column at the same row or up to two
/// rows lower, and on average they move down. No rain covers the cat.
///
/// Why it matters: rain that runs up or sideways reads as a fault, and
/// rain in front of the cat is rain inside the room.
#[test]
fn the_rain_runs_down_the_glass_behind_the_cat() {
    // Given:
    let mut scene = scene();
    let rain = PALETTE.foam;
    let (left, top, wide, tall) = LAYERS.iter().find(|(name, _)| *name == "rain").unwrap().1;
    let glass: Vec<(usize, usize)> = (top..top + tall)
        .flat_map(|y| (left..left + wide).map(move |x| (x, y)))
        .collect();
    let first = view(&scene, FITTED.width, FITTED.height);
    let cat: Vec<(usize, usize)> = glass
        .iter()
        .copied()
        .filter(|&(x, y)| first[y][x] == PALETTE.text)
        .collect();
    let heads = |rows: &[Vec<Color>]| -> Vec<(usize, usize)> {
        glass
            .iter()
            .copied()
            .filter(|&(x, y)| rows[y][x] == rain && rows[y + 1][x] != rain)
            .collect()
    };
    let mut before = view(&scene, FITTED.width, FITTED.height);
    let (mut kept, mut seen, mut moved) = (0, 0, 0_isize);

    for tick in 0..400 {
        // When:
        scene.tick(FITTED);
        let after = view(&scene, FITTED.width, FITTED.height);

        // Then:
        for &(x, y) in &cat {
            assert_ne!(
                after[y][x],
                rain,
                "tick {tick}: rain on the cat at {:?}",
                (x, y)
            );
        }
        let next = heads(&after);
        for (x, y) in heads(&before) {
            seen += 1;
            if let Some(step) = (0..=2).find(|step| next.contains(&(x, y + step))) {
                kept += 1;
                moved += step as isize;
            }
        }
        before = after;
    }
    assert!(
        seen > 0 && kept * 4 >= seen * 3,
        "{kept} of {seen} heads ran down"
    );
    assert!(
        moved as f32 / seen as f32 > 0.2,
        "the heads moved {moved} rows over {seen}"
    );
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
