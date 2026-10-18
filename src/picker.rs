use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, StatefulWidget,
    Widget,
};

use crate::mark::Mark;
use crate::theme::{self, Palette};

/// The least inside of the preview pane. A scene squeezed below it reads
/// as noise, so the pane goes and the list keeps the whole area.
const LEAST_PREVIEW_WIDTH: u16 = 24;
const LEAST_PREVIEW_HEIGHT: u16 = 8;

/// Columns between the list's frame and the pane, and outside each.
const GAP: u16 = 2;

const THUMB: &str = "┃";

/// The keys of a running scene, on the row under the list. Nothing names
/// them inside a scene, and the picker is where a first-time user starts.
pub const SCENE_KEYS: &str = "in a scene  tab next  v/s split  x close  h/j/k/l focus  esc scenes";

/// The list scrolls before it gives up rows, but the mark takes rows only
/// while this many scenes stay in view with their summaries, and the
/// summaries go only when this many no longer fit.
pub const LEAST_SCENES_IN_VIEW: usize = 5;

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
    pub palette: &'a Palette,
    pub entries: &'a [Entry<'a>],
    /// The entries the list frame takes its width from: every scene, so
    /// the frame keeps its width while a search narrows the list.
    pub sized_by: &'a [Entry<'a>],
    pub selected: usize,
    /// The first scene in view on the frame before. The list scrolls from
    /// it only as far as the selection needs, so it holds still while the
    /// selection moves inside it.
    pub top: usize,
    /// The app's tick count, which moves the glow of the title.
    pub tick: u32,
    /// The letters of a search, shown in the top rule. `None` when no
    /// search is on.
    pub query: Option<&'a str>,
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

    /// The first scene the list shows in `area`. `App` keeps it as the
    /// `top` of the next frame.
    pub fn first_in_view(&self, area: Rect) -> usize {
        let Layout {
            list, summaries, ..
        } = self.layout(area);
        self.scrolled(rows_in(list, summaries))
    }

    /// `top` moved only as far as keeps the selection in view, with one
    /// scene of context either side where the list shows three or more.
    fn scrolled(&self, visible: usize) -> usize {
        let count = self.entries.len();
        if visible == 0 || visible >= count {
            return 0;
        }
        let context = usize::from(visible >= 3);
        let after = if self.selected + 1 < count {
            context
        } else {
            0
        };
        let before = if self.selected > 0 { context } else { 0 };
        let lowest = (self.selected + 1 + after).saturating_sub(visible);
        let highest = self.selected - before;
        self.top.clamp(lowest, highest).min(count - visible)
    }

    /// The rows of the fewest scenes the list may show, with their
    /// summaries, and its frame.
    fn least_list(&self) -> usize {
        LEAST_SCENES_IN_VIEW.min(self.entries.len()) * 2 + 2
    }

    /// The mark takes rows only where the list keeps its least share of
    /// scenes with their summaries below it, and sits on the left edge of
    /// the content under it. It falls back to the word, and then to
    /// nothing.
    fn layout(&self, area: Rect) -> Layout {
        let with_summaries = self.least_list();
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

    /// Beside a pane the list takes the pane's rows, so the two frames
    /// read as one band, and the list takes its summaries from the
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
                height: band.height,
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

    /// A summary takes a row of its own, so on an area too short for the
    /// least share of scenes at two rows each the summaries go, and the
    /// names scroll on their own.
    fn has_summaries(&self, area: Rect) -> bool {
        self.least_list() <= usize::from(area.height)
    }

    fn frame(&self, area: Rect, summaries: bool) -> Rect {
        let widest = self
            .sized_by
            .iter()
            .map(|entry| {
                let named = name_row(entry);
                let summary = entry.summary.chars().count() + 2;
                if summaries { named.max(summary) } else { named }
            })
            .max();
        let content = widest.unwrap_or(0);
        let rules = [
            theme::PICKER_TITLE,
            theme::PICKER_HINT,
            theme::PICKER_SEARCH_HINT,
        ]
        .iter()
        .map(|rule| rule.chars().count())
        .max()
        .unwrap_or(0);
        let per = rows_per_scene(summaries);
        let room = usize::from(area.height).saturating_sub(2) / per * per;
        let rows = (self.entries.len() * per).min(room);
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

    /// A thumb on the right rule of the list, between its corners. The
    /// rule itself is the track, so the frame keeps its look.
    fn render_thumb(
        &self,
        frame: Rect,
        inner: Rect,
        first: usize,
        visible: usize,
        buf: &mut Buffer,
    ) {
        let rule = Rect::new(frame.right() - 1, inner.y, 1, inner.height);
        // The state counts scroll places, not scenes, so the thumb meets
        // the bottom of the rule when the last scene is in view.
        let mut state = ScrollbarState::new(self.entries.len() - visible + 1)
            .viewport_content_length(visible)
            .position(first);
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(None)
            .thumb_symbol(THUMB)
            .thumb_style(Style::default().fg(self.palette.subtle))
            .render(rule, buf, &mut state);
    }
}

impl Widget for Picker<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        buf.set_style(area, Style::default().bg(self.palette.base));
        let Layout {
            mark,
            list: frame,
            summaries,
            preview,
        } = self.layout(area);
        if let Some((mark, x, y)) = mark {
            let at = Rect::new(x, y, mark.width(), mark.height());
            Paragraph::new(mark.lines(self.palette, self.tick)).render(at.intersection(area), buf);
        }
        if let Some(inner) = preview {
            let area = Rect::new(inner.x - 1, inner.y - 1, inner.width + 2, inner.height + 2);
            let credit = self
                .entries
                .get(self.selected)
                .and_then(|entry| entry.credit);
            match credit {
                Some(credit) => cut_bottom(
                    pane(self.palette),
                    area.width,
                    &format!("─ {credit} "),
                    self.palette,
                ),
                None => pane(self.palette),
            }
            .render(area, buf);
        }
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(theme::BORDER)
            .border_style(Style::default().fg(self.palette.muted))
            .style(Style::default().bg(self.palette.surface))
            .title(match self.query {
                Some(query) => format!("{}/{query} ", theme::PICKER_TITLE),
                None => theme::PICKER_TITLE.to_owned(),
            });
        let hint = if self.query.is_some() {
            theme::PICKER_SEARCH_HINT
        } else {
            theme::PICKER_HINT
        };
        let block = cut_bottom(block, frame.width, hint, self.palette);
        let inner = block.inner(frame);
        block.render(frame, buf);

        let inner_width = usize::from(inner.width);
        let text_width = inner_width.saturating_sub(2);
        let clip = |text: &str| text.chars().take(text_width).collect::<String>();
        let marker = Style::default().fg(self.palette.marker());
        let visible = rows_in(frame, summaries);
        let first = self.scrolled(visible);
        if visible < self.entries.len() && !inner.is_empty() {
            self.render_thumb(frame, inner, first, visible, buf);
        }
        let mut rows = Vec::new();
        for (index, entry) in self.entries.iter().enumerate().skip(first).take(visible) {
            let current = index == self.selected;
            let (open, close) = if current { ("[", "]") } else { (" ", " ") };
            let name_style = Style::default().fg(if current {
                self.palette.marked()
            } else {
                self.palette.text
            });
            let mut row = vec![
                Span::styled(open, marker),
                Span::styled(clip(entry.name), name_style),
                Span::styled(close, marker),
            ];
            if let Some(gap) = inner_width.checked_sub(name_row(entry)) {
                row.push(Span::raw(" ".repeat(gap + 1)));
                row.push(Span::styled(
                    entry.speed,
                    Style::default().fg(self.palette.subtle),
                ));
            }
            rows.push(Line::from(row));
            if summaries {
                rows.push(Line::from(vec![
                    Span::raw(" "),
                    Span::styled(
                        clip(entry.summary),
                        Style::default().fg(self.palette.subtle),
                    ),
                ]));
            }
        }
        Paragraph::new(rows).render(inner, buf);
        self.render_scene_keys(area, frame, buf);
    }
}

