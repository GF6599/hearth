use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Widget};

use crate::corpus::{Corpus, SourceFile};
use crate::highlight;
use crate::minimap::Outline;
use crate::scene::{Rng, Scene, TICK_RATE};
use crate::snippet;
use crate::theme;
use crate::tree::{self, Tree};

pub use crate::snippet::MOST_SNIPPET_LINES;

/// Pauses count in keystrokes, so a faster speed shortens them too.
const LINE_PAUSE: std::ops::RangeInclusive<u32> = 2..=8;
const FILE_PAUSE: u32 = 150;

/// A tick types between half and one and a half times its share, so the
/// rhythm is uneven the way a person's is.
const JITTER: std::ops::Range<f64> = 0.5..1.5;

/// Each side of the frame keeps this share of the area clear, so every
/// snippet sits in the same frame and the padding does not move.
const PADDING_SHARE: u16 = 8;

/// One row holds the breadcrumb above the frame and the status line below.
const LEAST_PADDING_X: u16 = 2;
const LEAST_PADDING_Y: u16 = 1;

/// Two rules and one cell between them.
const SMALLEST_FRAME: u16 = 3;

/// Under this many columns a tree shows marks and the first letters of
/// names, which nobody can read.
const LEAST_TREE_WIDTH: u16 = 8;

/// Under this many cells a minimap is a column of blocks with no shape.
const LEAST_MINIMAP_WIDTH: u16 = 4;

/// A blank cell before the line numbers and another after them.
const GUTTER_GAP: usize = 1;

const FOLDER_COLOUR: Color = theme::SUBTLE;
const SEPARATOR_COLOUR: Color = theme::MUTED;
const FILE_COLOUR: Color = theme::FOAM;
const FUNCTION_MARK_COLOUR: Color = theme::PINE;
const FUNCTION_COLOUR: Color = theme::ROSE;
const STATUS_COLOUR: Color = theme::SUBTLE;
const TREE_COLOUR: Color = theme::SUBTLE;
const ARROW_COLOUR: Color = theme::MUTED;
const CURSOR_COLOUR: Color = theme::TEXT;
const FRAME_COLOUR: Color = theme::MUTED;
const GUTTER_COLOUR: Color = theme::MUTED;
const FILL: Color = theme::SURFACE;

/// Keystrokes per second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Speed(u16);

impl Speed {
    pub const DEFAULT: Speed = Speed(50);
    const SLOWEST: u16 = 1;
    const FASTEST: u16 = 1000;

    /// One step up, about a quarter faster, and never past the ceiling.
    pub fn faster(self) -> Speed {
        let step = (self.0 + self.0 / 4).max(self.0 + 1);
        Speed(step.min(Self::FASTEST))
    }

    /// One step down, about a fifth slower, and never under the floor.
    pub fn slower(self) -> Speed {
        let step = (self.0 - self.0 / 5).min(self.0 - 1);
        Speed(step.max(Self::SLOWEST))
    }

    pub fn from_cps(cps: i64) -> Result<Speed, String> {
        u16::try_from(cps)
            .ok()
            .filter(|cps| (Self::SLOWEST..=Self::FASTEST).contains(cps))
            .map(Speed)
            .ok_or_else(Self::range_message)
    }

    pub fn range_message() -> String {
        format!(
            "a speed is a whole number of keystrokes per second, from {} to {}",
            Self::SLOWEST,
            Self::FASTEST
        )
    }

    fn per_tick(self) -> f64 {
        f64::from(self.0) * TICK_RATE.as_secs_f64()
    }
}

impl fmt::Display for Speed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for Speed {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let cps = text.parse::<i64>().map_err(|_| Self::range_message())?;
        Self::from_cps(cps)
    }
}

#[derive(Debug)]
pub struct Typing {
    rng: Rng,
    corpus: Arc<Corpus>,
    speed: Speed,
    /// Keystrokes owed and not yet typed. The fraction carries from one
    /// tick to the next, so a speed under one keystroke per tick still types.
    credit: f64,
    /// The files that may hold a function. A file found to hold none
    /// leaves the list, so it is parsed once at most.
    candidates: Vec<usize>,
    /// The file and the first line of the snippet on screen.
    current: Option<(usize, usize)>,
    function: String,
    language: &'static str,
    /// Built once, because a large repository holds tens of thousands of
    /// paths and only the open path changes from one snippet to the next.
    tree: Tree,
    tree_rows: Vec<tree::Row>,
    outline: Outline,
    /// The snippet on screen, each glyph with its colour from the grammar.
    glyphs: Vec<(char, Color)>,
    /// The lines in the whole snippet, so the gutter does not widen while
    /// it types.
    line_count: usize,
    offset: usize,
    lines: Vec<Vec<(char, Color)>>,
    pause: u32,
    finished: bool,
}

