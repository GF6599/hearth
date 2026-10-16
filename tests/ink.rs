#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;
use std::sync::Arc;

use hearth::corpus::Corpus;
use hearth::scene::{Rng, Scene, SceneKind};
use hearth::scenes::ink::{self, DROP_EVERY, EMIT_TICKS, FULL_INK, Ink, LIFETIME};
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);

/// Each cell draws two pixels, the upper as the foreground.
const HALF_BLOCK: &str = "▀";

const TILE: usize = 4;

fn ticked(seed: u64, palette: &'static Palette, area: Rect, ticks: u32) -> Ink {
    let mut scene = Ink::new(Rng::seed_from_u64(seed), palette);
    for _ in 0..ticks {
        scene.tick(area);
    }
    scene
}

fn render(scene: &Ink, area: Rect, canvas: Rect) -> Buffer {
    let mut buffer = Buffer::empty(canvas);
    scene.render(area, &mut buffer);
    buffer
}

/// Every inked pixel of a render, as (column, pixel row) from the top
/// left of the area.
fn inked_pixels(buffer: &Buffer, area: Rect, base: Color) -> Vec<(usize, usize)> {
    let mut inked = Vec::new();
    for position in area.positions() {
        let cell = &buffer[position];
        let column = usize::from(position.x - area.x);
        let row = usize::from(position.y - area.y) * 2;
        if cell.fg != base {
            inked.push((column, row));
        }
        if cell.bg != base {
            inked.push((column, row + 1));
        }
    }
    inked
}

fn tile() -> impl Iterator<Item = (usize, usize)> {
    (0..TILE).flat_map(|row| (0..TILE).map(move |x| (x, row)))
}

/// Given: the scene on a forty by twenty area, in a dark and a light theme.
/// When: it renders after each of four hundred ticks.
/// Then: every cell is a half block whose two colours are the base or one
/// of the five accents.
///
/// Why it matters: thin ink is drawn by dither, not by a blend, so a blend
/// would show as a colour that the theme does not have.
#[test]
fn every_cell_is_the_water_or_an_accent() {
    let area = Rect::new(0, 0, 40, 20);
    for palette in [&theme::ROSE_PINE, &theme::ROSE_PINE_DAWN] {
        let mut allowed = ink::accents(palette).to_vec();
        allowed.push(palette.base);
        let mut scene = Ink::new(Rng::seed_from_u64(7), palette);
        for tick in 0..400 {
            // When:
            scene.tick(area);
            let buffer = render(&scene, area, area);

            // Then:
            for position in area.positions() {
                let cell = &buffer[position];
                assert_eq!(cell.symbol(), HALF_BLOCK, "tick {tick} at {position:?}");
                for colour in [cell.fg, cell.bg] {
                    assert!(
                        allowed.contains(&colour),
                        "tick {tick}: {colour:?} at {position:?}"
                    );
                }
            }
        }
    }
}

/// Given: a pixel with no ink, a pixel at `FULL_INK` of one accent, and a
/// pixel at half of it.
/// When: each shades every pixel of a four by four tile.
/// Then: no ink is all water, full ink is all accent, and half of it
/// colours half the tile and leaves the rest as water.
///
/// Why it matters: this is the dither. Without it a thread of ink is
/// either solid or gone, and the fade jumps instead of thinning.
#[test]
fn thin_ink_colours_a_share_of_the_tile() {
    let palette = &theme::ROSE_PINE;
    let love = ink::accents(palette)[0];
    for (amount, inked) in [(0.0, 0), (FULL_INK, 16), (FULL_INK / 2.0, 8)] {
        // When:
        let tile: Vec<Color> = tile()
            .map(|(x, row)| ink::shade(palette, &[amount, 0.0, 0.0, 0.0, 0.0], x, row))
            .collect();

        // Then:
        let accent = tile.iter().filter(|colour| **colour == love).count();
        let water = tile
            .iter()
            .filter(|colour| **colour == palette.base)
            .count();
        assert_eq!((accent, water), (inked, 16 - inked), "{amount}: {tile:?}");
    }
}

/// Given: a pixel that holds as much of the first accent as of the
/// third, and `FULL_INK` between them.
/// When: it shades every pixel of a four by four tile.
/// Then: half the tile is the one accent and half the other, with no
/// water.
///
/// Why it matters: where two clouds meet the ink mixes by dither. Drawing
/// only the densest ink cuts a hard edge between them.
#[test]
fn two_inks_in_one_pixel_mix_by_dither() {
    // Given:
    let palette = &theme::ROSE_PINE;
    let accents = ink::accents(palette);
    let half = FULL_INK / 2.0;

    // When:
    let tile: Vec<Color> = tile()
        .map(|(x, row)| ink::shade(palette, &[half, 0.0, half, 0.0, 0.0], x, row))
        .collect();

    // Then:
    let first = tile.iter().filter(|colour| **colour == accents[0]).count();
    let third = tile.iter().filter(|colour| **colour == accents[2]).count();
    assert_eq!((first, third), (8, 8), "{tile:?}");
}

