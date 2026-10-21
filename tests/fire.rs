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

const PALETTE: [Color; 12] = [
    theme::ROSE_PINE.base,
    theme::ROSE_PINE.surface,
    theme::ROSE_PINE.overlay,
    theme::ROSE_PINE.muted,
    theme::ROSE_PINE.subtle,
    theme::ROSE_PINE.text,
    theme::ROSE_PINE.love,
    theme::ROSE_PINE.gold,
    theme::ROSE_PINE.rose,
    theme::ROSE_PINE.pine,
    theme::ROSE_PINE.foam,
    theme::ROSE_PINE.iris,
];

fn band_of(colour: Color) -> usize {
    RAMP.iter()
        .position(|stop| *stop == colour)
        .unwrap_or_else(|| panic!("{colour:?} is not on the ramp"))
}

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
/// other colour in dark. A cell of the whole ramp can hold three colours
/// and draws only two, but a cell of two tones draws each pixel as it is.
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

/// The dither pattern repeats every four pixels in each direction.
const TILE: usize = 4;

fn tile() -> impl Iterator<Item = (usize, usize)> {
    (0..TILE).flat_map(|row| (0..TILE).map(move |x| (x, row)))
}

/// Given: the ramp.
/// When: each heat from none to full is read at each pixel of a tile.
/// Then: at every pixel the colours climb through base, overlay, muted,
/// love, rose, gold and text, in that order, with each stop shown.
///
/// Why it matters: a blend between two stops makes a colour that Rose Pine
/// does not have, and a skipped stop leaves a jump in the flame.
#[test]
fn the_ramp_climbs_through_the_seven_stops() {
    for (x, row) in tile() {
        // When:
        let mut shown: Vec<Color> = (0..=MAX_HEAT)
            .map(|heat| fire::color(&theme::ROSE_PINE, heat, x, row))
            .collect();
        shown.dedup();

        // Then:
        assert_eq!(shown, RAMP, "pixel ({x}, {row})");
    }
}

/// Given: each light palette, Rose Pine Dawn, Catppuccin Latte, Tokyo
/// Night Day, Kanagawa Lotus, Alucard and Solarized Light, where text is
/// the darkest role.
/// When: each heat from none to full is read at each pixel of a tile.
/// Then: at every pixel the colours climb through base, overlay, muted,
/// love, rose and gold, and end at gold.
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
        let ramp = [
            light.base,
            light.overlay,
            light.muted,
            light.love,
            light.rose,
            light.gold,
        ];
        for (x, row) in tile() {
            // When:
            let mut shown: Vec<Color> = (0..=MAX_HEAT)
                .map(|heat| fire::color(light, heat, x, row))
                .collect();
            shown.dedup();

            // Then:
            assert_eq!(shown, ramp, "pixel ({x}, {row})");
        }
    }
}

/// Given: the ramp.
/// When: each heat from none to full is drawn over one tile.
/// Then: the tile holds at most two neighbouring stops, and the sum of its
/// stops rises one step at a time from all base to all text.
///
/// Why it matters: this is the dither. A tile that jumps from one stop to
/// the next shows the hard bands that the dither exists to hide.
#[test]
fn a_tile_shades_from_each_stop_to_the_next() {
    let mut shades = Vec::new();
    for heat in 0..=MAX_HEAT {
        // When:
        let tile: Vec<usize> = tile()
            .map(|(x, row)| band_of(fire::color(&theme::ROSE_PINE, heat, x, row)))
            .collect();

        // Then:
        let (low, high) = (tile.iter().min().unwrap(), tile.iter().max().unwrap());
        assert!(high - low <= 1, "heat {heat}: {tile:?}");
        shades.push(tile.iter().sum::<usize>());
    }
    shades.dedup();
    let every_step: Vec<usize> = (0..=FULL * TILE * TILE).collect();
    assert_eq!(shades, every_step);
}

/// Given: a scene on a forty by twenty area.
/// When: it ticks thirty times and renders.
/// Then: every cell's foreground and background is a Rose Pine colour.
///
/// Why it matters: one colour outside the palette reads as a fault on a
/// surface where every other cell is Rose Pine.
#[test]
fn every_cell_is_a_rose_pine_colour() {
    // Given:
    let area = Rect::new(0, 0, 40, 20);
    let mut scene = scene(7);

    // When:
    for _ in 0..30 {
        scene.tick(area);
    }
    let buffer = render(&scene, area, area);

    // Then:
    for position in area.positions() {
        let cell = &buffer[position];
        for colour in [cell.fg, cell.bg] {
            assert!(PALETTE.contains(&colour), "{colour:?} at {position:?}");
        }
    }
}

/// Given: a scene on a twelve by six area.
/// When: it ticks and renders twenty times.
/// Then: on every frame the lowest row of pixels, a quarter of every
/// bottom cell, is at full heat.
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
            bottom.iter().all(|&band| band == FULL),
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
/// pixels two rows below it on the frame before, which is as far as the
/// dither can lift a pixel.
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
/// Then: it draws without a panic, the bottom row is at full heat, and no
/// cell outside the area is touched.
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
            assert!(bottom.iter().all(|&band| band == FULL), "{bottom:?}");
        }
    }
}
