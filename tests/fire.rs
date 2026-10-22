#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pixels;

use hearth::octant;
use hearth::scene::{Rng, Scene};
use hearth::scenes::fire::{self, Fire, MAX_HEAT};
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

fn scene(seed: u64) -> Fire {
    Fire::new(Rng::seed_from_u64(seed), &theme::ROSE_PINE)
}

fn render(scene: &Fire, area: Rect, canvas: Rect) -> Buffer {
    let mut buffer = Buffer::empty(canvas);
    scene.render(area, &mut buffer);
    buffer
}

/// The agreed ramp, from no heat to full heat.
const RAMP: [Color; 7] = [
    theme::ROSE_PINE.base,
    theme::ROSE_PINE.overlay,
    theme::ROSE_PINE.muted,
    theme::ROSE_PINE.love,
    theme::ROSE_PINE.rose,
    theme::ROSE_PINE.gold,
    theme::ROSE_PINE.text,
];

const FULL: usize = RAMP.len() - 1;

/// The band of the gold stop, from which the heat shades to text.
const HOTTEST: usize = FULL - 1;

/// The pixels across and down in each cell.
const ACROSS: usize = octant::CELL_WIDTH;
const DOWN: usize = octant::CELL_HEIGHT;

/// The pixels of each cell of a render, as the glyph and its two colours
/// draw them. A cell that is not an octant glyph gives `None` for each.
fn shown(buffer: &Buffer, area: Rect) -> Vec<Vec<Option<Color>>> {
    pixels::grid(buffer, area, |_, cell| {
        pixels::shown(cell).map_or([None; pixels::PIXELS], |shown| shown.map(Some))
    })
}

const DARK: Color = Color::Rgb(0, 0, 0);
const LIGHT: Color = Color::Rgb(255, 255, 255);

/// Rose Pine with each stop of the ramp from `band` up in light and every
/// other colour in dark. A pixel is light from the heat of the band's stop
/// up, dark at the heat of the stop below and under it, and a grey mix of
/// the two between. A cell of the whole ramp can hold eight colours and
/// draws only two, but a cell of these holds at most three.
fn from_band(band: usize) -> &'static Palette {
    let tone = |colour: Color| {
        if RAMP[band..].contains(&colour) {
            LIGHT
        } else {
            DARK
        }
    };
    let rose_pine = &theme::ROSE_PINE;
    Box::leak(Box::new(Palette {
        base: tone(rose_pine.base),
        surface: tone(rose_pine.surface),
        overlay: tone(rose_pine.overlay),
        muted: tone(rose_pine.muted),
        subtle: tone(rose_pine.subtle),
        text: tone(rose_pine.text),
        love: tone(rose_pine.love),
        gold: tone(rose_pine.gold),
        rose: tone(rose_pine.rose),
        pine: tone(rose_pine.pine),
        foam: tone(rose_pine.foam),
        iris: tone(rose_pine.iris),
        ..*rose_pine
    }))
}

/// One fire for each band above the coldest, from one seed. A palette
/// changes no tick, so the fires burn in step, and each one's light pixels
/// are those at its band or above.
struct Banded {
    fires: Vec<Fire>,
}

impl Banded {
    fn new(seed: u64) -> Self {
        Self {
            fires: (1..=FULL)
                .map(|band| Fire::new(Rng::seed_from_u64(seed), from_band(band)))
                .collect(),
        }
    }

    fn tick(&mut self, area: Rect) {
        for fire in &mut self.fires {
            fire.tick(area);
        }
    }

    fn renders(&self, area: Rect, canvas: Rect) -> Vec<Buffer> {
        self.fires
            .iter()
            .map(|fire| render(fire, area, canvas))
            .collect()
    }

    /// The band of each pixel of the area: the number of fires that draw
    /// it light.
    fn bands(&self, area: Rect, canvas: Rect) -> Vec<Vec<usize>> {
        let lit: Vec<Vec<Vec<Option<Color>>>> = self
            .renders(area, canvas)
            .iter()
            .map(|buffer| shown(buffer, area))
            .collect();
        let mut bands =
            vec![vec![0; usize::from(area.width) * ACROSS]; usize::from(area.height) * DOWN];
        for (y, row) in bands.iter_mut().enumerate() {
            for (x, band) in row.iter_mut().enumerate() {
                *band = lit.iter().filter(|rows| rows[y][x] == Some(LIGHT)).count();
            }
        }
        bands
    }
}

