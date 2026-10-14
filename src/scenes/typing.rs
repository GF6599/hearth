use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::corpus::{Corpus, SourceFile};
use crate::highlight;
use crate::scene::{Rng, Scene, TICK_RATE};
use crate::snippet;
use crate::theme;

pub use crate::snippet::MOST_SNIPPET_LINES;

/// Pauses count in keystrokes, so a faster speed shortens them too.
const LINE_PAUSE: std::ops::RangeInclusive<u32> = 2..=8;
const FILE_PAUSE: u32 = 150;

/// A tick types between half and one and a half times its share, so the
/// rhythm is uneven the way a person's is.
const JITTER: std::ops::Range<f64> = 0.5..1.5;

/// Ticks a changed speed stays on screen, about one second.
const NOTICE_TICKS: u32 = 16;

const PATH_COLOUR: Color = theme::FOAM;
const CURSOR_COLOUR: Color = theme::TEXT;
const NOTICE_COLOUR: Color = theme::SUBTLE;

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
        let range = Self::SLOWEST..=Self::FASTEST;
        match text.parse::<u16>() {
            Ok(cps) if range.contains(&cps) => Ok(Speed(cps)),
            _ => Err(format!(
                "a speed is a whole number of keystrokes per second, from {} to {}",
                Self::SLOWEST,
                Self::FASTEST
            )),
        }
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
    notice: u32,
    /// The files that may hold a function. A file found to hold none
    /// leaves the list, so it is parsed once at most.
    candidates: Vec<usize>,
    /// The file and the first line of the snippet on screen.
    current: Option<(usize, usize)>,
    /// The snippet on screen, each glyph with its colour from the grammar.
    glyphs: Vec<(char, Color)>,
    offset: usize,
    lines: Vec<Vec<(char, Color)>>,
    pause: u32,
    finished: bool,
}

impl Typing {
    pub fn new(rng: Rng, corpus: Arc<Corpus>, speed: Speed) -> Self {
        let mut scene = Self {
            rng,
            corpus,
            speed,
            credit: 0.0,
            notice: 0,
            candidates: Vec::new(),
            current: None,
            glyphs: Vec::new(),
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
            self.glyphs = glyphs(&source.text, &highlighted.colours, span);
            return;
        }
    }

    /// The header: the file and the line the snippet starts on.
    fn header(&self) -> String {
        self.current
            .and_then(|(file, first)| {
                let source = self.corpus.files().get(file)?;
                Some(format!("{}:{}", source.path, first + 1))
            })
            .unwrap_or_default()
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
    let spans = snippet::snippets(&lines, &highlighted.defines, &highlighted.comment);
    (highlighted, spans)
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
        self.notice = self.notice.saturating_sub(1);
        self.credit += self.speed.per_tick() * self.rng.random_range(JITTER);
        while self.credit >= 1.0 {
            self.credit -= 1.0;
            self.press();
        }
    }

    fn set_speed(&mut self, speed: Speed) {
        self.speed = speed;
        self.notice = NOTICE_TICKS;
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let width = usize::from(area.width);
        let mut put = |row: u16, start: usize, text: &mut dyn Iterator<Item = (char, Color)>| {
            for (column, (glyph, colour)) in (start..width).zip(text) {
                let Ok(column) = u16::try_from(column) else {
                    break;
                };
                buf[(area.x + column, area.y + row)]
                    .set_char(glyph)
                    .set_fg(colour);
            }
        };
        put(
            0,
            0,
            &mut self.header().chars().map(|glyph| (glyph, PATH_COLOUR)),
        );
        if self.notice > 0 {
            let notice = format!("{} cps", self.speed);
            put(
                0,
                width.saturating_sub(notice.len()),
                &mut notice.chars().map(|glyph| (glyph, NOTICE_COLOUR)),
            );
        }

        let visible = usize::from(area.height) - 1;
        if visible == 0 {
            return;
        }
        let first = self.lines.len().saturating_sub(visible);
        let shown = &self.lines[first..];
        for (index, line) in shown.iter().enumerate() {
            let Ok(row) = u16::try_from(index + 1) else {
                break;
            };
            put(row, 0, &mut line.iter().copied());
        }

        if let Some(last) = shown.last() {
            let column = last.len().min(width - 1);
            let row = shown.len();
            if let (Ok(column), Ok(row)) = (u16::try_from(column), u16::try_from(row)) {
                buf[(area.x + column, area.y + row)]
                    .set_char(' ')
                    .set_bg(CURSOR_COLOUR);
            }
        }
    }
}
