#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pixels;

use hearth::octant;
use hearth::scene::{Rng, Scene};
use hearth::scenes::embers::{BURN_TICKS, Embers, SETTLE_TICKS};
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 80, 24);

fn ticked(seed: u64, area: Rect, ticks: usize) -> Embers {
    let mut scene = Embers::new(Rng::seed_from_u64(seed), &theme::ROSE_PINE);
    for _ in 0..ticks {
        scene.tick(area);
    }
    scene
}

fn render(scene: &Embers, area: Rect, canvas: Rect) -> Buffer {
    let mut buffer = Buffer::empty(canvas);
    scene.render(area, &mut buffer);
    buffer
}

/// The pixels across and down in each cell.
const ACROSS: usize = octant::CELL_WIDTH;
const DOWN: usize = octant::CELL_HEIGHT;

/// The grate and the stones under it fill the lowest two cells.
const BELOW_LOGS: usize = 2 * DOWN;

/// The pixels of each cell, as its octant glyph and two colours draw them,
/// by pixel row. A cell that holds a spark instead gives `None` for each.
fn pixels(buffer: &Buffer) -> Vec<Vec<Option<Color>>> {
    pixels::grid(buffer, buffer.area, |_, cell| {
        pixels::shown(cell).map_or([None; pixels::PIXELS], |shown| shown.map(Some))
    })
}

const DARK: Color = Color::Rgb(0, 0, 0);
const LIGHT: Color = Color::Rgb(255, 255, 255);

/// Rose Pine with the colours of `set` in light and every other colour in
/// dark.
fn two_tone(set: &[Color]) -> &'static Palette {
    let tone = |colour: Color| if set.contains(&colour) { LIGHT } else { DARK };
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

/// Scenes from one seed, each in two tones: one set of colours in light
/// and the rest in dark. A palette changes no tick, so the scenes run in
/// step. A cell of the whole palette can hold three colours and draws only
/// two, but a cell of two tones draws each pixel as it is.
struct Toned {
    scenes: Vec<Embers>,
}

impl Toned {
    fn new(seed: u64, sets: &[Vec<Color>]) -> Self {
        Self {
            scenes: sets
                .iter()
                .map(|set| Embers::new(Rng::seed_from_u64(seed), two_tone(set)))
                .collect(),
        }
    }

    /// A scene for each colour the fire draws in, so the one that draws a
    /// pixel light names its colour.
    fn each_colour(seed: u64) -> Self {
        Self::new(seed, &colours().map(|colour| vec![colour]))
    }

    fn tick(&mut self, area: Rect) {
        for scene in &mut self.scenes {
            scene.tick(area);
        }
    }

    /// For each set, whether each pixel is in it, or `None` under a spark.
    fn sets(&self, area: Rect) -> Vec<Vec<Vec<Option<bool>>>> {
        self.scenes
            .iter()
            .map(|scene| {
                pixels(&render(scene, area, area))
                    .into_iter()
                    .map(|row| {
                        row.into_iter()
                            .map(|pixel| pixel.map(|colour| colour == LIGHT))
                            .collect()
                    })
                    .collect()
            })
            .collect()
    }

    /// The colour of each pixel, from the scenes of `each_colour`.
    fn colours(&self, area: Rect) -> Vec<Vec<Option<Color>>> {
        let sets = self.sets(area);
        let mut rows = vec![vec![None; sets[0][0].len()]; sets[0].len()];
        for (y, row) in rows.iter_mut().enumerate() {
            for (x, pixel) in row.iter_mut().enumerate() {
                *pixel = sets
                    .iter()
                    .position(|set| set[y][x] == Some(true))
                    .map(|index| colours()[index]);
            }
        }
        rows
    }
}

const fn glow() -> [Color; 3] {
    [
        theme::ROSE_PINE.love,
        theme::ROSE_PINE.rose,
        theme::ROSE_PINE.gold,
    ]
}

