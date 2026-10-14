use std::sync::Arc;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::corpus::Corpus;
use crate::scene::{Rng, Scene};
use crate::theme;

const MOST_GLYPHS_PER_TICK: u32 = 3;
const LONGEST_LINE_PAUSE: u32 = 4;

/// Ticks a finished file stays on screen before the next one starts.
const FILE_PAUSE: u32 = 48;

const PATH_COLOUR: Color = theme::FOAM;
const CODE_COLOUR: Color = theme::TEXT;
const COMMENT_COLOUR: Color = theme::MUTED;
const CURSOR_COLOUR: Color = theme::TEXT;

#[derive(Debug)]
pub struct Typing {
    rng: Rng,
    corpus: Arc<Corpus>,
    file: usize,
    glyphs: Vec<char>,
    offset: usize,
    lines: Vec<String>,
    pause: u32,
    finished: bool,
}

impl Typing {
    pub fn new(rng: Rng, corpus: Arc<Corpus>) -> Self {
        let mut scene = Self {
            rng,
            corpus,
            file: 0,
            glyphs: Vec::new(),
            offset: 0,
            lines: vec![String::new()],
            pause: 0,
            finished: false,
        };
        scene.open(0);
        scene
    }

    fn open(&mut self, file: usize) {
        self.file = file;
        self.glyphs = self
            .corpus
            .files()
            .get(file)
            .map(|source| source.text.chars().collect())
            .unwrap_or_default();
        self.offset = 0;
        self.lines = vec![String::new()];
        self.finished = false;
    }

    fn path(&self) -> &str {
        self.corpus
            .files()
            .get(self.file)
            .map_or("", |source| source.path.as_str())
    }
}

fn is_comment(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("//") || line.starts_with('#')
}

impl Scene for Typing {
    fn tick(&mut self, _area: Rect) {
        if self.pause > 0 {
            self.pause -= 1;
            return;
        }
        if self.finished {
            let next = (self.file + 1) % self.corpus.files().len().max(1);
            self.open(next);
            return;
        }
        let budget = self.rng.random_range(1..=MOST_GLYPHS_PER_TICK);
        for _ in 0..budget {
            let Some(&glyph) = self.glyphs.get(self.offset) else {
                self.finished = true;
                self.pause = FILE_PAUSE;
                return;
            };
            self.offset += 1;
            if glyph == '\n' {
                self.lines.push(String::new());
                self.pause = self.rng.random_range(1..=LONGEST_LINE_PAUSE);
                return;
            }
            if let Some(line) = self.lines.last_mut() {
                line.push(glyph);
            }
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let width = usize::from(area.width);
        let mut put = |row: u16, text: &str, colour: Color| {
            for (column, glyph) in text.chars().take(width).enumerate() {
                let Ok(column) = u16::try_from(column) else {
                    break;
                };
                buf[(area.x + column, area.y + row)]
                    .set_char(glyph)
                    .set_fg(colour);
            }
        };
        put(0, self.path(), PATH_COLOUR);

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
            let colour = if is_comment(line) {
                COMMENT_COLOUR
            } else {
                CODE_COLOUR
            };
            put(row, line, colour);
        }

        if let Some(last) = shown.last() {
            let column = last.chars().count().min(width - 1);
            let row = shown.len();
            if let (Ok(column), Ok(row)) = (u16::try_from(column), u16::try_from(row)) {
                buf[(area.x + column, area.y + row)]
                    .set_char(' ')
                    .set_bg(CURSOR_COLOUR);
            }
        }
    }
}
