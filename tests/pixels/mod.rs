#![allow(
    dead_code,
    reason = "each test binary builds this module and uses only some of it"
)]

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::{Mutex, OnceLock};

use hearth::octant::{self, CELL_HEIGHT, CELL_WIDTH};
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

pub const PIXELS: usize = CELL_WIDTH * CELL_HEIGHT;

/// The bits of a cell's octant glyph, or none for a cell that holds another
/// glyph, such as a letter. A test reads every cell of thousands of frames,
/// so the lookup is built once and a blank cell, the most common, skips it.
pub fn bits(cell: &Cell) -> Option<u8> {
    static BITS: OnceLock<HashMap<char, u8>> = OnceLock::new();
    let symbol = cell.symbol();
    if symbol == " " {
        return Some(0);
    }
    let bits = BITS.get_or_init(|| {
        (0..=u8::MAX)
            .map(|bits| (octant::glyph(bits), bits))
            .collect()
    });
    let mut glyphs = symbol.chars();
    let bits = *bits.get(&glyphs.next()?)?;
    glyphs.next().is_none().then_some(bits)
}

/// The colour each pixel of a cell shows, in the order of the glyph's bits,
/// or none for a cell that holds no octant glyph.
pub fn shown(cell: &Cell) -> Option<[Color; PIXELS]> {
    let bits = bits(cell)?;
    let mut colours = [cell.bg; PIXELS];
    for (pixel, colour) in colours.iter_mut().enumerate() {
        if bits >> pixel & 1 == 1 {
            *colour = cell.fg;
        }
    }
    Some(colours)
}

/// Each pixel of the octant cells of `area` in a colour that `keep`
/// accepts, with its place as (column, row) in pixels from the top left of
/// the area, cell by cell. A cell with another glyph shows no pixels. The
/// filter runs inside the loop because a test reads thousands of frames
/// and an iterator chain per cell is slow in a debug build.
pub fn showing(
    buffer: &Buffer,
    area: Rect,
    keep: impl Fn(Color) -> bool,
) -> Vec<((usize, usize), Color)> {
    let mut kept = Vec::new();
    for position in area.positions() {
        let Some(colours) = shown(&buffer[position]) else {
            continue;
        };
        let (left, top) = corner(position, area);
        for (pixel, colour) in colours.into_iter().enumerate() {
            if keep(colour) {
                kept.push((
                    (left + pixel % CELL_WIDTH, top + pixel / CELL_WIDTH),
                    colour,
                ));
            }
        }
    }
    kept
}

/// The pixels of `area`, by pixel row from the top and then by pixel column
/// from the left. `read` gives the pixels of each cell in the order of the
/// glyph's bits, so a test chooses what a cell with another glyph shows.
pub fn grid<T: Clone + Default>(
    buffer: &Buffer,
    area: Rect,
    read: impl Fn(Position, &Cell) -> [T; PIXELS],
) -> Vec<Vec<T>> {
    let mut rows = vec![
        vec![T::default(); usize::from(area.width) * CELL_WIDTH];
        usize::from(area.height) * CELL_HEIGHT
    ];
    for position in area.positions() {
        let (left, top) = corner(position, area);
        for (pixel, value) in read(position, &buffer[position]).iter().enumerate() {
            rows[top + pixel / CELL_WIDTH][left + pixel % CELL_WIDTH] = value.clone();
        }
    }
    rows
}

/// The distance between two colours in RGB. A colour with no RGB value
/// fails the test, as no distance to it is true.
pub fn distance(one: Color, other: Color) -> f32 {
    match (one, other) {
        (Color::Rgb(r, g, b), Color::Rgb(s, h, c)) => [(r, s), (g, h), (b, c)]
            .into_iter()
            .map(|(one, other)| (f32::from(one) - f32::from(other)).powi(2))
            .sum::<f32>()
            .sqrt(),
        _ => panic!("{one:?} or {other:?} has no RGB value"),
    }
}

