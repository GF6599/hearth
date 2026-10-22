#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pixels;

use std::sync::Arc;

use hearth::config::Background;
use hearth::corpus::Corpus;
use hearth::octant;
use hearth::scene::{Rng, Scene, SceneKind, Settings};
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

/// The pixels across and down in each cell.
const ACROSS: usize = octant::CELL_WIDTH;
const DOWN: usize = octant::CELL_HEIGHT;

/// The pixels across and down in each letter of the picture: a cell across
/// and half a cell down, so a letter is as wide as it is tall.
const LETTER: usize = 2;

/// The colour of each pixel of the area, two across and four down to a
/// cell, row by row, as the glyph and its two colours draw them.
/// A cell with another glyph shows its background.
fn pixels(buffer: &Buffer, area: Rect) -> Vec<Vec<Color>> {
    pixels::grid(buffer, area, |_, cell| {
        pixels::shown(cell).unwrap_or([cell.bg; pixels::PIXELS])
    })
}

/// The pixels of a pane as drawn. A cell holds two colours, so where the
/// rain or the steam meets two colours of the room a pixel can show
/// another's colour. A cell of the room alone draws each pixel as it is.
fn view(scene: &Study, width: u16, height: u16) -> Vec<Vec<Color>> {
    let area = Rect::new(0, 0, width, height);
    pixels(&render(scene, area, area), area)
}

/// `palette` with `colour` in light and every other colour in dark.
fn two_tone(palette: &Palette, colour: Color) -> &'static Palette {
    let tone = |each: Color| if each == colour { LIGHT } else { DARK };
    Box::leak(Box::new(Palette {
        base: tone(palette.base),
        surface: tone(palette.surface),
        overlay: tone(palette.overlay),
        muted: tone(palette.muted),
        subtle: tone(palette.subtle),
        text: tone(palette.text),
        love: tone(palette.love),
        gold: tone(palette.gold),
        rose: tone(palette.rose),
        pine: tone(palette.pine),
        foam: tone(palette.foam),
        iris: tone(palette.iris),
        ..*palette
    }))
}

const DARK: Color = Color::Rgb(0, 0, 0);
const LIGHT: Color = Color::Rgb(255, 255, 255);

/// Scenes from seed 7, one for each colour of a palette, each with its
/// colour in light and the rest in dark. A palette changes no tick, so the
/// scenes run in step, and a cell of two tones draws each pixel as it is.
struct Toned {
    scenes: Vec<(Color, Study)>,
}

impl Toned {
    fn new(palette: &'static Palette) -> Self {
        let mut colours = vec![
            palette.base,
            palette.surface,
            palette.overlay,
            palette.muted,
            palette.subtle,
            palette.text,
            palette.love,
            palette.gold,
            palette.rose,
            palette.pine,
            palette.foam,
            palette.iris,
        ];
        colours.dedup();
        let scenes = colours
            .into_iter()
            .map(|colour| {
                let scene = Study::new(Rng::seed_from_u64(7), two_tone(palette, colour));
                (colour, scene)
            })
            .collect();
        Self { scenes }
    }

    fn tick(&mut self, area: Rect) {
        for (_, scene) in &mut self.scenes {
            scene.tick(area);
        }
    }

    /// Whether each pixel of a pane is in `colour`.
    fn shows(&self, colour: Color, width: u16, height: u16) -> Vec<Vec<bool>> {
        let (_, scene) = self
            .scenes
            .iter()
            .find(|(each, _)| *each == colour)
            .unwrap();
        view(scene, width, height)
            .into_iter()
            .map(|row| row.into_iter().map(|pixel| pixel == LIGHT).collect())
            .collect()
    }

    /// The colour of each pixel of a pane, as the scene holds it.
    fn view(&self, width: u16, height: u16) -> Vec<Vec<Color>> {
        let lit: Vec<(Color, Vec<Vec<bool>>)> = self
            .scenes
            .iter()
            .map(|(colour, _)| (*colour, self.shows(*colour, width, height)))
            .collect();
        let mut rows = vec![vec![Color::Reset; lit[0].1[0].len()]; lit[0].1.len()];
        for (y, row) in rows.iter_mut().enumerate() {
            for (x, pixel) in row.iter_mut().enumerate() {
                if let Some((colour, _)) = lit.iter().find(|(_, shows)| shows[y][x]) {
                    *pixel = *colour;
                }
            }
        }
        rows
    }
}