/// The colour that a pixel at `heat` draws.
fn shade(palette: &Palette, heat: u16) -> Color {
    fire::color(palette, heat)
}

/// The largest difference in one channel between two colours.
fn step(one: Color, other: Color) -> u8 {
    match (one, other) {
        (Color::Rgb(r, g, b), Color::Rgb(s, h, c)) => [r.abs_diff(s), g.abs_diff(h), b.abs_diff(c)]
            .into_iter()
            .max()
            .unwrap(),
        _ => panic!("{one:?} or {other:?} has no RGB value"),
    }
}

/// The shades fewer than this between two stops show as bands.
const LEAST_SHADES: usize = 5;

/// Asserts that the heats of `palette` shade through `stops`: each stop's
/// own heat, evenly spaced, gives the stop exactly. Between two stops at
/// least `LEAST_SHADES` colours lie on the way from one to the other: each
/// is no nearer the lower stop than the one before and no further from the
/// upper, to a step of rounding. No two neighbouring heats differ by more
/// than a quarter of the two stops' difference and a step of rounding. An
/// even step of the mix is larger in RGB where a channel nears 0.
fn assert_shades(palette: &Palette, stops: &[Color]) {
    let span = MAX_HEAT / (stops.len() as u16 - 1);
    for (index, pair) in stops.windows(2).enumerate() {
        let (lower, upper) = (pair[0], pair[1]);
        let from = span * index as u16;
        let heats = from..=from + span;
        let colours: Vec<Color> = heats.clone().map(|heat| shade(palette, heat)).collect();
        assert_eq!(colours[0], lower, "heat {from}");
        assert_eq!(colours[colours.len() - 1], upper, "heat {}", from + span);
        let mut distinct = colours.clone();
        distinct.dedup();
        assert!(
            distinct.len() >= LEAST_SHADES,
            "{} shades from {lower:?} to {upper:?}: {distinct:?}",
            distinct.len()
        );
        let largest = step(lower, upper) / 4 + 1;
        for (heat, pair) in heats.zip(colours.windows(2)) {
            assert!(
                pixels::distance(lower, pair[1]) >= pixels::distance(lower, pair[0]) - 1.0
                    && pixels::distance(upper, pair[1]) <= pixels::distance(upper, pair[0]) + 1.0,
                "heat {heat} to {}: {:?} to {:?} turns back",
                heat + 1,
                pair[0],
                pair[1]
            );
            assert!(
                step(pair[0], pair[1]) <= largest,
                "heat {heat} to {}: {:?} to {:?} jumps",
                heat + 1,
                pair[0],
                pair[1]
            );
        }
    }
}

/// Given: the ramp.
/// When: each heat from none to full is read.
/// Then: the heats shade through base, overlay, muted, love, rose, gold and
/// text in that order, each stop at its own heat, and between two stops
/// through mixes of the two in small steps.
///
/// Why it matters: a skipped stop leaves a jump in the flame, and flat
/// stops show as hard bands. A dither of flat stops hides the bands at a
/// distance but reads as a grain of pixels up close.
#[test]
fn the_ramp_shades_through_the_seven_stops() {
    assert_shades(&theme::ROSE_PINE, &RAMP);
}

/// Given: each light palette, Rose Pine Dawn, Catppuccin Latte, Tokyo
/// Night Day, Kanagawa Lotus, Alucard and Solarized Light, where text is
/// the darkest role.
/// When: each heat from none to full is read.
/// Then: the heats shade through base, overlay, muted, love, rose and gold,
/// and end at gold.
///
/// Why it matters: on a light theme a text stop draws a dark band under
/// the flames, which reads as the coldest part and not the hottest.
#[test]
fn a_light_theme_ends_the_ramp_at_gold() {
    for light in [
        &theme::ROSE_PINE_DAWN,
        &theme::CATPPUCCIN_LATTE,
        &theme::TOKYO_NIGHT_DAY,
        &theme::KANAGAWA_LOTUS,
        &theme::DRACULA_ALUCARD,
        &theme::SOLARIZED_LIGHT,
    ] {
        assert_shades(
            light,
            &[
                light.base,
                light.overlay,
                light.muted,
                light.love,
                light.rose,
                light.gold,
            ],
        );
    }
}

