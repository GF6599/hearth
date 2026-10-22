#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pixels;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use hearth::config::Background;
use hearth::corpus::Corpus;
use hearth::octant::{CELL_HEIGHT, CELL_WIDTH};
use hearth::scene::{Rng, Scene, SceneKind, Settings};
use hearth::scenes::constellations::{Constellations, NAMES, TRAIL};
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette, luma};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);
const PALETTE: &Palette = &theme::ROSE_PINE;
/// Ticks enough for several figures, about a minute and a half.
const RUN: u32 = 1500;
/// The furthest a lit pixel lies from the line between the centres of two
/// stars, in pixels: the line runs through the pixel nearest each centre,
/// under three quarters of a pixel from it, and its soft edge reaches under
/// a pixel to each side.
const LINE_TOLERANCE: f32 = 1.75;
/// The steps of a share at which a test reads a mix, fine enough that a
/// share between two steps rounds to a colour one level from either.
const SHARE_STEPS: u16 = 4096;

fn ticked(seed: u64, area: Rect, ticks: u32) -> Constellations {
    let mut scene = Constellations::new(Rng::seed_from_u64(seed), PALETTE);
    for _ in 0..ticks {
        scene.tick(area);
    }
    scene
}

fn render(scene: &Constellations, area: Rect, canvas: Rect) -> Buffer {
    let mut buffer = Buffer::empty(canvas);
    scene.render(area, &mut buffer);
    buffer
}

/// Whether `colour` is a mix from `from` toward `to`, either end included,
/// to a level in each channel.
fn on_mix(colour: Color, from: Color, to: Color) -> bool {
    pixels::share_on(colour, from, to, SHARE_STEPS, 1).is_some()
}

/// Whether `colour` is a mix from the sky toward the colour of a figure.
fn in_a_figure(colour: Color) -> bool {
    on_mix(colour, PALETTE.base, PALETTE.iris) || on_mix(colour, PALETTE.base, PALETTE.foam)
}

/// Whether `colour` is a mix from the sky toward the gold of a shooting
/// star.
fn in_a_shooting_star(colour: Color) -> bool {
    on_mix(colour, PALETTE.base, PALETTE.gold)
}

/// The pixels of the cell that holds `pixel`.
fn cell_of(pixel: (f32, f32)) -> impl Iterator<Item = (f32, f32)> {
    let (left, top) = (
        pixel.0 - pixel.0 % CELL_WIDTH as f32,
        pixel.1 - pixel.1 % CELL_HEIGHT as f32,
    );
    (0..CELL_WIDTH * CELL_HEIGHT).map(move |index| {
        (
            left + (index % CELL_WIDTH) as f32,
            top + (index / CELL_WIDTH) as f32,
        )
    })
}

/// Every glyph of a render, by its place.
fn glyphs(buffer: &Buffer, area: Rect) -> BTreeMap<Position, String> {
    area.positions()
        .filter(|position| buffer[*position].symbol() != " ")
        .map(|position| (position, buffer[position].symbol().to_string()))
        .collect()
}

/// The stars of a render, by their place, each with its glyph and colour,
/// without the lines and the name of a figure.
fn stars(buffer: &Buffer, area: Rect) -> BTreeMap<Position, (String, Color)> {
    glyphs(buffer, area)
        .into_iter()
        .filter(|(_, glyph)| role(glyph).is_some())
        .map(|(position, glyph)| (position, (glyph, buffer[position].fg)))
        .collect()
}

/// Whether a star of `glyph` may show `colour`: its own role, or a mix
/// from it toward the role of a brightness next to it, which it turns
/// toward while it twinkles.
fn may_show(glyph: &str, colour: Color) -> bool {
    let (muted, subtle, text) = (PALETTE.muted, PALETTE.subtle, PALETTE.text);
    match glyph {
        "." => on_mix(colour, muted, subtle),
        "+" => on_mix(colour, subtle, muted) || on_mix(colour, subtle, text),
        "*" => on_mix(colour, text, subtle),
        _ => false,
    }
}

/// The role each brightness of star draws in, from the dimmest.
fn role(glyph: &str) -> Option<Color> {
    match glyph {
        "." => Some(PALETTE.muted),
        "+" => Some(PALETTE.subtle),
        "*" => Some(PALETTE.text),
        _ => None,
    }
}

/// The lit pixels of a render, in pixels from the top left of the area, each
/// with its colour. The sky shows the base, or the terminal's own colour
/// where nothing drew.
fn line_pixels(buffer: &Buffer, area: Rect) -> Vec<((f32, f32), Color)> {
    pixels::showing(buffer, area, |colour| {
        colour != PALETTE.base && colour != Color::Reset
    })
    .into_iter()
    .map(|((x, y), colour)| ((x as f32, y as f32), colour))
    .collect()
}

/// The centre of a star's cell, in pixels.
fn centre(star: Position) -> (f32, f32) {
    (
        f32::from(star.x) * CELL_WIDTH as f32 + 0.5,
        f32::from(star.y) * CELL_HEIGHT as f32 + 1.5,
    )
}

