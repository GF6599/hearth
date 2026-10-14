use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::theme;

/// The `[ ]` pair is drawn on every row and only coloured on the current
/// one, so the labels stay in one column as the selection moves.
#[derive(Debug)]
pub struct Picker<'a> {
    pub labels: &'a [&'a str],
    pub selected: usize,
}

impl Picker<'_> {
    fn frame(&self, area: Rect) -> Rect {
        let widest_label = self.labels.iter().map(|l| l.chars().count()).max();
        let content = widest_label.unwrap_or(0) + 2;
        let title = theme::PICKER_TITLE.chars().count();
        let width = u16::try_from(content.max(title) + 2).unwrap_or(u16::MAX);
        let height = u16::try_from(self.labels.len() + 2).unwrap_or(u16::MAX);
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
        let frame = self.frame(area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(theme::BORDER)
            .border_style(Style::default().fg(theme::MUTED))
            .style(Style::default().bg(theme::SURFACE))
            .title(theme::PICKER_TITLE);
        let inner = block.inner(frame);
        block.render(frame, buf);

        let label_width = usize::from(inner.width).saturating_sub(2);
        let marker = Style::default().fg(theme::MARKER);
        let rows = self.labels.iter().enumerate().map(|(index, label)| {
            let current = index == self.selected;
            let (open, close) = if current { ("[", "]") } else { (" ", " ") };
            let label_style =
                Style::default().fg(if current { theme::MARKED } else { theme::TEXT });
            let clipped: String = label.chars().take(label_width).collect();
            Line::from(vec![
                Span::styled(open, marker),
                Span::styled(clipped, label_style),
                Span::styled(close, marker),
            ])
        });
        Paragraph::new(rows.collect::<Vec<_>>()).render(inner, buf);
    }
}
