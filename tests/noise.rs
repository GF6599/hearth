#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pixels;

use std::collections::HashMap;

use hearth::octant;
use hearth::scene::{Rng, Scene};
use hearth::scenes::noise::Noise;
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);

fn render(scene: &Noise) -> Buffer {
    let mut buffer = Buffer::empty(AREA);
    scene.render(AREA, &mut buffer);
    buffer
}

/// The colour each pixel shows, by pixel row from the top and then by
/// pixel column from the left.
fn rows(buffer: &Buffer) -> Vec<Vec<Color>> {
    pixels::grid(buffer, buffer.area, |_, cell| {
        pixels::shown(cell).unwrap_or_else(|| panic!("{:?} is not an octant glyph", cell.symbol()))
    })
}

/// The theme colours the noise shades through, from the base to the text.
fn stops(palette: &Palette) -> [Color; 5] {
    [
        palette.base,
        palette.overlay,
        palette.muted,
        palette.subtle,
        palette.text,
    ]
}

/// The mixes a ramp step of this size apart are near enough to read as
/// one. A step of 1/64 of a stop moves a channel by less than two.
const STEPS: u16 = 64;

/// The place of `colour` along `stops`, in stops from the first, or none
/// for a colour that is no mix of two neighbouring stops. A mix is read
/// back to within two in each channel, so the rounding of a mix to 8 bits
/// does not count.
fn place(colour: Color, stops: &[Color], known: &mut HashMap<Color, Option<f32>>) -> Option<f32> {
    *known.entry(colour).or_insert_with(|| {
        let Color::Rgb(r, g, b) = colour else {
            return None;
        };
        let mut best: Option<(u32, f32)> = None;
        for (index, pair) in stops.windows(2).enumerate() {
            for step in 0..=STEPS {
                let share = f32::from(step) / f32::from(STEPS);
                let Color::Rgb(mr, mg, mb) = octant::mix(pair[0], pair[1], share) else {
                    continue;
                };
                let gap = [r.abs_diff(mr), g.abs_diff(mg), b.abs_diff(mb)]
                    .into_iter()
                    .map(u32::from)
                    .max()
                    .unwrap_or(0);
                if gap <= 2 && best.is_none_or(|(least, _)| gap < least) {
                    best = Some((gap, index as f32 + share));
                }
            }
        }
        best.map(|(_, place)| place)
    })
}

/// Given: the scene on a hundred by thirty area.
/// When: it ticks once and renders.
/// Then: every pixel is a mix of two neighbouring colours along the ramp
/// from the base through overlay, muted and subtle to the text. Some
/// pixels rest in the base, some reach subtle, and more than eight shades
/// show.
///
/// Why it matters: the noise shades smoothly from the base to the text,
/// so it needs the mixes between the theme colours and not four flat
/// colours. A colour off the ramp clashes with the theme.
#[test]
fn noise_shades_along_a_ramp_of_palette_colours() {
    // Given:
    let palette = &theme::ROSE_PINE;
    let mut scene = Noise::new(Rng::seed_from_u64(7), palette);

    // When:
    scene.tick(AREA);
    let shown = rows(&render(&scene));

    // Then:
    let stops = stops(palette);
    let mut known = HashMap::new();
    let mut places = Vec::new();
    for colour in shown.iter().flatten() {
        let place = place(*colour, &stops, &mut known)
            .unwrap_or_else(|| panic!("{colour:?} is off the ramp"));
        places.push(place);
    }
    assert!(places.contains(&0.0), "no pixel rests in the base");
    let highest = places.iter().copied().fold(0.0, f32::max);
    assert!(highest >= 3.0, "the highest pixel is at {highest} stops");
    assert!(known.len() > 8, "only {} shades show", known.len());
}

/// Given: the scene on a hundred by thirty area.
/// When: it ticks once and renders.
/// Then: of the pixels side by side or one above the other, ninety-eight
/// pairs in a hundred or more lie within half a stop of each other on the
/// ramp.
///
/// Why it matters: the noise reads as soft clouds when each pixel shades
/// into the next. Random levels put most pairs a whole stop or more
/// apart, and that reads as hard speckle.
#[test]
fn neighbouring_pixels_shade_into_each_other() {
    // Given:
    let palette = &theme::ROSE_PINE;
    let mut scene = Noise::new(Rng::seed_from_u64(7), palette);

    // When:
    scene.tick(AREA);
    let shown = rows(&render(&scene));

    // Then:
    let stops = stops(palette);
    let mut known = HashMap::new();
    let mut at = |colour: Color| place(colour, &stops, &mut known).unwrap_or(f32::NAN);
    let (mut pairs, mut near) = (0, 0);
    for y in 0..shown.len() {
        for x in 0..shown[y].len() {
            let here = at(shown[y][x]);
            let neighbours = [
                shown[y].get(x + 1).copied(),
                shown.get(y + 1).map(|row| row[x]),
            ];
            for there in neighbours.into_iter().flatten() {
                pairs += 1;
                if (here - at(there)).abs() <= 0.5 {
                    near += 1;
                }
            }
        }
    }
    assert!(near * 100 >= pairs * 98, "{near} of {pairs} pairs are near");
}

