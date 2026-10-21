#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pixels;

use std::collections::BTreeSet;
use std::sync::Arc;

use hearth::config::Background;
use hearth::corpus::Corpus;
use hearth::scene::{Rng, Scene, SceneKind, Settings};
use hearth::scenes::murmuration::{DOTS_PER_BIRD, FLOCK, HAWK_CLEARANCE, HAWK_EVERY, Murmuration};
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);

/// An octant cell is two dots wide and four tall.
const CELL_DOTS: (usize, usize) = (2, 4);

/// The colours a bird draws in, faint and bright, and the hawk's.
const FLIGHT: [Color; 3] = [
    theme::ROSE_PINE.subtle,
    theme::ROSE_PINE.text,
    theme::ROSE_PINE.love,
];

fn ticked(seed: u64, area: Rect, ticks: usize) -> Murmuration {
    let mut scene = Murmuration::new(Rng::seed_from_u64(seed), &theme::ROSE_PINE);
    for _ in 0..ticks {
        scene.tick(area);
    }
    scene
}

fn render(scene: &Murmuration, area: Rect, canvas: Rect) -> Buffer {
    let mut buffer = Buffer::empty(canvas);
    scene.render(area, &mut buffer);
    buffer
}

/// Each pixel of a render in one of `colours`, in dots from the top left
/// of the area.
fn dots_in(buffer: &Buffer, area: Rect, colours: &[Color]) -> BTreeSet<(usize, usize)> {
    let mut lit = BTreeSet::new();
    for position in area.positions() {
        let Some(shown) = pixels::shown(&buffer[position]) else {
            continue;
        };
        let column = usize::from(position.x - area.x) * CELL_DOTS.0;
        let row = usize::from(position.y - area.y) * CELL_DOTS.1;
        lit.extend(
            (0..8)
                .filter(|pixel| colours.contains(&shown[*pixel]))
                .map(|pixel| (column + pixel % CELL_DOTS.0, row + pixel / CELL_DOTS.0)),
        );
    }
    lit
}

/// Every lit dot of a render, a bird or the hawk, in dots from the top left
/// of the area.
fn lit_dots(buffer: &Buffer, area: Rect) -> BTreeSet<(usize, usize)> {
    dots_in(buffer, area, &FLIGHT)
}

fn dots_of(area: Rect) -> (f32, f32) {
    (
        f32::from(area.width) * CELL_DOTS.0 as f32,
        f32::from(area.height) * CELL_DOTS.1 as f32,
    )
}

fn assert_inside(scene: &Murmuration, area: Rect, when: &str) {
    let (width, height) = dots_of(area);
    for (x, y) in scene.birds() {
        assert!(
            (0.0..width).contains(&x) && (0.0..height).contains(&y),
            "{when}: bird at ({x}, {y}) outside {width} by {height} dots"
        );
    }
}

/// Given: a small, a mid-sized and a wide area.
/// When: the scene ticks once on each.
/// Then: the flock holds one bird for each `DOTS_PER_BIRD` dots, and no
/// fewer and no more than the bounds of `FLOCK`.
///
/// Why it matters: a fixed count is a speck on a wide terminal and a
/// solid mass in a picker preview.
#[test]
fn the_flock_grows_with_the_pane_within_its_bounds() {
    for area in [Rect::new(0, 0, 20, 5), AREA, Rect::new(0, 0, 300, 100)] {
        // When:
        let scene = ticked(7, area, 1);

        // Then:
        let dots = usize::from(area.width) * usize::from(area.height) * CELL_DOTS.0 * CELL_DOTS.1;
        let expected = (dots / DOTS_PER_BIRD).clamp(*FLOCK.start(), *FLOCK.end());
        assert_eq!(scene.birds().count(), expected, "{area:?}");
    }
}

/// Given: the scene on a hundred by thirty area, fifty ticks in.
/// When: it renders.
/// Then: the pixels lit in a bird's colour are the dots the birds sit on,
/// no more and no fewer.
///
/// Why it matters: a wrong bit in the octant table draws a bird one dot
/// off, and the folds of the flock smear.
#[test]
fn every_bird_shows_as_a_dot_and_every_dot_is_a_bird() {
    // Given:
    let scene = ticked(7, AREA, 50);

    // When:
    let shown = lit_dots(&render(&scene, AREA, AREA), AREA);

    // Then:
    let birds: BTreeSet<(usize, usize)> = scene
        .birds()
        .map(|(x, y)| (x as usize, y as usize))
        .collect();
    assert_eq!(shown, birds);
}

