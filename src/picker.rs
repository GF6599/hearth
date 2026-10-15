use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::mark::Mark;
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
    pub credit: Option<&'a str>,
}

/// The `[ ]` pair is drawn on every row and only coloured on the current
/// one, so the labels stay in one column as the selection moves.
#[derive(Debug)]
pub struct Picker<'a> {
    pub entries: &'a [Entry<'a>],
    pub selected: usize,
}

/// Where each part of the picker goes in one area.
#[derive(Debug)]
struct Layout {
    mark: Option<(Mark, u16, u16)>,
    list: Rect,
    summaries: bool,
    preview: Option<Rect>,
}

impl Picker<'_> {
    /// The inside of the pane to the right of the list, or `None` when the
    /// area leaves it too small. `App` ticks the preview in the same
    /// rectangle it draws it in.
    pub fn preview_area(&self, area: Rect) -> Option<Rect> {
        self.layout(area).preview
    }

    /// The mark takes rows only where the list keeps every summary below
    /// it, and sits on the left edge of the content under it. It falls
    /// back to the word, and then to nothing.
    fn layout(&self, area: Rect) -> Layout {
        let with_summaries = self.entries.len() * 2 + 2;
        for mark in [Mark::Art, Mark::Word] {
            let rows = mark.height() + 1;
            if usize::from(area.height) < usize::from(rows) + with_summaries + 2 {
                continue;
            }
            let body = Rect::new(area.x, area.y + rows, area.width, area.height - rows);
            let layout = self.body(body);
            if layout.list.x + mark.width() > area.right() {
                continue;
            }
            let top = layout
                .preview
                .map_or(layout.list.y, |inner| layout.list.y.min(inner.y - 1));
            return Layout {
                mark: Some((mark, layout.list.x, top - rows)),
                ..layout
            };
        }
        self.body(area)
    }

    /// Beside a pane the list starts on the pane's top row, so the two
    /// frames read as one band, and the list takes its summaries from the
    /// band's height rather than the area's. Alone it sits in the middle.
    fn body(&self, area: Rect) -> Layout {
        let band = Rect::new(
            area.x,
            area.y + 1,
            area.width,
            area.height.saturating_sub(2),
        );
        if let Some(preview) = self.preview_in(band) {
            let summaries = self.has_summaries(band);
            let column = Rect::new(band.x, band.y, self.list_width(band), band.height);
            let list = Rect {
                y: band.y,
                ..self.frame(column, summaries)
            };
            return Layout {
                mark: None,
                list,
                summaries,
                preview: Some(preview),
            };
        }
        let summaries = self.has_summaries(area);
        Layout {
            mark: None,
            list: self.frame(area, summaries),
            summaries,
            preview: None,
        }
    }

    fn preview_in(&self, band: Rect) -> Option<Rect> {
        let list = self.list_width(band);
        let pane = Rect::new(
            band.x + list,
            band.y,
            band.width.saturating_sub(list + GAP),
            band.height,
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
        let Layout {
            mark,
            list: frame,
            summaries,
            preview,
        } = self.layout(area);
        if let Some((mark, x, y)) = mark {
            let at = Rect::new(x, y, mark.width(), mark.height());
            Paragraph::new(mark.lines()).render(at.intersection(area), buf);
        }
        if let Some(inner) = preview {
            let area = Rect::new(inner.x - 1, inner.y - 1, inner.width + 2, inner.height + 2);
            let credit = self
                .entries
                .get(self.selected)
                .and_then(|entry| entry.credit);
            match credit {
                Some(credit) => cut_bottom(pane(), area.width, &format!("─ {credit} ")),
                None => pane(),
            }
            .render(area, buf);
        }
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(theme::BORDER)
            .border_style(Style::default().fg(theme::MUTED))
            .style(Style::default().bg(theme::SURFACE))
            .title(theme::PICKER_TITLE);
        let block = cut_bottom(block, frame.width, theme::PICKER_HINT);
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

/// Cuts `label`, written `─ text `, into the bottom rule of a block
/// `width` cells wide, in the hint's colours. A label cut at the corner
/// names a key or a person that does not exist, so it shows whole or not
/// at all.
fn cut_bottom(block: Block<'static>, width: u16, label: &str) -> Block<'static> {
    if usize::from(width) < label.chars().count() + 2 {
        return block;
    }
    let (lead, text) = label.split_at("─ ".len());
    block.title_bottom(Line::from(vec![
        Span::styled(lead.to_owned(), Style::default().fg(theme::MUTED)),
        Span::styled(text.to_owned(), Style::default().fg(theme::SUBTLE)),
    ]))
}

fn pane() -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(theme::BORDER)
        .border_style(Style::default().fg(theme::MUTED))
        .style(Style::default().bg(theme::BASE))
}
