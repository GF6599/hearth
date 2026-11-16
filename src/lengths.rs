use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::dialog::Dialog;
use crate::scenes::hourglass::Lengths;
use crate::theme::Palette;

const NAMES: [&str; 3] = ["work", "break", "long break"];
const TITLE: &str = "lengths";
const HINT: &str = "j k pick  + - change  enter save  esc cancel";
/// A row of the box: the mark, the name padded to the longest, and the
/// minutes to three places, so the minutes line up.
const ROW: usize = 2 + 10 + 1 + 3 + 4;

/// The lengths of the hourglass while the user sets them, before a save.
#[derive(Debug, Clone, Copy)]
pub struct LengthsEditor {
    lengths: Lengths,
    /// The length that `+` and `-` change: work, the break or the long
    /// break.
    row: usize,
}

impl LengthsEditor {
    pub fn new(lengths: Lengths) -> Self {
        Self { lengths, row: 0 }
    }

    pub fn lengths(&self) -> Lengths {
        self.lengths
    }

    pub fn next(&mut self) {
        self.row = (self.row + 1).min(NAMES.len() - 1);
    }

    pub fn previous(&mut self) {
        self.row = self.row.saturating_sub(1);
    }

    pub fn longer(&mut self) {
        self.change(1);
    }

    pub fn shorter(&mut self) {
        self.change(-1);
    }

    /// Moves the picked length by `minutes`, and leaves it where it is at
    /// the end of the range the file takes.
    fn change(&mut self, minutes: i64) {
        let length = self.length_mut();
        let now = i64::try_from(length.as_secs() / 60).unwrap_or(i64::MAX);
        if let Ok(changed) = Lengths::minutes(now.saturating_add(minutes)) {
            *length = changed;
        }
    }

    fn length_mut(&mut self) -> &mut Duration {
        match self.row {
            0 => &mut self.lengths.work,
            1 => &mut self.lengths.r#break,
            _ => &mut self.lengths.long_break,
        }
    }

    pub fn render(&self, area: Rect, palette: &Palette, buf: &mut Buffer) {
        let words = Style::default().fg(palette.subtle).bg(palette.base);
        let picked = Style::default().fg(palette.text).bg(palette.base);
        let minutes = [
            self.lengths.work,
            self.lengths.r#break,
            self.lengths.long_break,
        ]
        .map(|length| length.as_secs() / 60);
        let rows = NAMES
            .iter()
            .zip(minutes)
            .enumerate()
            .map(|(row, (name, minutes))| {
                let (mark, style) = if row == self.row {
                    ("›", picked)
                } else {
                    (" ", words)
                };
                vec![(format!("{mark} {name:<10} {minutes:>3} min"), style)]
            })
            .collect();
        Dialog {
            title: TITLE,
            rows,
            inner: ROW,
            hint: HINT,
        }
        .render(palette, area, buf);
    }
}
