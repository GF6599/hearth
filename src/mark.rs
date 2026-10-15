//! The `{hearth}` lockup. The art is `figlet -f ANSI_Shadow hearth`. The
//! braces are drawn from box runes, because a terminal cannot scale one
//! glyph to the art's height.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::theme;

pub const ART: [&str; 6] = [
    "██╗  ██╗███████╗ █████╗ ██████╗ ████████╗██╗  ██╗",
    "██║  ██║██╔════╝██╔══██╗██╔══██╗╚══██╔══╝██║  ██║",
    "███████║█████╗  ███████║██████╔╝   ██║   ███████║",
    "██╔══██║██╔══╝  ██╔══██║██╔══██╗   ██║   ██╔══██║",
    "██║  ██║███████╗██║  ██║██║  ██║   ██║   ██║  ██║",
    "╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝╚═╝  ╚═╝   ╚═╝   ╚═╝  ╚═╝",
];
const LEFT_BRACE: [&str; 6] = ["╭", "│", "┘", "┐", "│", "╰"];
const RIGHT_BRACE: [&str; 6] = ["╮", "│", "└", "┌", "│", "╯"];

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
            Mark::Art => art_width() + 4,
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

    pub fn lines(self) -> Vec<Line<'static>> {
        let brace = Style::default().fg(theme::SUBTLE);
        let name = Style::default().fg(theme::MARKED);
        match self {
            Mark::Art => (0..ART.len())
                .map(|row| {
                    Line::from(vec![
                        Span::styled(LEFT_BRACE[row], brace),
                        Span::raw(" "),
                        Span::styled(format!("{:<width$}", ART[row], width = art_width()), name),
                        Span::raw(" "),
                        Span::styled(RIGHT_BRACE[row], brace),
                    ])
                })
                .collect(),
            Mark::Word => {
                let word = WORD.trim_matches(|c| c == '{' || c == '}');
                vec![Line::from(vec![
                    Span::styled("{", brace),
                    Span::styled(word, name),
                    Span::styled("}", brace),
                ])]
            }
        }
    }
}

fn art_width() -> usize {
    ART.iter().map(|row| row.chars().count()).max().unwrap_or(0)
}
