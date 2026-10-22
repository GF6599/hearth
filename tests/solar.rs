#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pixels;

use std::collections::HashSet;
use std::f32::consts::TAU;
use std::sync::Arc;

use hearth::config::Background;
use hearth::corpus::Corpus;
use hearth::octant::{self, CELL_HEIGHT, CELL_WIDTH};
use hearth::scene::{Rng, Scene, SceneKind, Settings, TICK_RATE};
use hearth::scenes::solar::Solar;
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette, luma};
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
/// The least share of its colour the night side of a planet keeps, from
/// the overlay toward its colour.
const NIGHT_SHARE: f32 = 0.4;
/// The planets that share their colour with no other body. Mercury shares
/// subtle with the Moon, and Saturn shares gold with the sun.
const OWN_COLOUR: [usize; 6] = [1, 2, 3, 4, 6, 7];
/// A place in pixels from the top left of the pane.
type Place = (f32, f32);
/// The lit pixels of a frame, and where the Moon was in it.
type Frame = (Vec<(Place, Color)>, Place);

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

/// The colour of the palette nearest `colour` in RGB, the role a reader
/// names it by.
fn named(colour: Color) -> Color {
    let roles = [
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
    roles
        .into_iter()
        .min_by(|one, other| {
            (1.0 - nearness(colour, *one)).total_cmp(&(1.0 - nearness(colour, *other)))
        })
        .unwrap_or(PALETTE.base)
}

/// How near `colour` lies to `role`, from 0 at the sky's distance from it
/// to 1 at the role itself, by distance in RGB. Past the sky's distance it
/// falls below 0.
fn nearness(colour: Color, role: Color) -> f32 {
    let channels = |colour: Color| match colour {
        Color::Rgb(r, g, b) => [r, g, b].map(f32::from),
        _ => [0.0; 3],
    };
    let apart = |one: Color, other: Color| {
        let (one, other) = (channels(one), channels(other));
        (0..3)
            .map(|axis| (one[axis] - other[axis]).powi(2))
            .sum::<f32>()
            .sqrt()
    };
    1.0 - apart(colour, role) / apart(PALETTE.base, role).max(1.0)
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

/// The colour the pixel nearest a place in pixels shows, in a render whose
/// area starts at the top left of its buffer. The centre of the pixel at
/// (x, y) lies at (x, y).
fn colour_at(buffer: &Buffer, place: (f32, f32)) -> Option<Color> {
    let (x, y) = (place.0.round() as usize, place.1.round() as usize);
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

/// Whether a place lies in a cell that a body, a planet by its index or the
/// Moon after them, can colour. A cell shows two colours, so a body can
/// change the pixels of every cell it touches.
fn near_body(place: (f32, f32), bodies: &[(f32, f32)]) -> bool {
    bodies.iter().enumerate().any(|(body, at)| {
        let reach = if body == SATURN {
            SATURN_REACH
        } else {
            BODY_REACH
        };
        (place.0 - at.0).abs() <= reach + CELL_WIDTH as f32
            && (place.1 - at.1).abs() <= reach + CELL_HEIGHT as f32
    })
}

/// The middle of the pane, in pixels.
fn middle(area: Rect) -> (f32, f32) {
    (
        (f32::from(area.width) * CELL_WIDTH as f32 - 1.0) / 2.0,
        (f32::from(area.height) * CELL_HEIGHT as f32 - 1.0) / 2.0,
    )
}

/// Each planet keeps to one ellipse round the middle of the pane, inside
/// the ellipse of the next, and each orbit is a soft hairline.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders long enough for Neptune to go round twice.
/// Then: each planet's place stays on one ellipse centred on the middle of
/// the pane, where the sun stands, each ellipse lies inside the next, every
/// frame draws each planet where it is, and every lit pixel away from the
/// bodies and the sun's glow lies on one of the ellipses. Away from every
/// trail, each of those pixels is a partial mix, lighter than the sky and
/// darker than muted, and the orbits take at least three such shades.
///
/// Why it matters: a planet that drifts off its ellipse reads as a fault,
/// an orbit drawn where no planet goes reads as a stray line, and an orbit
/// of whole pixels in one colour reads as a dotted row of blocks.
#[test]
fn each_planet_keeps_to_its_own_ellipse() {
    for seed in 0..3 {
        // Given:
        let mut scene = ticked(seed, AREA, 1);
        let centre = middle(AREA);
        let glow = 2.0
            * sun_pixels(&render(&scene, AREA, AREA), AREA)
                .iter()
                .map(|pixel| distance(*pixel, centre))
                .fold(0.0, f32::max);
        let mut paths: Vec<Vec<(f32, f32)>> = vec![Vec::new(); 8];
        let mut orbit_pixels: Vec<(Place, Color, Vec<Place>)> = Vec::new();

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
                let mut bodies = planets.clone();
                bodies.push(scene.moon());
                orbit_pixels.extend(
                    lit_pixels(&buffer, AREA)
                        .into_iter()
                        .filter(|(pixel, _)| {
                            distance(*pixel, centre) > glow && !near_body(*pixel, &bodies)
                        })
                        .map(|(pixel, colour)| (pixel, colour, planets.clone())),
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
        let angle = |place: (f32, f32), (across, down): (f32, f32)| {
            (-(place.1 - centre.1) / down).atan2((place.0 - centre.0) / across)
        };
        let mut shades = HashSet::new();
        for (pixel, colour, planets) in &orbit_pixels {
            let on = ellipses.iter().position(|(across, down)| {
                let (dx, dy) = ((pixel.0 - centre.0) / across, (pixel.1 - centre.1) / down);
                ((dx * dx + dy * dy).sqrt() - 1.0).abs() * down <= 1.5
            });
            let Some(on) = on else {
                panic!("seed {seed}: orbit pixel {pixel:?} on no ellipse");
            };
            let lag =
                (angle(planets[on], ellipses[on]) - angle(*pixel, ellipses[on])).rem_euclid(TAU);
            if lag <= TAU / 8.0 + 0.1 {
                continue;
            }
            assert!(
                luma(PALETTE.base) < luma(*colour) && luma(*colour) < luma(PALETTE.muted),
                "seed {seed}: orbit pixel {pixel:?} in {colour:?} is no partial mix"
            );
            shades.insert(*colour);
        }
        assert!(
            shades.len() >= 3,
            "seed {seed}: the orbits take {} shades",
            shades.len()
        );
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
/// some pixel within two pixels of its middle shows a colour nearer its own
/// than any other of the palette.
/// The Moon stays within a few pixels of Earth, goes round it more than
/// twice, and where it stands clear some pixel within a pixel of its middle
/// lies at least halfway from the sky to its colour. Where Saturn stands clear of
/// every other body, a colour at least halfway from the sky to gold reaches
/// past its body on both sides at its own height.
///
/// Why it matters: planets in one colour read as dots and not as a solar
/// system, a Moon that wanders off reads as a stray planet, and a ring that
/// stands up reads as a fault.
#[test]
fn each_planet_has_its_colour_the_moon_and_the_ring() {
    let shows = |buffer: &Buffer, middle: (f32, f32), reach: f32, keep: &dyn Fn(Color) -> bool| {
        (-2..=2).any(|dy| {
            (-2..=2).any(|dx| {
                let place = (middle.0.round() + dx as f32, middle.1.round() + dy as f32);
                distance(place, middle) <= reach && colour_at(buffer, place).is_some_and(keep)
            })
        })
    };
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
                assert!(
                    shows(&buffer, planets[index], 2.0, &|colour| named(colour)
                        == *role),
                    "planet {index} at {:?} at tick {tick}",
                    planets[index]
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
            assert!(
                shows(&buffer, moon, 1.0, &|colour| nearness(
                    colour,
                    PALETTE.subtle
                ) >= 0.5),
                "the Moon at {moon:?} at tick {tick}"
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
                    nearness(*colour, PALETTE.gold) >= 0.5
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
/// middle shows a colour nearest Mercury's on the near side, the lower
/// half, and the sun on the far side. On the near side some cell at
/// Mercury's edge shows both a colour nearest Mercury's and the sun.
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
            assert!(
                shown.map(named) == Some(PALETTE.subtle),
                "Mercury in front in {shown:?} at tick {tick}"
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
                        cell.iter().any(|colour| named(*colour) == PALETTE.subtle)
                            && cell.contains(&PALETTE.gold)
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

/// Each planet leaves a short trail behind it that fades along its length.
/// Given: the scene at a hundred by thirty, ticked until Neptune has gone
/// round once, so each orbit's size is known.
/// When: it ticks and renders for a while longer.
/// Then: along an eighth of a turn behind each planet, every place on its
/// orbit away from the bodies and the sun's glow has a lit pixel within a
/// pixel of it. Read by the pixel there nearest the planet's colour, each
/// planet's trail lies nearer its colour in each quarter of its length
/// than in the next, and nearer in its last quarter than its orbit ahead
/// of it. A quarter that never stands clear of the bodies is left out.
///
/// Why it matters: a trail ahead of a planet reads as a fault, a trail that
/// ends in a hard edge reads as a second body, and a trail of scattered
/// pixels reads as noise.
#[test]
fn each_planet_leaves_a_trail_that_fades_behind_it() {
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
    let first = render(&scene, AREA, AREA);
    let glow = 2.0
        * sun_pixels(&first, AREA)
            .iter()
            .map(|pixel| distance(*pixel, middle(AREA)))
            .fold(0.0, f32::max);
    let eighth = TAU / 8.0;
    let places = 32;
    let (mut behind, mut ahead) = ([[(0.0_f32, 0_u32); 4]; 8], [(0.0_f32, 0_u32); 8]);

    for tick in 0..600 {
        // When:
        scene.tick(AREA);
        let buffer = render(&scene, AREA, AREA);
        let mut bodies = scene.planets();
        bodies.push(scene.moon());

        // Then:
        for index in OWN_COLOUR {
            let role = roles()[index];
            let (centre, across, down) = orbits[index];
            let planet = bodies[index];
            let now = (-(planet.1 - centre.1) / down).atan2((planet.0 - centre.0) / across);
            for step in 1..=places {
                let lag = eighth * step as f32 / places as f32;
                for (turn, behind_it) in [(now - lag, true), (now + lag, false)] {
                    let place = (centre.0 + across * turn.cos(), centre.1 - down * turn.sin());
                    let away = distance(place, middle(AREA)) > glow && !near_body(place, &bodies);
                    if !away {
                        continue;
                    }
                    let nearest = (-1..=1)
                        .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
                        .map(|(dx, dy)| (place.0.round() + dx as f32, place.1.round() + dy as f32))
                        .filter(|pixel| distance(*pixel, place) <= 1.0)
                        .filter_map(|pixel| colour_at(&buffer, pixel))
                        .filter(|colour| *colour != PALETTE.base)
                        .map(|colour| nearness(colour, role))
                        .fold(None, |best: Option<f32>, each| {
                            Some(best.map_or(each, |best| best.max(each)))
                        });
                    let Some(nearest) = nearest else {
                        panic!("planet {index}: no orbit at {place:?} at tick {tick}");
                    };
                    let bucket = if behind_it {
                        &mut behind[index][((step - 1) * 4 / places) as usize]
                    } else {
                        &mut ahead[index]
                    };
                    bucket.0 += nearest;
                    bucket.1 += 1;
                }
            }
        }
    }
    let mean = |(sum, count): (f32, u32)| sum / count as f32;
    for index in OWN_COLOUR {
        let quarters: Vec<f32> = behind[index]
            .into_iter()
            .filter(|(_, count)| *count > 0)
            .map(mean)
            .collect();
        assert!(!quarters.is_empty(), "planet {index}: no trail read");
        for pair in quarters.windows(2) {
            assert!(pair[0] > pair[1], "planet {index}: quarters {quarters:?}");
        }
        if ahead[index].1 > 0 {
            let (last, ahead) = (quarters[quarters.len() - 1], mean(ahead[index]));
            assert!(
                last > ahead,
                "planet {index}: last quarter {last} against {ahead} ahead"
            );
        }
    }
}

/// The sun glows into the sky round it, fading with distance.
/// Given: the scene at four hundred by a hundred and twenty, where the
/// glow spans many pixels.
/// When: it ticks and renders at three moments.
/// Then: from a pixel past the sun's disc to half its radius further, every
/// pixel away from the bodies is lighter than the sky, and the mean
/// lightness of each ring two pixels wide falls from the sun outward, from
/// below the sun's own.
///
/// Why it matters: a sun with a hard edge on a flat sky reads as a flat
/// disc and not as a light.
#[test]
fn the_sun_glows_into_the_sky() {
    // Given:
    let mut scene = ticked(7, GLIDE_AREA, 1);

    for _ in 0..3 {
        // When:
        for _ in 0..150 {
            scene.tick(GLIDE_AREA);
        }
        let buffer = render(&scene, GLIDE_AREA, GLIDE_AREA);
        let centre = sun(&buffer, GLIDE_AREA);
        let radius = sun_pixels(&buffer, GLIDE_AREA)
            .iter()
            .map(|pixel| distance(*pixel, centre))
            .fold(0.0, f32::max);
        let mut bodies = scene.planets();
        bodies.push(scene.moon());

        // Then:
        let (inner, outer) = (radius + 1.0, radius * 1.5);
        let mut rings = vec![(0_u64, 0_u64); ((outer - inner) / 2.0) as usize + 1];
        for (pixel, colour) in pixels::showing(&buffer, GLIDE_AREA, |_| true) {
            let place = (pixel.0 as f32, pixel.1 as f32);
            let out = distance(place, centre);
            if !(inner..outer).contains(&out) || near_body(place, &bodies) {
                continue;
            }
            assert!(
                luma(colour) > luma(PALETTE.base),
                "{place:?} in {colour:?} is no lighter than the sky"
            );
            let ring = &mut rings[((out - inner) / 2.0) as usize];
            ring.0 += u64::from(luma(colour));
            ring.1 += 1;
        }
        let means: Vec<f64> = rings
            .iter()
            .filter(|(_, count)| *count > 0)
            .map(|(sum, count)| *sum as f64 / *count as f64)
            .collect();
        assert!(means.len() > 10, "{} rings", means.len());
        assert!(
            means[0] < f64::from(luma(PALETTE.gold)),
            "the first ring is {} against the sun's {}",
            means[0],
            luma(PALETTE.gold)
        );
        for pair in means.windows(2) {
            assert!(pair[0] > pair[1], "ring lightness {means:?}");
        }
    }
}

/// Each planet is lit on the side that faces the sun.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders for a minute and a half.
/// Then: over the frames where a planet stands clear of the sun and of
/// every other body, its pixels within two pixels of its middle are
/// lighter on average on the side toward the sun than on the far side.
///
/// Why it matters: a planet in one flat colour reads as a coloured dot and
/// not as a lit world.
#[test]
fn each_planet_is_lit_on_the_side_that_faces_the_sun() {
    // Given:
    let mut scene = ticked(7, AREA, 1);
    let first = render(&scene, AREA, AREA);
    let centre = sun(&first, AREA);
    let reach = sun_pixels(&first, AREA)
        .iter()
        .map(|pixel| distance(*pixel, centre))
        .fold(0.0, f32::max);
    let mut checked = [0; 8];
    let (mut near, mut far) = ([(0_u64, 0_u64); 8], [(0_u64, 0_u64); 8]);

    for _ in 0..1500 {
        // When:
        scene.tick(AREA);
        let buffer = render(&scene, AREA, AREA);
        let planets = scene.planets();

        let mut bodies = planets.clone();
        bodies.push(scene.moon());
        for index in 0..planets.len() {
            if !stands_clear(&bodies, index, centre, reach) {
                continue;
            }
            let middle = planets[index];
            let length = distance(middle, centre);
            let toward = (
                (centre.0 - middle.0) / length,
                (centre.1 - middle.1) / length,
            );
            for dy in -2..=2 {
                for dx in -2..=2 {
                    let place = (middle.0.round() + dx as f32, middle.1.round() + dy as f32);
                    let Some(colour) =
                        colour_at(&buffer, place).filter(|_| distance(place, middle) <= 2.0)
                    else {
                        continue;
                    };
                    let side = (place.0 - middle.0) * toward.0 + (place.1 - middle.1) * toward.1;
                    let half = if side > 0.5 {
                        &mut near[index]
                    } else if side < -0.5 {
                        &mut far[index]
                    } else {
                        continue;
                    };
                    *half = (half.0 + u64::from(luma(colour)), half.1 + 1);
                }
            }
            checked[index] += 1;
        }
    }

    // Then:
    for (index, count) in checked.iter().enumerate() {
        assert!(*count > 50, "planet {index} checked {count} times");
        let mean = |(sum, count): (u64, u64)| sum as f64 / count.max(1) as f64;
        assert!(
            mean(near[index]) > mean(far[index]),
            "planet {index} is {} toward the sun and {} on the far side",
            mean(near[index]),
            mean(far[index])
        );
    }
}

/// Each planet stands out from its trail as a solid disc.
/// Given: the scene at a hundred by thirty.
/// When: it ticks and renders for a minute and a half.
/// Then: where a planet stands clear of the sun and of every other body,
/// every pixel within two pixels of its middle is lighter than the sky,
/// and on average those pixels are at least as light as the planet's night
/// shade, `NIGHT_SHARE` of the way from the overlay toward its colour. So
/// its disc is solid and at least four pixels across and down. A cell shows
/// two colours, so one pixel of the night side can take the colour of the
/// faint edge beside it, and the mean reads the disc as a whole.
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
            let mut inner = Vec::new();
            for dy in -2..=2 {
                for dx in -2..=2 {
                    let place = (middle.0.floor() + dx as f32, middle.1.floor() + dy as f32);
                    if distance(place, middle) > 2.0 {
                        continue;
                    }
                    let shown = colour_at(&buffer, place);
                    assert!(
                        shown.is_some_and(|colour| luma(colour) > luma(PALETTE.base)),
                        "planet {index} at {middle:?} has a hole at {place:?} in {shown:?} at tick {tick}"
                    );
                    inner.push(luma(shown.unwrap_or(PALETTE.base)));
                }
            }
            let night = luma(octant::mix(PALETTE.overlay, *role, NIGHT_SHARE));
            let mean = inner.iter().sum::<u32>() / inner.len().max(1) as u32;
            assert!(
                mean >= night,
                "planet {index} at {middle:?} is {mean} light inside, under its night shade {night}, at tick {tick}"
            );
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

/// A pane wide enough that Mercury moves about four pixels a tick, so a
/// frame between two ticks has room to show it between them.
const GLIDE_AREA: Rect = Rect::new(0, 0, 400, 120);
/// The shares of a tick that the frames between two ticks show.
const BETWEEN: [f64; 3] = [0.25, 0.5, 0.75];

/// How far along `heading` a planet's disc lies: the mean place of the lit
/// pixels near `from` and more than a pixel and a half to either side of
/// the line along `heading`, each weighed by how much lighter than the sky
/// it is. Only the disc reaches that far from its orbit, and a pixel at its
/// soft edge weighs by how much of it the disc covers. The pixels of cells
/// the Moon, at `moon`, can colour are left out.
fn front((lit, moon): &Frame, from: (f32, f32), reach: f32, heading: (f32, f32)) -> f32 {
    let (sum, weight) = lit
        .iter()
        .copied()
        .filter(|(pixel, _)| {
            let (dx, dy) = (pixel.0 - from.0, pixel.1 - from.1);
            distance(*pixel, from) <= reach
                && (dx * heading.1 - dy * heading.0).abs() > 1.5
                && distance(*pixel, *moon) > MOON_SPAN.0 + CELL_HEIGHT as f32
        })
        .fold((0.0, 0.0), |(sum, weight), (pixel, colour)| {
            let lighter = (luma(colour) - luma(PALETTE.base)) as f32;
            (
                sum + lighter * (pixel.0 * heading.0 + pixel.1 * heading.1),
                weight + lighter,
            )
        });
    sum / weight.max(1.0)
}

/// Each planet glides along its orbit between two ticks.
/// Given: the scene at four hundred by a hundred and twenty, where the
/// inner planets move two pixels or more a tick.
/// When: it renders at a quarter, a half and three quarters of the way to
/// its next tick, then ticks.
/// Then: where a planet other than Saturn stands clear of the sun, its
/// glow and every other body, and moves two pixels or more from one tick to
/// the next, its disc never goes back over the frames between, so it stays
/// between its places at the two ticks, and at half a tick it lies strictly
/// between the two. A slower planet moves less than half a pixel a frame,
/// finer than a cell of two colours shows. Saturn's ring is wider than its
/// disc, so its pixels do not mark the disc.
///
/// Why it matters: at a quarter speed a planet that moves only on a tick
/// jumps once every sixteen frames, and the orbit reads as a stutter.
#[test]
fn each_planet_glides_along_its_orbit_between_ticks() {
    // Given:
    let mut scene = ticked(7, GLIDE_AREA, 1);
    let first = render(&scene, GLIDE_AREA, GLIDE_AREA);
    let centre = sun(&first, GLIDE_AREA);
    let reach = sun_pixels(&first, GLIDE_AREA)
        .iter()
        .map(|pixel| distance(*pixel, centre))
        .fold(0.0, f32::max);
    let mut checked = 0;

    for _ in 0..80 {
        for _ in 0..37 {
            scene.tick(GLIDE_AREA);
        }
        let before = (
            lit_pixels(&render(&scene, GLIDE_AREA, GLIDE_AREA), GLIDE_AREA),
            scene.moon(),
        );
        let mut bodies = scene.planets();
        bodies.push(scene.moon());

        // When:
        let glides: Vec<Frame> = BETWEEN
            .iter()
            .map(|&progress| {
                scene.set_progress(progress);
                (
                    lit_pixels(&render(&scene, GLIDE_AREA, GLIDE_AREA), GLIDE_AREA),
                    scene.moon(),
                )
            })
            .collect();
        scene.set_progress(0.0);
        scene.tick(GLIDE_AREA);
        let after = (
            lit_pixels(&render(&scene, GLIDE_AREA, GLIDE_AREA), GLIDE_AREA),
            scene.moon(),
        );
        let mut next = scene.planets();
        next.push(scene.moon());

        // Then:
        for index in 0..MOON {
            if index == SATURN
                || !stands_clear(&bodies, index, centre, reach)
                || !stands_clear(&next, index, centre, reach)
                || distance(bodies[index], centre) <= 2.0 * reach
            {
                continue;
            }
            let (from, to) = (bodies[index], next[index]);
            let moved = distance(from, to);
            let heading = ((to.0 - from.0) / moved, (to.1 - from.1) / moved);
            let near = BODY_REACH + moved;
            if moved < 2.0 {
                continue;
            }
            let fronts: Vec<f32> = std::iter::once(&before)
                .chain(&glides)
                .chain(std::iter::once(&after))
                .map(|lit| front(lit, from, near, heading))
                .collect();
            for pair in fronts.windows(2) {
                assert!(
                    pair[0] <= pair[1],
                    "planet {index} goes back from {from:?} to {to:?}: {fronts:?}"
                );
            }
            let (start, half, end) = (fronts[0], fronts[2], fronts[4]);
            assert!(
                start < half && half < end,
                "planet {index} not between {from:?} and {to:?} at half a tick: {fronts:?}"
            );
            checked += 1;
        }
    }
    assert!(checked > 20, "{checked} planets moved two pixels");
}

/// A frame on a tick is the frame the scene drew before it glided.
/// Given: two scenes from one seed, one of them given a share of a tick
/// before each tick and none after it.
/// When: both tick and render.
/// Then: every frame of the two is the same.
///
/// Why it matters: the frame on a tick is the one every other test of the
/// scene reads, so a glide must not move it.
#[test]
fn a_frame_on_a_tick_is_the_frame_before_the_glide() {
    // Given:
    let mut glided = ticked(7, AREA, 1);
    let mut still = ticked(7, AREA, 1);

    for tick in 0..200 {
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

/// Whether `colour` lies part way along the mix from `from` toward `to`,
/// neither end, within `levels` of a step of the mix in each channel.
fn part_way(colour: Color, from: Color, to: Color, levels: u8) -> bool {
    colour != from && colour != to && pixels::share_on(colour, from, to, 4096, levels).is_some()
}

/// Over a see-through base, the solar system mixes nothing into it.
/// Given: the scene on Rose Pine with a see-through base.
/// When: it ticks and renders for twenty seconds.
/// Then: no pixel lies part way along the mix from the base toward a colour
/// the scene draws over the sky: the sun's gold, the orbits' colour half
/// way from the overlay to muted, and the Moon's subtle. A pixel on a mix
/// the scene draws between two other colours is left out: a planet's
/// shades from the overlay toward its colour and its trail from the
/// orbits' colour toward it, within two levels, as a trail over an orbit
/// is a mix rounded twice. The greys of the theme lie near one line from
/// the base, so such a shade can lie near a mix from the base too.
///
/// Why it matters: there the base is the terminal's own background, and a
/// mix with it is a colour the user never chose, a smudge round every edge.
#[test]
fn over_a_see_through_base_nothing_mixes_with_it() {
    // Given:
    let palette = &theme::FLAT_THEMES[theme::position("rose-pine").unwrap()];
    let mut scene = Solar::new(Rng::seed_from_u64(7), palette);
    let orbit = octant::mix(palette.overlay, palette.muted, 0.5);
    let drawn = [palette.gold, orbit, palette.subtle];
    let roles = [
        palette.subtle,
        palette.text,
        palette.foam,
        palette.love,
        palette.rose,
        palette.gold,
        palette.pine,
        palette.iris,
    ];
    let shaded = |colour: Color| {
        roles.iter().any(|role| {
            [palette.overlay, orbit]
                .iter()
                .any(|from| colour == *role || part_way(colour, *from, *role, 2))
        })
    };

    for tick in 0..330 {
        // When:
        scene.tick(AREA);
        if tick % 30 != 0 {
            continue;
        }
        let mut buffer = Buffer::empty(AREA);
        scene.render(AREA, &mut buffer);

        // Then:
        for (pixel, colour) in pixels::showing(&buffer, AREA, |colour| colour != palette.base) {
            if shaded(colour) {
                continue;
            }
            assert!(
                !drawn
                    .iter()
                    .any(|to| part_way(colour, palette.base, *to, 1)),
                "tick {tick}: {pixel:?} in {colour:?} mixes the base"
            );
        }
    }
}
