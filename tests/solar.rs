#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::f32::consts::TAU;
use std::sync::Arc;

use hearth::braille::{self, CELL_HEIGHT, CELL_WIDTH};
use hearth::config::Background;
use hearth::corpus::Corpus;
use hearth::scene::{Rng, Scene, SceneKind};
use hearth::scenes::solar::Solar;
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);
const PALETTE: &Palette = &theme::ROSE_PINE;
/// Ticks enough for Neptune to go round about twice.
const RUN: u32 = 3000;

fn ticked(seed: u64, area: Rect, ticks: u32) -> Solar {
    let mut scene = Solar::new(Rng::seed_from_u64(seed), PALETTE);
    for _ in 0..ticks {
        scene.tick(area);
    }
    scene
}

fn render(scene: &Solar, area: Rect, canvas: Rect) -> Buffer {
    let mut buffer = Buffer::empty(canvas);
    scene.render(area, &mut buffer);
    buffer
}

/// The lit braille dots of a render, in dots from the top left of the
/// area, each with the colour of its cell.
fn dots(buffer: &Buffer, area: Rect) -> Vec<((f32, f32), Color)> {
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

/// The middle of the gold dots, which only the sun draws.
fn sun(buffer: &Buffer, area: Rect) -> (f32, f32) {
    let gold: Vec<(f32, f32)> = dots(buffer, area)
        .into_iter()
        .filter(|(_, colour)| *colour == PALETTE.gold)
        .map(|(dot, _)| dot)
        .collect();
    assert!(!gold.is_empty(), "no sun in {area:?}");
    let count = gold.len() as f32;
    let (x, y) = gold
        .iter()
        .fold((0.0, 0.0), |sum, dot| (sum.0 + dot.0, sum.1 + dot.1));
    (x / count, y / count)
}

/// The middle of the pane, in dots.
fn middle(area: Rect) -> (f32, f32) {
    (
        (f32::from(area.width) * CELL_WIDTH as f32 - 1.0) / 2.0,
        (f32::from(area.height) * CELL_HEIGHT as f32 - 1.0) / 2.0,
    )
}

/// Each planet keeps to one ellipse round the sun, inside the ellipse of
/// the next.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders long enough for Neptune to go round twice.
/// Then: each planet's place stays on one ellipse centred on the sun, each
/// ellipse lies inside the next, every frame draws each planet where it is,
/// and every dotted orbit dot lies on one of the ellipses.
///
/// Why it matters: a planet that drifts off its ellipse reads as a fault,
/// and an orbit drawn where no planet goes reads as a stray line.
#[test]
fn each_planet_keeps_to_its_own_ellipse() {
    for seed in 0..3 {
        // Given:
        let mut scene = ticked(seed, AREA, 1);
        let centre = sun(&render(&scene, AREA, AREA), AREA);
        let mut paths: Vec<Vec<(f32, f32)>> = vec![Vec::new(); 8];
        let mut orbit_dots = Vec::new();

        for tick in 0..RUN {
            // When:
            scene.tick(AREA);
            let planets = scene.planets();
            assert_eq!(planets.len(), 8, "seed {seed}");
            if tick % 50 == 0 {
                let buffer = render(&scene, AREA, AREA);
                for (index, planet) in planets.iter().enumerate() {
                    let cell = (
                        (planet.0 / CELL_WIDTH as f32) as u16,
                        (planet.1 / CELL_HEIGHT as f32) as u16,
                    );
                    let symbol = buffer[cell].symbol().chars().next().map(u32::from);
                    assert!(
                        symbol.is_some_and(|code| (0x2801..=0x28ff).contains(&code)),
                        "seed {seed}: planet {index} not drawn at tick {tick}"
                    );
                }
                orbit_dots.extend(
                    dots(&buffer, AREA)
                        .into_iter()
                        .filter(|(_, colour)| *colour == PALETTE.muted)
                        .map(|(dot, _)| dot),
                );
            }
            for (path, planet) in paths.iter_mut().zip(planets) {
                path.push(planet);
            }
        }

        // Then:
        let ellipses: Vec<(f32, f32)> = paths
            .iter()
            .map(|path| {
                let across = path
                    .iter()
                    .map(|p| (p.0 - centre.0).abs())
                    .fold(0.0, f32::max);
                let down = path
                    .iter()
                    .map(|p| (p.1 - centre.1).abs())
                    .fold(0.0, f32::max);
                (across, down)
            })
            .collect();
        for (index, (path, (across, down))) in paths.iter().zip(&ellipses).enumerate() {
            for place in path {
                let (dx, dy) = ((place.0 - centre.0) / across, (place.1 - centre.1) / down);
                let reach = (dx * dx + dy * dy).sqrt();
                assert!(
                    (reach - 1.0).abs() < 0.1,
                    "seed {seed}: planet {index} at {place:?} is {reach} of its ellipse"
                );
            }
        }
        for pair in ellipses.windows(2) {
            assert!(
                pair[0].0 < pair[1].0 && pair[0].1 < pair[1].1,
                "seed {seed}: {:?} not inside {:?}",
                pair[0],
                pair[1]
            );
        }
        assert!(!orbit_dots.is_empty(), "seed {seed}: no orbit drawn");
        for dot in &orbit_dots {
            let on_one = ellipses.iter().any(|(across, down)| {
                let (dx, dy) = ((dot.0 - centre.0) / across, (dot.1 - centre.1) / down);
                ((dx * dx + dy * dy).sqrt() - 1.0).abs() * down <= 1.5
            });
            assert!(on_one, "seed {seed}: orbit dot {dot:?} on no ellipse");
        }
    }
}

/// An inner planet goes round faster than an outer one.
/// Given: the scene at a hundred by thirty.
/// When: it ticks for three minutes.
/// Then: the angle each planet turns through round the sun falls from
/// Mercury to Neptune, and Neptune still moves.
///
/// Why it matters: planets that all turn together read as a wheel and not
/// as a solar system.
#[test]
fn an_inner_planet_goes_round_faster_than_an_outer_one() {
    // Given:
    let mut scene = ticked(7, AREA, 1);
    let centre = sun(&render(&scene, AREA, AREA), AREA);
    let angle = |planet: (f32, f32)| (planet.1 - centre.1).atan2(planet.0 - centre.0);
    let mut last: Vec<f32> = scene.planets().into_iter().map(angle).collect();
    let mut turned = vec![0.0_f32; last.len()];

    // When:
    for _ in 0..RUN {
        scene.tick(AREA);
        for ((planet, before), total) in scene.planets().into_iter().zip(&mut last).zip(&mut turned)
        {
            let now = angle(planet);
            let mut step = now - *before;
            if step > TAU / 2.0 {
                step -= TAU;
            } else if step < -TAU / 2.0 {
                step += TAU;
            }
            *total += step.abs();
            *before = now;
        }
    }

    // Then:
    for pair in turned.windows(2) {
        assert!(pair[0] > pair[1], "{turned:?}");
    }
    assert!(turned[7] > TAU / 2.0, "Neptune turns {} radians", turned[7]);
}

/// The sun stays in the middle of the pane, and after a resize too.
/// Given: the scene at a hundred by thirty.
/// When: it renders, then ticks and renders at sixty by twenty.
/// Then: the sun's middle is the middle of the pane each time.
///
/// Why it matters: a sun off the middle pushes the outer orbits out of the
/// pane on one side.
#[test]
fn the_sun_stays_in_the_middle_after_a_resize() {
    // Given:
    let mut scene = ticked(7, AREA, 5);

    for area in [AREA, Rect::new(0, 0, 60, 20)] {
        // When:
        scene.tick(area);
        let centre = sun(&render(&scene, area, area), area);

        // Then:
        let middle = middle(area);
        assert!(
            (centre.0 - middle.0).abs() <= 1.0 && (centre.1 - middle.1).abs() <= 1.0,
            "sun at {centre:?}, middle {middle:?} in {area:?}"
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
    let mut scene = ticked(7, AREA, 40);
    let inside_only = |buffer: &Buffer, area: Rect| {
        for position in canvas.positions() {
            if !area.contains(position) {
                assert_eq!(buffer[position].symbol(), " ", "glyph outside {area:?}");
                assert_eq!(buffer[position].fg, Color::Reset, "colour outside {area:?}");
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

/// Given: the scene name `solar`.
/// When: it is looked up and built, then ticked and rendered.
/// Then: the name finds a scene, and its first frame already shows the sun.
///
/// Why it matters: a scene that is built but not listed cannot be chosen
/// from the command line, the picker or Tab.
#[test]
fn solar_is_a_scene_by_name() {
    // When:
    let kind = SceneKind::from_name("solar").expect("solar is listed");
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
    sun(&buffer, AREA);
}