fn distance_to_segment(point: (f32, f32), from: (f32, f32), to: (f32, f32)) -> f32 {
    let (along_x, along_y) = (to.0 - from.0, to.1 - from.1);
    let length = along_x * along_x + along_y * along_y;
    let share = if length == 0.0 {
        0.0
    } else {
        (((point.0 - from.0) * along_x + (point.1 - from.1) * along_y) / length).clamp(0.0, 1.0)
    };
    let nearest = (from.0 + share * along_x, from.1 + share * along_y);
    ((point.0 - nearest.0).powi(2) + (point.1 - nearest.1).powi(2)).sqrt()
}

/// Each name of a figure that a render shows, with its row and its first
/// column.
fn names_shown(buffer: &Buffer, area: Rect) -> Vec<(u16, u16, &'static str)> {
    let mut shown = Vec::new();
    for y in area.top()..area.bottom() {
        let row: Vec<&str> = (area.left()..area.right())
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        let text = row.concat();
        for name in NAMES {
            for (byte, _) in text.match_indices(name) {
                let column = text[..byte].chars().count() as u16;
                shown.push((y - area.y, column, name));
            }
        }
    }
    shown
}

/// Every star is a dot, a cross or an asterisk in its own brightness.
/// Given: the scene at a hundred by thirty, after a few ticks.
/// When: it renders.
/// Then: every glyph it draws is `.` in muted, `+` in subtle or `*` in
/// text, or in a mix from that role toward the role of a brightness next
/// to it, and the sky holds stars of all three.
///
/// Why it matters: a star in another glyph can be two cells wide on some
/// terminals and tear the pane, and a star in another role breaks the
/// theme.
#[test]
fn every_star_draws_in_its_own_brightness() {
    for seed in 0..5 {
        // Given:
        let scene = ticked(seed, AREA, 10);

        // When:
        let buffer = render(&scene, AREA, AREA);

        // Then:
        let mut seen = BTreeSet::new();
        for (position, glyph) in glyphs(&buffer, AREA) {
            role(&glyph).unwrap_or_else(|| panic!("seed {seed}: {glyph:?} at {position:?}"));
            assert!(
                may_show(&glyph, buffer[position].fg),
                "seed {seed}: {glyph:?} in {:?} at {position:?}",
                buffer[position].fg
            );
            seen.insert(glyph);
        }
        assert_eq!(
            seen,
            BTreeSet::from(["*", "+", "."].map(String::from)),
            "seed {seed}"
        );
    }
}

/// Stars twinkle in place.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders for half a minute.
/// Then: the stars stay in the same places, some of them change their
/// brightness, and on every frame most of them show their usual one. A
/// star's brightness is its glyph and its colour.
///
/// Why it matters: a star that moves reads as a fault, a sky that never
/// changes reads as a still image, and a sky that changes all at once reads
/// as noise and not as a twinkle.
#[test]
fn stars_twinkle_but_stay_in_place() {
    for seed in 0..5 {
        // Given:
        let mut scene = ticked(seed, AREA, 1);

        // When:
        let mut frames = Vec::new();
        for _ in 0..500 {
            scene.tick(AREA);
            frames.push(stars(&render(&scene, AREA, AREA), AREA));
        }

        // Then:
        let places: BTreeSet<Position> = frames[0].keys().copied().collect();
        for frame in &frames {
            let now: BTreeSet<Position> = frame.keys().copied().collect();
            assert_eq!(now, places, "seed {seed}: a star moved");
        }
        let usual: BTreeMap<Position, &(String, Color)> = places
            .iter()
            .map(|place| {
                let mut counts = HashMap::new();
                for frame in &frames {
                    *counts.entry(&frame[place]).or_insert(0) += 1;
                }
                let most = counts.into_iter().max_by_key(|(_, count)| *count).unwrap();
                (*place, most.0)
            })
            .collect();
        let twinkled = places
            .iter()
            .filter(|place| frames.iter().any(|frame| &frame[place] != usual[place]))
            .count();
        assert!(twinkled > 0, "seed {seed}: no star twinkles");
        for (tick, frame) in frames.iter().enumerate() {
            let away = places
                .iter()
                .filter(|place| &frame[place] != usual[place])
                .count();
            assert!(
                away * 4 < places.len(),
                "seed {seed}: {away} of {} stars away from their usual brightness at tick {tick}",
                places.len()
            );
        }
    }
}