impl Typing {
    pub fn new(rng: Rng, corpus: Arc<Corpus>, speed: Speed) -> Self {
        let tree = Tree::new(corpus.files().iter().map(|source| source.path.as_str()));
        let mut scene = Self {
            rng,
            corpus,
            speed,
            credit: 0.0,
            candidates: Vec::new(),
            current: None,
            function: String::new(),
            language: "",
            tree,
            tree_rows: Vec::new(),
            outline: Outline::default(),
            glyphs: Vec::new(),
            line_count: 0,
            offset: 0,
            lines: vec![Vec::new()],
            pause: 0,
            finished: false,
        };
        scene.candidates = (0..scene.corpus.files().len()).collect();
        scene.open_next();
        scene
    }

    /// Picks a snippet at random from a random file, never the one on
    /// screen again while another exists.
    fn open_next(&mut self) {
        let previous = self.current.take();
        self.glyphs.clear();
        self.offset = 0;
        self.lines = vec![Vec::new()];
        self.finished = false;
        let corpus = Arc::clone(&self.corpus);
        while !self.candidates.is_empty() {
            let slot = self.rng.random_range(0..self.candidates.len());
            let file = self.candidates[slot];
            let source = &corpus.files()[file];
            let (highlighted, spans) = snippets(source);
            if spans.is_empty() {
                self.candidates.swap_remove(slot);
                continue;
            }
            let mut pick = self.rng.random_range(0..spans.len());
            if previous == Some((file, spans[pick].start)) {
                if spans.len() == 1 && self.candidates.len() > 1 {
                    continue;
                }
                pick = (pick + 1) % spans.len();
            }
            let span = spans[pick].clone();
            self.current = Some((file, span.start));
            // The lines above the definition are comments and attributes,
            // which name no function, so the first name is the snippet's own.
            self.function = highlighted.functions[span.clone()]
                .iter()
                .find_map(Clone::clone)
                .unwrap_or_default();
            self.language = highlighted.language;
            self.tree_rows = self.tree.rows(&source.path);
            self.outline = Outline::new(&source.text, &highlighted.colours);
            self.line_count = span.len();
            self.glyphs = glyphs(&source.text, &highlighted.colours, span);
            return;
        }
    }