/// Whether `colour` is a mix of `one` and `other`: it lies on the way from
/// one to the other, to a step of rounding, and a twentieth of the way or
/// more from each, so a colour a step from either is no mix. A colour with
/// no RGB value, the terminal's own background, is no mix.
pub fn between(colour: Color, one: Color, other: Color) -> bool {
    let Color::Rgb(..) = colour else {
        return false;
    };
    let (span, near, far) = (
        distance(one, other),
        distance(one, colour),
        distance(colour, other),
    );
    near >= span / 20.0 && far >= span / 20.0 && near + far <= span + 2.0
}

pub fn place_distance(from: (f32, f32), to: (f32, f32)) -> f32 {
    (to.0 - from.0).hypot(to.1 - from.1)
}

/// The mixes `share_on` has read, by their two ends and steps: each colour
/// of a mix with the first step that rounds to it.
type Mixes = HashMap<(Color, Color, u16), HashMap<Color, u16>>;

/// How far `colour` lies along the mix from `from` toward `to`, from 0 to
/// 1, read at `steps` even steps: the share of the first step whose mix
/// lies nearest by the largest gap in one channel, or none where that gap
/// is more than `levels`. So `to` can read a step short of 1, where an
/// earlier step already rounds to it. More steps or more levels match
/// more colours, and so weaken a test that a colour is no mix, so each
/// caller states its own.
pub fn share_on(colour: Color, from: Color, to: Color, steps: u16, levels: u8) -> Option<f32> {
    static MIXES: OnceLock<Mutex<Mixes>> = OnceLock::new();
    let Color::Rgb(r, g, b) = colour else {
        return None;
    };
    let mut mixes = MIXES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap();
    let firsts = mixes.entry((from, to, steps)).or_insert_with(|| {
        let mut firsts = HashMap::new();
        for step in 0..=steps {
            let mix = octant::mix(from, to, f32::from(step) / f32::from(steps));
            firsts.entry(mix).or_insert(step);
        }
        firsts
    });
    let near = |channel: u8, gap: u8| channel.saturating_sub(gap)..=channel.saturating_add(gap);
    (0..=levels)
        .find_map(|gap| {
            near(r, gap)
                .flat_map(|nr| {
                    near(g, gap).flat_map(move |ng| near(b, gap).map(move |nb| (nr, ng, nb)))
                })
                .filter(|(nr, ng, nb)| {
                    r.abs_diff(*nr).max(g.abs_diff(*ng)).max(b.abs_diff(*nb)) == gap
                })
                .filter_map(|(nr, ng, nb)| firsts.get(&Color::Rgb(nr, ng, nb)).copied())
                .min()
        })
        .map(|step| f32::from(step) / f32::from(steps))
}

const MIX_SAMPLES: f32 = 200.0;

/// Every colour within a channel level of a mix from `base` toward one of
/// `colours`, at each of `samples` out of `MIX_SAMPLES` along the way,
/// less the colours within a level of the base or of one of `colours`
/// themselves.
pub fn base_mixes(base: Color, colours: &[Color], samples: Range<u16>) -> HashSet<(u8, u8, u8)> {
    let mut near: HashSet<(u8, u8, u8)> = colours
        .iter()
        .flat_map(|colour| {
            samples.clone().flat_map(move |sample| {
                around(octant::mix(base, *colour, f32::from(sample) / MIX_SAMPLES))
            })
        })
        .collect();
    for end in colours.iter().chain([&base]) {
        for each in around(*end) {
            near.remove(&each);
        }
    }
    near
}

/// Each colour within a channel level of `colour`.
fn around(colour: Color) -> Vec<(u8, u8, u8)> {
    let Color::Rgb(r, g, b) = colour else {
        panic!("{colour:?} has no RGB value");
    };
    (0..27)
        .map(|at| {
            let step = |channel: u8, by: u8| channel.saturating_add(by).saturating_sub(1);
            (step(r, at % 3), step(g, at / 3 % 3), step(b, at / 9))
        })
        .collect()
}

/// The top left pixel of the cell at `position`, from the top left of
/// `area`.
fn corner(position: Position, area: Rect) -> (usize, usize) {
    (
        usize::from(position.x - area.x) * CELL_WIDTH,
        usize::from(position.y - area.y) * CELL_HEIGHT,
    )
}
