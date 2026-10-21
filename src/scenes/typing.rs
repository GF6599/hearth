use std::fmt;
use std::ops::{Range, RangeInclusive};
use std::str::FromStr;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use rand::seq::SliceRandom;
use rand::{RngExt, SeedableRng};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Widget};

use crate::config::Background;
use crate::corpus::{Corpus, SourceFile};
use crate::highlight;
use crate::minimap::Outline;
use crate::octant::{CELL_HEIGHT, CELL_WIDTH, Canvas};
use crate::scene::{self, FRAME_RATE, Rng, Scene};
use crate::snippet;
use crate::theme::{self, Palette};
use crate::tree::{self, Tree};

pub use crate::snippet::MOST_SNIPPET_LINES;

pub const FINDING: &str = "finding a function long enough";

/// The most text the UI thread measures itself. The grammar takes about
/// 1.25 ms a KB, so this is about two thirds of a tick, and a small
/// repository shows its code on the first frame with no notice.
const INLINE_BYTES: usize = 32 * 1024;

/// Pauses count in keystrokes, so a faster speed shortens them too.
const LINE_PAUSE: RangeInclusive<u32> = 2..=8;
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

    pub fn cps(self) -> u16 {
        self.0
    }

    pub fn range_message() -> String {
        format!(
            "a speed is a whole number of keystrokes per second, from {} to {}",
            Self::SLOWEST,
            Self::FASTEST
        )
    }

    fn per_frame(self) -> f64 {
        f64::from(self.0) * FRAME_RATE.as_secs_f64()
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

/// The share of the frame's rows of code that a function must fill to be
/// typed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shortest(f64);

impl Shortest {
    pub const DEFAULT: Shortest = Shortest(0.5);
    const SHARES: RangeInclusive<f64> = 0.0..=1.0;

    pub fn from_share(share: f64) -> Result<Shortest, String> {
        if Self::SHARES.contains(&share) {
            Ok(Shortest(share))
        } else {
            Err(Self::range_message())
        }
    }

    pub fn range_message() -> String {
        format!(
            "must be a share of the frame from {} to {}",
            Self::SHARES.start(),
            Self::SHARES.end()
        )
    }

    /// A function at the cap always fills, because no snippet is longer
    /// and a taller frame would otherwise find no function to type.
    fn fills(self, lines: usize, rows: u16) -> bool {
        lines >= MOST_SNIPPET_LINES || lines as f64 >= self.0 * f64::from(rows)
    }
}

impl Default for Shortest {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Debug)]
pub struct Typing {
    rng: Rng,
    palette: &'static Palette,
    corpus: Arc<Corpus>,
    speed: Speed,
    /// Keystrokes owed and not yet typed. The fraction carries from one
    /// tick to the next, so a speed under one keystroke per tick still types.
    credit: f64,
    /// The lines of the longest function in each file, 0 for none, once
    /// the file is measured. A file known to be too short is not parsed again.
    longest: Vec<Option<usize>>,
    /// The files in an order the seed fixes. The scan measures them in this
    /// order and the picks take them in it, so the seed fixes the snippets
    /// however long the scan takes.
    order: Vec<usize>,
    /// The place in `order` where the next pick starts.
    next: usize,
    /// Each file and its longest function, on their way from the scan
    /// thread. `None` once the scan ends, and for a corpus the UI thread
    /// measures itself.
    scan: Option<Receiver<(usize, usize)>>,
    /// Chooses the function in a file. `rng` draws on every tick, and a
    /// wait for the scan adds ticks, so the choice has a source of its own.
    picks: Rng,
    shortest: Shortest,
    /// The rows of code the frame holds, learned on each tick, because only
    /// the area gives the frame its size. `None` before the first tick,
    /// which opens the first snippet.
    rows: Option<u16>,
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
    background: Background,
}

impl Typing {
    /// Types every function, however short, unless `with_shortest` says
    /// otherwise. A corpus of more than `INLINE_BYTES` is measured on a
    /// thread of its own, so a large repository does not hold up a tick.
    pub fn new(mut rng: Rng, palette: &'static Palette, corpus: Arc<Corpus>, speed: Speed) -> Self {
        let tree = Tree::new(corpus.files().iter().map(|source| source.path.as_str()));
        let mut order: Vec<usize> = (0..corpus.files().len()).collect();
        order.shuffle(&mut rng);
        let picks = Rng::seed_from_u64(rng.random());
        let bytes: usize = corpus.files().iter().map(|source| source.text.len()).sum();
        let scan = if bytes > INLINE_BYTES {
            scan(Arc::clone(&corpus), order.clone())
        } else {
            None
        };
        Self {
            rng,
            palette,
            longest: vec![None; corpus.files().len()],
            order,
            next: 0,
            scan,
            picks,
            corpus,
            speed,
            credit: 0.0,
            shortest: Shortest(0.0),
            rows: None,
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
            background: Background::Theme,
        }
    }