const fn wood() -> [Color; 3] {
    [
        theme::ROSE_PINE.subtle,
        theme::ROSE_PINE.muted,
        theme::ROSE_PINE.overlay,
    ]
}

/// Each colour the fire draws in.
const fn colours() -> [Color; 7] {
    let [subtle, muted, overlay] = wood();
    let [love, rose, gold] = glow();
    [
        theme::ROSE_PINE.base,
        subtle,
        muted,
        overlay,
        love,
        rose,
        gold,
    ]
}

/// Given: the scene on an eighty by twenty-four area.
/// When: it ticks once and renders.
/// Then: every pixel in the top half is the base colour.
///
/// Why it matters: the fire sits low in a dark hearth, and a wall of colour
/// up the screen reads as the doom fire, not as logs on a grate.
#[test]
fn the_top_of_the_screen_stays_dark() {
    // Given, when:
    let shown = pixels(&render(&ticked(7, AREA, 1), AREA, AREA));

    // Then:
    for (y, row) in shown.iter().enumerate().take(shown.len() / 2) {
        for (x, pixel) in row.iter().enumerate() {
            if let Some(colour) = pixel {
                assert_eq!(*colour, theme::ROSE_PINE.base, "pixel ({x}, {y})");
            }
        }
    }
}

/// Given: the scene on an eighty by twenty-four area.
/// When: it ticks once and renders.
/// Then: the lowest pixel row holds stones in muted with overlay mortar, a
/// grate bar in subtle spans at least a third of the width above them, and
/// wood sits above the grate.
///
/// Why it matters: the fire rests on something, so it reads as a
/// fireplace and not as a glow that floats.
#[test]
fn the_logs_rest_on_a_grate_over_hearth_stones() {
    // Given, when:
    let shown = pixels(&render(&ticked(7, AREA, 1), AREA, AREA));
    let palette = &theme::ROSE_PINE;
    let count = |row: &[Option<Color>], colour: Color| {
        row.iter().filter(|pixel| **pixel == Some(colour)).count()
    };

    // Then:
    let bottom = shown.last().unwrap();
    assert!(count(bottom, palette.muted) > 0, "no stones");
    assert!(
        bottom
            .iter()
            .all(|pixel| *pixel == Some(palette.muted) || *pixel == Some(palette.overlay)),
        "stones in another colour: {bottom:?}"
    );
    let grate = (0..shown.len())
        .rev()
        .find(|&y| count(&shown[y], palette.subtle) >= shown[y].len() / 3)
        .expect("no grate bar");
    let wood_above = shown[..grate].iter().any(|row| {
        row.iter()
            .any(|pixel| pixel.is_some_and(|c| c != palette.base))
    });
    assert!(wood_above, "nothing above the grate at row {grate}");
}

/// Given: the scene on an eighty by twenty-four area.
/// When: it renders after one tick and again after forty.
/// Then: some pixels change. Every change is a glow that pulses, wood that
/// starts to glow, or wood that burns away, and none is in the stones or
/// the grate.
///
/// Why it matters: the glow is what makes the fire live, and wood that
/// grows back, or a stone or the grate that flickers, reads as a fault.
#[test]
fn only_the_glow_and_the_burn_change_the_picture() {
    // Given:
    let mut fire = Toned::each_colour(7);
    fire.tick(AREA);
    let before = fire.colours(AREA);

    // When:
    for _ in 0..39 {
        fire.tick(AREA);
    }
    let after = fire.colours(AREA);

    // Then:
    let palette = &theme::ROSE_PINE;
    let grate = before.len() - BELOW_LOGS;
    let mut changed = 0;
    for (y, (old, new)) in before.iter().zip(&after).enumerate() {
        for (x, (old, new)) in old.iter().zip(new).enumerate() {
            let (Some(old), Some(new)) = (old, new) else {
                continue;
            };
            if old == new {
                continue;
            }
            changed += 1;
            assert!(y < grate, "the grate or a stone changed at ({x}, {y})");
            let glows = glow().contains(new);
            let burnt = *new == palette.base && !glow().contains(old);
            assert!(
                glows || burnt,
                "pixel ({x}, {y}) went from {old:?} to {new:?}"
            );
        }
    }
    assert!(changed > 0, "the glow held still");
}