    /// The folders, the file and the function, with as many leading folders
    /// dropped as `width` needs. The file and the function always stay,
    /// because the reader cannot get them from anywhere else on screen.
    fn breadcrumb(&self, width: u16) -> Line<'_> {
        let path = self
            .current
            .and_then(|(file, _)| self.corpus.files().get(file))
            .map_or("", |source| source.path.as_str());
        let mut folders: Vec<&str> = path.split('/').collect();
        let file = folders.pop().unwrap_or_default();
        let separator = Span::styled(" › ", Style::default().fg(SEPARATOR_COLOUR));
        let tail = [
            Span::styled(file, Style::default().fg(FILE_COLOUR)),
            separator.clone(),
            Span::styled("fn ", Style::default().fg(FUNCTION_MARK_COLOUR)),
            Span::styled(self.function.as_str(), Style::default().fg(FUNCTION_COLOUR)),
        ];
        let from = |start: usize| {
            let lead = folders[start..].iter().flat_map(|&folder| {
                [
                    Span::styled(folder, Style::default().fg(FOLDER_COLOUR)),
                    separator.clone(),
                ]
            });
            Line::from(lead.chain(tail.iter().cloned()).collect::<Vec<_>>())
        };
        let start = (0..folders.len())
            .find(|&start| from(start).width() <= usize::from(width))
            .unwrap_or(folders.len());
        from(start)
    }

    /// The cursor's line and column in the file, both from 1.
    fn place(&self, first: usize) -> String {
        let column = self.lines.last().map_or(0, Vec::len) + 1;
        let line = first + self.lines.len();
        format!("Ln {line}, Col {column}")
    }

    /// The digits of the last line number, which every number is padded to.
    fn digits(&self) -> usize {
        let last = self.current.map_or(0, |(_, first)| first + self.line_count);
        last.to_string().len()
    }

    fn frame(&self, area: Rect) -> Rect {
        // A breadcrumb with no room for folders still holds the file and the
        // function, so the frame is never narrower than those two.
        let tail = u16::try_from(self.breadcrumb(0).width()).unwrap_or(u16::MAX);
        let width = padded(area.width, LEAST_PADDING_X).max(tail.min(area.width));
        let height = padded(area.height, LEAST_PADDING_Y);
        Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        )
    }

    /// A file's name lines up with the names of the folders beside it, and
    /// the current file stays near the middle when the tree is taller than
    /// the frame.
    fn render_tree(&self, area: Rect, frame: Rect, buf: &mut Buffer) {
        let width = (frame.x - area.x).saturating_sub(2);
        if width < LEAST_TREE_WIDTH {
            return;
        }
        let x = area.x + 1;
        let height = usize::from(frame.height);
        let current = self
            .tree_rows
            .iter()
            .position(|row| row.kind == tree::Kind::File { current: true })
            .unwrap_or(0);
        let top = current
            .saturating_sub(height / 2)
            .min(self.tree_rows.len().saturating_sub(height));
        let plain = Style::default().fg(TREE_COLOUR);
        let marker = Style::default().fg(theme::MARKER);
        for (y, row) in (frame.y..frame.bottom()).zip(&self.tree_rows[top..]) {
            let indent = " ".repeat(row.depth);
            let line = match row.kind {
                tree::Kind::Folder { open } => Line::from(vec![
                    Span::raw(indent),
                    Span::styled(
                        if open { "▾ " } else { "▸ " },
                        Style::default().fg(ARROW_COLOUR),
                    ),
                    Span::styled(row.name.as_str(), plain),
                ]),
                tree::Kind::File { current } => {
                    // The closing mark always fits, so the name is cut
                    // before it rather than the mark at the tree's edge.
                    let room = usize::from(width).saturating_sub(row.depth + 3);
                    let name: String = row.name.chars().take(room).collect();
                    let (open, close, name_style) = if current {
                        ("[", "]", Style::default().fg(theme::MARKED))
                    } else {
                        (" ", " ", plain)
                    };
                    Line::from(vec![
                        Span::raw(indent + " "),
                        Span::styled(open, marker),
                        Span::styled(name, name_style),
                        Span::styled(close, marker),
                    ])
                }
            };
            buf.set_line(x, y, &line, width);
        }
    }

    fn render_minimap(&self, area: Rect, frame: Rect, first: usize, buf: &mut Buffer) {
        let width = (area.right() - frame.right()).saturating_sub(2);
        if width < LEAST_MINIMAP_WIDTH {
            return;
        }
        let x = frame.right() + 1;
        let snippet = first..first + self.line_count;
        let cursor = first + self.lines.len() - 1;
        let cells = self.outline.cells(
            usize::from(width),
            usize::from(frame.height),
            snippet,
            cursor,
        );
        for (y, row) in (frame.y..frame.bottom()).zip(cells) {
            for (x, halves) in (x..x + width).zip(row) {
                let cell = &mut buf[(x, y)];
                match halves {
                    (Some(upper), lower) => {
                        cell.set_char('▀').set_fg(upper);
                        if let Some(lower) = lower {
                            cell.set_bg(lower);
                        }
                    }
                    (None, Some(lower)) => {
                        cell.set_char('▄').set_fg(lower);
                    }
                    (None, None) => {}
                }
            }
        }
    }

    /// A part that does not fit goes whole, the language first and then
    /// the speed, so no word is cut while a whole part could go instead.
    fn render_status(&self, frame: Rect, first: usize, buf: &mut Buffer) {
        let style = Style::default().fg(STATUS_COLOUR);
        let y = frame.bottom();
        let room = usize::from(frame.width);
        let place = self.place(first);
        let with_speed = format!("{place}  {} cps", self.speed);
        let with_speed_width = with_speed.chars().count();
        if self.language.chars().count() + 1 + with_speed_width <= room {
            buf.set_string(frame.x, y, self.language, style);
        }
        let right = if with_speed_width <= room {
            with_speed
        } else {
            place
        };
        let right_width = right.chars().count();
        let right_x = frame.right() - u16::try_from(right_width.min(room)).unwrap_or(0);
        buf.set_stringn(right_x, y, &right, room, style);
    }

    /// Spends one keystroke: on a pause, on opening the next file, or on
    /// one glyph.
    fn press(&mut self) {
        if self.pause > 0 {
            self.pause -= 1;
            return;
        }
        if self.finished {
            self.open_next();
            return;
        }
        let Some(&(glyph, colour)) = self.glyphs.get(self.offset) else {
            self.finished = true;
            self.pause = FILE_PAUSE;
            return;
        };
        self.offset += 1;
        if glyph == '\n' {
            self.lines.push(Vec::new());
            self.pause = self.rng.random_range(LINE_PAUSE);
        } else if let Some(line) = self.lines.last_mut() {
            line.push((glyph, colour));
        }
    }
}

