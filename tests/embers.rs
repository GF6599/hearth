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
        ..*rose_pine
    }))
}

/// Scenes from one seed, each in two tones: one set of colours in light
/// and the rest in dark. A palette changes no tick, so the scenes run in
/// step. A cell of the whole palette can hold three colours and draws only
/// two, but a cell of two tones draws each pixel in one of them or a grey
/// between them where the scene mixes a colour of the set with another.
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

    /// The colour of each pixel, from the scenes of `each_colour`, or
    /// `None` for a pixel no scene draws in light: a mix, such as a shade
    /// of the glow or of the wood or air the glow lights, which each scene
    /// draws in a grey, and a pixel under a spark.
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

/// The distance between two colours in RGB.
fn distance(one: Color, other: Color) -> f32 {
    match (one, other) {
        (Color::Rgb(r, g, b), Color::Rgb(s, h, c)) => [(r, s), (g, h), (b, c)]
            .into_iter()
            .map(|(one, other)| (f32::from(one) - f32::from(other)).powi(2))
            .sum::<f32>()
            .sqrt(),
        _ => f32::INFINITY,
    }
}

/// Whether `colour` is a mix of `one` and `other`: it lies on the way from
/// one to the other, to a step of rounding, and a twentieth of the way or
/// more from each, so a colour a step from either is no mix.
fn between(colour: Color, one: Color, other: Color) -> bool {
    let (span, near, far) = (
        distance(one, other),
        distance(one, colour),
        distance(colour, other),
    );
    near >= span / 20.0 && far >= span / 20.0 && near + far <= span + 2.0
}

/// Whether `colour` lies on a ramp of `stops`: a stop, or a mix of two
/// stops next to each other.
fn on_ramp(colour: Color, stops: &[Color]) -> bool {
    stops.contains(&colour)
        || stops
            .windows(2)
            .any(|pair| between(colour, pair[0], pair[1]))
}

/// The wood from its shadow to its lit top, and the glow from dull to
/// bright.
const fn wood_ramp() -> [Color; 3] {
    let [subtle, muted, overlay] = wood();
    [overlay, muted, subtle]
}

