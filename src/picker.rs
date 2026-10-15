use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::theme;

#[derive(Debug, Clone, Copy)]
pub struct Entry<'a> {
    pub name: &'a str,
    pub summary: &'a str,
}

/// The `[ ]` pair is drawn on every row and only coloured on the current
/// one, so the labels stay in one column as the selection moves.
#[derive(Debug)]
pub struct Picker<'a> {
    pub entries: &'a [Entry<'a>],
    pub selected: usize,
}

impl Picker<'_> {
    /// A summary takes a row of its own, so on an area too short for two
    /// rows a scene the summaries go and every name stays in the list.
    fn has_summaries(&self, area: Rect) -> bool {
        self.entries.len() * 2 + 2 <= usize::from(area.height)
    }

    fn frame(&self, area: Rect, summaries: bool) -> Rect {
        let widest = self
            .entries
            .iter()
            .map(|entry| {
                let name = entry.name.chars().count();
                let summary = entry.summary.chars().count();
                if summaries { name.max(summary) } else { name }
            })
            .max();
        let content = widest.unwrap_or(0) + 2;
        let title = theme::PICKER_TITLE.chars().count();
        let rows = self.entries.len() * if summaries { 2 } else { 1 };
        let width = u16::try_from(content.max(title) + 2).unwrap_or(u16::MAX);
        let height = u16::try_from(rows + 2).unwrap_or(u16::MAX);
        let width = width.min(area.width);
        let height = height.min(area.height);
        Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        )
    }
}

impl Widget for Picker<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        buf.set_style(area, Style::default().bg(theme::BASE));
        let summaries = self.has_summaries(area);
        let frame = self.frame(area, summaries);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(theme::BORDER)
            .border_style(Style::default().fg(theme::MUTED))
            .style(Style::default().bg(theme::SURFACE))
            .title(theme::PICKER_TITLE);
        let inner = block.inner(frame);
        block.render(frame, buf);

        let text_width = usize::from(inner.width).saturating_sub(2);
        let clip = |text: &str| text.chars().take(text_width).collect::<String>();
        let marker = Style::default().fg(theme::MARKER);
        let mut rows = Vec::new();
        for (index, entry) in self.entries.iter().enumerate() {
            let current = index == self.selected;
            let (open, close) = if current { ("[", "]") } else { (" ", " ") };
            let name_style = Style::default().fg(if current { theme::MARKED } else { theme::TEXT });
            rows.push(Line::from(vec![
                Span::styled(open, marker),
                Span::styled(clip(entry.name), name_style),
                Span::styled(close, marker),
            ]));
            if summaries {
                rows.push(Line::from(vec![
                    Span::raw(" "),
                    Span::styled(clip(entry.summary), Style::default().fg(theme::SUBTLE)),
                ]));
            }
        }
        Paragraph::new(rows).render(inner, buf);
    }
}
