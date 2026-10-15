use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::theme;

/// The least inside of the preview pane. A scene squeezed below it reads
/// as noise, so the pane goes and the list keeps the whole area.
const LEAST_PREVIEW_WIDTH: u16 = 24;
const LEAST_PREVIEW_HEIGHT: u16 = 8;

/// Columns between the list's frame and the pane, and outside each.
const GAP: u16 = 2;

#[derive(Debug, Clone, Copy)]
pub struct Entry<'a> {
    pub name: &'a str,
    pub summary: &'a str,
    pub speed: &'a str,
}

/// The `[ ]` pair is drawn on every row and only coloured on the current
/// one, so the labels stay in one column as the selection moves.
#[derive(Debug, Clone, Copy)]
pub struct Picker<'a> {
    pub entries: &'a [Entry<'a>],
    pub selected: usize,
}

impl Picker<'_> {
    /// A summary takes a row of its own, so on an area too short for two
    /// rows a scene the summaries go and every name stays in the list.
    /// The inside of the pane to the right of the list, or `None` when the
    /// area leaves it too small. `App` ticks the preview in the same
    /// rectangle it draws it in.
    pub fn preview_area(&self, area: Rect) -> Option<Rect> {
        let list = self.list_width(area);
        let pane = Rect::new(
            area.x + list,
            area.y + 1,
            area.width.saturating_sub(list + GAP),
            area.height.saturating_sub(2),
        );
        let inner = Rect::new(
            pane.x + 1,
            pane.y + 1,
            pane.width.saturating_sub(2),
            pane.height.saturating_sub(2),
        );
        (inner.width >= LEAST_PREVIEW_WIDTH && inner.height >= LEAST_PREVIEW_HEIGHT)
            .then_some(inner)
    }

    fn list_width(&self, area: Rect) -> u16 {
        self.frame(area, self.has_summaries(area)).width + 2 * GAP
    }

    fn has_summaries(&self, area: Rect) -> bool {
        self.entries.len() * 2 + 2 <= usize::from(area.height)
    }

    fn frame(&self, area: Rect, summaries: bool) -> Rect {
        let widest = self
            .entries
            .iter()
            .map(|entry| {
                let named = name_row(entry);
                let summary = entry.summary.chars().count() + 2;
                if summaries { named.max(summary) } else { named }
            })
            .max();
        let content = widest.unwrap_or(0);
        let rules = theme::PICKER_TITLE
            .chars()
            .count()
            .max(theme::PICKER_HINT.chars().count());
        let rows = self.entries.len() * if summaries { 2 } else { 1 };
        let width = u16::try_from(content.max(rules) + 2).unwrap_or(u16::MAX);
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
        let preview = self.preview_area(area);
        let summaries = self.has_summaries(area);
        let list_area = match preview {
            Some(inner) => {
                pane().render(
                    Rect::new(inner.x - 1, inner.y - 1, inner.width + 2, inner.height + 2),
                    buf,
                );
                Rect::new(area.x, area.y, self.list_width(area), area.height)
            }
            None => area,
        };
        let frame = self.frame(list_area, summaries);
        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_type(theme::BORDER)
            .border_style(Style::default().fg(theme::MUTED))
            .style(Style::default().bg(theme::SURFACE))
            .title(theme::PICKER_TITLE);
        // A hint cut at the corner names keys that do not exist, so it
        // shows whole or not at all.
        if usize::from(frame.width) >= theme::PICKER_HINT.chars().count() + 2 {
            let (lead, keys) = theme::PICKER_HINT.split_at("─ ".len());
            block = block.title_bottom(Line::from(vec![
                Span::styled(lead, Style::default().fg(theme::MUTED)),
                Span::styled(keys, Style::default().fg(theme::SUBTLE)),
            ]));
        }
        let inner = block.inner(frame);
        block.render(frame, buf);

        let inner_width = usize::from(inner.width);
        let text_width = inner_width.saturating_sub(2);
        let clip = |text: &str| text.chars().take(text_width).collect::<String>();
        let marker = Style::default().fg(theme::MARKER);
        let mut rows = Vec::new();
        for (index, entry) in self.entries.iter().enumerate() {
            let current = index == self.selected;
            let (open, close) = if current { ("[", "]") } else { (" ", " ") };
            let name_style = Style::default().fg(if current { theme::MARKED } else { theme::TEXT });
            let mut row = vec![
                Span::styled(open, marker),
                Span::styled(clip(entry.name), name_style),
                Span::styled(close, marker),
            ];
            if let Some(gap) = inner_width.checked_sub(name_row(entry)) {
                row.push(Span::raw(" ".repeat(gap + 1)));
                row.push(Span::styled(
                    entry.speed,
                    Style::default().fg(theme::SUBTLE),
                ));
            }
            rows.push(Line::from(row));
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

/// The cells a name row needs to show its speed: the pair, the name, one
/// gap, the speed and a blank cell before the border. A row with fewer
/// drops the whole speed, because one cut at the border reads as another
/// speed, `1` for `1.5×`.
fn name_row(entry: &Entry) -> usize {
    entry.name.chars().count() + entry.speed.chars().count() + 4
}

fn pane() -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(theme::BORDER)
        .border_style(Style::default().fg(theme::MUTED))
        .style(Style::default().bg(theme::BASE))
}