/// Given: the terminal's background on Rose Pine, where the base is
/// see-through.
/// When: each heat from none to full is read.
/// Then: each heat below the overlay's own heat is the base or the overlay,
/// and never a mix of the two.
///
/// Why it matters: a mix with the base is an opaque colour over the
/// terminal's own background, and shows as a smudge over a transparent
/// terminal.
#[test]
fn a_see_through_base_keeps_the_top_of_the_fire_hard() {
    // Given:
    let palette = &theme::FLAT_THEMES[0];
    assert!(palette.see_through);

    // When, then:
    for heat in 0..MAX_HEAT / FULL as u16 {
        let colour = shade(palette, heat);
        assert!(
            colour == palette.base || colour == palette.overlay,
            "heat {heat} draws {colour:?}"
        );
    }
}

/// Given: a scene on a forty by twenty area.
/// When: it ticks thirty times and renders.
/// Then: every cell's foreground and background is a shade of the ramp,
/// and the frame shows more than three times as many shades as the ramp
/// has stops.
///
/// Why it matters: a colour off the ramp is not a mix of the theme's
/// colours and reads as a fault, and a frame of few shades shows the hard
/// bands of the stops.
#[test]
fn every_cell_is_a_shade_of_the_ramp() {
    // Given:
    let area = Rect::new(0, 0, 40, 20);
    let mut scene = scene(7);
    let ramp: Vec<Color> = (0..=MAX_HEAT)
        .map(|heat| shade(&theme::ROSE_PINE, heat))
        .collect();

    // When:
    for _ in 0..30 {
        scene.tick(area);
    }
    let buffer = render(&scene, area, area);

    // Then:
    let mut shown: Vec<Color> = Vec::new();
    for position in area.positions() {
        let cell = &buffer[position];
        for colour in [cell.fg, cell.bg] {
            assert!(ramp.contains(&colour), "{colour:?} at {position:?}");
            if !shown.contains(&colour) {
                shown.push(colour);
            }
        }
    }
    assert!(
        shown.len() > 3 * RAMP.len(),
        "{} shades: {shown:?}",
        shown.len()
    );
}

/// Given: a scene on a twelve by six area.
/// When: it ticks and renders twenty times.
/// Then: on every frame the lowest row of pixels, a quarter of every
/// bottom cell, is at the hottest band. The row over it shades from gold
/// to text and shares its cells, and a cell draws two colours, so a pixel
/// at full heat can show as the nearest of those shades: at least gold.
///
/// Why it matters: the bottom row is the fuel. A tick that decays it puts
/// the fire out within seconds.
#[test]
fn the_bottom_row_stays_at_full_heat() {
    let area = Rect::new(0, 0, 12, 6);
    let mut fires = Banded::new(7);
    for tick in 0..20 {
        fires.tick(area);
        let grid = fires.bands(area, area);
        let bottom = grid.last().unwrap();
        assert!(
            bottom.iter().all(|&band| band >= HOTTEST),
            "tick {tick}: bottom row {bottom:?}"
        );
    }
}

/// The heat climbs two rows of pixels a tick, half a cell, and drifts a
/// pixel to the side for each row, so it comes from as far as two pixels
/// to either side.
const CLIMB: usize = 2;