/// A star twinkles by its colour and keeps its glyph.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders for half a minute.
/// Then: each star shows one glyph on every frame, and some frame shows a
/// star in a colour between two roles, neither of them.
///
/// Why it matters: a glyph that swaps on a twinkle jumps from one shape to
/// another, where a star in the sky brightens and dims.
#[test]
fn a_star_twinkles_by_its_colour_and_keeps_its_glyph() {
    for seed in 0..5 {
        // Given:
        let mut scene = ticked(seed, AREA, 1);
        let first = stars(&render(&scene, AREA, AREA), AREA);
        let mut between = 0;

        for tick in 0..500 {
            // When:
            scene.tick(AREA);
            let now = stars(&render(&scene, AREA, AREA), AREA);

            // Then:
            for (place, (glyph, colour)) in &now {
                assert_eq!(
                    glyph, &first[place].0,
                    "seed {seed}: the star at {place:?} at tick {tick}"
                );
                let roles = [PALETTE.muted, PALETTE.subtle, PALETTE.text];
                between += usize::from(!roles.contains(colour));
            }
        }
        assert!(between > 0, "seed {seed}: no star shows a mix");
    }
}

/// A figure's line runs only between its own stars.
/// Given: the scene at a hundred by thirty, for a minute and a half.
/// When: it renders on every tick.
/// Then: each figure joins four to six stars of the sky, and every lit pixel
/// shows a mix from the sky toward iris or foam in a cell that a segment
/// between two of them crosses. A cell shows two colours, so the line's
/// colour can fall on any pixel of a cell it crosses. With no figure every
/// lit pixel is a shooting star's.
///
/// Why it matters: a pixel off the figure reads as a stray mark, and a line
/// that ends where no star is reads as a fault.
#[test]
fn every_line_pixel_lies_between_two_stars_of_the_figure() {
    for seed in 0..5 {
        // Given:
        let mut scene = ticked(seed, AREA, 1);
        let mut lit = 0;

        for tick in 0..RUN {
            // When:
            scene.tick(AREA);
            let buffer = render(&scene, AREA, AREA);

            // Then:
            let line = line_pixels(&buffer, AREA);
            let Some((_, figure)) = scene.figure() else {
                assert!(
                    line.iter().all(|(_, colour)| in_a_shooting_star(*colour)),
                    "seed {seed}: line pixels with no figure at tick {tick}"
                );
                continue;
            };
            assert!(
                (4..=6).contains(&figure.len()),
                "seed {seed}: {} stars at tick {tick}",
                figure.len()
            );
            let sky = stars(&buffer, AREA);
            for star in &figure {
                assert!(sky.contains_key(star), "seed {seed}: no star at {star:?}");
            }
            for (pixel, colour) in &line {
                assert!(
                    in_a_figure(*colour),
                    "seed {seed}: a pixel in {colour:?} at tick {tick}"
                );
                let on_a_segment = cell_of(*pixel).any(|place| {
                    figure.iter().enumerate().any(|(index, from)| {
                        figure[index + 1..].iter().any(|to| {
                            distance_to_segment(place, centre(*from), centre(*to)) <= LINE_TOLERANCE
                        })
                    })
                });
                assert!(
                    on_a_segment,
                    "seed {seed}: a stray pixel at {pixel:?} at tick {tick}"
                );
            }
            lit += line.len();
        }
        assert!(lit > 0, "seed {seed}: no figure drew a line");
    }
}

/// A figure's line has soft edges and fades by its colour.
/// Given: the scene at a hundred by thirty, for a minute and a half.
/// When: it renders on every tick.
/// Then: each figure shows a frame whose line takes at least three mixes
/// from the sky toward its colour, and over its last twenty frames the
/// lightest pixel of its line never gets lighter and ends less than half
/// way from the sky to the figure's colour.
///
/// Why it matters: a line of whole pixels in one colour reads as a row of
/// blocks, and a line that fades by dropping pixels breaks into dots.
#[test]
fn a_figure_line_is_soft_and_fades_by_its_colour() {
    for seed in 0..5 {
        // Given:
        let mut scene = ticked(seed, AREA, 1);
        let mut figures = 0;
        let (mut softest, mut lightest) = (0, Vec::new());

        for tick in 0..RUN {
            // When:
            scene.tick(AREA);
            let buffer = render(&scene, AREA, AREA);

            // Then:
            if scene.figure().is_none() {
                if !lightest.is_empty() {
                    assert!(softest >= 3, "seed {seed}: {softest} mixes, to tick {tick}");
                    let fade = &lightest[lightest.len().saturating_sub(20)..];
                    for pair in fade.windows(2) {
                        assert!(
                            pair[0] >= pair[1],
                            "seed {seed}: the fade {fade:?}, to tick {tick}"
                        );
                    }
                    let figure = lightest.iter().max().copied().unwrap_or(0);
                    let last = fade[fade.len() - 1];
                    assert!(
                        (last - luma(PALETTE.base)) * 2 < figure - luma(PALETTE.base),
                        "seed {seed}: the fade ends at {last} from {figure}, to tick {tick}"
                    );
                    figures += 1;
                }
                (softest, lightest) = (0, Vec::new());
                continue;
            }
            let line: BTreeSet<(u8, u8, u8)> = line_pixels(&buffer, AREA)
                .into_iter()
                .filter(|(_, colour)| in_a_figure(*colour))
                .filter_map(|(_, colour)| match colour {
                    Color::Rgb(r, g, b) => Some((r, g, b)),
                    _ => None,
                })
                .collect();
            softest = softest.max(line.len());
            lightest.push(
                line.iter()
                    .map(|&(r, g, b)| luma(Color::Rgb(r, g, b)))
                    .max()
                    .unwrap_or_else(|| luma(PALETTE.base)),
            );
        }
        assert!(figures > 0, "seed {seed}: no figure ended");
    }
}