/// Every role of Rose Pine.
const ROLES: [Color; 12] = [
    PALETTE.base,
    PALETTE.surface,
    PALETTE.overlay,
    PALETTE.muted,
    PALETTE.subtle,
    PALETTE.text,
    PALETTE.love,
    PALETTE.gold,
    PALETTE.rose,
    PALETTE.pine,
    PALETTE.foam,
    PALETTE.iris,
];

/// The scene in each colour of Rose Pine, ticked once.
fn toned() -> Toned {
    let mut scenes = Toned::new(PALETTE);
    scenes.tick(AREA);
    scenes
}

/// The whole picture as a part of itself.
const WHOLE: (usize, usize, usize, usize) = (0, 0, PICTURE.0, PICTURE.1);

/// Where a view of a part of the picture starts on a pane of `cells`, in
/// pixels: half the room left over, which is less than none when the view
/// is cut. A letter is a cell across, so across it starts on a whole cell.
/// Down, a letter is half a cell, and a cell draws two colours of its
/// pixels, so the picture starts on a whole cell, half a cell higher where
/// the room left over leaves it on a half, and each cell holds the same
/// pixels of the picture at every size.
fn offset(
    cells: (u16, u16),
    (_, from_y, wide, tall): (usize, usize, usize, usize),
) -> (isize, isize) {
    let room = (
        usize::from(cells.0) * ACROSS / LETTER,
        usize::from(cells.1) * DOWN / LETTER,
    );
    let first = (from_y as isize - (room.1 as isize - tall as isize) / 2) * LETTER as isize;
    let first = first.div_euclid(DOWN as isize) * DOWN as isize;
    (
        (room.0 as isize - wide as isize) / 2 * LETTER as isize,
        (from_y * LETTER) as isize - first,
    )
}

/// A part of the picture in pixels, from one in letters.
fn in_pixels(
    (left, top, wide, tall): (usize, usize, usize, usize),
) -> (usize, usize, usize, usize) {
    (left * LETTER, top * LETTER, wide * LETTER, tall * LETTER)
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

/// The layer whose area holds the pixel at (x, y).
fn layer_at(x: usize, y: usize) -> Option<&'static str> {
    let (x, y) = (x / LETTER, y / LETTER);
    LAYERS
        .iter()
        .find(|(_, (left, top, wide, tall))| {
            (*left..left + wide).contains(&x) && (*top..top + tall).contains(&y)
        })
        .map(|(name, _)| *name)
}

