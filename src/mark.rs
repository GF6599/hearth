//! The `{hearth}` lockup. The art is `figlet -f ANSI_Shadow hearth`. The
//! font has no braces, so they are drawn in its style by hand: blocks for
//! the face and box runes for the shadow, with the last row shadow only.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

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

    /// The mark at `tick` ticks into the app. Only the art glows, because a
    /// few glowing letters in the word read as flicker.
    pub fn lines(self, palette: &Palette, tick: u32) -> Vec<Line<'static>> {
        let brace = Style::default().fg(palette.marker());
        match self {
            Mark::Art => (0..ART.len())
                .map(|row| {
                    let art = format!("{:<width$}", ART[row], width = art_width());
                    let mut spans = vec![Span::styled(LEFT_BRACE[row], brace), Span::raw(" ")];
                    spans.extend(art.chars().enumerate().map(|(column, glyph)| {
                        Span::styled(
                            glyph.to_string(),
                            Style::default().fg(ember(palette, column, tick)),
                        )
                    }));
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(RIGHT_BRACE[row], brace));
                    Line::from(spans)
                })
                .collect(),
            Mark::Word => {
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

/// The colour of an art column `tick` ticks in. It rests in the marked
/// colour for half its pulse, then warms through rose to love and back.
fn ember(palette: &Palette, column: usize, tick: u32) -> Color {
    let column = u32::try_from(column).unwrap_or(0);
    let phase = tick.wrapping_add(glow_offset(column)) % GLOW_TICKS;
    match phase * 8 / GLOW_TICKS {
        0..=3 => palette.marked(),
        4 | 7 => palette.rose,
        _ => palette.love,
    }
}

/// A column's place in the pulse: the sum of two triangle waves of unlike
/// periods across the columns. Neighbours sit a few ticks apart, so a few
/// columns warm together, and the patches drift and change like embers.
/// The sum spans more than half the pulse, so some column always glows
/// and some always rests.
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