/// The share of love in a pixel of air the glow lights: its distance from
/// the base over love's.
fn lit_share(colour: Color) -> f32 {
    let palette = &theme::ROSE_PINE;
    distance(palette.base, colour) / distance(palette.base, palette.love)
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
/// Then: of the pixels in one colour of the fire on both frames, some
/// change. Every change is a glow that pulses, wood that starts to glow,
/// or wood that burns away, and none is in the stones or the grate. A mix
/// reads as no colour, so the test reads the base, the stones, the grate,
/// and the wood and the glow where they show a colour of their own, and
/// not their shades between or the air the glow lights.
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
/// base colour or on air the glow lights with love, in a cell above the
/// grate, and some spark reaches the top half of the screen.
///
/// Why it matters: the glyph shrinks and the colour cools as a spark rises,
/// and a spark below the grate or one that never climbs breaks the picture
/// of heat going up a chimney. A spark on the dark base in lit air shows a
/// dark square around it.
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
            assert_eq!(fg, colour, "{glyph} at ({x}, {y})");
            assert!(
                bg == palette.base || between(bg, palette.base, palette.love),
                "{glyph} at ({x}, {y}) on {bg:?}"
            );
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
/// Then: some glowing pixel shows another shade of the glow than a pixel
/// in the same half of its cell.
///
/// Why it matters: each pixel of a log glows on its own pulse. A glow that
/// pulses in blocks of half a cell shows a coarse grain.
#[test]
fn the_glow_pulses_in_pixels_a_quarter_of_a_cell_tall() {
    // Given, when:
    let shown = pixels(&render(&ticked(7, AREA, 40), AREA, AREA));

    // Then:
    let glowing = |colour: Option<Color>| colour.filter(|colour| on_ramp(*colour, &glow()));
    let half = DOWN / 2;
    let fine = (0..shown.len()).any(|y| {
        (0..shown[y].len()).any(|x| {
            let Some(colour) = glowing(shown[y][x]) else {
                return false;
            };
            let (left, top) = (x - x % ACROSS, y - y % half);
            (top..top + half).any(|row| {
                (left..left + ACROSS)
                    .filter_map(|column| glowing(shown[row][column]))
                    .any(|other| other != colour)
            })
        })
    });
    assert!(fine, "each half of a cell glows in one shade");
}

/// The shares of a tick at which a test draws a frame between two ticks.
const BETWEEN: [f64; 3] = [0.25, 0.5, 0.75];

/// The cells that hold a spark, at `progress` from the last tick towards
/// the next.
fn sparks_at(scene: &mut Embers, progress: f64) -> Vec<(u16, u16)> {
    scene.set_progress(progress);
    sparks(&render(scene, AREA, AREA))
        .into_iter()
        .map(|(x, y, ..)| (x, y))
        .collect()
}

/// Given: the scene on an eighty by twenty-four area.
/// When: it runs for four hundred ticks, and draws frames at a quarter, a
/// half and three quarters of the way to the next tick.
/// Then: some spark shows in the cell over its own in a frame between, and
/// it is there on the next tick.
///
/// Why it matters: a spark that climbs only on a tick moves a cell at a
/// time on the tick, at a slow speed seconds apart, and a glide lets it
/// climb when it gets there.
#[test]
fn a_spark_climbs_into_the_next_cell_between_two_ticks() {
    // Given:
    let mut scene = ticked(7, AREA, 1);
    let mut before = sparks_at(&mut scene, 0.0);
    let mut climbed = 0;

    for _ in 0..400 {
        // When:
        let frames: Vec<Vec<(u16, u16)>> = BETWEEN
            .iter()
            .map(|progress| sparks_at(&mut scene, *progress))
            .collect();
        scene.tick(AREA);
        let after = sparks_at(&mut scene, 0.0);

        // Then:
        for &(x, y) in &before {
            let over = (x, y.wrapping_sub(1));
            if !before.contains(&over)
                && after.contains(&over)
                && frames.iter().any(|frame| frame.contains(&over))
            {
                climbed += 1;
            }
        }
        before = after;
    }
    assert!(climbed > 0, "no spark climbed between two ticks");
}

/// The first row of pixels in each column that is not the base colour, in
/// a scene that draws the base in light. A spark's cell has no pixels.
fn tops(buffer: &Buffer) -> Vec<Option<usize>> {
    let shown = pixels(buffer);
    (0..shown[0].len())
        .map(|x| (0..shown.len()).find(|&y| shown[y][x] == Some(DARK)))
        .collect()
}

/// Given: two scenes from one seed, with the base colour in light, on an
/// eighty by twenty-four area.
/// When: both run until a burnt log gives way to a new one, and one is told
/// half a tick after each tick.
/// Then: on each tick the two draw the same frame, and some column whose
/// top is two pixels lower on the next tick, as the new log falls, has its
/// top one pixel lower in the frame half way.
///
/// Why it matters: a log that falls only on a tick drops two pixels at a
/// time, and a glide drops it one pixel at a time. A frame on a tick that
/// changes is a tick that changes.
#[test]
fn a_falling_log_sits_between_two_ticks() {
    // Given:
    let base = two_tone(&[theme::ROSE_PINE.base]);
    let mut glide = Embers::new(Rng::seed_from_u64(7), base);
    let mut plain = Embers::new(Rng::seed_from_u64(7), base);
    let quiet = *BURN_TICKS.start() - 2;
    for _ in 0..quiet {
        glide.tick(AREA);
        plain.tick(AREA);
        glide.set_progress(0.5);
    }
    let mut before = Vec::new();
    let mut between = false;

    for tick in quiet..*BURN_TICKS.end() + 200 {
        // When:
        let half = tops(&render(&glide, AREA, AREA));
        glide.tick(AREA);
        plain.tick(AREA);
        glide.set_progress(0.0);

        // Then:
        let frame = render(&glide, AREA, AREA);
        assert!(frame == render(&plain, AREA, AREA), "tick {tick}");
        let after = tops(&frame);
        between = before.iter().enumerate().any(|(x, top)| match top {
            Some(y) => after[x] == Some(y + 2) && half[x] == Some(y + 1),
            None => false,
        });
        if between {
            break;
        }
        glide.set_progress(0.5);
        before = after;
    }
    assert!(between, "no log fell between two ticks");
}

/// Given: the scene on an eighty by twenty-four area.
/// When: it ticks once and renders.
/// Then: down some column of a log the wood shows more than three shades,
/// its top is lighter than its underside on average, and some pixel of
/// wood is darker than the wood above and below it, a line along the grain.
///
/// Why it matters: a log in three flat bands reads as a striped slab, and
/// a log shaded from its lit top to its dark underside, with lines along
/// its grain, reads as round wood with bark.
#[test]
fn a_log_is_lit_on_top_shaded_under_and_lined_along_its_grain() {
    // Given, when:
    let shown = pixels(&render(&ticked(7, AREA, 1), AREA, AREA));

    // Then:
    let wood = |colour: Option<Color>| colour.filter(|colour| on_ramp(*colour, &wood_ramp()));
    let grate = shown.len() - BELOW_LOGS;
    let mut runs: Vec<Vec<Color>> = Vec::new();
    for x in 0..shown[0].len() {
        let mut run = Vec::new();
        for row in shown.iter().take(grate) {
            match wood(row[x]) {
                Some(colour) => run.push(colour),
                None if run.len() >= LOG_RUN => runs.push(std::mem::take(&mut run)),
                None => run.clear(),
            }
        }
    }
    assert!(!runs.is_empty(), "no column of wood {LOG_RUN} pixels deep");
    let shades = |run: &[Color]| {
        let mut seen: Vec<Color> = Vec::new();
        for colour in run {
            if !seen.contains(colour) {
                seen.push(*colour);
            }
        }
        seen.len()
    };
    assert!(
        runs.iter().any(|run| shades(run) > 3),
        "no column of a log shows more than three shades of wood"
    );
    let mean = |values: Vec<u32>| values.iter().sum::<u32>() / values.len() as u32;
    let tops = mean(runs.iter().map(|run| theme::luma(run[0])).collect());
    let undersides = mean(
        runs.iter()
            .map(|run| theme::luma(run[run.len() - 1]))
            .collect(),
    );
    assert!(
        tops > undersides,
        "the tops of the logs have a mean luma of {tops} and their undersides {undersides}"
    );
    let lined = runs.iter().any(|run| {
        run.windows(3).any(|three| {
            let [above, line, below] = [three[0], three[1], three[2]].map(theme::luma);
            line < above && line < below
        })
    });
    assert!(lined, "no line along the grain of a log");
}

/// The fewest pixels of wood down a column that the shading test reads.
const LOG_RUN: usize = 8;

/// The rings of air around the glow that the fade test reads, by whole
/// pixels from the nearest glowing pixel, and the most love the last ring
/// holds as a share of the first's. The air near the fire also takes the
/// brightest of more glows, so even a light of one strength holds a little
/// less love further out, but no less than this.
const RINGS: std::ops::RangeInclusive<usize> = 1..=4;
const FADED: f32 = 2.0 / 3.0;

/// Given: the scene on an eighty by twenty-four area, and each pixel of
/// the air above the grate in the ring of whole pixels it lies from the
/// nearest glowing pixel.
/// When: it ticks once and renders.
/// Then: some pixels of the air are mixes of the base and love. From the
/// first ring to the fourth each ring of them holds less love on average
/// than the ring inside it, and the fourth under two thirds of the first's.
///
/// Why it matters: a glow that ends at the edge of the wood reads as a
/// picture of embers, and a warm light that fades out into the dark around
/// them reads as a fire in a room. A light of one strength out to its edge
/// reads as a halo cut out of paper.
#[test]
fn the_glow_lights_the_air_around_the_fire() {
    // Given, when:
    let shown = pixels(&render(&ticked(7, AREA, 1), AREA, AREA));

    // Then:
    let palette = &theme::ROSE_PINE;
    let grate = shown.len() - BELOW_LOGS;
    let glowing: Vec<(usize, usize)> = (0..grate)
        .flat_map(|y| (0..shown[y].len()).map(move |x| (x, y)))
        .filter(|&(x, y)| shown[y][x].is_some_and(|colour| on_ramp(colour, &glow())))
        .collect();
    let lit: Vec<(usize, usize, Color)> = (0..grate)
        .flat_map(|y| (0..shown[y].len()).map(move |x| (x, y)))
        .filter_map(|(x, y)| {
            let colour = shown[y][x]?;
            between(colour, palette.base, palette.love).then_some((x, y, colour))
        })
        .collect();
    assert!(!lit.is_empty(), "no air the glow lights");
    let ring = |x: usize, y: usize| {
        glowing
            .iter()
            .map(|(gx, gy)| (gx.abs_diff(x) as f32).hypot(gy.abs_diff(y) as f32))
            .fold(f32::INFINITY, f32::min)
            .round() as usize
    };
    let shares: Vec<f32> = RINGS
        .map(|at| {
            let ring: Vec<f32> = lit
                .iter()
                .filter(|(x, y, _)| ring(*x, *y) == at)
                .map(|(_, _, colour)| lit_share(*colour))
                .collect();
            assert!(!ring.is_empty(), "no lit air in ring {at}");
            ring.iter().sum::<f32>() / ring.len() as f32
        })
        .collect();
    let (first, last) = (shares[0], shares[shares.len() - 1]);
    assert!(
        shares.windows(2).all(|pair| pair[0] > pair[1]) && last < first * FADED,
        "the love in each ring of air from the glow out: {shares:?}"
    );
}

/// Given: the scene on an eighty by twenty-four area.
/// When: it runs forty ticks, with a frame after each.
/// Then: some frame shows a glowing pixel that is a mix of love and rose
/// or of rose and gold.
///
/// Why it matters: a glow in three flat colours pulses in steps, and a
/// glow through the shades between them pulses as embers do.
#[test]
fn the_glow_shades_between_love_rose_and_gold() {
    // Given:
    let mut scene = ticked(7, AREA, 1);
    let [love, rose, gold] = glow();
    let mut shaded = false;

    for _ in 0..40 {
        // When:
        scene.tick(AREA);
        let shown = pixels(&render(&scene, AREA, AREA));

        // Then:
        shaded |= shown
            .iter()
            .flatten()
            .flatten()
            .any(|colour| between(*colour, love, rose) || between(*colour, rose, gold));
    }
    assert!(shaded, "the glow showed only love, rose and gold");
}

/// Given: the scene on an eighty by twenty-four area on Rose Pine with the
/// terminal's background, where the base is see-through.
/// When: it ticks forty times and renders.
/// Then: no pixel is a mix of the base and a colour of the glow: love,
/// rose or gold. The wood's greys lie on a line from the base, so a shade
/// between two of them would read as a mix with the base, and the test
/// reads the glow's colours only.
///
/// Why it matters: a mix with the base is an opaque colour over the
/// terminal's own background, and lit air or a glowing edge drawn in it
/// shows as a smudge around the fire on a transparent terminal.
#[test]
fn over_the_terminal_background_the_air_stays_clear() {
    // Given:
    let palette = &theme::FLAT_THEMES[0];
    assert!(palette.see_through);
    let mut scene = Embers::new(Rng::seed_from_u64(7), palette);

    // When:
    for _ in 0..40 {
        scene.tick(AREA);
    }
    let shown = pixels(&render(&scene, AREA, AREA));

    // Then:
    for (y, row) in shown.iter().enumerate() {
        for (x, colour) in row.iter().enumerate() {
            for other in [palette.love, palette.rose, palette.gold] {
                if let Some(colour) = colour {
                    assert!(
                        !between(*colour, palette.base, other),
                        "{colour:?} at ({x}, {y}) mixes the base and {other:?}"
                    );
                }
            }
        }
    }
}
