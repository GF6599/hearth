#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pixels;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, OnceLock};

use hearth::config::Background;
use hearth::corpus::Corpus;
use hearth::octant::{self, CELL_HEIGHT, CELL_WIDTH};
use hearth::scene::{Rng, Scene, SceneKind, Settings};
use hearth::scenes::murmuration::{
    FLOCK, HAWK_CLEARANCE, HAWK_EVERY, Murmuration, PIXELS_PER_BIRD,
};
use hearth::scenes::typing::Speed;
use hearth::theme::{self, Palette};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);

/// The steps of a share at which a test reads a mix, fine enough that a
/// share between two steps rounds to a colour one level from either.
const SHARE_STEPS: u16 = 4096;

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

/// Each pixel of a render in one of `colours`, in pixels from the top left
/// of the area.
fn pixels_in(buffer: &Buffer, area: Rect, colours: &[Color]) -> BTreeSet<(usize, usize)> {
    pixels::showing(buffer, area, |colour| colours.contains(&colour))
        .into_iter()
        .map(|(place, _)| place)
        .collect()
}

/// The steps of each mix a test has read, by its two ends: each colour of
/// the mix with the first step that rounds to it.
type Mixes = HashMap<(Color, Color), HashMap<Color, u16>>;

/// How far `colour` lies along the mix from `from` toward `to`, from 0 to
/// 1, or none for a colour off it.
fn share_on(colour: Color, from: Color, to: Color) -> Option<f32> {
    static MIXES: OnceLock<Mutex<Mixes>> = OnceLock::new();
    let mut mixes = MIXES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap();
    let mixes = mixes.entry((from, to)).or_insert_with(|| {
        let mut steps = HashMap::new();
        for step in 0..=SHARE_STEPS {
            let mix = octant::mix(from, to, f32::from(step) / f32::from(SHARE_STEPS));
            steps.entry(mix).or_insert(step);
        }
        steps
    });
    let Color::Rgb(r, g, b) = colour else {
        return None;
    };
    if let Some(step) = mixes.get(&colour) {
        return Some(f32::from(*step) / f32::from(SHARE_STEPS));
    }
    let near = |channel: u8| channel.saturating_sub(1)..=channel.saturating_add(1);
    near(r)
        .flat_map(|r| near(g).flat_map(move |g| near(b).map(move |b| Color::Rgb(r, g, b))))
        .find_map(|near| mixes.get(&near))
        .map(|step| f32::from(*step) / f32::from(SHARE_STEPS))
}

/// Whether `colour` shows a bird: lighter than the lightest role of the
/// sky by an eighth of the way to subtle, the faintest a bird draws in.
fn lit_by_a_bird(colour: Color) -> bool {
    let (sky, faint) = (
        luma(theme::ROSE_PINE.overlay),
        luma(theme::ROSE_PINE.subtle),
    );
    luma(colour) * 8 >= sky * 8 + (faint - sky)
}

/// Every pixel of the sky a bird lights, in pixels from the top left of
/// the area, with its lightness.
fn lit_pixels(buffer: &Buffer, area: Rect) -> BTreeMap<(usize, usize), u32> {
    let sky = usize::from(tree_rows(buffer, &theme::ROSE_PINE)[0] - area.y) * CELL_HEIGHT;
    pixels::showing(buffer, area, lit_by_a_bird)
        .into_iter()
        .filter(|((_, y), _)| *y < sky)
        .map(|(place, colour)| (place, luma(colour)))
        .collect()
}

/// The colour most pixels of each row of pixels show, from the top.
fn rows_of(buffer: &Buffer, area: Rect) -> Vec<Color> {
    let grid = pixels::grid(buffer, area, |_, cell| {
        pixels::shown(cell).unwrap_or([Color::Reset; pixels::PIXELS])
    });
    grid.iter()
        .map(|row| {
            let mut counts: Vec<(Color, usize)> = Vec::new();
            for colour in row {
                match counts.iter_mut().find(|(each, _)| each == colour) {
                    Some((_, count)) => *count += 1,
                    None => counts.push((*colour, 1)),
                }
            }
            counts
                .into_iter()
                .max_by_key(|(_, count)| *count)
                .map_or(Color::Reset, |(colour, _)| colour)
        })
        .collect()
}

