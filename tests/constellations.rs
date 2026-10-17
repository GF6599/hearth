#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use hearth::braille::{self, CELL_HEIGHT, CELL_WIDTH};
use hearth::config::Background;
use hearth::corpus::Corpus;
use hearth::scene::{Rng, Scene, SceneKind};
use hearth::scenes::constellations::{Constellations, NAMES, TRAIL};
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);
const PALETTE: &Palette = &theme::ROSE_PINE;
/// Ticks enough for several figures, about a minute and a half.
const RUN: u32 = 1500;
/// The furthest a lit dot lies from the line between the centres of two
/// stars, in dots: half a dot of rounding at each end, and half a dot of
/// stepping along the line.
const LINE_TOLERANCE: f32 = 1.5;

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

/// Every glyph of a render, by its place.
fn glyphs(buffer: &Buffer, area: Rect) -> BTreeMap<Position, String> {
    area.positions()
        .filter(|position| buffer[*position].symbol() != " ")
        .map(|position| (position, buffer[position].symbol().to_string()))
        .collect()
}

/// The stars of a render, by their place, without the lines and the name
/// of a figure.
fn stars(buffer: &Buffer, area: Rect) -> BTreeMap<Position, String> {
    glyphs(buffer, area)
        .into_iter()
        .filter(|(_, glyph)| role(glyph).is_some())
        .collect()
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

/// The lit braille dots of a render, in dots from the top left of the
/// area, each with the colour of its cell.
fn line_dots(buffer: &Buffer, area: Rect) -> Vec<((f32, f32), Color)> {
    let mut dots = Vec::new();
    for position in area.positions() {
        let cell = &buffer[position];
        let Some(code) = cell.symbol().chars().next().map(u32::from) else {
            continue;
        };
        if !(0x2801..=0x28ff).contains(&code) {
            continue;
        }
        for column in 0..CELL_WIDTH {
            for row in 0..CELL_HEIGHT {
                if (code - 0x2800) & braille::dot(column, row) != 0 {
                    let x = usize::from(position.x - area.x) * CELL_WIDTH + column;
                    let y = usize::from(position.y - area.y) * CELL_HEIGHT + row;
                    dots.push(((x as f32, y as f32), cell.fg));
                }
            }
        }
    }
    dots
}

/// The centre of a star's cell, in dots.
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
/// Then: every glyph it draws is `.` in muted, `+` in subtle or `*` in text,
/// and the sky holds stars of all three.
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
            let colour =
                role(&glyph).unwrap_or_else(|| panic!("seed {seed}: {glyph:?} at {position:?}"));
            assert_eq!(
                buffer[position].fg, colour,
                "seed {seed}: {glyph:?} at {position:?}"
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
/// brightness, and on every frame most of them show their usual one.
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
        let usual: BTreeMap<Position, &String> = places
            .iter()
            .map(|place| {
                let mut counts = BTreeMap::new();
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

/// A figure's line runs only between its own stars.
/// Given: the scene at a hundred by thirty, for a minute and a half.
/// When: it renders on every tick.
/// Then: each figure joins four to six stars of the sky, every lit dot lies
/// on a segment between two of them, in iris or foam, and no dot shows
/// without a figure.
///
/// Why it matters: a dot off the figure reads as a stray mark, and a line
/// that ends where no star is reads as a fault.
#[test]
fn every_line_dot_lies_between_two_stars_of_the_figure() {
    for seed in 0..5 {
        // Given:
        let mut scene = ticked(seed, AREA, 1);
        let mut lit = 0;

        for tick in 0..RUN {
            // When:
            scene.tick(AREA);
            let buffer = render(&scene, AREA, AREA);

            // Then:
            let dots = line_dots(&buffer, AREA);
            let Some((_, figure)) = scene.figure() else {
                assert!(
                    dots.iter().all(|(_, colour)| *colour == PALETTE.gold),
                    "seed {seed}: line dots with no figure at tick {tick}"
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
            for (dot, colour) in &dots {
                assert!(
                    *colour == PALETTE.iris || *colour == PALETTE.foam,
                    "seed {seed}: a dot in {colour:?} at tick {tick}"
                );
                let on_a_segment = figure.iter().enumerate().any(|(index, from)| {
                    figure[index + 1..].iter().any(|to| {
                        distance_to_segment(*dot, centre(*from), centre(*to)) <= LINE_TOLERANCE
                    })
                });
                assert!(
                    on_a_segment,
                    "seed {seed}: a stray dot at {dot:?} at tick {tick}"
                );
            }
            lit += dots.len();
        }
        assert!(lit > 0, "seed {seed}: no figure drew a line");
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
/// Why it matters: a dot or a letter left behind builds up into clutter, and
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

fn gold_dots(buffer: &Buffer) -> Vec<(f32, f32)> {
    line_dots(buffer, AREA)
        .into_iter()
        .filter(|(_, colour)| *colour == PALETTE.gold)
        .map(|(dot, _)| dot)
        .collect()
}

fn distance(from: (f32, f32), to: (f32, f32)) -> f32 {
    (to.0 - from.0).hypot(to.1 - from.1)
}

fn centroid(dots: &[(f32, f32)]) -> (f32, f32) {
    let count = dots.len() as f32;
    let (x, y) = dots
        .iter()
        .fold((0.0, 0.0), |sum, dot| (sum.0 + dot.0, sum.1 + dot.1));
    (x / count, y / count)
}

/// A shooting star crosses part of the sky at a slant and burns out, with a
/// short trail.
/// Given: the scene at a hundred by thirty, for three minutes.
/// When: it renders on every tick.
/// Then: the gold dots of each frame lie on one line at a slant, no longer
/// than the trail. Each shooting star moves along its line further than its
/// trail and is gone within three seconds, and more than one shows.
///
/// Why it matters: a streak that lies flat or falls straight reads as a
/// fault, a long trail reads as a line of a figure, and a streak that stays
/// reads as a stray mark.
#[test]
fn a_shooting_star_crosses_at_a_slant_with_a_short_trail() {
    for seed in 0..5 {
        // Given:
        let mut scene = ticked(seed, AREA, 1);
        let mut streak: Option<((f32, f32), (f32, f32), u32)> = None;
        let mut streaks = 0;

        for tick in 0..SKY_RUN {
            // When:
            scene.tick(AREA);
            let dots = gold_dots(&render(&scene, AREA, AREA));

            // Then:
            if dots.is_empty() {
                if let Some((first, last, ticks)) = streak.take() {
                    assert!(
                        ticks <= STREAK_TICKS,
                        "seed {seed}: a streak shows {ticks} ticks, to tick {tick}"
                    );
                    assert!(
                        distance(first, last) > TRAIL as f32,
                        "seed {seed}: a streak moves {} dots, to tick {tick}",
                        distance(first, last)
                    );
                    streaks += 1;
                }
                continue;
            }
            let ends = dots
                .iter()
                .flat_map(|from| dots.iter().map(move |to| (*from, *to)))
                .max_by(|a, b| distance(a.0, a.1).total_cmp(&distance(b.0, b.1)))
                .unwrap();
            let span = distance(ends.0, ends.1);
            assert!(
                span <= TRAIL as f32 + 1.0,
                "seed {seed}: a trail of {span} dots at tick {tick}"
            );
            for dot in &dots {
                assert!(
                    distance_to_segment(*dot, ends.0, ends.1) <= 1.0,
                    "seed {seed}: {dot:?} off the streak at tick {tick}"
                );
            }
            if span >= 4.0 {
                let (across, down) = ((ends.1.0 - ends.0.0).abs(), (ends.1.1 - ends.0.1).abs());
                assert!(
                    down >= across * 0.2 && down <= across * 5.0,
                    "seed {seed}: a streak {across} across and {down} down at tick {tick}"
                );
            }
            let here = centroid(&dots);
            streak = Some(match streak {
                Some((first, _, ticks)) => (first, here, ticks + 1),
                None => (here, here, 1),
            });
        }
        assert!(streaks > 1, "seed {seed}: {streaks} shooting stars");
    }
}
