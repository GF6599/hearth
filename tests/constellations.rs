#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use hearth::corpus::Corpus;
use hearth::scene::{Rng, Scene, SceneKind};
use hearth::scenes::constellations::Constellations;
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);
const PALETTE: &Palette = &theme::ROSE_PINE;

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

/// The stars of a render, by their place.
fn stars(buffer: &Buffer, area: Rect) -> BTreeMap<Position, String> {
    area.positions()
        .filter(|position| buffer[*position].symbol() != " ")
        .map(|position| (position, buffer[position].symbol().to_string()))
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
        for (position, glyph) in stars(&buffer, AREA) {
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
    let mut scene = kind.build(7, PALETTE, &Arc::new(Corpus::embedded()), Speed::DEFAULT);
    scene.tick(AREA);

    // Then:
    let mut buffer = Buffer::empty(AREA);
    scene.render(AREA, &mut buffer);
    assert!(!stars(&buffer, AREA).is_empty());
}