/// A scene in two tones for the wood and two for the glow.
fn wood_and_glow_scenes(seed: u64) -> Toned {
    Toned::new(seed, &[wood().to_vec(), glow().to_vec()])
}

/// Wood and glow pixels above the grate, from the scenes of
/// `wood_and_glow_scenes`.
fn wood_and_glow(fire: &Toned) -> (usize, usize) {
    let sets = fire.sets(AREA);
    let count = |set: &[Vec<Option<bool>>]| {
        set[..set.len() - BELOW_LOGS]
            .iter()
            .flatten()
            .filter(|pixel| **pixel == Some(true))
            .count()
    };
    (count(&sets[0]), count(&sets[1]))
}

/// Given: the scene on an eighty by twenty-four area.
/// When: it runs for half the shortest burn.
/// Then: there is less wood, and a larger share of what is left glows.
///
/// Why it matters: a log that burns for minutes and looks the same at the
/// end as at the start is not burning.
#[test]
fn a_log_burns_shorter_and_glows_more() {
    // Given:
    let mut fire = wood_and_glow_scenes(7);
    fire.tick(AREA);
    let (wood_before, glow_before) = wood_and_glow(&fire);

    // When:
    for _ in 0..*BURN_TICKS.start() / 2 {
        fire.tick(AREA);
    }

    // Then:
    let (wood_after, glow_after) = wood_and_glow(&fire);
    assert!(
        wood_after < wood_before,
        "wood {wood_before} to {wood_after}"
    );
    let share = |wood: usize, glow: usize| glow as f64 / (wood + glow) as f64;
    assert!(
        share(wood_after, glow_after) > share(wood_before, glow_before),
        "glow {glow_before} of {wood_before} to {glow_after} of {wood_after}"
    );
}

/// Given: the scene on an eighty by twenty-four area.
/// When: it runs past the longest burn, sampling the wood every ten ticks.
/// Then: the wood falls and later rises again by at least a log's worth,
/// and some pixel glows in every sample.
///
/// Why it matters: a burnt log must give way to a new one, and the fire
/// must never go out in between.
#[test]
fn a_burnt_log_gives_way_to_a_new_one_and_the_fire_never_goes_out() {
    // Given:
    let mut fire = wood_and_glow_scenes(7);
    fire.tick(AREA);
    let mut samples = Vec::new();

    // When:
    for tick in 0..*BURN_TICKS.end() + 600 {
        fire.tick(AREA);
        if tick % 10 == 0 {
            samples.push(wood_and_glow(&fire));
        }
    }

    // Then:
    assert!(
        samples.iter().all(|&(_, glow)| glow > 0),
        "the fire went out: {samples:?}"
    );
    let low = samples
        .iter()
        .enumerate()
        .min_by_key(|(_, (wood, _))| *wood)
        .map(|(at, _)| at)
        .unwrap();
    let rise = samples[low..]
        .windows(2)
        .map(|pair| pair[1].0.saturating_sub(pair[0].0))
        .max()
        .unwrap_or(0);
    assert!(
        rise >= NEW_LOG_WOOD,
        "no new log after the low point: {samples:?}"
    );
}

/// Wood pixels that only a new log adds between two samples: ten cells.
const NEW_LOG_WOOD: usize = 10 * ACROSS * DOWN;