/// Given: the scene on a hundred by thirty area.
/// When: it ticks once and renders.
/// Then: a third or more of the cells show two colours in one octant glyph.
///
/// Why it matters: the edge between two shades runs through the cells at
/// the grain of a pixel, a quarter of a cell tall. Noise at the grain of a
/// cell shows each cell in one colour throughout.
#[test]
fn noise_varies_within_a_cell() {
    // Given:
    let mut scene = Noise::new(Rng::seed_from_u64(7), &theme::ROSE_PINE);

    // When:
    scene.tick(AREA);
    let buffer = render(&scene);

    // Then:
    let split = buffer
        .area
        .positions()
        .filter(|&position| {
            let cell = &buffer[position];
            let bits = pixels::bits(cell);
            cell.fg != cell.bg && bits.is_some_and(|bits| bits != 0 && bits != u8::MAX)
        })
        .count();
    let cells = usize::try_from(AREA.area()).unwrap();
    assert!(split * 3 >= cells, "{split} of {cells} cells split");
}

/// Noise drifts calmly: it changes a few of its pixels each tick, each by
/// a small step.
/// Given: the scene on a hundred by thirty area, ticked once.
/// When: it ticks fifty times more.
/// Then: from one tick to the next, between one pixel in fifty and one in
/// five show another colour, on average, and ninety-nine changes in a
/// hundred or more move half a stop or less along the ramp.
///
/// Why it matters: a screensaver must move, but a screen that draws many
/// pixels again each tick, or jumps a pixel from dark to light, flickers
/// too fast to rest the eye.
#[test]
fn noise_drifts_a_few_pixels_a_small_step_each_tick() {
    // Given:
    let palette = &theme::ROSE_PINE;
    let mut scene = Noise::new(Rng::seed_from_u64(7), palette);
    scene.tick(AREA);
    let mut before = rows(&render(&scene));
    let stops = stops(palette);
    let mut known = HashMap::new();
    let mut shares = Vec::new();
    let (mut changes, mut small) = (0, 0);

    for _ in 0..50 {
        // When:
        scene.tick(AREA);
        let after = rows(&render(&scene));
        let mut changed = 0;
        for (old, new) in before.iter().flatten().zip(after.iter().flatten()) {
            if old != new {
                changed += 1;
                let from = place(*old, &stops, &mut known).unwrap_or(f32::NAN);
                let to = place(*new, &stops, &mut known).unwrap_or(f32::NAN);
                if (from - to).abs() <= 0.5 {
                    small += 1;
                }
            }
        }
        changes += changed;
        let count = after.iter().map(Vec::len).sum::<usize>();
        shares.push(f64::from(changed) / count as f64);
        before = after;
    }

    // Then:
    let mean = shares.iter().sum::<f64>() / shares.len() as f64;
    assert!(
        (0.02..0.2).contains(&mean),
        "{mean} of the pixels changed a tick"
    );
    assert!(
        small * 100 >= changes * 99,
        "{small} of {changes} changes are small"
    );
}

/// Given: the scene in the flat Rose Pine theme, whose base is the
/// terminal's own background.
/// When: it ticks once and renders.
/// Then: no pixel is a mix of the base and overlay other than the two
/// ends.
///
/// Why it matters: a mix with a see-through base paints an opaque colour
/// that guesses the user's background. Over a background of another
/// colour, the edge of each cloud then shows a halo.
#[test]
fn noise_keeps_its_edges_hard_on_a_see_through_base() {
    // Given:
    let palette = &theme::FLAT_THEMES[0];
    let mut scene = Noise::new(Rng::seed_from_u64(7), palette);

    // When:
    scene.tick(AREA);
    let shown = rows(&render(&scene));

    // Then:
    let stops = stops(palette);
    let mut known = HashMap::new();
    for colour in shown.iter().flatten() {
        if *colour == palette.base {
            continue;
        }
        let place = place(*colour, &stops, &mut known)
            .unwrap_or_else(|| panic!("{colour:?} is off the ramp"));
        assert!(place >= 1.0, "{colour:?} mixes the base at {place}");
    }
}
