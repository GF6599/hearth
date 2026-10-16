#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeSet;
use std::sync::Arc;

use hearth::corpus::Corpus;
use hearth::scene::{Rng, Scene, SceneKind};
use hearth::scenes::murmuration::{DOTS_PER_BIRD, FLOCK, Murmuration};
use hearth::scenes::typing::Speed;
use hearth::theme;
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);

/// A braille cell is two dots wide and four tall.
const CELL_DOTS: (usize, usize) = (2, 4);

const BLANK_BRAILLE: u32 = 0x2800;

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

/// The dots a braille glyph lights, as (column, row) inside its cell, by
/// the Unicode table: bits 0 to 2 run down the left column, bits 3 to 5
/// down the right, and bits 6 and 7 are the bottom row.
fn braille_dots(symbol: &str) -> Option<Vec<(usize, usize)>> {
    let glyph = u32::from(symbol.chars().next()?);
    let bits = glyph
        .checked_sub(BLANK_BRAILLE)
        .filter(|bits| *bits <= 0xFF)?;
    Some(
        (0..8)
            .filter(|bit| bits & (1 << bit) != 0)
            .map(|bit| match bit {
                0..=2 => (0, bit),
                3..=5 => (1, bit - 3),
                6 => (0, 3),
                _ => (1, 3),
            })
            .collect(),
    )
}

/// Every lit dot of a render, in dots from the top left of the area.
fn lit_dots(buffer: &Buffer, area: Rect) -> BTreeSet<(usize, usize)> {
    let mut lit = BTreeSet::new();
    for position in area.positions() {
        let Some(dots) = braille_dots(buffer[position].symbol()) else {
            continue;
        };
        let column = usize::from(position.x - area.x) * CELL_DOTS.0;
        let row = usize::from(position.y - area.y) * CELL_DOTS.1;
        lit.extend(dots.into_iter().map(|(x, y)| (column + x, row + y)));
    }
    lit
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
/// Then: the dots the braille glyphs light are the dots the birds sit on,
/// no more and no fewer.
///
/// Why it matters: a wrong bit in the braille table draws a bird one dot
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
/// Then: every cell is blank or braille, a cell with one or two dots is
/// subtle, and a cell with three or more is text.
///
/// Why it matters: one braille cell takes one colour, so the dots in it
/// are what shades the flock from a faint edge to a bright core.
#[test]
fn a_cell_with_more_birds_is_brighter() {
    // Given, when:
    let buffer = render(&ticked(7, AREA, 100), AREA, AREA);

    // Then:
    let mut shades = BTreeSet::new();
    for position in AREA.positions() {
        let cell = &buffer[position];
        if cell.symbol() == " " {
            continue;
        }
        let dots = braille_dots(cell.symbol())
            .unwrap_or_else(|| panic!("{:?} at {position:?}", cell.symbol()));
        let expected: Color = match dots.len() {
            0 => continue,
            1 | 2 => theme::ROSE_PINE.subtle,
            _ => theme::ROSE_PINE.text,
        };
        assert_eq!(cell.fg, expected, "{} dots at {position:?}", dots.len());
        shades.insert(dots.len() > 2);
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
/// Then: the name finds a scene, and its frame shows braille dots.
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
        &Arc::new(Corpus::embedded()),
        Speed::DEFAULT,
    );
    scene.tick(AREA);

    // Then:
    let mut buffer = Buffer::empty(AREA);
    scene.render(AREA, &mut buffer);
    assert!(!lit_dots(&buffer, AREA).is_empty());
}
