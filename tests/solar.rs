#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pixels;

use std::collections::HashSet;
use std::f32::consts::TAU;
use std::sync::Arc;

use hearth::config::Background;
use hearth::corpus::Corpus;
use hearth::octant::{CELL_HEIGHT, CELL_WIDTH};
use hearth::scene::{Rng, Scene, SceneKind, Settings, TICK_RATE};
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
const SATURN: usize = 5;
const EARTH: usize = 2;
const MOON: usize = 8;
/// The furthest Saturn's ring reaches from its middle, in pixels, with room
/// to spare.
const SATURN_REACH: f32 = 10.0;
/// How far a body reaches from its middle, across and down, in pixels, with
/// room to spare. A cell shows two colours, so a body can change a cell it
/// shares with another, and the reach keeps a cell past its disc.
const PLANET_SPAN: (f32, f32) = (6.0, 8.0);
const EARTH_SPAN: (f32, f32) = (4.0, 6.0);
const MOON_SPAN: (f32, f32) = (1.5, 1.5);
/// The furthest a pixel of a planet's disc lies from its middle, in pixels,
/// with room to spare. The largest planet's radius is 4.4.
const BODY_REACH: f32 = 5.0;
/// The planets that share their colour with no other body. Mercury shares
/// subtle with the Moon, and Saturn shares gold with the sun.
const OWN_COLOUR: [usize; 6] = [1, 2, 3, 4, 6, 7];
/// The role each planet draws in, Mercury to Neptune.
fn roles() -> [Color; 8] {
    [
        PALETTE.subtle,
        PALETTE.text,
        PALETTE.foam,
        PALETTE.love,
        PALETTE.rose,
        PALETTE.gold,
        PALETTE.pine,
        PALETTE.iris,
    ]
}

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

/// The lit pixels of a render, those in any colour but the base, in pixels
/// from the top left of the area, each with its colour.
fn lit_pixels(buffer: &Buffer, area: Rect) -> Vec<((f32, f32), Color)> {
    pixels::showing(buffer, area, |colour| colour != PALETTE.base)
        .into_iter()
        .map(|((x, y), colour)| ((x as f32, y as f32), colour))
        .collect()
}

/// The colour the pixel at a place in pixels shows, in a render whose area
/// starts at the top left of its buffer.
fn colour_at(buffer: &Buffer, place: (f32, f32)) -> Option<Color> {
    let (x, y) = (place.0 as usize, place.1 as usize);
    let cell = (
        u16::try_from(x / CELL_WIDTH).ok()?,
        u16::try_from(y / CELL_HEIGHT).ok()?,
    );
    let shown = pixels::shown(buffer.cell(cell)?)?;
    Some(shown[y % CELL_HEIGHT * CELL_WIDTH + x % CELL_WIDTH])
}

/// The sun's pixels: the largest group of touching gold pixels. Saturn, its
/// ring and its trail are gold too, and lie well apart from the sun.
fn sun_pixels(buffer: &Buffer, area: Rect) -> Vec<(f32, f32)> {
    let gold: HashSet<(i32, i32)> = lit_pixels(buffer, area)
        .into_iter()
        .filter(|(_, colour)| *colour == PALETTE.gold)
        .map(|(pixel, _)| (pixel.0 as i32, pixel.1 as i32))
        .collect();
    let mut seen = HashSet::new();
    let mut largest = Vec::new();
    for &start in &gold {
        if !seen.insert(start) {
            continue;
        }
        let mut group = vec![start];
        let mut next = vec![start];
        while let Some((x, y)) = next.pop() {
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let near = (x + dx, y + dy);
                    if gold.contains(&near) && seen.insert(near) {
                        group.push(near);
                        next.push(near);
                    }
                }
            }
        }
        if group.len() > largest.len() {
            largest = group;
        }
    }
    largest
        .into_iter()
        .map(|(x, y)| (x as f32, y as f32))
        .collect()
}

/// The middle of the sun's pixels.
fn sun(buffer: &Buffer, area: Rect) -> (f32, f32) {
    let gold = sun_pixels(buffer, area);
    assert!(!gold.is_empty(), "no sun in {area:?}");
    let count = gold.len() as f32;
    let (x, y) = gold
        .iter()
        .fold((0.0, 0.0), |sum, pixel| (sum.0 + pixel.0, sum.1 + pixel.1));
    (x / count, y / count)
}