/// The name of a figure shows beside it, and only while the figure shows.
/// Given: the scene at a hundred by thirty, for a minute and a half.
/// When: it renders on every tick.
/// Then: while a figure shows, its name shows once and whole, on a row
/// within two rows of the figure and over its columns. With no figure, no
/// name shows.
///
/// Why it matters: a name far from its figure names nothing, and a name
/// with no figure is a stray label.
#[test]
fn the_name_shows_beside_the_figure_only_while_it_shows() {
    for seed in 0..5 {
        // Given:
        let mut scene = ticked(seed, AREA, 1);
        let mut named = 0;

        for tick in 0..RUN {
            // When:
            scene.tick(AREA);
            let buffer = render(&scene, AREA, AREA);

            // Then:
            let shown = names_shown(&buffer, AREA);
            let Some((name, figure)) = scene.figure() else {
                assert!(
                    shown.is_empty(),
                    "seed {seed}: {shown:?} with no figure at tick {tick}"
                );
                continue;
            };
            assert_eq!(
                shown.iter().map(|(_, _, each)| *each).collect::<Vec<_>>(),
                [name],
                "seed {seed} at tick {tick}"
            );
            let (row, column, _) = shown[0];
            let top = figure.iter().map(|star| star.y).min().unwrap();
            let bottom = figure.iter().map(|star| star.y).max().unwrap();
            let left = figure.iter().map(|star| star.x).min().unwrap();
            let right = figure.iter().map(|star| star.x).max().unwrap();
            assert!(
                row + 2 >= top && row <= bottom + 2,
                "seed {seed}: {name:?} on row {row}, the figure on rows {top} to {bottom}"
            );
            let width = name.chars().count() as u16;
            assert!(
                column <= right && column + width > left,
                "seed {seed}: {name:?} from column {column}, the figure on {left} to {right}"
            );
            named += 1;
        }
        assert!(named > 0, "seed {seed}: no figure was named");
    }
}

/// A figure that fades leaves the sky as it found it, and another follows.
/// Given: the scene at a hundred by thirty.
/// When: it ticks until a figure has shown and faded, then on for a minute.
/// Then: the frame after the fade holds only stars, in the places they held
/// before the figure, and a new figure shows within the minute.
///
/// Why it matters: a pixel or a letter left behind builds up into clutter, and
/// a sky that shows one figure and no more stays still after it.
#[test]
fn a_faded_figure_leaves_only_stars() {
    for seed in 0..5 {
        // Given:
        let mut scene = ticked(seed, AREA, 1);

        // When:
        let mut before;
        let mut ticks = 0;
        loop {
            before = stars(&render(&scene, AREA, AREA), AREA);
            scene.tick(AREA);
            ticks += 1;
            if scene.figure().is_some() {
                break;
            }
            assert!(ticks < RUN, "seed {seed}: no figure");
        }
        while scene.figure().is_some() {
            scene.tick(AREA);
            ticks += 1;
            assert!(ticks < RUN, "seed {seed}: the figure never fades");
        }

        // Then:
        let buffer = render(&scene, AREA, AREA);
        let after: BTreeMap<Position, String> = glyphs(&buffer, AREA)
            .into_iter()
            .filter(|(position, _)| buffer[*position].fg != PALETTE.gold)
            .collect();
        for (position, glyph) in &after {
            assert!(
                role(glyph).is_some(),
                "seed {seed}: {glyph:?} left at {position:?}"
            );
        }
        assert_eq!(
            after.keys().collect::<BTreeSet<_>>(),
            before.keys().collect::<BTreeSet<_>>(),
            "seed {seed}"
        );
        let next = (0..1000).find(|_| {
            scene.tick(AREA);
            scene.figure().is_some()
        });
        assert!(
            next.is_some(),
            "seed {seed}: no second figure within a minute"
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
                assert_eq!(buffer[position].fg, Color::Reset, "colour outside {area:?}");
            }
        }
    }
}

/// A frame drawn right after the terminal shrinks stays inside the new
/// size.
/// Given: a scene that ticked on a hundred by thirty area.
/// When: it renders on a twenty by six area inside a larger canvas, before
/// it ticks at that size.
/// Then: it draws without a panic, and no star lands outside the area.
///
/// Why it matters: the main loop draws a frame before the next tick, so the
/// first frame after a resize draws the stars of the old size.
#[test]
fn a_frame_after_a_resize_and_before_a_tick_stays_inside() {
    // Given:
    let scene = ticked(7, AREA, 10);
    let canvas = Rect::new(0, 0, 40, 12);
    let area = Rect::new(3, 2, 20, 6);

    // When:
    let buffer = render(&scene, area, canvas);

    // Then:
    for position in canvas.positions() {
        if !area.contains(position) {
            assert_eq!(buffer[position].symbol(), " ", "star at {position:?}");
        }
    }
}