/// Given: the scene on a hundred by thirty area.
/// When: it renders after the third tick and again before the second drop
/// can fall.
/// Then: the later frame inks more pixels, and their mean row is lower.
///
/// Why it matters: this is the scene. A drop that holds its shape is a
/// blot, and one that does not sink is a stain on the glass.
#[test]
fn a_drop_opens_out_as_it_sinks() {
    let base = theme::ROSE_PINE.base;
    let mean_row = |pixels: &[(usize, usize)]| {
        pixels.iter().map(|(_, row)| *row as f32).sum::<f32>() / pixels.len() as f32
    };
    for seed in [1, 7, 42] {
        // When:
        let early = ticked(seed, &theme::ROSE_PINE, AREA, 3);
        let early = inked_pixels(&render(&early, AREA, AREA), AREA, base);
        let late = ticked(seed, &theme::ROSE_PINE, AREA, *DROP_EVERY.start() - 1);
        assert_eq!(late.drops().count(), 1, "seed {seed}");
        let late = inked_pixels(&render(&late, AREA, AREA), AREA, base);

        // Then:
        assert!(
            !early.is_empty(),
            "seed {seed}: the first drop falls at once"
        );
        assert!(
            late.len() > early.len() * 3,
            "seed {seed}: {} then {} pixels",
            early.len(),
            late.len()
        );
        assert!(
            mean_row(&late) > mean_row(&early) + 5.0,
            "seed {seed}: row {} then {}",
            mean_row(&early),
            mean_row(&late)
        );
    }
}

/// Given: the scene on a hundred by thirty area.
/// When: it ticks long enough for the first drop's ink to run out.
/// Then: the first drop is there after one tick and gone by then.
///
/// Why it matters: ink that never fades fills the pane with one flat
/// colour within minutes.
#[test]
fn each_drop_fades_away() {
    for seed in [1, 7, 42] {
        // Given:
        let mut scene = ticked(seed, &theme::ROSE_PINE, AREA, 1);
        assert!(scene.drops().any(|(serial, _)| serial == 0), "seed {seed}");

        // When:
        for _ in 0..EMIT_TICKS + LIFETIME.end() + 1 {
            scene.tick(AREA);
        }

        // Then:
        assert!(
            scene.drops().all(|(serial, _)| serial != 0),
            "seed {seed}: the first drop is still there"
        );
    }
}

/// Given: the scene on a hundred by thirty area.
/// When: it ticks for long enough to drop at least twelve times.
/// Then: no drop has the accent of the drop just before it.
///
/// Why it matters: two drops of one colour in a row read as one blot, and
/// the colours stop mixing.
#[test]
fn no_drop_has_the_accent_of_the_one_before() {
    for seed in [1, 7, 42] {
        // When:
        let mut seen = BTreeMap::new();
        let mut scene = Ink::new(Rng::seed_from_u64(seed), &theme::ROSE_PINE);
        for _ in 0..DROP_EVERY.end() * 12 {
            scene.tick(AREA);
            seen.extend(scene.drops());
        }

        // Then:
        let accents: Vec<Color> = seen.into_values().collect();
        assert!(accents.len() >= 12, "seed {seed}: {} drops", accents.len());
        for pair in accents.windows(2) {
            assert_ne!(pair[0], pair[1], "seed {seed}: {accents:?}");
        }
    }
}

/// Given: the scene on a thirty by ten area.
/// When: it ticks long enough for several drops to sink to the bottom.
/// Then: every particle is inside the pane on every tick.
///
/// Why it matters: a particle outside the pane is ink the render cannot
/// draw, which reads as ink that vanished.
#[test]
fn the_ink_stays_in_the_pane() {
    let area = Rect::new(0, 0, 30, 10);
    let (width, rows) = (30.0, 20.0);
    let mut scene = Ink::new(Rng::seed_from_u64(7), &theme::ROSE_PINE);
    for tick in 0..DROP_EVERY.end() * 4 {
        // When:
        scene.tick(area);

        // Then:
        for (x, y) in scene.particles() {
            assert!(
                (0.0..width).contains(&x) && (0.0..rows).contains(&y),
                "tick {tick}: particle at ({x}, {y})"
            );
        }
    }
}

/// Given: a scene that ticked on a hundred by thirty area.
/// When: it ticks and renders on a twenty by six area inside a larger
/// canvas, then on one cell, then on none.
/// Then: it draws without a panic, every particle is inside the new area,
/// and no cell outside the area is touched.
#[test]
fn a_smaller_area_drops_the_old_ink() {
    // Given:
    let canvas = Rect::new(0, 0, 40, 12);
    let mut scene = ticked(7, &theme::ROSE_PINE, AREA, 40);

    // When, then:
    for area in [
        Rect::new(3, 2, 20, 6),
        Rect::new(3, 2, 1, 1),
        Rect::new(3, 2, 0, 0),
    ] {
        scene.tick(area);
        let (width, rows) = (f32::from(area.width), f32::from(area.height) * 2.0);
        for (x, y) in scene.particles() {
            assert!(
                (0.0..width).contains(&x) && (0.0..rows).contains(&y),
                "{area:?}: particle at ({x}, {y})"
            );
        }
        let buffer = render(&scene, area, canvas);
        for position in canvas.positions() {
            if !area.contains(position) {
                assert_eq!(buffer[position].symbol(), " ", "glyph outside {area:?}");
                assert_eq!(buffer[position].bg, Color::Reset, "colour outside {area:?}");
            }
        }
    }
}

/// Given: the scene name `ink`.
/// When: it is looked up and built, then ticked and rendered.
/// Then: the name finds a scene, and its first frame already shows ink.
///
/// Why it matters: a scene that is built but not listed cannot be chosen
/// from the command line, the picker or Tab.
#[test]
fn ink_is_a_scene_by_name() {
    // When:
    let kind = SceneKind::from_name("ink").expect("ink is listed");
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
    assert!(!inked_pixels(&buffer, AREA, theme::ROSE_PINE.base).is_empty());
}