/// Given: seeds 1 to 5.
/// When: each draws twice from the same seed, and once per seed.
/// Then: one seed draws the same fire both times, and not every seed
/// draws the same fire.
///
/// Why it matters: `--seed` replays a run, and `r` asks for a new stack of
/// logs.
#[test]
fn the_seed_chooses_the_stack_of_logs() {
    let frame = |seed| render(&ticked(seed, AREA, 1), AREA, AREA);

    // When, then:
    assert_eq!(frame(1), frame(1));
    let frames: Vec<Buffer> = (1..=5).map(frame).collect();
    assert!(frames.iter().any(|other| *other != frames[0]));
}

/// Given: areas from nothing to a few cells.
/// When: the scene ticks and renders into each, placed inside a larger
/// buffer.
/// Then: nothing panics, and no cell outside the area changes.
///
/// Why it matters: a split pane can be one cell high, and a scene that
/// draws past its pane paints over its neighbour.
#[test]
fn a_tiny_area_draws_nothing_outside_itself() {
    let canvas = Rect::new(0, 0, 20, 10);
    for (width, height) in [(0, 0), (1, 1), (5, 2), (12, 3)] {
        // Given:
        let area = Rect::new(4, 4, width, height);

        // When:
        let shown = render(&ticked(7, area, 3), area, canvas);

        // Then:
        let blank = Buffer::empty(canvas);
        for position in canvas.positions() {
            if !area.contains(position) {
                assert_eq!(shown[position], blank[position], "{area} at {position:?}");
            }
        }
    }
}

/// The cells that hold a spark, as `(x, y, glyph, fg, bg)`.
fn sparks(buffer: &Buffer) -> Vec<(u16, u16, String, Color, Color)> {
    buffer
        .area
        .positions()
        .filter_map(|position| {
            let cell = &buffer[position];
            ["•", "·", "."].contains(&cell.symbol()).then(|| {
                (
                    position.x,
                    position.y,
                    cell.symbol().to_string(),
                    cell.fg,
                    cell.bg,
                )
            })
        })
        .collect()
}

/// Given: the scene on an eighty by twenty-four area.
/// When: it runs for four hundred ticks, with a frame after each.
/// Then: every spark is `•` in gold, `·` in rose or `.` in muted, on the
/// base colour, in a cell above the grate, and some spark reaches the top
/// half of the screen.
///
/// Why it matters: the glyph shrinks and the colour cools as a spark rises,
/// and a spark below the grate or one that never climbs breaks the picture
/// of heat going up a chimney.
#[test]
fn sparks_rise_above_the_grate_and_cool_as_they_shrink() {
    // Given:
    let mut scene = ticked(7, AREA, 1);
    let palette = &theme::ROSE_PINE;
    let grate_cell = AREA.height - 2;
    let mut seen = 0;
    let mut high = false;

    for _ in 0..400 {
        // When:
        scene.tick(AREA);
        let shown = render(&scene, AREA, AREA);

        // Then:
        for (x, y, glyph, fg, bg) in sparks(&shown) {
            let colour = match glyph.as_str() {
                "•" => palette.gold,
                "·" => palette.rose,
                _ => palette.muted,
            };
            assert_eq!((fg, bg), (colour, palette.base), "{glyph} at ({x}, {y})");
            assert!(
                y < grate_cell,
                "{glyph} at ({x}, {y}) is not above the grate"
            );
            seen += 1;
            high |= y < AREA.height / 2;
        }
    }
    assert!(seen > 0, "no spark");
    assert!(high, "no spark reached the top half");
}