/// Given: the scene name `constellations`.
/// When: it is looked up and built, then ticked and rendered.
/// Then: the name finds a scene, and its first frame already shows stars.
///
/// Why it matters: a scene that is built but not listed cannot be chosen
/// from the command line, the picker or Tab.
#[test]
fn constellations_is_a_scene_by_name() {
    // When:
    let kind = SceneKind::from_name("constellations").expect("constellations is listed");
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
    assert!(!glyphs(&buffer, AREA).is_empty());
}

/// Ticks enough for a few shooting stars, about three minutes.
const SKY_RUN: u32 = 3000;
/// The most ticks one shooting star shows, about three seconds.
const STREAK_TICKS: u32 = 50;

/// The furthest a lit pixel lies from the line of a shooting star, in
/// pixels: its soft edge reaches under a pixel from its line, and a line
/// read from the pixels of cells lies off the true one by up to half a
/// pixel.
const STREAK_TOLERANCE: f32 = 1.5;

/// One shooting star so far: the middle of its pixels on its first frame
/// and on its last, and its frames.
struct Streak {
    first: (f32, f32),
    last: (f32, f32),
    ticks: u32,
}

/// The pixels of a shooting star, none while a figure shows. A shooting
/// star never flies while a figure shows, and a pixel of a figure faded
/// near the sky lies near a mix toward gold too.
fn shooting_star(scene: &Constellations, buffer: &Buffer) -> Vec<(f32, f32)> {
    if scene.figure().is_some() {
        return Vec::new();
    }
    gold_pixels(buffer)
}

/// The line of a shooting star: the middle of its light and the way along
/// it, the axis its pixels spread along most, each weighed by how much
/// lighter than the sky it is.
fn streak_line(buffer: &Buffer) -> ((f32, f32), (f32, f32)) {
    let lit: Vec<((f32, f32), f32)> = line_pixels(buffer, AREA)
        .into_iter()
        .filter(|(_, colour)| in_a_shooting_star(*colour))
        .map(|(pixel, colour)| (pixel, (luma(colour) - luma(PALETTE.base)) as f32))
        .collect();
    let weight: f32 = lit.iter().map(|(_, light)| light).sum::<f32>().max(1.0);
    let middle = lit.iter().fold((0.0, 0.0), |sum, (pixel, light)| {
        (
            sum.0 + pixel.0 * light / weight,
            sum.1 + pixel.1 * light / weight,
        )
    });
    let (across, down, both) = lit.iter().fold((0.0, 0.0, 0.0), |sums, (pixel, light)| {
        let (x, y) = (pixel.0 - middle.0, pixel.1 - middle.1);
        (
            sums.0 + light * x * x,
            sums.1 + light * y * y,
            sums.2 + light * x * y,
        )
    });
    let angle = 0.5 * (2.0 * both).atan2(across - down);
    (middle, (angle.cos(), angle.sin()))
}

/// The pixels in a mix from the sky toward gold.
fn gold_pixels(buffer: &Buffer) -> Vec<(f32, f32)> {
    line_pixels(buffer, AREA)
        .into_iter()
        .filter(|(_, colour)| in_a_shooting_star(*colour))
        .map(|(pixel, _)| pixel)
        .collect()
}

fn distance(from: (f32, f32), to: (f32, f32)) -> f32 {
    (to.0 - from.0).hypot(to.1 - from.1)
}

fn centroid(pixels: &[(f32, f32)]) -> (f32, f32) {
    let count = pixels.len() as f32;
    let (x, y) = pixels
        .iter()
        .fold((0.0, 0.0), |sum, pixel| (sum.0 + pixel.0, sum.1 + pixel.1));
    (x / count, y / count)
}

