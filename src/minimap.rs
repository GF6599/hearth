//! A whole file drawn small: each line from its indentation to its end, two
//! lines to a cell row.

use std::ops::Range;

use ratatui::style::Color;

use crate::theme::Palette;

/// The columns of source that the full width stands for. A fixed number,
/// so one very long line does not shrink every other line.
const SOURCE_COLUMNS: usize = 80;

/// The upper and the lower half of one cell.
pub type Halves = (Option<Color>, Option<Color>);

#[derive(Debug, Default)]
pub struct Outline {
    /// The colour of each glyph of each line, and `None` for a blank.
    lines: Vec<Vec<Option<Color>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Outside,
    Snippet,
    Cursor,
}

impl Outline {
    /// `colours` holds one colour for each `char` of `text`.
    pub fn new(text: &str, colours: &[Color]) -> Self {
        let mut lines = Vec::new();
        let mut offset = 0;
        for line in text.split_inclusive('\n') {
            let glyphs = line
                .trim_end_matches('\n')
                .chars()
                .zip(colours.get(offset..).unwrap_or_default())
                .map(|(glyph, &colour)| (!glyph.is_whitespace()).then_some(colour))
                .collect();
            offset += line.chars().count();
            lines.push(glyphs);
        }
        Self { lines }
    }

    /// A file taller than two lines a row is scaled down so all of it fits.
    /// A cell that stands for several lines takes its colour from the one
    /// of the highest tier: the cursor line, then the snippet, then the rest.
    pub fn cells(
        &self,
        width: usize,
        rows: usize,
        snippet: Range<usize>,
        cursor: usize,
        palette: &Palette,
    ) -> Vec<Vec<Halves>> {
        let halves = rows * 2;
        let total = self.lines.len();
        let lines_of = |half: usize| -> Range<usize> {
            if total <= halves {
                half.min(total)..(half + 1).min(total)
            } else {
                half * total / halves..(half + 1) * total / halves
            }
        };
        let tier = |line: usize| {
            if line == cursor {
                Tier::Cursor
            } else if snippet.contains(&line) {
                Tier::Snippet
            } else {
                Tier::Outside
            }
        };
        // A see-through base does not show, so a mix toward it is a dark
        // shade over whatever the terminal shows. The mid grey reads as
        // fainter over a dark terminal and a light one.
        let fade = if palette.see_through {
            palette.muted
        } else {
            palette.base
        };
        let half = |half: usize, cell: usize| -> Option<Color> {
            let start = cell * SOURCE_COLUMNS / width;
            let end = ((cell + 1) * SOURCE_COLUMNS / width).max(start + 1);
            let mut best: Option<(Tier, Color)> = None;
            for line in lines_of(half) {
                let glyphs = &self.lines[line];
                let found = glyphs.get(start..end.min(glyphs.len())).unwrap_or_default();
                if let Some(colour) = found.iter().flatten().next() {
                    let here = tier(line);
                    if best.is_none_or(|(kept, _)| here > kept) {
                        best = Some((here, *colour));
                    }
                }
            }
            best.map(|(tier, colour)| match tier {
                Tier::Cursor => colour,
                Tier::Snippet => dim(colour, fade),
                Tier::Outside => palette.overlay,
            })
        };
        (0..rows)
            .map(|row| {
                (0..width)
                    .map(|cell| (half(row * 2, cell), half(row * 2 + 1, cell)))
                    .collect()
            })
            .collect()
    }
}

/// Halfway from `colour` to `toward`, so a snippet line reads fainter than
/// the cursor line and brighter than the rest of the file.
fn dim(colour: Color, toward: Color) -> Color {
    let (Color::Rgb(r, g, b), Color::Rgb(tr, tg, tb)) = (colour, toward) else {
        return colour;
    };
    let mid = |a: u8, b: u8| u8::try_from((u16::from(a) + u16::from(b)) / 2).unwrap_or(a);
    Color::Rgb(mid(r, tr), mid(g, tg), mid(b, tb))
}