/// Given: the scene on an eighty by twenty-four area.
/// When: it runs past the longest wait before a settle, counting the young
/// sparks, the gold `•`, in each frame.
/// Then: the count reaches a burst's level, and first does so no sooner
/// than the shortest wait.
///
/// Why it matters: a settle is the rare event that makes the fire feel
/// alive, and one that comes at once or never is no event at all.
#[test]
fn a_log_settles_with_a_burst_of_sparks_after_a_while() {
    // Given:
    let mut scene = ticked(7, AREA, 1);
    let last = usize::try_from(*SETTLE_TICKS.end()).unwrap() + 60;

    // When:
    let young: Vec<usize> = (0..last)
        .map(|_| {
            scene.tick(AREA);
            let shown = sparks(&render(&scene, AREA, AREA));
            shown
                .iter()
                .filter(|(.., glyph, _, _)| glyph == "•")
                .count()
        })
        .collect();

    // Then:
    let first = young
        .iter()
        .position(|&count| count >= BURST_YOUNG)
        .unwrap_or_else(|| panic!("no burst: {young:?}"));
    let soonest = usize::try_from(*SETTLE_TICKS.start()).unwrap();
    assert!(first >= soonest, "a burst at tick {first}: {young:?}");
}

/// Young sparks on screen that only a burst reaches. Over forty seeds the
/// usual flow showed at most ten and a burst at least eighteen.
const BURST_YOUNG: usize = 14;

/// Given: the scene on an eighty by twenty-four area.
/// When: it runs past the longest burn and a little more, so a new log
/// has dropped in from above the pile.
/// Then: every pixel in the top half is the base colour again.
///
/// Why it matters: a new log that does not fall hangs in the air over the
/// fire.
#[test]
fn a_new_log_falls_onto_the_pile() {
    // Given, when:
    let ticks = usize::try_from(*BURN_TICKS.end()).unwrap() + 200;
    let shown = pixels(&render(&ticked(7, AREA, ticks), AREA, AREA));

    // Then:
    for (y, row) in shown.iter().enumerate().take(shown.len() / 2) {
        for (x, pixel) in row.iter().enumerate() {
            if let Some(colour) = pixel {
                assert_eq!(*colour, theme::ROSE_PINE.base, "pixel ({x}, {y})");
            }
        }
    }
}

/// Given: the scene on an eighty by twenty-four area.
/// When: it ticks forty times and renders.
/// Then: some cells draw an octant glyph that splits a half of the cell,
/// across or down.
///
/// Why it matters: logs drawn only in half blocks have a quarter of the
/// pixels the screen can show, and their glow looks coarse.
#[test]
fn the_fire_draws_in_octant_glyphs() {
    // Given, when:
    let buffer = render(&ticked(7, AREA, 40), AREA, AREA);

    // Then:
    let halves = [0x00, 0x0f, 0xf0, 0xff].map(octant::glyph);
    let split = AREA
        .positions()
        .filter(|position| {
            let cell = &buffer[*position];
            pixels::bits(cell).is_some()
                && !cell.symbol().chars().all(|glyph| halves.contains(&glyph))
        })
        .count();
    assert!(split > 0, "every block is a half block or a space");
}

/// Given: the scene on an eighty by twenty-four area.
/// When: it ticks forty times and renders.
/// Then: some glowing pixel shows another colour of the glow than a pixel
/// in the same half of its cell.
///
/// Why it matters: each pixel of a log glows on its own pulse. A glow that
/// pulses in blocks of half a cell shows a coarse grain.
#[test]
fn the_glow_pulses_in_pixels_a_quarter_of_a_cell_tall() {
    // Given:
    let mut fire = Toned::each_colour(7);

    // When:
    for _ in 0..40 {
        fire.tick(AREA);
    }
    let shown = fire.colours(AREA);

    // Then:
    let half = DOWN / 2;
    let fine = (0..shown.len()).any(|y| {
        (0..shown[y].len()).any(|x| {
            let Some(colour) = shown[y][x].filter(|colour| glow().contains(colour)) else {
                return false;
            };
            let (left, top) = (x - x % ACROSS, y - y % half);
            (top..top + half).any(|row| {
                (left..left + ACROSS)
                    .filter_map(|column| shown[row][column])
                    .any(|other| other != colour && glow().contains(&other))
            })
        })
    });
    assert!(fine, "each half of a cell glows in one colour");
}