/// Whether a body, a planet by its index or the Moon, stands far enough
/// from the sun and from every other body that no other can share a cell
/// with its middle. A cell reaches a cell's height past the two pixels round
/// the middle that a check reads.
fn stands_clear(bodies: &[(f32, f32)], index: usize, centre: (f32, f32), reach: f32) -> bool {
    let span = |body: usize| match body {
        MOON => MOON_SPAN,
        EARTH => EARTH_SPAN,
        _ => PLANET_SPAN,
    };
    let place = bodies[index];
    distance(place, centre) > reach + 2.0 + CELL_HEIGHT as f32
        && bodies.iter().enumerate().all(|(other, near)| {
            other == index
                || (near.0 - place.0).abs() > span(index).0 + span(other).0 + 1.0
                || (near.1 - place.1).abs() > span(index).1 + span(other).1 + 1.0
        })
}

fn distance(from: (f32, f32), to: (f32, f32)) -> f32 {
    (to.0 - from.0).hypot(to.1 - from.1)
}

/// The middle of the pane, in pixels.
fn middle(area: Rect) -> (f32, f32) {
    (
        (f32::from(area.width) * CELL_WIDTH as f32 - 1.0) / 2.0,
        (f32::from(area.height) * CELL_HEIGHT as f32 - 1.0) / 2.0,
    )
}