impl Picker<'_> {
    /// On the row under the list frame, from its left edge, whole or not at
    /// all, so no key shows cut.
    fn render_scene_keys(&self, area: Rect, frame: Rect, buf: &mut Buffer) {
        let y = frame.bottom();
        let room = usize::from(area.right().saturating_sub(frame.x));
        if y >= area.bottom() || SCENE_KEYS.chars().count() > room {
            return;
        }
        let (lead, keys) = SCENE_KEYS.split_at("in a scene".len());
        let line = Line::from(vec![
            Span::styled(lead, Style::default().fg(self.palette.muted)),
            Span::styled(keys, Style::default().fg(self.palette.subtle)),
        ]);
        buf.set_line(frame.x, y, &line, area.right() - frame.x);
    }
}

fn rows_per_scene(summaries: bool) -> usize {
    if summaries { 2 } else { 1 }
}

/// The scenes a list frame shows inside its rules.
fn rows_in(list: Rect, summaries: bool) -> usize {
    usize::from(list.height).saturating_sub(2) / rows_per_scene(summaries)
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
fn cut_bottom(block: Block<'static>, width: u16, label: &str, palette: &Palette) -> Block<'static> {
    if usize::from(width) < label.chars().count() + 2 {
        return block;
    }
    let (lead, text) = label.split_at("─ ".len());
    block.title_bottom(Line::from(vec![
        Span::styled(lead.to_owned(), Style::default().fg(palette.muted)),
        Span::styled(text.to_owned(), Style::default().fg(palette.subtle)),
    ]))
}

fn pane(palette: &Palette) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(theme::BORDER)
        .border_style(Style::default().fg(palette.muted))
        .style(Style::default().bg(palette.base))
}
