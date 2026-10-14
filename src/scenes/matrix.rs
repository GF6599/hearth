use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::scene::{Rng, Scene};
use crate::theme;

/// Half-width katakana, then digits and capitals. Full-width katakana are
/// two cells wide and tear every column to their right, so they stay out.
pub fn glyphs() -> impl Iterator<Item = char> {
    (0xFF66..=0xFF9D)
        .filter_map(char::from_u32)
        .chain('0'..='9')
        .chain('A'..='Z')
}

pub const HEAD: Color = theme::TEXT;
const NEAR_TAIL: Color = theme::FOAM;
const FAR_TAIL: Color = theme::PINE;

/// Ticks per row, drawn per column, so neighbours fall at their own pace.
const SLOWEST: u16 = 3;

/// One in this many ticks, a column re-rolls one glyph in its tail.
const CHURN: u16 = 4;

#[derive(Debug)]
struct Column {
    /// The row of the head. Negative while the column waits above the top.
    head: i32,
    tail: i32,
    ticks_per_row: u16,
    ticks_since_step: u16,
    glyphs: Vec<char>,
}

impl Column {
    fn new(rng: &mut Rng, table: &[char], height: u16) -> Self {
        let rows = i32::from(height);
        let shortest = (rows / 3).max(2);
        let mut column = Self {
            head: rng.random_range(-rows..rows.max(1)),
            tail: rng.random_range(shortest..=rows.max(shortest)),
            ticks_per_row: rng.random_range(1..=SLOWEST),
            ticks_since_step: 0,
            glyphs: Vec::with_capacity(usize::from(height)),
        };
        column.glyphs = (0..height).map(|_| pick(rng, table)).collect();
        column
    }

    /// Sends the column back above the top with a fresh pace, after a wait
    /// of up to one screen height, so restarts do not line up.
    fn restart(&mut self, rng: &mut Rng, height: u16) {
        let rows = i32::from(height);
        let shortest = (rows / 3).max(2);
        self.head = -rng.random_range(1..=rows.max(1));
        self.tail = rng.random_range(shortest..=rows.max(shortest));
        self.ticks_per_row = rng.random_range(1..=SLOWEST);
        self.ticks_since_step = 0;
    }

    fn tick(&mut self, rng: &mut Rng, table: &[char], height: u16) {
        self.ticks_since_step += 1;
        if self.ticks_since_step >= self.ticks_per_row {
            self.ticks_since_step = 0;
            self.head += 1;
            if let Ok(row) = usize::try_from(self.head)
                && let Some(glyph) = self.glyphs.get_mut(row)
            {
                *glyph = pick(rng, table);
            }
        }
        if rng.random_range(0..CHURN) == 0 && !self.glyphs.is_empty() {
            let row = rng.random_range(0..self.glyphs.len());
            self.glyphs[row] = pick(rng, table);
        }
        if self.head - self.tail >= i32::from(height) {
            self.restart(rng, height);
        }
    }
}

fn pick(rng: &mut Rng, table: &[char]) -> char {
    table[rng.random_range(0..table.len())]
}

#[derive(Debug)]
pub struct Matrix {
    rng: Rng,
    table: Vec<char>,
    size: (u16, u16),
    columns: Vec<Column>,
}

impl Matrix {
    pub fn new(rng: Rng) -> Self {
        Self {
            rng,
            table: glyphs().collect(),
            size: (0, 0),
            columns: Vec::new(),
        }
    }
}

impl Scene for Matrix {
    fn tick(&mut self, area: Rect) {
        if self.size != (area.width, area.height) {
            self.size = (area.width, area.height);
            self.columns = (0..area.width)
                .map(|_| Column::new(&mut self.rng, &self.table, area.height))
                .collect();
        }
        for column in &mut self.columns {
            column.tick(&mut self.rng, &self.table, area.height);
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        for (x, column) in (0..area.width).zip(&self.columns) {
            for y in 0..area.height {
                let distance = column.head - i32::from(y);
                if distance < 0 || distance > column.tail {
                    continue;
                }
                let Some(glyph) = column.glyphs.get(usize::from(y)) else {
                    continue;
                };
                let color = if distance == 0 {
                    HEAD
                } else if distance * 3 <= column.tail {
                    NEAR_TAIL
                } else {
                    FAR_TAIL
                };
                buf[(area.x + x, area.y + y)].set_char(*glyph).set_fg(color);
            }
        }
    }
}