/// Each pixel of the hawk, flying at `height` pixels from the top, with
/// how far it lies along the mix from the sky of its row toward love: every
/// pixel within a cell of its height a quarter of the way or more along
/// that mix. Near the sky every mix comes close to every other, so a faint
/// bird would read as the hawk too.
fn hawk_pixels(buffer: &Buffer, area: Rect, height: f32) -> BTreeMap<(usize, usize), f32> {
    let skies = rows_of(buffer, area);
    let rows =
        (height - CELL_HEIGHT as f32).max(0.0) as usize..=(height + CELL_HEIGHT as f32) as usize;
    pixels::showing(buffer, area, |_| true)
        .into_iter()
        .filter(|((_, y), _)| rows.contains(y))
        .filter_map(|((x, y), colour)| {
            let sky = skies[y];
            if colour == sky {
                return None;
            }
            let share = share_on(colour, sky, theme::ROSE_PINE.love)?;
            (share >= 0.25).then_some(((x, y), share))
        })
        .collect()
}

/// The pixels of the cell that holds `pixel`.
fn cell_of(pixel: (usize, usize)) -> impl Iterator<Item = (usize, usize)> {
    let (left, top) = (
        pixel.0 - pixel.0 % CELL_WIDTH,
        pixel.1 - pixel.1 % CELL_HEIGHT,
    );
    (0..CELL_WIDTH * CELL_HEIGHT)
        .map(move |index| (left + index % CELL_WIDTH, top + index / CELL_WIDTH))
}

fn pixels_of(area: Rect) -> (f32, f32) {
    (
        f32::from(area.width) * CELL_WIDTH as f32,
        f32::from(area.height) * CELL_HEIGHT as f32,
    )
}

fn assert_inside(scene: &Murmuration, area: Rect, when: &str) {
    let (width, height) = pixels_of(area);
    for (x, y) in scene.birds() {
        assert!(
            (0.0..width).contains(&x) && (0.0..height).contains(&y),
            "{when}: bird at ({x}, {y}) outside {width} by {height} pixels"
        );
    }
}

/// Given: a small, a mid-sized and a wide area.
/// When: the scene ticks once on each.
/// Then: the flock holds one bird for each `PIXELS_PER_BIRD` pixels, and no
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
        let count = usize::from(area.width) * usize::from(area.height) * CELL_WIDTH * CELL_HEIGHT;
        let expected = (count / PIXELS_PER_BIRD).clamp(*FLOCK.start(), *FLOCK.end());
        assert_eq!(scene.birds().count(), expected, "{area:?}");
    }
}

/// Given: the scene on a hundred by thirty area, fifty ticks in.
/// When: it renders.
/// Then: every bird lights the pixel it sits on or one beside it, and every
/// pixel a bird lights lies in a cell with a pixel within one pixel of a
/// bird. A bird shares its light between the four pixels nearest its
/// middle, and a cell shows two colours, so its light can fall on any pixel
/// of the cell.
///
/// Why it matters: a wrong bit in the octant table draws a bird away from
/// its place, and the folds of the flock smear.
#[test]
fn every_bird_shows_where_it_sits_and_every_light_is_a_bird() {
    // Given:
    let scene = ticked(7, AREA, 50);

    // When:
    let shown = lit_pixels(&render(&scene, AREA, AREA), AREA);

    // Then:
    let birds: Vec<(f32, f32)> = scene.birds().collect();
    let near = |place: (usize, usize), bird: (f32, f32)| {
        (place.0 as f32 - bird.0.floor()).abs() <= 1.0
            && (place.1 as f32 - bird.1.floor()).abs() <= 1.0
    };
    for bird in &birds {
        assert!(
            shown.keys().any(|pixel| near(*pixel, *bird)),
            "no light at the bird at {bird:?}"
        );
    }
    for pixel in shown.keys() {
        assert!(
            cell_of(*pixel).any(|place| birds.iter().any(|bird| near(place, *bird))),
            "a light at {pixel:?} with no bird"
        );
    }
}