/// Given: the scene on a hundred by thirty area, a hundred ticks in.
/// When: it renders.
/// Then: every cell above the trees is octant pixels, with the birds of a
/// cell in one shade over the sky: subtle for one or two birds, and text
/// for three or more.
///
/// Why it matters: a cell holds the sky and one shade, so the birds in it
/// are what shades the flock from a faint edge to a bright core.
#[test]
fn a_cell_with_more_birds_is_brighter() {
    // Given, when:
    let buffer = render(&ticked(7, AREA, 100), AREA, AREA);

    // Then:
    let mut shades = BTreeSet::new();
    let first_tree = tree_rows(&buffer, &theme::ROSE_PINE)[0];
    for position in AREA.positions().filter(|position| position.y < first_tree) {
        let shown = pixels::shown(&buffer[position])
            .unwrap_or_else(|| panic!("{:?} at {position:?}", buffer[position].symbol()));
        let birds: Vec<Color> = shown
            .into_iter()
            .filter(|colour| FLIGHT.contains(colour))
            .collect();
        let expected: Color = match birds.len() {
            0 => continue,
            1 | 2 => theme::ROSE_PINE.subtle,
            _ => theme::ROSE_PINE.text,
        };
        assert!(
            birds.iter().all(|colour| *colour == expected),
            "{} birds in {birds:?} at {position:?}",
            birds.len()
        );
        shades.insert(birds.len() > 2);
    }
    assert_eq!(shades.len(), 2, "the frame shows only one shade");
}

/// Given: the scene on a hundred by thirty area.
/// When: it ticks a thousand times.
/// Then: after every tick every bird is inside the pane.
///
/// Why it matters: a bird that leaves is cut from the frame, and the flock
/// thins until the screen is empty.
#[test]
fn the_flock_stays_in_the_pane() {
    let mut scene = Murmuration::new(Rng::seed_from_u64(7), &theme::ROSE_PINE);
    for tick in 0..1000 {
        scene.tick(AREA);
        assert_inside(&scene, AREA, &format!("tick {tick}"));
    }
}

/// Given: the scene on a hundred by thirty area, three hundred ticks in.
/// When: it ticks fifteen hundred more times, about ninety seconds.
/// Then: at every hundredth tick nine birds in ten have another bird
/// within four dots, and the middle of the flock moves across at least a
/// third of the width.
///
/// Why it matters: a flock that scatters is noise, and a flock that holds
/// one place is a cloud and not a murmuration.
#[test]
fn the_flock_holds_together_and_wanders() {
    // Given:
    let mut scene = ticked(7, AREA, 300);
    let (width, _) = dots_of(AREA);

    // When:
    let mut middles = Vec::new();
    for tick in 0..1500 {
        scene.tick(AREA);
        if tick % 100 != 0 {
            continue;
        }

        // Then:
        let birds: Vec<(f32, f32)> = scene.birds().collect();
        let close = birds
            .iter()
            .enumerate()
            .filter(|(i, (x, y))| {
                birds
                    .iter()
                    .enumerate()
                    .any(|(j, (ox, oy))| *i != j && (x - ox).hypot(y - oy) <= 4.0)
            })
            .count();
        assert!(
            close * 10 >= birds.len() * 9,
            "tick {tick}: {close} of {} birds near another",
            birds.len()
        );
        middles.push(birds.iter().map(|(x, _)| x).sum::<f32>() / birds.len() as f32);
    }
    let least = middles.iter().copied().fold(f32::MAX, f32::min);
    let most = middles.iter().copied().fold(f32::MIN, f32::max);
    assert!(most - least >= width / 3.0, "{middles:?}");
}