/// A shooting star crosses part of the sky at a slant and burns out, with a
/// short trail.
/// Given: the scene at a hundred by thirty, for three minutes.
/// When: it renders on every tick.
/// Then: the pixels of gold mixes of each frame lie in cells that the line
/// of the streak crosses, within its soft edge, and that line slants. The
/// line runs through the middle of the streak's light along the axis its
/// light spreads along most. The streak is no longer along it than the
/// trail and its soft edge. A cell shows two colours, so a faint colour
/// can fall on any pixel of a cell the streak crosses. Each shooting star
/// moves along its line further than its trail and is gone within three
/// seconds, and more than one shows.
///
/// Why it matters: a streak that lies flat or falls straight reads as a
/// fault, a long trail reads as a line of a figure, and a streak that stays
/// reads as a stray mark.
#[test]
fn a_shooting_star_crosses_at_a_slant_with_a_short_trail() {
    for seed in 0..5 {
        // Given:
        let mut scene = ticked(seed, AREA, 1);
        let mut streak: Option<Streak> = None;
        let mut streaks = 0;

        for tick in 0..SKY_RUN {
            // When:
            scene.tick(AREA);
            let buffer = render(&scene, AREA, AREA);
            let gold = shooting_star(&scene, &buffer);

            // Then:
            if gold.is_empty() {
                if let Some(Streak { first, last, ticks }) = streak.take() {
                    assert!(
                        ticks <= STREAK_TICKS,
                        "seed {seed}: a streak shows {ticks} ticks, to tick {tick}"
                    );
                    assert!(
                        distance(first, last) > TRAIL as f32,
                        "seed {seed}: a streak moves {} pixels, to tick {tick}",
                        distance(first, last)
                    );
                    streaks += 1;
                }
                continue;
            }
            let (middle, heading) = streak_line(&buffer);
            let along = |place: (f32, f32)| {
                (place.0 - middle.0) * heading.0 + (place.1 - middle.1) * heading.1
            };
            let off = |place: (f32, f32)| {
                ((place.0 - middle.0) * heading.1 - (place.1 - middle.1) * heading.0).abs()
            };
            let (least, most) = gold
                .iter()
                .fold((f32::MAX, f32::MIN), |(least, most), pixel| {
                    (least.min(along(*pixel)), most.max(along(*pixel)))
                });
            let span = most - least;
            assert!(
                span <= TRAIL as f32 + STREAK_TOLERANCE,
                "seed {seed}: a trail of {span} pixels at tick {tick}"
            );
            for pixel in &gold {
                assert!(
                    cell_of(*pixel).any(|place| off(place) <= STREAK_TOLERANCE),
                    "seed {seed}: {pixel:?} off the streak at tick {tick}"
                );
            }
            if span >= 4.0 {
                let (across, down) = (heading.0.abs(), heading.1.abs());
                assert!(
                    down >= across * 0.2 && down <= across * 5.0,
                    "seed {seed}: a streak {across} across and {down} down at tick {tick}"
                );
            }
            let here = centroid(&gold);
            streak = Some(match streak {
                Some(Streak { first, ticks, .. }) => Streak {
                    first,
                    last: here,
                    ticks: ticks + 1,
                },
                None => Streak {
                    first: here,
                    last: here,
                    ticks: 1,
                },
            });
        }
        assert!(streaks > 1, "seed {seed}: {streaks} shooting stars");
    }
}

/// A shooting star's trail fades from its head back.
/// Given: the scene at a hundred by thirty, for three minutes.
/// When: it renders on every tick.
/// Then: on three frames of four or more where a shooting star shows six
/// pixels or more and showed on the frame before, the half of its pixels
/// nearer its head, the way it moved, is lighter on average than the half
/// nearer its tail. A cell shows two colours, so a faint streak that ends
/// in a cell with its brighter middle can read lighter at its tail.
///
/// Why it matters: a trail in one colour reads as a dash that slides, and
/// not as a streak that burns behind its head.
#[test]
fn a_shooting_star_fades_from_its_head_back() {
    let (mut checked, mut fading) = (0, 0);
    for seed in 0..5 {
        // Given:
        let mut scene = ticked(seed, AREA, 1);
        let mut before: Vec<(f32, f32)> = Vec::new();

        for _ in 0..SKY_RUN {
            // When:
            scene.tick(AREA);
            let buffer = render(&scene, AREA, AREA);
            let lit: Vec<((f32, f32), Color)> = line_pixels(&buffer, AREA)
                .into_iter()
                .filter(|(_, colour)| scene.figure().is_none() && in_a_shooting_star(*colour))
                .collect();

            // Then:
            let now: Vec<(f32, f32)> = lit.iter().map(|(pixel, _)| *pixel).collect();
            if before.is_empty() || now.len() < 6 {
                before = now;
                continue;
            }
            let (from, to) = (centroid(&before), centroid(&now));
            let along = |pixel: &(f32, f32)| {
                (pixel.0 - from.0) * (to.0 - from.0) + (pixel.1 - from.1) * (to.1 - from.1)
            };
            let mut ordered = lit.clone();
            ordered.sort_by(|one, other| along(&one.0).total_cmp(&along(&other.0)));
            let half = ordered.len() / 2;
            let mean = |pixels: &[((f32, f32), Color)]| {
                pixels
                    .iter()
                    .map(|(_, colour)| f64::from(luma(*colour)))
                    .sum::<f64>()
                    / pixels.len() as f64
            };
            let (tail, head) = (
                mean(&ordered[..half]),
                mean(&ordered[ordered.len() - half..]),
            );
            fading += usize::from(head > tail);
            checked += 1;
            before = now;
        }
    }
    assert!(checked > 20, "{checked} frames checked");
    assert!(
        fading * 4 >= checked * 3,
        "{fading} of {checked} frames lighter at the head"
    );
}

/// The shares of a tick that the frames between two ticks show.
const BETWEEN: [f64; 3] = [0.25, 0.5, 0.75];