/// Given: the scene on a hundred by thirty area, a hundred ticks in.
/// When: it renders.
/// Then: every cell above the trees is octant pixels, and the lightest
/// pixel of a cell with three birds or more is lighter on average than
/// that of a cell with one or two.
///
/// Why it matters: a cell holds the sky and one shade, so the birds in it
/// are what shades the flock from a faint edge to a bright core.
#[test]
fn a_cell_with_more_birds_is_brighter() {
    // Given, when:
    let scene = ticked(7, AREA, 100);
    let buffer = render(&scene, AREA, AREA);

    // Then:
    let first_tree = tree_rows(&buffer, &theme::ROSE_PINE)[0];
    let mut birds: HashMap<(usize, usize), usize> = HashMap::new();
    for (x, y) in scene.birds() {
        *birds
            .entry((x as usize / CELL_WIDTH, y as usize / CELL_HEIGHT))
            .or_insert(0) += 1;
    }
    let (mut faint, mut bright) = ((0_u64, 0_u64), (0_u64, 0_u64));
    for position in AREA.positions().filter(|position| position.y < first_tree) {
        let shown = pixels::shown(&buffer[position])
            .unwrap_or_else(|| panic!("{:?} at {position:?}", buffer[position].symbol()));
        let lightest = u64::from(shown.into_iter().map(luma).max().unwrap_or(0));
        let sum = match birds.get(&(usize::from(position.x), usize::from(position.y))) {
            None => continue,
            Some(1 | 2) => &mut faint,
            Some(_) => &mut bright,
        };
        *sum = (sum.0 + lightest, sum.1 + 1);
    }
    assert!(faint.1 > 0 && bright.1 > 0, "{faint:?} {bright:?}");
    assert!(
        bright.0 * faint.1 > faint.0 * bright.1,
        "cells of three birds or more {bright:?}, of one or two {faint:?}"
    );
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
/// within four pixels, and the middle of the flock moves across at least a
/// third of the width.
///
/// Why it matters: a flock that scatters is noise, and a flock that holds
/// one place is a cloud and not a murmuration.
#[test]
fn the_flock_holds_together_and_wanders() {
    // Given:
    let mut scene = ticked(7, AREA, 300);
    let (width, _) = pixels_of(AREA);

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
    assert!(!lit_pixels(&buffer, AREA).is_empty());
}

/// The rows of a render that hold the trees, top first: from the row of the
/// highest pixel of tree that stands on the ground to the bottom. The sky
/// between the trees draws as the sky does, so a row is known by its
/// trees, and a bird in the colour of a tree does not stand on the ground.
fn tree_rows(buffer: &Buffer, palette: &Palette) -> Vec<u16> {
    let area = buffer.area;
    let trees = pixels_in(buffer, area, &[palette.muted]);
    let bottom = usize::from(area.height) * CELL_HEIGHT;
    let highest = (0..usize::from(area.width) * CELL_WIDTH)
        .filter_map(|x| {
            (0..bottom)
                .rev()
                .take_while(|y| trees.contains(&(x, *y)))
                .last()
        })
        .min()
        .expect("no trees");
    (u16::try_from(highest / CELL_HEIGHT).unwrap()..area.bottom()).collect()
}

/// Given: the scene on a hundred by thirty area.
/// When: it ticks once and renders.
/// Then: above the trees, away from the cells within a pixel of a bird,
/// each row of pixels shows one colour, no row is darker than the row
/// above it, the top row is base and the row over the trees is overlay,
/// and the sky shows more colours than its three roles.
///
/// Why it matters: the sky is dusk, light low over the trees and dark
/// above, and a sky in bands or in a checkered dither reads as stripes in
/// the terminal, not as a sky.
#[test]
fn the_sky_lightens_smoothly_toward_the_trees() {
    // Given, when:
    let scene = ticked(7, AREA, 1);
    let buffer = render(&scene, AREA, AREA);

    // Then:
    let palette = &theme::ROSE_PINE;
    let flown: BTreeSet<(usize, usize)> = scene
        .birds()
        .flat_map(|(x, y)| {
            let (x, y) = (x.floor() as usize, y.floor() as usize);
            [
                (x.saturating_sub(1), y.saturating_sub(1)),
                (x + 1, y + 1),
                (x.saturating_sub(1), y + 1),
                (x + 1, y.saturating_sub(1)),
            ]
        })
        .map(|(x, y)| (x / CELL_WIDTH, y / CELL_HEIGHT))
        .collect();
    let sky = usize::from(tree_rows(&buffer, palette)[0]) * CELL_HEIGHT;
    let grid = pixels::grid(&buffer, AREA, |_, cell| {
        pixels::shown(cell).unwrap_or([Color::Reset; pixels::PIXELS])
    });
    let mut rows = Vec::new();
    for (y, row) in grid.iter().enumerate().take(sky) {
        let colours: BTreeSet<(u8, u8, u8)> = row
            .iter()
            .enumerate()
            .filter(|(x, _)| !flown.contains(&(x / CELL_WIDTH, y / CELL_HEIGHT)))
            .map(|(_, colour)| match colour {
                Color::Rgb(r, g, b) => (*r, *g, *b),
                other => panic!("{other:?} in row {y}"),
            })
            .collect();
        assert_eq!(colours.len(), 1, "row {y}: {colours:?}");
        let (r, g, b) = colours.into_iter().next().unwrap();
        rows.push(Color::Rgb(r, g, b));
    }
    assert!(
        rows.windows(2).all(|pair| luma(pair[0]) <= luma(pair[1])),
        "{rows:?}"
    );
    assert_eq!(rows.first(), Some(&palette.base), "{rows:?}");
    assert_eq!(rows.last(), Some(&palette.overlay), "{rows:?}");
    let shades: BTreeSet<u32> = rows.iter().map(|colour| luma(*colour)).collect();
    assert!(shades.len() > 3, "{} shades", shades.len());
}

/// Given: the scene on a hundred by thirty area.
/// When: it ticks once and renders.
/// Then: the tree rows hold pixels on the mix from the overlay sky toward
/// the muted trees, each column of pixels turns toward the trees from top
/// to bottom and never back, the lowest row of pixels is all tree, and the
/// trees, counted to the pixels at least half way to muted, stand at three
/// or more heights, none of them filling every tree row.
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
    for x in 0..usize::from(AREA.width) * CELL_WIDTH {
        let shares: Vec<f32> = trees
            .iter()
            .flat_map(|&y| {
                let position = ((x / CELL_WIDTH) as u16, y);
                let shown = pixels::shown(&buffer[position])
                    .unwrap_or_else(|| panic!("{:?} at {position:?}", buffer[position].symbol()));
                (0..CELL_HEIGHT).map(move |row| shown[row * CELL_WIDTH + x % CELL_WIDTH])
            })
            .map(|colour| {
                share_on(colour, palette.overlay, palette.muted)
                    .unwrap_or_else(|| panic!("column {x}: {colour:?}"))
            })
            .collect();
        assert!(
            shares.windows(2).all(|pair| pair[0] <= pair[1]),
            "column {x}: {shares:?}"
        );
        let ground = buffer[((x / CELL_WIDTH) as u16, *trees.last().unwrap())].clone();
        assert_eq!(
            pixels::shown(&ground).map(|shown| shown[pixels::PIXELS - CELL_WIDTH + x % CELL_WIDTH]),
            Some(palette.muted),
            "column {x} has no ground"
        );
        heights.insert(shares.iter().filter(|share| **share >= 0.5).count());
    }
    assert!(heights.len() >= 3, "{heights:?}");
    assert!(
        heights.iter().all(|h| *h < trees.len() * CELL_HEIGHT),
        "a tree fills every row: {heights:?}"
    );
}