/// For each tick of a run, the layers whose pixels changed from the frame
/// before. Every changed pixel must lie in a layer's area. Each area starts
/// on a whole letter, so the pixels outside it in a cell are one letter of
/// one colour, and they show as they are.
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
/// letters lower, and on average they move down. No rain covers the cat,
/// the pixels in text in the lower half of the glass. The moon is in text
/// too, but it is outside and in the upper half.
///
/// Why it matters: rain that runs up or sideways reads as a fault, and
/// rain in front of the cat is rain inside the room.
#[test]
fn the_rain_runs_down_the_glass_behind_the_cat() {
    // Given:
    let mut scenes = toned();
    let rain = PALETTE.foam;
    let (left, top, wide, tall) =
        in_pixels(LAYERS.iter().find(|(name, _)| *name == "rain").unwrap().1);
    let glass: Vec<(usize, usize)> = (top..top + tall)
        .flat_map(|y| (left..left + wide).map(move |x| (x, y)))
        .collect();
    let first = scenes.shows(PALETTE.text, FITTED.width, FITTED.height);
    let cat: Vec<(usize, usize)> = glass
        .iter()
        .copied()
        .filter(|&(x, y)| y >= top + tall / 2 && first[y][x])
        .collect();
    let mut before = scenes.shows(rain, FITTED.width, FITTED.height);
    let (mut kept, mut seen, mut moved) = (0, 0, 0_isize);

    for tick in 0..400 {
        // When:
        scenes.tick(FITTED);
        let after = scenes.shows(rain, FITTED.width, FITTED.height);

        // Then:
        for &(x, y) in &cat {
            assert!(!after[y][x], "tick {tick}: rain on the cat at {:?}", (x, y));
        }
        let next = heads(&glass, &after);
        for (x, y) in heads(&glass, &before) {
            seen += 1;
            if let Some(step) = (0..=2 * LETTER).find(|step| next.contains(&(x, y + step))) {
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
        moved as f32 / seen as f32 > 0.2 * LETTER as f32,
        "the heads moved {moved} pixels over {seen}"
    );
}

/// The lowest pixel of each run of rain on the glass: a pixel in the rain
/// with none below it.
fn heads(glass: &[(usize, usize)], rain: &[Vec<bool>]) -> Vec<(usize, usize)> {
    glass
        .iter()
        .copied()
        .filter(|&(x, y)| rain[y][x] && !rain[y + 1][x])
        .collect()
}

/// The rain runs down a quarter of a cell at a time.
/// Given: the scene on a pane the size of the picture.
/// When: it ticks for a minute, and the lowest pixel of each run of rain is
/// found in each frame.
/// Then: the runs end on rows of pixels at the top of a letter as well as
/// on rows at its bottom.
///
/// Why it matters: rain that moves half a cell at a time jumps down the
/// glass instead of running.
#[test]
fn the_rain_runs_down_a_quarter_of_a_cell_at_a_time() {
    // Given:
    let mut scenes = toned();
    let (left, top, wide, tall) =
        in_pixels(LAYERS.iter().find(|(name, _)| *name == "rain").unwrap().1);
    let glass: Vec<(usize, usize)> = (top..top + tall)
        .flat_map(|y| (left..left + wide).map(move |x| (x, y)))
        .collect();
    let mut ends = [0; LETTER];

    // When:
    for _ in 0..1000 {
        scenes.tick(FITTED);
        let rain = scenes.shows(PALETTE.foam, FITTED.width, FITTED.height);
        for (_, y) in heads(&glass, &rain) {
            ends[y % LETTER] += 1;
        }
    }

    // Then:
    assert!(
        ends.iter().all(|&count| count > 0),
        "runs end on rows {ends:?} of a letter"
    );
}

/// The shares of a tick at which a test draws a frame between two ticks.
const BETWEEN: [f64; 3] = [0.25, 0.5, 0.75];

/// A scene from seed 7 with `colour` in light and every other colour in
/// dark, so each pixel of that colour shows as it is.
fn lit_scene(colour: Color) -> Study {
    Study::new(Rng::seed_from_u64(7), two_tone(PALETTE, colour))
}

/// Whether each pixel of the picture is lit, at `progress` from the last
/// tick towards the next.
fn lit_at(scene: &mut Study, progress: f64) -> Vec<Vec<bool>> {
    scene.set_progress(progress);
    view(scene, FITTED.width, FITTED.height)
        .into_iter()
        .map(|row| row.into_iter().map(|pixel| pixel == LIGHT).collect())
        .collect()
}

/// The pixels of a layer's area, by column and row.
fn area_of(layer: &str) -> Vec<(usize, usize)> {
    let (left, top, wide, tall) =
        in_pixels(LAYERS.iter().find(|(name, _)| *name == layer).unwrap().1);
    (top..top + tall)
        .flat_map(|y| (left..left + wide).map(move |x| (x, y)))
        .collect()
}

/// Given: two scenes from one seed on a pane the size of the picture.
/// When: both tick, and one is told a share of a tick and then none before
/// each frame.
/// Then: the two frames are the same.
///
/// Why it matters: a frame on a tick is the frame the scene always drew, so
/// a glide between ticks changes no tick.
#[test]
fn the_frame_on_a_tick_does_not_change() {
    // Given:
    let mut glide = scene();
    let mut plain = scene();

    for tick in 0..300 {
        // When:
        glide.tick(FITTED);
        plain.tick(FITTED);
        glide.set_progress(0.5);
        let _ = view(&glide, FITTED.width, FITTED.height);
        glide.set_progress(0.0);

        // Then:
        assert!(
            view(&glide, FITTED.width, FITTED.height) == view(&plain, FITTED.width, FITTED.height),
            "tick {tick}"
        );
    }
}

/// Given: the scene on a pane the size of the picture.
/// When: it ticks, and draws frames at a quarter, a half and three quarters
/// of the way to the next tick.
/// Then: some run of rain that ends two pixels lower on the next tick ends
/// one pixel lower in a frame between.
///
/// Why it matters: rain that moves only on a tick steps two pixels at a
/// time at a slow speed, and a glide runs it down one pixel at a time.
#[test]
fn the_rain_runs_on_between_two_ticks() {
    // Given:
    let mut scene = lit_scene(PALETTE.foam);
    let glass = area_of("rain");
    scene.tick(FITTED);
    let mut before = heads(&glass, &lit_at(&mut scene, 0.0));
    let mut between = 0;

    for _ in 0..400 {
        // When:
        let frames: Vec<Vec<(usize, usize)>> = BETWEEN
            .iter()
            .map(|progress| heads(&glass, &lit_at(&mut scene, *progress)))
            .collect();
        scene.tick(FITTED);
        let after = heads(&glass, &lit_at(&mut scene, 0.0));

        // Then:
        for &(x, y) in &before {
            if after.contains(&(x, y + 2)) && frames.iter().any(|heads| heads.contains(&(x, y + 1)))
            {
                between += 1;
            }
        }
        before = after;
    }
    assert!(between > 0, "no run of rain ended between two ticks");
}

/// Given: the scene on a pane the size of the picture.
/// When: it ticks, and draws frames at a quarter, a half and three quarters
/// of the way to the next tick.
/// Then: each pixel of rain in a frame between that was not rain on the
/// tick lies at most two letters under rain of the tick in its column, or
/// under the frame or the cat, its light in text and its shade in subtle,
/// which hide the rain behind them, or under the top of the glass.
///
/// Why it matters: a drop that starts again at the top of the glass must
/// not draw a streak across the glass on its way there.
#[test]
fn a_drop_that_starts_again_draws_no_streak_between_ticks() {
    // Given:
    let mut scene = lit_scene(PALETTE.foam);
    let glass = area_of("rain");
    let top = glass.iter().map(|&(_, y)| y).min().unwrap();
    let bottom = glass.iter().map(|&(_, y)| y).max().unwrap();
    let reach = 2 * LETTER;
    let room = toned();
    let bars = room.shows(PALETTE.muted, FITTED.width, FITTED.height);
    let text = room.shows(PALETTE.text, FITTED.width, FITTED.height);
    let shade = room.shows(PALETTE.subtle, FITTED.width, FITTED.height);
    // The moon is in text and the stars in subtle too, but they are in the
    // upper half, and the rain runs over them.
    let hides =
        |x: usize, y: usize| bars[y][x] || ((text[y][x] || shade[y][x]) && 2 * y > top + bottom);

    for tick in 0..400 {
        scene.tick(FITTED);
        let on_tick = lit_at(&mut scene, 0.0);

        // When:
        for progress in BETWEEN {
            let frame = lit_at(&mut scene, progress);

            // Then:
            for &(x, y) in &glass {
                if frame[y][x] && !on_tick[y][x] {
                    let below_rain = (1..=reach)
                        .filter(|up| y >= *up)
                        .any(|up| on_tick[y - up][x] || hides(x, y - up));
                    assert!(
                        below_rain || y < top + reach,
                        "tick {tick} at {progress}: rain at {:?} far from the rain of the tick",
                        (x, y)
                    );
                }
            }
        }
    }
}

/// Given: the scene on a pane the size of the picture.
/// When: it ticks for two minutes, and draws frames at a quarter, a half
/// and three quarters of the way to the next tick.
/// Then: some frame between shows the steam in neither the place of the
/// tick before it nor the place of the tick after it.
///
/// Why it matters: steam that moves only on a tick moves across and up in
/// one step, and a glide moves it a pixel at a time.
#[test]
fn the_steam_rises_between_two_ticks() {
    // Given:
    let mut scene = lit_scene(PALETTE.subtle);
    let air = area_of("steam");
    let steam =
        |frame: &[Vec<bool>]| -> Vec<bool> { air.iter().map(|&(x, y)| frame[y][x]).collect() };
    scene.tick(FITTED);
    let mut before = steam(&lit_at(&mut scene, 0.0));
    let mut between = 0;

    for _ in 0..RUN {
        // When:
        let frames: Vec<Vec<bool>> = BETWEEN
            .iter()
            .map(|progress| steam(&lit_at(&mut scene, *progress)))
            .collect();
        scene.tick(FITTED);
        let after = steam(&lit_at(&mut scene, 0.0));

        // Then:
        between += frames
            .iter()
            .filter(|frame| **frame != before && **frame != after)
            .count();
        before = after;
    }
    assert!(between > 0, "the steam moved only on a tick");
}

/// Given: the scene on a pane the size of the picture.
/// When: it ticks for a minute and renders after each tick.
/// Then: some cells draw an octant glyph that splits a half of the cell,
/// across or down.
///
/// Why it matters: a room drawn only in half blocks moves its rain and
/// steam in coarse steps.
#[test]
fn the_room_draws_in_octant_glyphs() {
    // Given:
    let mut scene = scene();
    let halves = [0x00, 0x0f, 0xf0, 0xff].map(octant::glyph);
    let mut split = 0;

    // When:
    for _ in 0..1000 {
        scene.tick(FITTED);
        let buffer = render(&scene, FITTED, FITTED);

        // Then:
        split += FITTED
            .positions()
            .filter(|position| {
                let glyph = buffer[*position].symbol().chars().next().unwrap();
                !halves.contains(&glyph)
            })
            .count();
    }
    assert!(split > 0, "every cell is a half block or a space");
}

/// The picture is centred in a pane larger than it.
/// Given: the scene on a pane of a hundred by thirty, and on one ten cells
/// wider and three rows taller.
/// When: both render.
/// Then: the larger pane shows the same pixels five cells further right and
/// two cells further down, so the picture keeps the middle of each: the
/// room is three letters further down, and the picture starts on the whole
/// cell under that. The picture holds at least eight colours of the theme.
///
/// Why it matters: a picture stuck at a corner of a large terminal reads
/// as a fault.
#[test]
fn the_picture_is_centred_in_a_larger_pane() {
    // Given:
    let scenes = toned();
    assert!(AREA.width as usize >= PICTURE.0 && AREA.height as usize * 2 >= PICTURE.1);

    // When:
    let (smaller, larger) = (scenes.view(100, 30), scenes.view(110, 33));

    // Then:
    shows_from(&smaller, &larger, (5 * ACROSS as isize, 2 * DOWN as isize));
    let (left, top) = offset((100, 30), WHOLE);
    let (_, _, wide, tall) = in_pixels((0, 0, PICTURE.0, PICTURE.1));
    let mut colours: Vec<Color> = Vec::new();
    for row in &smaller[top as usize..top as usize + tall] {
        for colour in &row[left as usize..left as usize + wide] {
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
    let scenes = toned();
    let small = (56, 18);
    assert!(small.0 >= LEAST.0 && small.1 >= LEAST.1);
    assert!((small.0 as usize) < PICTURE.0 && (small.1 as usize) * 2 < PICTURE.1);

    // When:
    let (smaller, larger) = (scenes.view(small.0, small.1), scenes.view(100, 30));

    // Then:
    let (large_left, large_top) = offset((100, 30), WHOLE);
    let (small_left, small_top) = offset(small, WHOLE);
    shows_from(
        &smaller,
        &larger,
        (large_left - small_left, large_top - small_top),
    );
}

/// The window part of the picture on a pane under the least size, and the
/// pixel of the pane where it starts.
fn window_on(small: (u16, u16)) -> (Vec<Vec<Color>>, (isize, isize)) {
    (toned().view(small.0, small.1), offset(small, WINDOW))
}

/// Where the window part starts on a hundred by thirty, where the whole
/// room shows.
fn window_in_room() -> (Vec<Vec<Color>>, (isize, isize)) {
    let (left, top) = offset((100, 30), WHOLE);
    let (from_x, from_y, _, _) = in_pixels(WINDOW);
    (
        toned().view(100, 30),
        (left + from_x as isize, top + from_y as isize),
    )
}

/// Below the least size the whole window shows.
/// Given: the scene on a pane under the least size with room for the
/// window part of the picture, and on a hundred by thirty.
/// When: both render.
/// Then: the small pane shows the window part, the window, the cat and the
/// sill, the same as it is in the room. The window's frame, the muted
/// pixels of the window part, lies whole inside the part with a letter or
/// more to spare on every side, and it has as much room on its left as on
/// its right, to a letter.
///
/// Why it matters: a room cut to a small pane is a strip of desk that
/// reads as nothing, and a window cut at its frame reads as a fault.
#[test]
fn below_the_least_size_the_whole_window_shows() {
    // Given:
    let small = (34, 14);
    assert!(small.0 < LEAST.0 || small.1 < LEAST.1);
    assert!(small.0 as usize >= WINDOW.2 && small.1 as usize * 2 >= WINDOW.3);

    // When:
    let (smaller, (left, top)) = window_on(small);
    let (larger, (from_x, from_y)) = window_in_room();

    // Then:
    let (_, _, wide, tall) = in_pixels(WINDOW);
    for y in 0..tall {
        for x in 0..wide {
            let (at_x, at_y) = ((left + x as isize) as usize, (top + y as isize) as usize);
            assert_eq!(
                smaller[at_y][at_x],
                larger[(from_y + y as isize) as usize][(from_x + x as isize) as usize],
                "pixel {:?} of the window part",
                (x, y)
            );
        }
    }
    let frame: Vec<(usize, usize)> = (0..tall)
        .flat_map(|y| (0..wide).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            smaller[(top + y as isize) as usize][(left + x as isize) as usize] == PALETTE.muted
        })
        .collect();
    assert!(!frame.is_empty(), "no frame");
    for &(x, y) in &frame {
        assert!(
            x >= LETTER && y >= LETTER && x + LETTER < wide && y + LETTER < tall,
            "the frame reaches the window part's edge at {:?}",
            (x, y)
        );
    }
    let first = frame.iter().map(|(x, _)| *x).min().unwrap();
    let last = frame.iter().map(|(x, _)| *x).max().unwrap();
    let right = wide - 1 - last;
    assert!(
        first.abs_diff(right) <= LETTER,
        "the frame has {first} pixels on its left and {right} on its right"
    );
}

/// A pane too small for the window shows its middle.
/// Given: the scene on a pane smaller than the window part, and on a
/// hundred by thirty.
/// When: both render.
/// Then: every pixel of the small pane is the pixel of the window part as
/// it is in the room, and the window's middle bar, the columns with the
/// most muted, is in the middle of the pane to half a letter, so the
/// window is cut evenly from each side.
///
/// Why it matters: a window cut from one corner loses its frame on one
/// side only and reads as off centre.
#[test]
fn a_pane_smaller_than_the_window_shows_its_middle() {
    // Given:
    let small = (24, 9);
    assert!((small.0 as usize) < WINDOW.2 && (small.1 as usize) * 2 < WINDOW.3);

    // When:
    let (smaller, (left, top)) = window_on(small);
    let (larger, (from_x, from_y)) = window_in_room();

    // Then:
    shows_from(&smaller, &larger, (from_x - left, from_y - top));
    let muted = |x: usize| smaller.iter().filter(|row| row[x] == PALETTE.muted).count();
    let most = (0..smaller[0].len()).map(muted).max().unwrap();
    let bar: Vec<usize> = (0..smaller[0].len())
        .filter(|x| muted(*x) == most)
        .collect();
    let middle = bar.iter().sum::<usize>() as f32 / bar.len() as f32;
    let centre = (smaller[0].len() as f32 - 1.0) / 2.0;
    assert!(
        (middle - centre).abs() <= 0.5 * LETTER as f32,
        "the middle bar is at {middle}, the pane's middle at {centre}"
    );
}

/// The window shows a night sky.
/// Given: the scene in Rose Pine and in Rose Pine Dawn.
/// When: it renders on a pane the size of the picture.
/// Then: the colour most of the glass shows is the theme's base, and the
/// glass shows no pine.
///
/// Why it matters: a teal sky reads as day, and the scene is a night.
#[test]
fn the_window_shows_a_night_sky() {
    for palette in [&theme::ROSE_PINE, &theme::ROSE_PINE_DAWN] {
        // Given:
        let mut scenes = Toned::new(palette);
        scenes.tick(FITTED);

        // When:
        let rows = scenes.view(FITTED.width, FITTED.height);

        // Then:
        let (left, top, wide, tall) =
            in_pixels(LAYERS.iter().find(|(name, _)| *name == "rain").unwrap().1);
        let mut counts: Vec<(Color, usize)> = Vec::new();
        for row in &rows[top..top + tall] {
            for colour in &row[left..left + wide] {
                assert_ne!(*colour, palette.pine, "pine on the glass");
                match counts.iter_mut().find(|(each, _)| each == colour) {
                    Some((_, count)) => *count += 1,
                    None => counts.push((*colour, 1)),
                }
            }
        }
        let most = counts.iter().max_by_key(|(_, count)| *count).unwrap().0;
        assert_eq!(most, palette.base, "{counts:?}");
    }
}

/// The lamp lights the wall and the desk below it, and dims as it
/// flickers.
/// Given: the scene on a pane the size of the picture.
/// When: it ticks over about two minutes.
/// Then: in the first frame the gold of the lamp reaches ten letters or
/// more below the top of its shade, and in some frame of the run the lower
/// half of the lamp's area holds less gold than in the first. The pixels
/// of the lamp's area that differ between the first frame and the dimmest,
/// the light, reach from the shade to the desk, twelve letters or more,
/// show on the wall within four to seven letters of the shade's top, and
/// are all warm in the first frame: gold, rose, the bulb's text, or a mix
/// along the light from the wall through rose and gold to text.
///
/// Why it matters: a lamp that throws no light reads as a picture of a
/// lamp, and a glow that stays bright while the bulb dims reads as a
/// fault.
#[test]
fn the_lamp_lights_the_desk_and_dims_as_it_flickers() {
    // Given:
    let mut scene = scene();
    let (left, top, wide, tall) =
        in_pixels(LAYERS.iter().find(|(name, _)| *name == "lamp").unwrap().1);
    let gold = |rows: &[Vec<Color>], from: usize| -> Vec<usize> {
        (from..top + tall)
            .flat_map(|y| (left..left + wide).map(move |x| (x, y)))
            .filter(|&(x, y)| rows[y][x] == PALETTE.gold)
            .map(|(_, y)| y)
            .collect()
    };
    let first = view(&scene, FITTED.width, FITTED.height);
    let lit = gold(&first, top);
    let (highest, lowest) = (*lit.iter().min().unwrap(), *lit.iter().max().unwrap());
    assert!(
        lowest >= highest + 10 * LETTER,
        "the light reaches from row {highest} to {lowest}"
    );
    let lower = top + tall / 2;
    let bright = gold(&first, lower).len();

    // When:
    let (mut dimmest, mut dim) = (bright, first.clone());
    for _ in 0..RUN {
        scene.tick(FITTED);
        let rows = view(&scene, FITTED.width, FITTED.height);
        let count = gold(&rows, lower).len();
        if count < dimmest {
            (dimmest, dim) = (count, rows);
        }
    }

    // Then:
    assert!(
        dimmest < bright,
        "the glow held {bright} gold pixels in every frame"
    );
    let lit: Vec<usize> = (top..top + tall)
        .filter(|&y| (left..left + wide).any(|x| first[y][x] != dim[y][x]))
        .collect();
    let (from, to) = (lit[0], lit[lit.len() - 1]);
    assert!(to - from >= 12 * LETTER, "the light changes rows {lit:?}");
    assert!(
        lit.iter()
            .any(|y| (from + 4 * LETTER..from + 8 * LETTER).contains(y)),
        "no light on the wall under the shade: rows {lit:?}"
    );
    for y in top..top + tall {
        for x in left..left + wide {
            if first[y][x] != dim[y][x] {
                let light = [PALETTE.overlay, PALETTE.rose, PALETTE.gold, PALETTE.text];
                let colour = first[y][x];
                let warm = light[1..].contains(&colour)
                    || light
                        .windows(2)
                        .any(|pair| pixels::between(colour, pair[0], pair[1]));
                assert!(warm, "the light at {:?} is {:?}", (x, y), first[y][x]);
            }
        }
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
/// Then: the name finds a scene, and its first frame shows the cat and the
/// lamp.
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
        Settings::DEFAULT,
    );
    scene.tick(AREA);

    // Then:
    let mut buffer = Buffer::empty(AREA);
    scene.render(AREA, &mut buffer);
    let rows = pixels(&buffer, AREA);
    assert!(rows.iter().flatten().any(|colour| *colour == PALETTE.gold));
    assert!(rows.iter().flatten().any(|colour| *colour == PALETTE.text));
}

/// The pixels of a pane the size of the picture that lie outside every
/// layer, by column and row, so they show the room that stays still.
fn still(rows: &[Vec<Color>]) -> Vec<(usize, usize)> {
    (0..rows.len())
        .flat_map(|y| (0..rows[y].len()).map(move |x| (x, y)))
        .filter(|&(x, y)| layer_at(x, y).is_none())
        .collect()
}

/// Given: the scene on a pane the size of the picture.
/// When: it ticks once and renders.
/// Then: outside every layer, some letter of the picture, two pixels
/// across and two down, holds two colours.
///
/// Why it matters: a picture drawn a letter to a colour has a quarter of
/// the detail the screen shows, and its edges step in squares of two
/// pixels.
#[test]
fn the_room_has_detail_at_the_size_of_a_pixel() {
    // Given, when:
    let rows = view(&scene(), FITTED.width, FITTED.height);

    // Then:
    let fine = still(&rows).iter().any(|&(x, y)| {
        let (left, top) = (x - x % LETTER, y - y % LETTER);
        (top..top + LETTER)
            .any(|row| (left..left + LETTER).any(|column| rows[row][column] != rows[y][x]))
    });
    assert!(fine, "every letter of the room is one colour");
}

/// Given: the scene on a pane the size of the picture.
/// When: it ticks once and renders.
/// Then: outside every layer, some pixel is a mix of two roles of the
/// theme.
///
/// Why it matters: a room in flat roles has no light and shade, and a mix
/// of two of its own colours lights or shades a part and keeps to the
/// theme.
#[test]
fn the_room_is_shaded_with_mixes_of_its_colours() {
    // Given, when:
    let rows = view(&scene(), FITTED.width, FITTED.height);

    // Then:
    let shaded = still(&rows).iter().any(|&(x, y)| {
        let colour = rows[y][x];
        !ROLES.contains(&colour)
            && ROLES.iter().any(|one| {
                ROLES
                    .iter()
                    .any(|other| pixels::between(colour, *one, *other))
            })
    });
    assert!(shaded, "the room shows no mix of two of its colours");
}

/// The most light the glow further out holds, as a share of the light near
/// the bulb, so a glow of one strength fails.
const FADED: f32 = 2.0 / 3.0;

/// Given: the scene on a pane the size of the picture, before the lamp
/// first flickers.
/// When: it ticks once and renders.
/// Then: on the wall under the bulb, in the lamp's area, some pixels are
/// mixes of the wall and rose or of rose and gold, and those further than
/// three pixels from the bulb, its text, lie under two thirds as far from
/// the wall on average as those within three.
///
/// Why it matters: light drawn in flat gold and rose reads as a pattern on
/// the wall, and a glow that fades out from the bulb reads as light.
#[test]
fn the_lamp_glows_on_the_wall_and_fades_from_the_bulb() {
    // Given, when:
    let rows = view(&scene(), FITTED.width, FITTED.height);

    // Then:
    let (left, top, wide, tall) =
        in_pixels(LAYERS.iter().find(|(name, _)| *name == "lamp").unwrap().1);
    let lamp: Vec<(usize, usize)> = (top..top + tall)
        .flat_map(|y| (left..left + wide).map(move |x| (x, y)))
        .collect();
    let bulb: Vec<(usize, usize)> = lamp
        .iter()
        .copied()
        .filter(|&(x, y)| rows[y][x] == PALETTE.text)
        .collect();
    assert!(!bulb.is_empty(), "no bulb");
    let centre = |pick: fn(&(usize, usize)) -> usize| {
        bulb.iter().map(pick).sum::<usize>() as f32 / bulb.len() as f32
    };
    let (bx, by) = (centre(|(x, _)| *x), centre(|(_, y)| *y));
    let wall = PALETTE.overlay;
    let glow: Vec<(f32, f32)> = lamp
        .iter()
        .filter(|&&(_, y)| y as f32 > by)
        .filter_map(|&(x, y)| {
            let colour = rows[y][x];
            let mixed = pixels::between(colour, wall, PALETTE.rose)
                || pixels::between(colour, PALETTE.rose, PALETTE.gold);
            mixed.then(|| {
                let far = (x as f32 - bx).hypot(y as f32 - by);
                (far, pixels::distance(wall, colour))
            })
        })
        .collect();
    assert!(!glow.is_empty(), "no glow of the lamp on the wall");
    let mean = |values: Vec<f32>| values.iter().sum::<f32>() / values.len().max(1) as f32;
    let light = |near: bool| {
        mean(
            glow.iter()
                .filter(|(far, _)| (*far <= 3.0) == near)
                .map(|(_, light)| *light)
                .collect(),
        )
    };
    let (near, out) = (light(true), light(false));
    assert!(
        out < near * FADED,
        "the glow near the bulb lies {near} from the wall and further out {out}"
    );
}

/// Given: the scene on a pane the size of the picture on Rose Pine with the
/// terminal's background, where the base is see-through, the night sky and
/// the screen.
/// When: it runs for a minute, a frame after each tick.
/// Then: no pixel is a mix of the base and a colour the study draws by the
/// sky and the screen: the gold and rose of the town, the lamp and the
/// code, the foam of the rain and the code, the love of a book and the pine
/// of the vase. The greys, text and the pale iris lie on a line from the
/// base, so a shade between two greys would read as a mix with the base,
/// and the test reads these colours only. It reads no role of the
/// theme, and no shade of the lamp's glow from the wall through rose to
/// gold, because the wall lies close to the base.
///
/// Why it matters: a mix with the base is an opaque colour over the
/// terminal's own background, and shows as a smudge over a transparent
/// terminal.
#[test]
fn over_the_terminal_background_no_pixel_mixes_with_the_base() {
    // Given:
    let palette = &theme::FLAT_THEMES[0];
    assert!(palette.see_through);
    let mut scene = Study::new(Rng::seed_from_u64(7), palette);
    let drawn = [
        palette.gold,
        palette.rose,
        palette.foam,
        palette.love,
        palette.pine,
    ];

    for tick in 0..1000 {
        // When:
        scene.tick(FITTED);
        let rows = view(&scene, FITTED.width, FITTED.height);

        // Then:
        let glow = |colour: Color| {
            pixels::between(colour, palette.overlay, palette.rose)
                || pixels::between(colour, palette.rose, palette.gold)
        };
        for (y, row) in rows.iter().enumerate() {
            for (x, colour) in row.iter().enumerate() {
                if ROLES.contains(colour) || glow(*colour) {
                    continue;
                }
                for other in drawn {
                    assert!(
                        !pixels::between(*colour, palette.base, other),
                        "tick {tick}: {colour:?} at ({x}, {y}) mixes the base and {other:?}"
                    );
                }
            }
        }
    }
}