/// The frame on a tick, the frames between it and the next tick, and the
/// frame on the next tick, each read by `read`. The scene ticks once.
fn glide<T>(scene: &mut Constellations, read: impl Fn(&Buffer) -> T) -> Vec<T> {
    let mut frames = vec![read(&render(scene, AREA, AREA))];
    for progress in BETWEEN {
        scene.set_progress(progress);
        frames.push(read(&render(scene, AREA, AREA)));
    }
    scene.set_progress(0.0);
    scene.tick(AREA);
    frames.push(read(&render(scene, AREA, AREA)));
    frames
}

/// The furthest a line can pass from the centre of a cell and still cross
/// it, in pixels, with room to spare.
const STAR_REACH: f32 = 3.0;

/// The least and the most of `pixels` along `heading`.
fn ends_along(pixels: &[(f32, f32)], heading: (f32, f32)) -> (f32, f32) {
    pixels
        .iter()
        .map(|pixel| pixel.0 * heading.0 + pixel.1 * heading.1)
        .fold((f32::MAX, f32::MIN), |(least, most), along| {
            (least.min(along), most.max(along))
        })
}

/// A shooting star glides along its line between two ticks.
/// Given: the scene at a hundred by thirty, for three minutes.
/// When: on each tick with a shooting star, it renders at a quarter, a half
/// and three quarters of the way to its next tick, then ticks.
/// Then: where the shooting star still shows on the next tick, crosses no
/// star, and the middle of its light, its pixels weighed by how much
/// lighter than the sky each is, moves two pixels or more from one tick to
/// the next, its head and that middle never go back along its line, and
/// the middle at half a tick lies strictly between the two. The head stops
/// at the end of the run, part way through a tick, and the trail then
/// moves its light about a pixel a tick, finer than a cell of two colours
/// shows. The end of the trail fades into the sky, and a cell of two
/// colours shows its faintest pixel on one frame and not on the next. A
/// star's glyph hides the pixels of its cell, so a shooting star that
/// crosses a star can seem to step back.
///
/// Why it matters: at a quarter speed a shooting star that moves only on a
/// tick jumps four pixels once every sixteen frames, and reads as a dotted
/// line and not as a streak.
#[test]
fn a_shooting_star_glides_between_ticks() {
    let mut moving = 0;
    for seed in 0..3 {
        // Given:
        let mut scene = ticked(seed, AREA, 1);

        for tick in 0..SKY_RUN {
            if shooting_star(&scene, &render(&scene, AREA, AREA)).is_empty() {
                scene.tick(AREA);
                continue;
            }

            // When:
            let lights = glide(&mut scene, |buffer| {
                line_pixels(buffer, AREA)
                    .into_iter()
                    .filter(|(_, colour)| in_a_shooting_star(*colour))
                    .map(|(pixel, colour)| (pixel, (luma(colour) - luma(PALETTE.base)) as f32))
                    .collect::<Vec<_>>()
            });
            let frames: Vec<Vec<(f32, f32)>> = lights
                .iter()
                .map(|frame| frame.iter().map(|(pixel, _)| *pixel).collect())
                .collect();

            // Then:
            let (first, last) = (&frames[0], &frames[frames.len() - 1]);
            if last.is_empty() {
                continue;
            }
            let (from, to) = (centroid(first), centroid(last));
            let moved = distance(from, to);
            let heading = ((to.0 - from.0) / moved, (to.1 - from.1) / moved);
            let ends: Vec<(f32, f32)> = frames
                .iter()
                .map(|frame| ends_along(frame, heading))
                .collect();
            let along = |pixel: &(f32, f32)| pixel.0 * heading.0 + pixel.1 * heading.1;
            let tail = first
                .iter()
                .copied()
                .min_by(|a, b| along(a).total_cmp(&along(b)))
                .unwrap();
            let head = last
                .iter()
                .copied()
                .max_by(|a, b| along(a).total_cmp(&along(b)))
                .unwrap();
            let sky = stars(&render(&scene, AREA, AREA), AREA);
            if sky
                .keys()
                .any(|star| distance_to_segment(centre(*star), tail, head) <= STAR_REACH)
            {
                continue;
            }
            let middles: Vec<f32> = lights
                .iter()
                .map(|frame| {
                    let weight: f32 = frame.iter().map(|(_, light)| light).sum();
                    frame
                        .iter()
                        .map(|(pixel, light)| along(pixel) * light)
                        .sum::<f32>()
                        / weight.max(1.0)
                })
                .collect();
            let (start, half, end) = (middles[0], middles[2], middles[4]);
            if end - start < 2.0 {
                continue;
            }
            for (pair, middle) in ends.windows(2).zip(middles.windows(2)) {
                assert!(
                    pair[0].1 <= pair[1].1 && middle[0] <= middle[1],
                    "seed {seed}: the shooting star goes back at tick {tick}: {ends:?} {middles:?}"
                );
            }
            assert!(
                start < half && half < end,
                "seed {seed}: the streak not between its ticks at tick {tick}: {middles:?}"
            );
            moving += 1;
        }
    }
    assert!(moving > 20, "{moving} shooting stars moved two pixels");
}