/// Given: a scene on a sixteen by eight area.
/// When: it ticks forty times, with a render after each.
/// Then: every pixel's stop is at most one above the hottest of the five
/// pixels two rows below it on the frame before. A cell draws two colours,
/// so a grey pixel between two stops can show as the light one, but no
/// further.
///
/// Why it matters: heat that appears from nowhere, or climbs, reads as
/// sparks in the sky instead of a fire.
#[test]
fn heat_only_comes_from_below() {
    // Given:
    let area = Rect::new(0, 0, 16, 8);
    let mut fires = Banded::new(7);
    fires.tick(area);
    let mut previous = fires.bands(area, area);

    // When:
    for tick in 0..40 {
        fires.tick(area);
        let current = fires.bands(area, area);

        // Then:
        for (y, row) in current.iter().enumerate().take(current.len() - CLIMB) {
            let below = &previous[y + CLIMB];
            for (x, &band) in row.iter().enumerate() {
                let hottest_below = (x.saturating_sub(CLIMB)..=(x + CLIMB).min(below.len() - 1))
                    .map(|sx| below[sx])
                    .max()
                    .unwrap();
                assert!(
                    band <= hottest_below + 1,
                    "tick {tick}: stop {band} at ({x}, {y}) above at most {hottest_below}"
                );
            }
        }
        previous = current;
    }
}

/// The heat climbs half a cell a tick, in rows a quarter of a cell tall.
/// Given: a scene on a twelve by twenty-four area, tall enough that the
/// heat cools little in a row.
/// When: it ticks once.
/// Then: the lowest three rows of pixels are warm across the whole width,
/// and every row above them is cold.
///
/// Why it matters: a fire that climbs a whole cell a tick rises at twice
/// its pace, and one drawn in half cells shows its flames in coarse steps.
#[test]
fn the_heat_climbs_half_a_cell_a_tick_in_quarter_cells() {
    // Given:
    let area = Rect::new(0, 0, 12, 24);
    let mut fires = Banded::new(7);

    // When:
    fires.tick(area);

    // Then:
    let grid = fires.bands(area, area);
    let warm = grid.len() - 1 - CLIMB;
    for (y, row) in grid.iter().enumerate() {
        let expected = y >= warm;
        assert!(
            row.iter().all(|&band| (band > 0) == expected),
            "row {y} of {}: {row:?}",
            grid.len()
        );
    }
}

/// Given: a scene on a forty by twenty area.
/// When: it ticks thirty times and renders.
/// Then: some cells draw an octant glyph that splits a half of the cell,
/// across or down.
///
/// Why it matters: a fire drawn only in half blocks has a quarter of the
/// pixels the screen can show, and its flames look coarse.
#[test]
fn the_flames_draw_in_octant_glyphs() {
    // Given:
    let area = Rect::new(0, 0, 40, 20);
    let mut scene = scene(7);

    // When:
    for _ in 0..30 {
        scene.tick(area);
    }
    let buffer = render(&scene, area, area);

    // Then:
    let halves = [0x00, 0x0f, 0xf0, 0xff].map(octant::glyph);
    let split = area
        .positions()
        .filter(|position| {
            let glyph = buffer[*position].symbol().chars().next().unwrap();
            !halves.contains(&glyph)
        })
        .count();
    assert!(split > 0, "every cell is a half block or a space");
}

/// Given: a scene that ticked on a twenty by ten area.
/// When: it ticks and renders on a five by three area inside a larger
/// canvas, then on one cell, then on none.
/// Then: it draws without a panic, the bottom row is at the hottest band,
/// and no cell outside the area is touched.
#[test]
fn a_smaller_area_drops_the_old_buffer() {
    // Given:
    let canvas = Rect::new(0, 0, 10, 10);
    let mut fires = Banded::new(7);
    for _ in 0..5 {
        fires.tick(Rect::new(0, 0, 20, 10));
    }

    // When, then:
    for area in [
        Rect::new(2, 2, 5, 3),
        Rect::new(2, 2, 1, 1),
        Rect::new(2, 2, 0, 0),
    ] {
        fires.tick(area);
        for buffer in fires.renders(area, canvas) {
            for position in canvas.positions() {
                if !area.contains(position) {
                    assert_eq!(buffer[position].symbol(), " ", "glyph outside {area:?}");
                    assert_eq!(buffer[position].bg, Color::Reset, "colour outside {area:?}");
                }
            }
        }
        if let Some(bottom) = fires.bands(area, canvas).last() {
            assert!(bottom.iter().all(|&band| band >= HOTTEST), "{bottom:?}");
        }
    }
}