/// Given: a scene that ticked on a hundred by thirty area.
/// When: it ticks and renders on a twenty by six area inside a larger
/// canvas, then on one cell, then on none.
/// Then: it draws without a panic, every bird is inside the new area, and
/// no cell outside the area is touched.
#[test]
fn a_smaller_area_drops_the_old_flock() {
    // Given:
    let canvas = Rect::new(0, 0, 40, 12);
    let mut scene = ticked(7, AREA, 20);

    // When, then:
    for area in [
        Rect::new(3, 2, 20, 6),
        Rect::new(3, 2, 1, 1),
        Rect::new(3, 2, 0, 0),
    ] {
        scene.tick(area);
        assert_inside(&scene, area, &format!("{area:?}"));
        let buffer = render(&scene, area, canvas);
        for position in canvas.positions() {
            if !area.contains(position) {
                assert_eq!(buffer[position].symbol(), " ", "glyph outside {area:?}");
                assert_eq!(buffer[position].bg, Color::Reset, "colour outside {area:?}");
            }
        }
    }
}

/// Given: the scene name `murmuration`.
/// When: it is looked up and built, then ticked and rendered.
/// Then: the name finds a scene, and its frame shows birds.
///
/// Why it matters: a scene that is built but not listed cannot be chosen
/// from the command line, the picker or Tab.
#[test]
fn murmuration_is_a_scene_by_name() {
    // When:
    let kind = SceneKind::from_name("murmuration").expect("murmuration is listed");
    let mut scene = kind.build(
        7,
        &theme::ROSE_PINE,
        Background::Theme,
        &Arc::new(Corpus::embedded()),
        Speed::DEFAULT,
        Settings::DEFAULT,
    );
    scene.tick(AREA);

    // Then:
    let mut buffer = Buffer::empty(AREA);
    scene.render(AREA, &mut buffer);
    assert!(!lit_dots(&buffer, AREA).is_empty());
}

/// The rows of a render that hold the trees, top first: from the row of the
/// highest pixel of tree that stands on the ground to the bottom. The sky
/// between the trees draws as the sky does, so a row is known by its
/// trees, and a bird in the colour of a tree does not stand on the ground.
fn tree_rows(buffer: &Buffer, palette: &Palette) -> Vec<u16> {
    let area = buffer.area;
    let trees = dots_in(buffer, area, &[palette.muted]);
    let bottom = usize::from(area.height) * CELL_DOTS.1;
    let highest = (0..usize::from(area.width) * CELL_DOTS.0)
        .filter_map(|x| {
            (0..bottom)
                .rev()
                .take_while(|y| trees.contains(&(x, *y)))
                .last()
        })
        .min()
        .expect("no trees");
    (u16::try_from(highest / CELL_DOTS.1).unwrap()..area.bottom()).collect()
}

/// Given: the scene on a hundred by thirty area.
/// When: it ticks once and renders.
/// Then: down the rows above the trees, the cell backgrounds go from base
/// through surface to overlay, a row holds one colour or two neighbouring
/// ones, no row goes back to a darker band, and each edge between two
/// bands is a row that mixes them.
///
/// Why it matters: the sky is dusk, light low over the trees and dark
/// above, and a hard line between two bands reads as a fault in the
/// terminal, not as a sky.
#[test]
fn the_sky_lightens_in_dithered_bands_toward_the_trees() {
    // Given, when:
    let buffer = render(&ticked(7, AREA, 1), AREA, AREA);

    // Then:
    let palette = &theme::ROSE_PINE;
    let bands = [palette.base, palette.surface, palette.overlay];
    let trees = tree_rows(&buffer, palette);
    let sky = 0..*trees.first().expect("no trees");
    let mut rows = Vec::new();
    for y in sky {
        let used: BTreeSet<usize> = (0..AREA.width)
            .map(|x| {
                let bg = buffer[(x, y)].bg;
                bands
                    .iter()
                    .position(|band| *band == bg)
                    .unwrap_or_else(|| panic!("{bg:?} at ({x}, {y})"))
            })
            .collect();
        let (low, high) = (*used.first().unwrap(), *used.last().unwrap());
        assert!(high - low <= 1, "row {y}: {used:?}");
        rows.push((low, high));
    }
    assert!(rows.windows(2).all(|pair| pair[0] <= pair[1]), "{rows:?}");
    assert_eq!(rows.first(), Some(&(0, 0)), "{rows:?}");
    assert_eq!(rows.last(), Some(&(2, 2)), "{rows:?}");
    assert!(rows.contains(&(0, 1)) && rows.contains(&(1, 2)), "{rows:?}");
}