/// A figure's line draws itself between two ticks.
/// Given: the scene at a hundred by thirty, for a minute and a half.
/// When: on each tick, it renders at a quarter, a half and three quarters
/// of the way to its next tick, then ticks.
/// Then: where the line reaches more cells from one tick to the next, each
/// frame lights every cell the frame before it lit. A cell shows two
/// colours, so a pixel of the line can take another of the line's colours
/// as its cell gains pixels, but a cell the line crosses always shows the
/// line. Where it grows by two pixels or more, the line at half a tick
/// holds more pixels than the first and fewer than the last in three
/// frames of four or more. A star's glyph hides the pixels of its cell, so
/// a line that reaches a star can seem to pause.
///
/// Why it matters: at a quarter speed a line that grows only on a tick
/// jumps once every sixteen frames, and the pen reads as a stutter.
#[test]
fn a_figure_line_draws_itself_between_ticks() {
    let (mut growing, mut between) = (0, 0);
    for seed in 0..5 {
        // Given:
        let mut scene = ticked(seed, AREA, 1);

        for tick in 0..RUN {
            if scene.figure().is_none() {
                scene.tick(AREA);
                continue;
            }

            // When:
            let frames = glide(&mut scene, |buffer| {
                line_pixels(buffer, AREA)
                    .into_iter()
                    .filter(|(_, colour)| in_a_figure(*colour))
                    .map(|((x, y), _)| (x as i32, y as i32))
                    .collect::<BTreeSet<(i32, i32)>>()
            });
            let cells: Vec<BTreeSet<(i32, i32)>> = frames
                .iter()
                .map(|frame| {
                    frame
                        .iter()
                        .map(|(x, y)| (x / CELL_WIDTH as i32, y / CELL_HEIGHT as i32))
                        .collect()
                })
                .collect();

            // Then:
            let (first, last) = (&frames[0], &frames[frames.len() - 1]);
            if first.is_empty() || cells[cells.len() - 1].len() <= cells[0].len() {
                continue;
            }
            for pair in cells.windows(2) {
                assert!(
                    pair[0].is_subset(&pair[1]),
                    "seed {seed}: the line loses a cell at tick {tick}"
                );
            }
            if last.len() >= first.len() + 2 {
                let half = frames[2].len();
                between += usize::from(first.len() < half && half < last.len());
                growing += 1;
            }
        }
    }
    assert!(growing > 50, "{growing} lines grew two pixels");
    assert!(
        between * 4 >= growing * 3,
        "{between} of {growing} lines between their ticks at half a tick"
    );
}

/// A frame on a tick is the frame the scene drew before it glided.
/// Given: two scenes from one seed, one of them given a share of a tick
/// before each tick and none after it.
/// When: both tick and render for three minutes.
/// Then: every frame of the two is the same.
///
/// Why it matters: the frame on a tick is the one every other test of the
/// scene reads, so a glide must not move it.
#[test]
fn a_frame_on_a_tick_is_the_frame_before_the_glide() {
    // Given:
    let mut glided = ticked(7, AREA, 1);
    let mut still = ticked(7, AREA, 1);

    for tick in 0..SKY_RUN {
        // When:
        glided.set_progress(0.5);
        glided.set_progress(0.0);
        glided.tick(AREA);
        still.tick(AREA);

        // Then:
        assert_eq!(
            render(&glided, AREA, AREA),
            render(&still, AREA, AREA),
            "tick {tick}"
        );
    }
}

/// Over a see-through base, the lines and the shooting stars mix nothing
/// into it.
/// Given: the scene on Rose Pine with a see-through base.
/// When: it ticks and renders for three minutes.
/// Then: no pixel lies part way along the mix from the base toward a colour
/// the scene draws over the sky: a figure's iris or foam, or a shooting
/// star's gold.
///
/// Why it matters: there the base is the terminal's own background, and a
/// mix with it is a colour the user never chose, a smudge round every edge.
#[test]
fn over_a_see_through_base_nothing_mixes_with_it() {
    // Given:
    let palette = &theme::FLAT_THEMES[theme::position("rose-pine").unwrap()];
    let mut scene = Constellations::new(Rng::seed_from_u64(0), palette);
    let drawn = [palette.iris, palette.foam, palette.gold];
    let mut lit = 0;

    for tick in 0..SKY_RUN {
        // When:
        scene.tick(AREA);
        if tick % 5 != 0 {
            continue;
        }
        let buffer = render(&scene, AREA, AREA);

        // Then:
        let shown = pixels::showing(&buffer, AREA, |colour| {
            colour != palette.base && colour != Color::Reset
        });
        lit += shown.len();
        for (pixel, colour) in shown {
            assert!(
                !drawn
                    .iter()
                    .any(|to| { colour != *to && on_mix(colour, palette.base, *to) }),
                "tick {tick}: {pixel:?} in {colour:?} mixes the base"
            );
        }
    }
    assert!(lit > 0, "nothing drawn");
}
