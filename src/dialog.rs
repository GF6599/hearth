use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::theme::Palette;

/// A box over the middle of a pane, with its title in the top rule and its
/// keys under it.
#[derive(Debug)]
pub struct Dialog<'a> {
    pub title: &'a str,
    /// Each row of text, in runs of one style each, `inner` columns wide.
    pub rows: Vec<Vec<(String, Style)>>,
    /// The columns between the rules, less a space on each side.
    pub inner: usize,
    pub hint: &'a str,
}

impl Dialog<'_> {
    /// Draws the box in the middle of `area`, over what the pane drew, with
    /// its keys under it where they fit.
    pub fn render(self, palette: &Palette, area: Rect, buf: &mut Buffer) {
        let rule = Style::default().fg(palette.subtle).bg(palette.base);
        let title = Style::default().fg(palette.foam).bg(palette.base);
        let words = Style::default().fg(palette.subtle).bg(palette.base);
        let width = self.inner + 4;

        let mut lines: Vec<Vec<(String, Style)>> = vec![vec![
            ("┌ ".to_string(), rule),
            (self.title.to_string(), title),
            (
                format!(" {}┐", "─".repeat(width - self.title.len() - 4)),
                rule,
            ),
        ]];
        for row in self.rows {
            let mut line = vec![("│ ".to_string(), rule)];
            line.extend(row);
            line.push((" │".to_string(), rule));
            lines.push(line);
        }
        lines.push(vec![(format!("└{}┘", "─".repeat(width - 2)), rule)]);

        let width = u16::try_from(width).unwrap_or(u16::MAX);
        let height = u16::try_from(lines.len() + 1).unwrap_or(u16::MAX);
        let x = area.x + area.width.saturating_sub(width) / 2;
        let y = area.y + area.height.saturating_sub(height) / 2;
        for (line, y) in lines.into_iter().zip(y..area.bottom()) {
            let mut column = x;
            for (text, style) in line {
                let room = usize::from(area.right().saturating_sub(column));
                let (end, _) = buf.set_stringn(column, y, &text, room, style);
                column = end;
            }
        }
        let hint = u16::try_from(self.hint.len()).unwrap_or(u16::MAX);
        let row = y + height - 1;
        if hint <= area.width && row < area.bottom() {
            let x = area.x + (area.width - hint) / 2;
            buf.set_string(x, row, self.hint, words);
        }
    }
}