/// Each planet keeps to one ellipse round the middle of the pane, inside
/// the ellipse of the next.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders long enough for Neptune to go round twice.
/// Then: each planet's place stays on one ellipse centred on the middle of
/// the pane, where the sun stands, each
/// ellipse lies inside the next, every frame draws each planet where it is,
/// and every pixel of a dotted orbit lies on one of the ellipses.
///
/// Why it matters: a planet that drifts off its ellipse reads as a fault,
/// and an orbit drawn where no planet goes reads as a stray line.
#[test]
fn each_planet_keeps_to_its_own_ellipse() {
    for seed in 0..3 {
        // Given:
        let mut scene = ticked(seed, AREA, 1);
        let centre = middle(AREA);
        let mut paths: Vec<Vec<(f32, f32)>> = vec![Vec::new(); 8];
        let mut orbit_pixels = Vec::new();

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
                    assert!(
                        pixels::shown(&buffer[cell])
                            .is_some_and(|cell| cell.iter().any(|colour| *colour != PALETTE.base)),
                        "seed {seed}: planet {index} not drawn at tick {tick}"
                    );
                }
                orbit_pixels.extend(
                    lit_pixels(&buffer, AREA)
                        .into_iter()
                        .filter(|(_, colour)| *colour == PALETTE.muted)
                        .map(|(pixel, _)| pixel),
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
        assert!(!orbit_pixels.is_empty(), "seed {seed}: no orbit drawn");
        for pixel in &orbit_pixels {
            let on_one = ellipses.iter().any(|(across, down)| {
                let (dx, dy) = ((pixel.0 - centre.0) / across, (pixel.1 - centre.1) / down);
                ((dx * dx + dy * dy).sqrt() - 1.0).abs() * down <= 1.5
            });
            assert!(on_one, "seed {seed}: orbit pixel {pixel:?} on no ellipse");
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
        Settings::DEFAULT,
    );
    scene.tick(AREA);

    // Then:
    let mut buffer = Buffer::empty(AREA);
    scene.render(AREA, &mut buffer);
    assert!(
        lit_pixels(&buffer, AREA)
            .iter()
            .any(|(_, colour)| *colour == PALETTE.gold)
    );
}

/// Each planet draws in its own colour, the Moon goes round Earth, and
/// Saturn wears a flat ring.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders for a minute and a half.
/// Then: where a planet stands clear of the sun and of every other body,
/// the pixel at its middle shows its own colour. The Moon stays within a
/// few pixels of Earth, goes round it more than twice, and shows in its
/// colour where it stands clear. Where Saturn stands clear of every other body, its gold reaches
/// past its body on both sides at its own height.
///
/// Why it matters: planets in one colour read as dots and not as a solar
/// system, a Moon that wanders off reads as a stray planet, and a ring that
/// stands up reads as a fault.
#[test]
fn each_planet_has_its_colour_the_moon_and_the_ring() {
    // Given:
    let mut scene = ticked(7, AREA, 1);
    let centre = sun(&render(&scene, AREA, AREA), AREA);
    let reach = sun_pixels(&render(&scene, AREA, AREA), AREA)
        .iter()
        .map(|pixel| distance(*pixel, centre))
        .fold(0.0, f32::max);
    let mut checked = [0; 8];
    let mut moon_checked = 0;
    let mut ring_checked = 0;
    let mut moon_turned = 0.0_f32;
    let earth_angle =
        |moon: (f32, f32), earth: (f32, f32)| (moon.1 - earth.1).atan2(moon.0 - earth.0);
    let mut moon_last = earth_angle(scene.moon(), scene.planets()[EARTH]);

    for tick in 0..1500 {
        // When:
        scene.tick(AREA);
        let buffer = render(&scene, AREA, AREA);
        let planets = scene.planets();
        let moon = scene.moon();

        // Then:
        let mut bodies: Vec<(f32, f32)> = planets.clone();
        bodies.push(moon);
        let clear = |index: usize| stands_clear(&bodies, index, centre, reach);
        for (index, role) in roles().iter().enumerate() {
            if clear(index) {
                assert_eq!(
                    colour_at(&buffer, planets[index]),
                    Some(*role),
                    "planet {index} at tick {tick}"
                );
                checked[index] += 1;
            }
        }
        assert!(
            distance(moon, planets[EARTH]) <= 7.5,
            "the Moon {moon:?} far from Earth {:?} at tick {tick}",
            planets[EARTH]
        );
        if clear(MOON) {
            assert_eq!(
                colour_at(&buffer, moon),
                Some(PALETTE.subtle),
                "the Moon at tick {tick}"
            );
            moon_checked += 1;
        }
        let now = earth_angle(moon, planets[EARTH]);
        let mut step = now - moon_last;
        if step > TAU / 2.0 {
            step -= TAU;
        } else if step < -TAU / 2.0 {
            step += TAU;
        }
        moon_turned += step;
        moon_last = now;

        let saturn = planets[SATURN];
        let saturn_clear = bodies
            .iter()
            .enumerate()
            .all(|(other, place)| other == SATURN || distance(*place, saturn) > SATURN_REACH + 8.0);
        if saturn_clear {
            let gold = lit_pixels(&buffer, AREA);
            let reaches = |side: f32| {
                gold.iter().any(|(pixel, colour)| {
                    let across = side * (pixel.0 - saturn.0);
                    *colour == PALETTE.gold
                        && (pixel.1 - saturn.1).abs() <= 1.0
                        && (6.5..=SATURN_REACH).contains(&across)
                })
            };
            assert!(
                reaches(-1.0) && reaches(1.0),
                "no flat ring round Saturn {saturn:?} at tick {tick}"
            );
            ring_checked += 1;
        }
    }
    for (index, count) in checked.iter().enumerate() {
        assert!(*count > 50, "planet {index} checked {count} times");
    }
    assert!(moon_checked > 50, "the Moon checked {moon_checked} times");
    assert!(ring_checked > 50, "the ring checked {ring_checked} times");
    assert!(
        moon_turned.abs() > 2.0 * TAU,
        "the Moon turned {moon_turned}"
    );
}

/// Mercury passes in front of the sun and behind it, and in front it shows
/// with the sun in one cell.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders while Mercury goes round several times.
/// Then: where Mercury stands well inside the sun's disc, the pixel at its
/// middle shows Mercury on the near side, the lower half, and the sun on
/// the far side. On the near side some cell at Mercury's edge shows both
/// Mercury and the sun.
///
/// Why it matters: a planet seen through the sun breaks the depth of the
/// tilted view, a planet that vanishes in front of it reads as a fault, and
/// a cell in one colour cuts a square notch out of the sun round it.
#[test]
fn mercury_passes_in_front_of_the_sun_and_behind_it() {
    // Given:
    let mut scene = ticked(7, AREA, 1);
    let first = render(&scene, AREA, AREA);
    let centre = sun(&first, AREA);
    let reach = sun_pixels(&first, AREA)
        .iter()
        .map(|pixel| distance(*pixel, centre))
        .fold(0.0, f32::max);
    let (mut in_front, mut behind, mut shared) = (0, 0, 0);

    for tick in 0..1000 {
        // When:
        scene.tick(AREA);
        let buffer = render(&scene, AREA, AREA);
        let mercury = scene.planets()[0];

        // Then:
        if distance(mercury, centre) > reach - 3.0 {
            continue;
        }
        let shown = colour_at(&buffer, mercury);
        if mercury.1 > centre.1 {
            assert_eq!(
                shown,
                Some(PALETTE.subtle),
                "Mercury in front at tick {tick}"
            );
            in_front += 1;
            let both = AREA.positions().any(|position| {
                let near = (
                    (f32::from(position.x) + 0.5) * CELL_WIDTH as f32,
                    (f32::from(position.y) + 0.5) * CELL_HEIGHT as f32,
                );
                let cell = pixels::shown(&buffer[position]);
                distance(near, mercury) <= 4.0
                    && cell.is_some_and(|cell| {
                        cell.contains(&PALETTE.subtle) && cell.contains(&PALETTE.gold)
                    })
            });
            shared += usize::from(both);
        } else {
            assert_eq!(shown, Some(PALETTE.gold), "Mercury behind at tick {tick}");
            behind += 1;
        }
    }
    assert!(
        in_front > 0 && behind > 0,
        "{in_front} in front, {behind} behind"
    );
    assert!(shared > 0, "Mercury never shared a cell with the sun");
}

/// Each planet leaves a short trail behind it that thins toward its end.
/// Given: the scene at a hundred by thirty, ticked until Neptune has gone
/// round once, so each orbit's size is known.
/// When: it ticks and renders for a while longer.
/// Then: every pixel in a planet's own colour outside its disc lies on its
/// orbit, behind it, and within an eighth of a turn. The second quarter of
/// the trail holds more than twice the pixels of the last quarter, and the
/// last quarter is not empty.
///
/// Why it matters: a trail ahead of a planet or off its orbit reads as a
/// fault, and a trail that ends in a hard edge reads as a second body.
#[test]
fn each_planet_leaves_a_trail_that_thins_behind_it() {
    // Given:
    let mut scene = ticked(7, AREA, 0);
    let mut paths: Vec<Vec<(f32, f32)>> = vec![Vec::new(); 8];
    for _ in 0..1600 {
        scene.tick(AREA);
        for (path, planet) in paths.iter_mut().zip(scene.planets()) {
            path.push(planet);
        }
    }
    let orbits: Vec<((f32, f32), f32, f32)> = paths
        .iter()
        .map(|path| {
            let (left, right) = path
                .iter()
                .fold((f32::MAX, f32::MIN), |(l, r), p| (l.min(p.0), r.max(p.0)));
            let (top, bottom) = path
                .iter()
                .fold((f32::MAX, f32::MIN), |(t, b), p| (t.min(p.1), b.max(p.1)));
            (
                ((left + right) / 2.0, (top + bottom) / 2.0),
                (right - left) / 2.0,
                (bottom - top) / 2.0,
            )
        })
        .collect();
    let angle = |index: usize, place: (f32, f32)| {
        let (centre, across, down) = orbits[index];
        (-(place.1 - centre.1) / down).atan2((place.0 - centre.0) / across)
    };
    let eighth = TAU / 8.0;
    let mut quarters = [[0; 4]; 8];

    for tick in 0..600 {
        // When:
        scene.tick(AREA);
        let buffer = render(&scene, AREA, AREA);
        let planets = scene.planets();

        // Then:
        let lit = lit_pixels(&buffer, AREA);
        for index in OWN_COLOUR {
            let planet = planets[index];
            let role = roles()[index];
            let (centre, across, down) = orbits[index];
            for (pixel, _) in lit
                .iter()
                .filter(|(pixel, colour)| *colour == role && distance(*pixel, planet) > BODY_REACH)
            {
                let at = angle(index, *pixel);
                let on_orbit = (centre.0 + across * at.cos(), centre.1 - down * at.sin());
                assert!(
                    distance(*pixel, on_orbit) <= 1.5,
                    "planet {index}: {pixel:?} off its orbit at tick {tick}"
                );
                let lag = (angle(index, planet) - at).rem_euclid(TAU);
                assert!(
                    lag <= eighth + 0.1,
                    "planet {index}: {pixel:?} is {lag} behind it at tick {tick}"
                );
                quarters[index][((lag / eighth * 4.0) as usize).min(3)] += 1;
            }
        }
    }
    for index in OWN_COLOUR {
        let [_, second, _, last] = quarters[index];
        assert!(
            last > 0 && second > 2 * last,
            "planet {index}: quarters {:?}",
            quarters[index]
        );
    }
}

/// Each planet stands out from its trail as a solid disc.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders for a minute and a half.
/// Then: where a planet stands clear of the sun and of every other body,
/// every pixel within two pixels of its middle shows its colour, so its disc
/// is solid and at least four pixels across and down.
///
/// Why it matters: a planet drawn as a few pixels reads as one more pixel of
/// its own trail.
#[test]
fn each_planet_is_a_solid_disc() {
    // Given:
    let mut scene = ticked(7, AREA, 1);
    let first = render(&scene, AREA, AREA);
    let centre = sun(&first, AREA);
    let reach = sun_pixels(&first, AREA)
        .iter()
        .map(|pixel| distance(*pixel, centre))
        .fold(0.0, f32::max);
    let mut checked = [0; 8];

    for tick in 0..1500 {
        // When:
        scene.tick(AREA);
        let buffer = render(&scene, AREA, AREA);
        let planets = scene.planets();

        // Then:
        let mut bodies = planets.clone();
        bodies.push(scene.moon());
        for (index, role) in roles().iter().enumerate() {
            if !stands_clear(&bodies, index, centre, reach) {
                continue;
            }
            let middle = planets[index];
            for dy in -2..=2 {
                for dx in -2..=2 {
                    let place = (middle.0.floor() + dx as f32, middle.1.floor() + dy as f32);
                    if distance(place, middle) > 2.0 {
                        continue;
                    }
                    assert_eq!(
                        colour_at(&buffer, place),
                        Some(*role),
                        "planet {index} at {middle:?} has a hole at {place:?} at tick {tick}"
                    );
                }
            }
            checked[index] += 1;
        }
    }
    for (index, count) in checked.iter().enumerate() {
        assert!(*count > 50, "planet {index} checked {count} times");
    }
}

/// The planets and the Moon still move evenly after a week of running.
/// Given: the scene at a hundred by thirty, with the size of Mercury's orbit
/// and of the Moon's orbit round Earth measured over their first turns.
/// When: it ticks for a week at the tick rate.
/// Then: each tick still turns Mercury, and the Moon round Earth, by the
/// same angle as at the start, to within a twentieth.
///
/// Why it matters: a screensaver runs for days, and a planet that jerks
/// forward and stalls reads as a fault.
#[test]
fn the_planets_move_evenly_after_a_week() {
    // Given:
    let mut scene = ticked(7, AREA, 0);
    let bodies = |scene: &Solar| {
        let earth = scene.planets()[EARTH];
        let moon = scene.moon();
        [scene.planets()[0], (moon.0 - earth.0, moon.1 - earth.1)]
    };
    let mut early: Vec<[(f32, f32); 2]> = Vec::new();
    for _ in 0..200 {
        scene.tick(AREA);
        early.push(bodies(&scene));
    }
    let ellipses: Vec<((f32, f32), f32, f32)> = (0..2)
        .map(|body| {
            let (left, right) = early.iter().fold((f32::MAX, f32::MIN), |(l, r), p| {
                (l.min(p[body].0), r.max(p[body].0))
            });
            let (top, bottom) = early.iter().fold((f32::MAX, f32::MIN), |(t, b), p| {
                (t.min(p[body].1), b.max(p[body].1))
            });
            (
                ((left + right) / 2.0, (top + bottom) / 2.0),
                (right - left) / 2.0,
                (bottom - top) / 2.0,
            )
        })
        .collect();
    let steps = |path: &[[(f32, f32); 2]], body: usize| -> Vec<f32> {
        let (centre, across, down) = ellipses[body];
        let angles: Vec<f32> = path
            .iter()
            .map(|p| (-(p[body].1 - centre.1) / down).atan2((p[body].0 - centre.0) / across))
            .collect();
        angles
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).rem_euclid(TAU))
            .collect()
    };
    let week = 7 * 24 * 60 * 60 * 1000 / TICK_RATE.as_millis() as u32;

    // When:
    for _ in 200..week {
        scene.tick(AREA);
    }
    let mut late = Vec::new();
    for _ in 0..40 {
        scene.tick(AREA);
        late.push(bodies(&scene));
    }

    // Then:
    for body in 0..2 {
        let first = steps(&early, body);
        let even = first.iter().sum::<f32>() / first.len() as f32;
        for step in steps(&late, body) {
            assert!(
                (step / even - 1.0).abs() < 0.05,
                "body {body} turns {step} a tick, not {even}"
            );
        }
    }
}
