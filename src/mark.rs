//! The `{hearth}` lockup. The art is `figlet -f ANSI_Shadow hearth`. The
//! font has no braces, so they are drawn in its style by hand: blocks for
//! the face and box runes for the shadow, with the last row shadow only.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::octant;
use crate::scene::FRAMES_PER_TICK;
use crate::theme::Palette;

pub const ART: [&str; 6] = [
    "██╗  ██╗███████╗ █████╗ ██████╗ ████████╗██╗  ██╗",
    "██║  ██║██╔════╝██╔══██╗██╔══██╗╚══██╔══╝██║  ██║",
    "███████║█████╗  ███████║██████╔╝   ██║   ███████║",
    "██╔══██║██╔══╝  ██╔══██║██╔══██╗   ██║   ██╔══██║",
    "██║  ██║███████╗██║  ██║██║  ██║   ██║   ██║  ██║",
    "╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝╚═╝  ╚═╝   ╚═╝   ╚═╝  ╚═╝",
];
const LEFT_BRACE: [&str; 6] = [" ███╗", " ██╔╝", "██╔╝ ", "╚██╗ ", " ███╗", " ╚══╝"];
const RIGHT_BRACE: [&str; 6] = ["███╗ ", "╚██╗ ", " ╚██╗", " ██╔╝", "███╔╝", "╚══╝ "];
const BRACE_WIDTH: usize = 5;

/// Ticks for one ember pulse of an art column, about six seconds.
pub const GLOW_TICKS: u32 = 96;
const GLOW_FRAMES: u32 = GLOW_TICKS * FRAMES_PER_TICK;
/// Frames by which each row of the art warms before the row above it, so
/// the glow climbs the letters as heat does.
const RISE: u32 = 6;
/// How far a shadow rune lies from its glow toward the base, so the
/// shadow reads as depth and not as part of the letter.
const SHADOW: f32 = 0.45;
/// The share of the glow beside it that a brace takes at the glow's
/// hottest. Under a half, so the marker stays gold.
const CATCH: f32 = 0.4;

/// What shows where the art does not fit, so the name never wraps or is
/// cut into fragments.
pub const WORD: &str = "{hearth}";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Art,
    Word,
}

impl Mark {
    pub fn width(self) -> u16 {
        let cells = match self {
            Mark::Art => art_width() + 2 * (BRACE_WIDTH + 1),
            Mark::Word => WORD.chars().count(),
        };
        u16::try_from(cells).unwrap_or(u16::MAX)
    }

    pub fn height(self) -> u16 {
        match self {
            Mark::Art => u16::try_from(ART.len()).unwrap_or(u16::MAX),
            Mark::Word => 1,
        }
    }

    /// The mark at `frame` frames into the app. Only the art glows,
    /// because a few glowing letters in the word read as flicker.
    pub fn lines(self, palette: &Palette, frame: u32) -> Vec<Line<'static>> {
        match self {
            Mark::Art => (0..ART.len())
                .map(|row| {
                    let width = art_width();
                    let art = format!("{:<width$}", ART[row]);
                    let brace = |glyphs: &'static str, column: usize| {
                        let heat = heat(column, row, frame);
                        let glow = octant::ramp(&ember(palette), heat);
                        let colour = octant::mix(palette.marker(), glow, CATCH * heat);
                        Span::styled(glyphs, Style::default().fg(colour))
                    };
                    let mut spans = vec![brace(LEFT_BRACE[row], 0), Span::raw(" ")];
                    spans.extend(art.chars().enumerate().map(|(column, glyph)| {
                        if glyph == ' ' {
                            return Span::raw(" ");
                        }
                        let glow = octant::ramp(&ember(palette), heat(column, row, frame));
                        let colour = if glyph == '█' {
                            glow
                        } else {
                            octant::mix(glow, palette.base, SHADOW)
                        };
                        Span::styled(glyph.to_string(), Style::default().fg(colour))
                    }));
                    spans.push(Span::raw(" "));
                    spans.push(brace(RIGHT_BRACE[row], width - 1));
                    Line::from(spans)
                })
                .collect(),
            Mark::Word => {
                let brace = Style::default().fg(palette.marker());
                let word = WORD.trim_matches(|c| c == '{' || c == '}');
                vec![Line::from(vec![
                    Span::styled("{", brace),
                    Span::styled(word, Style::default().fg(palette.marked())),
                    Span::styled("}", brace),
                ])]
            }
        }
    }
}

/// The colours of the glow from rest to its hottest.
fn ember(palette: &Palette) -> [Color; 4] {
    [palette.marked(), palette.rose, palette.love, palette.gold]
}

/// The heat of one cell of the art `frame` frames in, from 0 at rest to 1
/// at its hottest. A cell rests for half its pulse, then warms and cools
/// in one smooth hump.
fn heat(column: usize, row: usize, frame: u32) -> f32 {
    let column = u32::try_from(column).unwrap_or(0);
    let row = u32::try_from(row).unwrap_or(0);
    let lead = glow_offset(column) * FRAMES_PER_TICK + RISE * row;
    let phase = frame.wrapping_add(lead) % GLOW_FRAMES;
    let Some(warm) = phase.checked_sub(GLOW_FRAMES / 2) else {
        return 0.0;
    };
    let peak = GLOW_FRAMES / 4;
    let swell = triangle(warm, GLOW_FRAMES / 2) as f32 / peak as f32;
    swell * swell * (3.0 - 2.0 * swell)
}

/// A column's place in the pulse, in ticks: the sum of two triangle waves
/// of unlike periods across the columns. Neighbours sit a few ticks apart,
/// so a few columns warm together, and the patches drift and change like
/// embers. The sum spans more than half the pulse, so some column always
/// glows and some always rests.
fn glow_offset(column: u32) -> u32 {
    triangle(column * 5, 64) + triangle(column * 3 + 20, GLOW_TICKS)
}

/// Rises from zero to half of `period` and falls back, once a period.
fn triangle(x: u32, period: u32) -> u32 {
    let x = x % period;
    x.min(period - x)
}

fn art_width() -> usize {
    ART.iter().map(|row| row.chars().count()).max().unwrap_or(0)
}