    pub fn with_background(mut self, background: Background) -> Self {
        self.background = background;
        self
    }

    pub fn with_shortest(mut self, shortest: Shortest) -> Self {
        self.shortest = shortest;
        self
    }

    fn receive(&mut self) {
        let mut ended = false;
        if let Some(scan) = &self.scan {
            loop {
                match scan.try_recv() {
                    Ok((file, lines)) => self.longest[file] = Some(lines),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        ended = true;
                        break;
                    }
                }
            }
        }
        if ended {
            self.scan = None;
        }
    }

    /// Opens a function that fills the frame from the next file in `order`
    /// that holds one, never the one on screen again while another fills
    /// it. With none, the longest function in the corpus. A file the scan
    /// has not reached ends the search, and the snippet on screen stays
    /// until a later call finds the file measured.
    fn open_next(&mut self) {
        self.receive();
        let previous = self.current;
        let (shortest, rows) = (self.shortest, self.rows.unwrap_or(0));
        let fills = |lines: usize| shortest.fills(lines, rows);
        let corpus = Arc::clone(&self.corpus);
        let count = self.order.len();
        for step in 0..count {
            let place = (self.next + step) % count;
            let file = self.order[place];
            let source = &corpus.files()[file];
            let (highlighted, spans) = match self.longest[file] {
                Some(lines) if lines == 0 || !fills(lines) => continue,
                Some(_) => snippets(source, self.palette),
                None if self.scan.is_some() => {
                    self.next = place;
                    return;
                }
                None => {
                    let measured = snippets(source, self.palette);
                    self.longest[file] = Some(measured.1.iter().map(Range::len).max().unwrap_or(0));
                    measured
                }
            };
            let long: Vec<Range<usize>> = spans
                .into_iter()
                .filter(|span| fills(span.len()) && previous != Some((file, span.start)))
                .collect();
            if long.is_empty() {
                continue;
            }
            let span = long[self.picks.random_range(0..long.len())].clone();
            self.next = (place + 1) % count;
            self.open(file, source, &highlighted, span);
            return;
        }
        let longest = (0..self.longest.len())
            .filter_map(|file| {
                self.longest[file]
                    .filter(|&lines| lines > 0)
                    .map(|lines| (lines, file))
            })
            .max_by_key(|&(lines, _)| lines);
        if let Some((_, file)) = longest {
            let source = &corpus.files()[file];
            let (highlighted, spans) = snippets(source, self.palette);
            if let Some(span) = spans.into_iter().max_by_key(Range::len) {
                self.open(file, source, &highlighted, span);
            }
        }
    }

    fn open(
        &mut self,
        file: usize,
        source: &SourceFile,
        highlighted: &highlight::Highlighted,
        span: Range<usize>,
    ) {
        self.offset = 0;
        self.lines = vec![Vec::new()];
        self.finished = false;
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
        self.glyphs = glyphs(&source.text, &highlighted.colours, span, self.palette);
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
        let separator = Span::styled(" › ", Style::default().fg(self.palette.muted));
        let tail = [
            Span::styled(file, Style::default().fg(self.palette.foam)),
            separator.clone(),
            Span::styled("fn ", Style::default().fg(self.palette.pine)),
            Span::styled(
                self.function.as_str(),
                Style::default().fg(self.palette.rose),
            ),
        ];
        let from = |start: usize| {
            let lead = folders[start..].iter().flat_map(|&folder| {
                [
                    Span::styled(folder, Style::default().fg(self.palette.subtle)),
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
        let height = frame_height(area);
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
        let plain = Style::default().fg(self.palette.subtle);
        let marker = Style::default().fg(self.palette.marker());
        for (y, row) in (frame.y..frame.bottom()).zip(&self.tree_rows[top..]) {
            let indent = " ".repeat(row.depth);
            let line = match row.kind {
                tree::Kind::Folder { open } => Line::from(vec![
                    Span::raw(indent),
                    Span::styled(
                        if open { "▾ " } else { "▸ " },
                        Style::default().fg(self.palette.muted),
                    ),
                    Span::styled(row.name.as_str(), plain),
                ]),
                tree::Kind::File { current } => {
                    // The closing mark always fits, so the name is cut
                    // before it rather than the mark at the tree's edge.
                    let room = usize::from(width).saturating_sub(row.depth + 3);
                    let name: String = row.name.chars().take(room).collect();
                    let (open, close, name_style) = if current {
                        ("[", "]", Style::default().fg(self.palette.marked()))
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
            self.palette,
        );
        let mut canvas = Canvas::new(width, frame.height, self.palette);
        // Over the terminal's own background each line takes one row of
        // pixels and leaves the row under it clear, because two full halves
        // of lines read as a solid panel there. A cell then holds the base
        // and the lines, and it draws two colours, so both lines take the
        // upper line's colour.
        let terminal = self.background == Background::Terminal;
        let rows: [Range<usize>; 2] = if terminal {
            [0..1, CELL_HEIGHT / 2..CELL_HEIGHT / 2 + 1]
        } else {
            [0..CELL_HEIGHT / 2, CELL_HEIGHT / 2..CELL_HEIGHT]
        };
        for (row, cells) in cells.iter().enumerate() {
            for (column, &(upper, lower)) in cells.iter().enumerate() {
                for (half, line) in rows.iter().zip([upper, lower]) {
                    let Some(own) = line else { continue };
                    let colour = if terminal { upper.unwrap_or(own) } else { own };
                    for dy in half.clone() {
                        for dx in 0..CELL_WIDTH {
                            canvas.set(column * CELL_WIDTH + dx, row * CELL_HEIGHT + dy, colour);
                        }
                    }
                }
            }
        }
        canvas.render(Rect::new(x, frame.y, width, frame.height), buf);
    }

    /// A part that does not fit goes whole, the language first and then
    /// the speed, so no word is cut while a whole part could go instead.
    fn render_status(&self, frame: Rect, first: usize, buf: &mut Buffer) {
        let style = Style::default().fg(self.palette.subtle);
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
        // Only the functions count here, so any palette does.
        .any(|source| !snippets(source, &theme::ROSE_PINE).1.is_empty())
}

/// Measures the longest function of each file in `order` on a thread of its
/// own. The thread ends when the scene that reads it is gone.
fn scan(corpus: Arc<Corpus>, order: Vec<usize>) -> Option<Receiver<(usize, usize)>> {
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("typing scan".into())
        .spawn(move || {
            for file in order {
                // Only the functions count here, so any palette does.
                let (_, spans) = snippets(&corpus.files()[file], &theme::ROSE_PINE);
                let lines = spans.iter().map(Range::len).max().unwrap_or(0);
                if sender.send((file, lines)).is_err() {
                    return;
                }
            }
        })
        .ok()
        .map(|_| receiver)
}

fn snippets(source: &SourceFile, palette: &Palette) -> (highlight::Highlighted, Vec<Range<usize>>) {
    let highlighted = highlight::highlight(&source.path, &source.text, palette);
    let lines: Vec<&str> = source.text.lines().collect();
    let spans = snippet::snippets(&lines, &highlighted.functions, &highlighted.comment);
    (highlighted, spans)
}

/// The frame's width follows the breadcrumb, but its height follows the
/// area alone, so a tick can learn it.
fn frame_height(area: Rect) -> u16 {
    padded(area.height, LEAST_PADDING_Y)
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
fn glyphs(
    text: &str,
    colours: &[Color],
    span: Range<usize>,
    palette: &Palette,
) -> Vec<(char, Color)> {
    let mut out = Vec::new();
    let mut offset = 0;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        let glyphs = line.trim_end_matches('\n').chars().count();
        if span.contains(&index) {
            if !out.is_empty() {
                out.push(('\n', palette.text));
            }
            let chars = line.chars().take(glyphs);
            out.extend(chars.zip(colours[offset..].iter().copied()));
        }
        offset += line.chars().count();
    }
    out
}

impl Scene for Typing {
    fn tick(&mut self, area: Rect) {
        // The rules above and below the code take a row each.
        self.rows = Some(frame_height(area).saturating_sub(2));
        if self.current.is_none() {
            self.open_next();
            // The wait for the first function draws nothing, so the frames
            // after it do not depend on how long it took.
            if self.current.is_none() {
                return;
            }
        }
        self.credit += self.speed.per_frame() * self.rng.random_range(JITTER);
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
            if self.scan.is_some() {
                scene::draw_notice(FINDING, area, self.palette, buf);
            }
            return;
        };
        if area.is_empty() {
            return;
        }
        let frame = self.frame(area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(theme::BORDER)
            .border_style(Style::default().fg(self.palette.muted))
            .style(Style::default().bg(self.palette.surface));
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
                let style = Style::default().fg(self.palette.muted);
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
            buf[(x, y)].set_char(' ').set_bg(self.palette.text);
        }
    }
}