/// Given: the scene on a hundred by thirty area.
/// When: it ticks once and renders.
/// Then: the tree rows hold pixels of muted trees or overlay sky, each
/// column of pixels is sky above and tree below, the lowest row of pixels
/// is all tree, and the trees stand at three or more heights, none of them
/// filling every tree row.
///
/// Why it matters: a pixel of tree with sky under it floats, and a tree
/// line of one height reads as a wall, not as woods at dusk.
#[test]
fn trees_rise_from_the_ground_in_muted() {
    // Given, when:
    let buffer = render(&ticked(7, AREA, 1), AREA, AREA);

    // Then:
    let palette = &theme::ROSE_PINE;
    let trees = tree_rows(&buffer, palette);
    let mut heights = BTreeSet::new();
    for x in 0..usize::from(AREA.width) * CELL_DOTS.0 {
        let pixels: Vec<Color> = trees
            .iter()
            .flat_map(|&y| {
                let position = ((x / CELL_DOTS.0) as u16, y);
                let shown = pixels::shown(&buffer[position])
                    .unwrap_or_else(|| panic!("{:?} at {position:?}", buffer[position].symbol()));
                (0..CELL_DOTS.1).map(move |row| shown[row * CELL_DOTS.0 + x % CELL_DOTS.0])
            })
            .collect();
        let sky = pixels.iter().take_while(|p| **p == palette.overlay).count();
        assert!(
            pixels[sky..].iter().all(|p| *p == palette.muted),
            "column {x}: {pixels:?}"
        );
        assert!(sky < pixels.len(), "column {x} has no ground");
        heights.insert(pixels.len() - sky);
    }
    assert!(heights.len() >= 3, "{heights:?}");
    assert!(
        heights.iter().all(|h| *h < trees.len() * CELL_DOTS.1),
        "a tree fills every row: {heights:?}"
    );
}

/// Given: the scene on a hundred by thirty area.
/// When: it ticks a thousand times.
/// Then: after every tick every bird is in a row above the trees.
///
/// Why it matters: a cell holds two colours, so a bird in a cell of tree
/// and sky would take the place of one of them and cut a hole into the
/// trees.
#[test]
fn the_flock_stays_above_the_trees() {
    // Given:
    let mut scene = ticked(7, AREA, 1);
    let first_tree = *tree_rows(&render(&scene, AREA, AREA), &theme::ROSE_PINE)
        .first()
        .expect("no trees");

    // When, then:
    for tick in 0..1000 {
        scene.tick(AREA);
        for (x, y) in scene.birds() {
            assert!(
                (y as usize) / CELL_DOTS.1 < usize::from(first_tree),
                "tick {tick}: bird at ({x}, {y}) in the trees"
            );
        }
    }
}

/// Perceived brightness, with the weights of ITU-R BT.601.
fn luma(colour: Color) -> u32 {
    match colour {
        Color::Rgb(r, g, b) => 299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b),
        other => panic!("{other:?} is not a palette colour"),
    }
}

/// Given: the scene in each theme.
/// When: it ticks once and renders.
/// Then: the top row of the sky is the darkest of base, surface and
/// overlay, and the row above the trees is the lightest.
///
/// Why it matters: dusk is light low over the trees. A light theme orders
/// its three roles another way, and a sky that is brightest in the middle
/// reads as a stripe, not as dusk.
#[test]
fn every_theme_lights_the_sky_low_over_the_trees() {
    for (name, palette) in theme::THEMES {
        // Given, when:
        let mut scene = Murmuration::new(Rng::seed_from_u64(7), palette);
        scene.tick(AREA);
        let buffer = render(&scene, AREA, AREA);

        // Then:
        let roles = [palette.base, palette.surface, palette.overlay];
        let darkest = roles.iter().copied().min_by_key(|c| luma(*c)).unwrap();
        let lightest = roles.iter().copied().max_by_key(|c| luma(*c)).unwrap();
        let low = tree_rows(&buffer, palette)[0] - 1;
        for x in 0..AREA.width {
            assert_eq!(buffer[(x, 0)].bg, darkest, "{name}: top at {x}");
            assert_eq!(buffer[(x, low)].bg, lightest, "{name}: low at {x}");
        }
    }
}

/// Where the hawk is after each tick, `None` while the sky is clear.
fn hawk_track(seed: u64, ticks: u32) -> (Murmuration, Vec<Option<(f32, f32)>>) {
    let mut scene = Murmuration::new(Rng::seed_from_u64(seed), &theme::ROSE_PINE);
    let track = (0..ticks)
        .map(|_| {
            scene.tick(AREA);
            scene.hawk()
        })
        .collect();
    (scene, track)
}