/// Whether any file holds a function to type. The search stops at the
/// first file with a function, so a repository of code answers in a few
/// parses and only one with no function parses every file.
pub fn has_snippet(corpus: &Corpus) -> bool {
    corpus
        .files()
        .iter()
        .any(|source| !snippets(source).1.is_empty())
}

fn snippets(source: &SourceFile) -> (highlight::Highlighted, Vec<std::ops::Range<usize>>) {
    let highlighted = highlight::highlight(&source.path, &source.text);
    let lines: Vec<&str> = source.text.lines().collect();
    let spans = snippet::snippets(&lines, &highlighted.functions, &highlighted.comment);
    (highlighted, spans)
}

fn padded(room: u16, least: u16) -> u16 {
    let padding = (room / PADDING_SHARE).max(least);
    match room.checked_sub(2 * padding) {
        Some(left) if left >= SMALLEST_FRAME => left,
        _ => room,
    }
}

/// The glyphs of the lines in `span`, each with its colour, and a newline
/// between two lines.
fn glyphs(text: &str, colours: &[Color], span: std::ops::Range<usize>) -> Vec<(char, Color)> {
    let mut out = Vec::new();
    let mut offset = 0;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        let glyphs = line.trim_end_matches('\n').chars().count();
        if span.contains(&index) {
            if !out.is_empty() {
                out.push(('\n', theme::TEXT));
            }
            let chars = line.chars().take(glyphs);
            out.extend(chars.zip(colours[offset..].iter().copied()));
        }
        offset += line.chars().count();
    }
    out
}

impl Scene for Typing {
    fn tick(&mut self, _area: Rect) {
        self.credit += self.speed.per_tick() * self.rng.random_range(JITTER);
        while self.credit >= 1.0 {
            self.credit -= 1.0;
            self.press();
        }
    }

    fn set_speed(&mut self, speed: Speed) {
        self.speed = speed;
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let Some((_, first)) = self.current else {
            return;
        };
        if area.is_empty() {
            return;
        }
        let frame = self.frame(area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(theme::BORDER)
            .border_style(Style::default().fg(FRAME_COLOUR))
            .style(Style::default().bg(FILL));
        let inner = block.inner(frame);
        block.render(frame, buf);
        if frame.y > area.y {
            let line = self.breadcrumb(frame.width);
            buf.set_line(frame.x, frame.y - 1, &line, frame.width);
        }
        if frame.bottom() < area.bottom() {
            self.render_status(frame, first, buf);
        }
        self.render_tree(area, frame, buf);
        self.render_minimap(area, frame, first, buf);
        if inner.is_empty() {
            return;
        }

        // The gutter goes when it would leave no cell for the code.
        let digits = self.digits();
        let gutter = GUTTER_GAP + digits + GUTTER_GAP;
        let gutter = if gutter < usize::from(inner.width) {
            gutter
        } else {
            0
        };
        let code_x = inner.x + u16::try_from(gutter).unwrap_or(0);
        let top = self.lines.len().saturating_sub(usize::from(inner.height));
        let shown = &self.lines[top..];
        for (y, (index, line)) in (inner.y..inner.bottom()).zip(shown.iter().enumerate()) {
            if gutter > 0 {
                let number = format!("{:>digits$}", first + top + index + 1);
                let style = Style::default().fg(GUTTER_COLOUR);
                let gutter_x = inner.x + u16::try_from(GUTTER_GAP).unwrap_or(0);
                buf.set_stringn(gutter_x, y, number, digits, style);
            }
            for (x, &(glyph, colour)) in (code_x..inner.right()).zip(line) {
                buf[(x, y)].set_char(glyph).set_fg(colour);
            }
        }

        if let Some(last) = shown.last() {
            let room = usize::from(inner.right() - code_x - 1);
            let x = code_x + u16::try_from(last.len().min(room)).unwrap_or(0);
            let y = inner.y + u16::try_from(shown.len() - 1).unwrap_or(0);
            buf[(x, y)].set_char(' ').set_bg(CURSOR_COLOUR);
        }
    }
}