/// The tops of the trees mix into the sky.
/// Given: the scene on a hundred by thirty area.
/// When: it ticks once and renders.
/// Then: some pixel of the tree rows lies part way along the mix from the
/// overlay sky toward the muted trees, neither of them.
///
/// Why it matters: a crown of whole pixels steps up in blocks, and reads as
/// a stair and not as a rounded tree.
#[test]
fn the_tops_of_the_trees_mix_into_the_sky() {
    // Given, when:
    let buffer = render(&ticked(7, AREA, 1), AREA, AREA);

    // Then:
    let palette = &theme::ROSE_PINE;
    let first = usize::from(tree_rows(&buffer, palette)[0]) * CELL_HEIGHT;
    let soft = pixels::showing(&buffer, AREA, |colour| {
        colour != palette.overlay && colour != palette.muted
    })
    .into_iter()
    .filter(|((_, y), colour)| {
        *y >= first && share_on(*colour, palette.overlay, palette.muted).is_some()
    })
    .count();
    assert!(soft > 0, "no pixel of the trees mixes into the sky");
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
                (y as usize) / CELL_HEIGHT < usize::from(first_tree),
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
/// Then: the top row of pixels of the sky is the darkest of base, surface
/// and overlay, and the row of pixels over the trees is the lightest.
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
        let low = usize::from(tree_rows(&buffer, palette)[0]) * CELL_HEIGHT - 1;
        let grid = pixels::grid(&buffer, AREA, |_, cell| {
            pixels::shown(cell).unwrap_or([Color::Reset; pixels::PIXELS])
        });
        for (x, (top, over)) in grid[0].iter().zip(&grid[low]).enumerate() {
            assert_eq!(*top, darkest, "{name}: top at {x}");
            assert_eq!(*over, lightest, "{name}: low at {x}");
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
    let (width, _) = pixels_of(AREA);
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
/// `HAWK_CLEARANCE` pixels of it.
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
/// Then: some pixels lie on the mix from the sky of their row toward love,
/// one of them at least half way, and every one of them lies in a cell
/// with a pixel within four pixels across and two up or down of the hawk.
/// A cell shows two colours, so the hawk's colour can fall on any pixel of
/// a cell it crosses.
///
/// Why it matters: a pixel in love away from the hawk reads as a smear of
/// it or as a second hawk.
#[test]
fn the_hawk_draws_in_love_over_the_flock() {
    // Given:
    let (width, _) = pixels_of(AREA);
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
    let love = hawk_pixels(&buffer, AREA, hy);
    for pixel in love.keys() {
        assert!(
            cell_of(*pixel)
                .any(|(x, y)| { (x as f32 - hx).abs() <= 4.0 && (y as f32 - hy).abs() <= 2.0 }),
            "pixel {pixel:?} toward love, the hawk at ({hx}, {hy})"
        );
    }
    assert!(
        love.values().any(|share| *share >= 0.5),
        "the hawk is not drawn: {love:?}"
    );
}

/// A bird alone shares its light between the pixels it overlaps.
/// Given: the scene on a hundred by thirty area, a hundred ticks in.
/// When: it ticks and renders for half a minute.
/// Then: birds fly alone, with no other bird within `ALONE` pixels, and
/// some of them light two pixels or more round their places.
///
/// Why it matters: a bird that lights one whole pixel jumps from pixel to
/// pixel, and the flock reads as a grid of dots and not as birds in the
/// air.
#[test]
fn a_bird_alone_shares_its_light_between_the_pixels_it_overlaps() {
    // Given:
    let mut scene = ticked(7, AREA, 100);
    let (mut most, mut seen) = (0, 0);

    for _ in 0..500 {
        // When:
        scene.tick(AREA);
        let lit = lit_pixels(&render(&scene, AREA, AREA), AREA);

        // Then:
        let birds: Vec<(f32, f32)> = scene.birds().collect();
        for (index, bird) in birds.iter().enumerate() {
            let alone = birds.iter().enumerate().all(|(other, near)| {
                other == index || (near.0 - bird.0).hypot(near.1 - bird.1) > ALONE
            });
            if !alone {
                continue;
            }
            seen += 1;
            let (x, y) = (bird.0.floor(), bird.1.floor());
            let round = lit
                .keys()
                .filter(|(px, py)| (*px as f32 - x).abs() <= 1.0 && (*py as f32 - y).abs() <= 1.0)
                .count();
            most = most.max(round);
        }
    }
    assert!(seen > 0, "no bird flew alone");
    assert!(most >= 2, "a bird alone lights at most {most} pixels");
}

/// The hawk mixes into the sky at its edges.
/// Given: the scene on a hundred by thirty area, ticked until the hawk is
/// in the middle third of the pane.
/// When: it renders.
/// Then: the hawk's pixels lie at three or more places along the mix from
/// the sky toward love.
///
/// Why it matters: a hawk of whole pixels in one colour reads as a stamp
/// that steps across the sky, and not as a bird.
#[test]
fn the_hawk_mixes_into_the_sky_at_its_edges() {
    // Given:
    let (width, _) = pixels_of(AREA);
    let mut scene = Murmuration::new(Rng::seed_from_u64(7), &theme::ROSE_PINE);
    let (_, hy) = (0..HAWK_EVERY.end() + 600)
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
    let shares: BTreeSet<u32> = hawk_pixels(&buffer, AREA, hy)
        .values()
        .map(|share| (share * f32::from(SHARE_STEPS)) as u32)
        .collect();
    assert!(shares.len() >= 3, "the hawk shows {shares:?}");
}

/// The shares of a tick that the frames between two ticks show.
const BETWEEN: [f64; 3] = [0.25, 0.5, 0.75];
/// No other bird comes this close to a bird that a test follows alone, in
/// pixels, on either tick. A bird moves less than two pixels a tick, so
/// the two never share a pixel between the ticks.
const ALONE: f32 = 8.0;

/// The pixels a bird can light from one tick to the next: every pixel from
/// the pixel of the first place to the pixel of the second, on both axes,
/// and one beside them, as a bird shares its light with the pixels nearest
/// its middle.
fn span(from: (f32, f32), to: (f32, f32)) -> ((usize, usize), (usize, usize)) {
    let (a, b) = (
        (from.0 as usize, from.1 as usize),
        (to.0 as usize, to.1 as usize),
    );
    (
        (
            a.0.min(b.0).saturating_sub(1),
            a.1.min(b.1).saturating_sub(1),
        ),
        (a.0.max(b.0) + 1, a.1.max(b.1) + 1),
    )
}

fn within(pixel: (usize, usize), span: ((usize, usize), (usize, usize))) -> bool {
    (span.0.0..=span.1.0).contains(&pixel.0) && (span.0.1..=span.1.1).contains(&pixel.1)
}

/// Each bird glides from its place on one tick to its place on the next.
/// Given: the scene on a hundred by thirty area, a hundred ticks in.
/// When: on each tick for half a minute, it renders at a quarter, a half
/// and three quarters of the way to its next tick, then ticks.
/// Then: every pixel a bird lights lies in a cell with a pixel between the
/// pixels of some bird on the two ticks, and between its two pixels each
/// bird that flies alone shows. A cell shows two colours, so a bird's light
/// can fall on any pixel of a cell it crosses, and in a fold of the flock a
/// faint bird takes the colour of the brighter birds round it. Of the birds
/// that fly alone and change pixel from one tick to the next, more than
/// half show their lightest pixel in a new pixel before the tick.
///
/// Why it matters: at a quarter speed a bird that moves only on a tick
/// jumps once every sixteen frames, and the flock reads as a stutter and
/// not as a fold.
#[test]
fn each_bird_glides_between_ticks() {
    // Given:
    let mut scene = ticked(7, AREA, 100);
    let (mut moved, mut early) = (0, 0);

    for tick in 0..500 {
        let before: Vec<(f32, f32)> = scene.birds().collect();

        // When:
        let frames: Vec<BTreeMap<(usize, usize), u32>> = BETWEEN
            .iter()
            .map(|&progress| {
                scene.set_progress(progress);
                lit_pixels(&render(&scene, AREA, AREA), AREA)
            })
            .collect();
        scene.set_progress(0.0);
        scene.tick(AREA);
        let after: Vec<(f32, f32)> = scene.birds().collect();

        // Then:
        let spans: Vec<_> = before
            .iter()
            .zip(&after)
            .map(|(from, to)| span(*from, *to))
            .collect();
        let alone = |index: usize| {
            let (from, to) = (before[index], after[index]);
            before.iter().zip(&after).enumerate().all(|(other, near)| {
                other == index
                    || ((near.0.0 - from.0).hypot(near.0.1 - from.1) > ALONE
                        && (near.1.0 - to.0).hypot(near.1.1 - to.1) > ALONE)
            })
        };
        for (progress, shown) in BETWEEN.iter().zip(&frames) {
            for pixel in shown.keys() {
                assert!(
                    cell_of(*pixel).any(|place| spans.iter().any(|span| within(place, *span))),
                    "tick {tick}: a bird at {pixel:?} at {progress} of a tick, between no two places"
                );
            }
            for (index, span) in spans.iter().enumerate() {
                if !alone(index) {
                    continue;
                }
                assert!(
                    shown.keys().any(|pixel| within(*pixel, *span)),
                    "tick {tick}: bird {index} not between {span:?} at {progress} of a tick"
                );
            }
        }
        for (index, (from, to)) in before.iter().zip(&after).enumerate() {
            let first = (from.0 as usize, from.1 as usize);
            if !alone(index) || first == (to.0 as usize, to.1 as usize) {
                continue;
            }
            moved += 1;
            let shown_at = |shown: &BTreeMap<(usize, usize), u32>| {
                shown
                    .iter()
                    .filter(|(pixel, _)| within(**pixel, spans[index]))
                    .max_by_key(|(_, light)| **light)
                    .map(|(pixel, _)| *pixel)
            };
            early += usize::from(frames.iter().any(|shown| shown_at(shown) != Some(first)));
        }
    }
    assert!(moved > 100, "{moved} birds alone changed pixel");
    assert!(
        early * 2 > moved,
        "{early} of {moved} birds alone show in a new pixel before the tick"
    );
}

/// The hawk glides from its place on one tick to its place on the next.
/// Given: the scene on a hundred by thirty area.
/// When: on each tick of a pass of the hawk, it renders at a quarter, a half
/// and three quarters of the way to its next tick, then ticks.
/// Then: while the hawk is well inside the pane on both ticks, the middle
/// of its pixels, each weighed by how far it lies toward love, never goes
/// back from a frame to the frame half a tick later. Of the ticks on which
/// that middle moves, more than half show it moved at three quarters of a
/// tick. The hawk mixes into the sky at its edges, so its middle moves with
/// it by less than a pixel, and a cell of two colours moves the middle of
/// a shape by a few hundredths of a pixel from frame to frame, less than
/// the hawk moves in half a tick.
///
/// Why it matters: at a quarter speed a hawk that moves only on a tick
/// jumps once every sixteen frames, and its pass reads as a stutter.
#[test]
fn the_hawk_glides_between_ticks() {
    // Given:
    let (width, _) = pixels_of(AREA);
    let mut scene = Murmuration::new(Rng::seed_from_u64(7), &theme::ROSE_PINE);
    let (mut moved, mut early) = (0, 0);
    let front = |buffer: &Buffer, heading: f32, height: f32| {
        let pixels = hawk_pixels(buffer, AREA, height);
        let weight: f32 = pixels.values().sum();
        pixels
            .iter()
            .map(|((x, _), share)| heading * *x as f32 * share)
            .sum::<f32>()
            / weight.max(f32::MIN_POSITIVE)
    };

    for tick in 0..HAWK_EVERY.end() * 2 + 600 {
        let Some(from) = scene.hawk().filter(|(x, _)| (8.0..width - 8.0).contains(x)) else {
            scene.tick(AREA);
            continue;
        };

        // When:
        let mut frames = vec![render(&scene, AREA, AREA)];
        for progress in BETWEEN {
            scene.set_progress(progress);
            frames.push(render(&scene, AREA, AREA));
        }
        scene.set_progress(0.0);
        scene.tick(AREA);
        let Some(to) = scene.hawk().filter(|(x, _)| (8.0..width - 8.0).contains(x)) else {
            continue;
        };
        frames.push(render(&scene, AREA, AREA));

        // Then:
        let heading = (to.0 - from.0).signum();
        let fronts: Vec<f32> = frames
            .iter()
            .map(|frame| front(frame, heading, from.1))
            .collect();
        for (earlier, later) in fronts.iter().zip(&fronts[2..]) {
            assert!(
                earlier <= later,
                "tick {tick}: the hawk goes back from {from:?} to {to:?}: {fronts:?}"
            );
        }
        if fronts[0] < fronts[4] {
            moved += 1;
            early += usize::from(fronts[3] > fronts[0]);
        }
    }
    assert!(moved > 50, "the hawk changed pixel on {moved} ticks");
    assert!(
        early * 2 > moved,
        "the hawk shows in a new pixel before the tick on {early} of {moved} ticks"
    );
}

/// A frame on a tick is the frame the scene drew before it glided.
/// Given: two scenes from one seed, one of them given a share of a tick
/// before each tick and none after it.
/// When: both tick and render through a pass of the hawk and a resize.
/// Then: every frame of the two is the same.
///
/// Why it matters: the frame on a tick is the one every other test of the
/// scene reads, so a glide must not move it.
#[test]
fn a_frame_on_a_tick_is_the_frame_before_the_glide() {
    // Given:
    let mut glided = ticked(7, AREA, 1);
    let mut still = ticked(7, AREA, 1);
    let small = Rect::new(0, 0, 60, 20);

    for tick in 0..HAWK_EVERY.end() + 600 {
        // When:
        let area = if tick < *HAWK_EVERY.end() {
            AREA
        } else {
            small
        };
        glided.set_progress(0.5);
        glided.set_progress(0.0);
        glided.tick(area);
        still.tick(area);

        // Then:
        assert_eq!(
            render(&glided, area, area),
            render(&still, area, area),
            "tick {tick}"
        );
    }
}

/// Over a see-through base, the flock, the hawk and the trees mix nothing
/// into it.
/// Given: the scene on Rose Pine with a see-through base and the terminal's
/// background.
/// When: it ticks and renders through a pass of the hawk.
/// Then: no pixel lies part way along the mix from the base toward a colour
/// the scene draws over the sky: the birds' subtle and text, the hawk's
/// love and the trees' muted. A colour the scene draws, and a bird's mix
/// from subtle toward text, is left out: the greys of the theme lie near
/// one line from the base, so muted lies near the mix from it to text.
///
/// Why it matters: there the base is the terminal's own background, and a
/// mix with it is a colour the user never chose, a smudge round every edge.
#[test]
fn over_a_see_through_base_nothing_mixes_with_it() {
    // Given:
    let palette = &theme::FLAT_THEMES[theme::position("rose-pine").unwrap()];
    let mut scene =
        Murmuration::new(Rng::seed_from_u64(7), palette).with_background(Background::Terminal);
    let drawn = [palette.subtle, palette.text, palette.love, palette.muted];
    let mut hawk = 0;

    for tick in 0..HAWK_EVERY.end() + 300 {
        // When:
        scene.tick(AREA);
        if tick % 25 != 0 && scene.hawk().is_none() {
            continue;
        }
        hawk += usize::from(scene.hawk().is_some());
        let buffer = render(&scene, AREA, AREA);

        // Then:
        for (pixel, colour) in pixels::showing(&buffer, AREA, |colour| colour != palette.base) {
            if drawn.contains(&colour) || share_on(colour, palette.subtle, palette.text).is_some() {
                continue;
            }
            assert!(
                !drawn.iter().any(|to| {
                    share_on(colour, palette.base, *to).is_some_and(|share| share > 0.0)
                }),
                "tick {tick}: {pixel:?} in {colour:?} mixes the base"
            );
        }
    }
    assert!(hawk > 0, "no hawk came");
}