/// Given: the scene on a hundred by thirty area.
/// When: it ticks long enough for three waits of the longest `HAWK_EVERY`
/// and three crossings.
/// Then: the hawk comes at least twice, each wait for it, from the start
/// or from the last pass, is inside `HAWK_EVERY`, and each pass starts
/// past one side of the pane and ends past the other.
///
/// Why it matters: a hawk that is always there keeps the flock split, and
/// the calm folds never show.
#[test]
fn a_hawk_crosses_the_sky_now_and_then() {
    // Given, when:
    let ticks = HAWK_EVERY.end() * 3 + 900;
    let (_, track) = hawk_track(7, ticks);

    // Then:
    let (width, _) = dots_of(AREA);
    let mut passes = 0;
    let mut clear_since = 0;
    let mut entered = None;
    // `track[i]` is the hawk after `i + 1` ticks, so the count starts at 1.
    for (tick, place) in (1u32..).zip(&track) {
        match (place, entered) {
            (Some((x, _)), None) => {
                let wait = tick - clear_since;
                assert!(
                    HAWK_EVERY.contains(&wait),
                    "waited {wait} ticks before tick {tick}"
                );
                entered = Some(*x);
            }
            (None, Some(start)) => {
                let (end, _) = track[tick as usize - 2].unwrap();
                let crossed = (start < 0.0 && end >= width) || (start >= width && end < 0.0);
                assert!(crossed, "a pass from {start} to {end}");
                passes += 1;
                clear_since = tick;
                entered = None;
            }
            _ => {}
        }
    }
    assert!(passes >= 2, "{passes} passes in {ticks} ticks");
}

/// Given: the scene on a hundred by thirty area.
/// When: it ticks through two passes of the hawk.
/// Then: on every tick with the hawk in the sky, no bird is within
/// `HAWK_CLEARANCE` dots of it.
///
/// Why it matters: the hawk is the one event of the scene, and what shows
/// it is the flock opening around it.
#[test]
fn the_flock_opens_around_the_hawk() {
    // Given:
    let mut scene = Murmuration::new(Rng::seed_from_u64(7), &theme::ROSE_PINE);
    let mut seen = 0;

    for tick in 0..HAWK_EVERY.end() * 2 + 600 {
        // When:
        scene.tick(AREA);
        let Some((hx, hy)) = scene.hawk() else {
            continue;
        };
        seen += 1;

        // Then:
        for (x, y) in scene.birds() {
            let distance = (x - hx).hypot(y - hy);
            assert!(
                distance >= HAWK_CLEARANCE,
                "tick {tick}: bird at ({x}, {y}) {distance} from the hawk at ({hx}, {hy})"
            );
        }
    }
    assert!(seen > 0, "no hawk came");
}

/// Given: the scene on a hundred by thirty area, ticked until the hawk is
/// in the middle third of the pane.
/// When: it renders.
/// Then: some pixels are in love, and every one of them is within four
/// dots across and two up or down of the hawk.
///
/// Why it matters: a pixel in love away from the hawk reads as a smear of
/// it or as a second hawk.
#[test]
fn the_hawk_draws_in_love_over_the_flock() {
    // Given:
    let (width, _) = dots_of(AREA);
    let mut scene = Murmuration::new(Rng::seed_from_u64(7), &theme::ROSE_PINE);
    let (hx, hy) = (0..HAWK_EVERY.end() + 600)
        .find_map(|_| {
            scene.tick(AREA);
            scene
                .hawk()
                .filter(|(x, _)| *x > width / 3.0 && *x < width * 2.0 / 3.0)
        })
        .expect("no hawk reached the middle third");

    // When:
    let buffer = render(&scene, AREA, AREA);

    // Then:
    let love = dots_in(&buffer, AREA, &[theme::ROSE_PINE.love]);
    for (x, y) in &love {
        let (x, y) = (*x as f32, *y as f32);
        assert!(
            (x - hx).abs() <= 4.0 && (y - hy).abs() <= 2.0,
            "dot ({x}, {y}) in love, the hawk at ({hx}, {hy})"
        );
    }
    assert!(!love.is_empty(), "the hawk is not drawn");
}
