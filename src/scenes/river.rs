use std::sync::Arc;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};

use crate::braille::{self, CELL_HEIGHT, CELL_WIDTH};
use crate::corpus::Corpus;
use crate::history::Commit;
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

pub const NOTICE: &str = "no git history to draw here";

/// The dot rows from one commit to the next. Three cells, so a lane that
/// moves across has room to curve.
const ROW_GAP: i64 = 12;
/// The most dots from one lane to the next. A history with many lanes at
/// once packs them closer, down to one cell.
const LANE_GAP: i64 = 8;
/// The width of a lane of water, in dots.
const WATER: i64 = 4;
/// The cells from the rightmost lane at the subject's row to the subject.
const SUBJECT_GAP: i64 = 2;

/// What a cell shows when more than one thing falls in it, from the
/// lowest. One braille cell holds one colour, so a stone covers the water
/// in its cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Layer {
    Water,
    Stone,
}

/// One commit in the river: the lane its stone is in, and the lines from
/// its row to the next row down, each from a lane on this row to a lane on
/// the next. The lines of the oldest row run on past the bottom, to
/// parents older than the history read.
#[derive(Debug, Clone)]
struct Row {
    lane: usize,
    merge: bool,
    lines: Vec<(usize, usize)>,
}

#[derive(Debug)]
pub struct River {
    palette: &'static Palette,
    corpus: Arc<Corpus>,
    rows: Vec<Row>,
    /// The most lanes the history holds at once.
    widest: usize,
}

impl River {
    pub fn new(_rng: Rng, palette: &'static Palette, corpus: Arc<Corpus>) -> Self {
        let (rows, widest) = lay_out(corpus.history().commits());
        Self {
            palette,
            corpus,
            rows,
            widest,
        }
    }

    /// The world dot row at the middle of the pane. The oldest commit stands
    /// there, so the river shows the newer commits above it.
    fn focus(&self) -> i64 {
        (self.rows.len() as i64 - 1) * ROW_GAP
    }

    /// The rightmost lane with water at the row of commit `index`.
    fn rightmost(&self, index: usize) -> usize {
        let above = index
            .checked_sub(1)
            .map(|above| &self.rows[above].lines[..])
            .unwrap_or_default();
        self.rows[index]
            .lines
            .iter()
            .map(|line| line.0)
            .chain(above.iter().map(|line| line.1))
            .fold(self.rows[index].lane, usize::max)
    }

    fn draw_notice(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let length = NOTICE.chars().count() as u16;
        let x = area.x + area.width.saturating_sub(length) / 2;
        buf.set_stringn(
            x,
            area.y + area.height / 2,
            NOTICE,
            usize::from(area.width - (x - area.x)),
            Style::default().fg(self.palette.subtle),
        );
    }
}

/// Lays the commits out in lanes the way `git log --graph` does, the
/// newest first. Each lane waits for the commit it leads to. A commit takes
/// the first lane that waits for it, or the first free one, and every
/// other lane that waits for it joins it there. Its first parent goes on in
/// its lane, even when another lane waits for that parent too, so the main
/// line keeps to the left as it does in `git log --graph`. Each other
/// parent takes a lane already waiting for it or a free one.
fn lay_out(commits: &[Commit]) -> (Vec<Row>, usize) {
    let mut waiting: Vec<Option<&str>> = Vec::new();
    let mut rows: Vec<Row> = Vec::with_capacity(commits.len());
    let mut after: Vec<Vec<Option<&str>>> = Vec::with_capacity(commits.len());
    let mut widest = 0;
    for commit in commits {
        let hash = Some(commit.hash.as_str());
        let lane = waiting
            .iter()
            .position(|slot| *slot == hash)
            .or_else(|| waiting.iter().position(Option::is_none))
            .unwrap_or_else(|| {
                waiting.push(None);
                waiting.len() - 1
            });
        for slot in &mut waiting {
            if *slot == hash {
                *slot = None;
            }
        }
        let before = waiting.clone();
        let mut fed = Vec::new();
        for (index, parent) in commit.parents.iter().enumerate() {
            let parent = Some(parent.as_str());
            if index > 0
                && let Some(slot) = waiting.iter().position(|slot| *slot == parent)
            {
                fed.push(slot);
                continue;
            }
            let slot = if index == 0 {
                lane
            } else if let Some(free) = waiting.iter().position(Option::is_none) {
                free
            } else {
                waiting.push(None);
                waiting.len() - 1
            };
            waiting[slot] = parent;
            fed.push(slot);
        }
        let mut lines: Vec<(usize, usize)> = (0..waiting.len())
            .filter(|slot| waiting[*slot].is_some() && before.get(*slot) == Some(&waiting[*slot]))
            .map(|slot| (slot, slot))
            .collect();
        lines.extend(fed.into_iter().map(|slot| (lane, slot)));
        widest = widest.max(waiting.len()).max(lane + 1);
        while waiting.last() == Some(&None) {
            waiting.pop();
        }
        after.push(waiting.clone());
        rows.push(Row {
            lane,
            merge: commit.parents.len() > 1,
            lines,
        });
    }
    // A line ends at the lane of the commit it waits for, if that is the
    // next one down, and goes on in its own lane if not.
    for index in 0..rows.len().saturating_sub(1) {
        let next = Some(commits[index + 1].hash.as_str());
        let next_lane = rows[index + 1].lane;
        for line in &mut rows[index].lines {
            if after[index][line.1] == next {
                line.1 = next_lane;
            }
        }
    }
    (rows, widest)
}

/// Eases from 0 to 1 with no slope at either end, so a lane leaves one
/// column and meets the next running straight down.
fn ease(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

impl Scene for River {
    fn tick(&mut self, _area: Rect) {}

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if self.rows.is_empty() {
            self.draw_notice(area, buf);
            return;
        }
        let (columns, lines) = (usize::from(area.width), usize::from(area.height));
        if columns == 0 || lines == 0 {
            return;
        }
        let (across, down) = ((columns * CELL_WIDTH) as i64, (lines * CELL_HEIGHT) as i64);
        let middle = (lines / 2 * CELL_HEIGHT) as i64 + 2;
        let widest = self.widest.max(1) as i64;
        let gap = (across / 2 / widest).min(LANE_GAP) & !1;
        let gap = gap.max(2);
        let left = ((across / 4 - widest * gap / 2) & !1).max(2);
        let lane_x = |lane: usize| left + lane as i64 * gap;
        let top = self.focus() - middle;

        let mut cells: Vec<Option<(Layer, u32, Color)>> = vec![None; columns * lines];
        let mut plot = |x: i64, y: i64, layer: Layer, colour: Color| {
            if x < 0 || y < 0 || x >= across || y >= down {
                return;
            }
            let (x, y) = (x as usize, y as usize);
            let index = y / CELL_HEIGHT * columns + x / CELL_WIDTH;
            let bit = braille::dot(x % CELL_WIDTH, y % CELL_HEIGHT);
            match &mut cells[index] {
                Some(cell) if cell.0 > layer => {}
                Some(cell) if cell.0 == layer => cell.1 |= bit,
                cell => *cell = Some((layer, bit, colour)),
            }
        };

        let first = ((top - ROW_GAP) / ROW_GAP).max(0) as usize;
        let last = ((top + down) / ROW_GAP + 1).max(0) as usize;
        let shown = first..=last.min(self.rows.len() - 1);
        for index in shown.clone() {
            let row = &self.rows[index];
            let y = index as i64 * ROW_GAP - top;
            let oldest = index + 1 == self.rows.len();
            for &(from, to) in &row.lines {
                let (from_x, to_x) = (lane_x(from) as f64, lane_x(to) as f64);
                for step in 0..=ROW_GAP {
                    let x = from_x + (to_x - from_x) * ease(step as f64 / ROW_GAP as f64);
                    let x = x.round() as i64;
                    for across in x..x + WATER {
                        plot(across, y + step, Layer::Water, self.palette.pine);
                    }
                }
                if oldest {
                    for below in y + ROW_GAP..down {
                        for across in lane_x(to)..lane_x(to) + WATER {
                            plot(across, below, Layer::Water, self.palette.pine);
                        }
                    }
                }
            }
        }
        for index in shown {
            let row = &self.rows[index];
            let (x, y) = (lane_x(row.lane), index as i64 * ROW_GAP - top);
            let colour = if row.merge {
                self.palette.iris
            } else {
                self.palette.subtle
            };
            for dy in -2..=1 {
                for dx in -2..=WATER + 1 {
                    let corner = (dx == -2 || dx == WATER + 1) && (dy == -2 || dy == 1);
                    if !corner {
                        plot(x + dx, y + dy, Layer::Stone, colour);
                    }
                }
            }
        }

        for (index, cell) in cells.into_iter().enumerate() {
            let Some((_, dots, colour)) = cell else {
                continue;
            };
            let (column, line) = ((index % columns) as u16, (index / columns) as u16);
            buf[Position::new(area.x + column, area.y + line)]
                .set_char(braille::glyph(dots))
                .set_fg(colour);
        }

        let at_middle = (self.focus() as f64 / ROW_GAP as f64).round() as usize;
        let Some(commit) = self.corpus.history().commits().get(at_middle) else {
            return;
        };
        let column = (lane_x(self.rightmost(at_middle)) + WATER) / CELL_WIDTH as i64 + SUBJECT_GAP;
        let Ok(column) = u16::try_from(column) else {
            return;
        };
        if column >= area.width {
            return;
        }
        buf.set_stringn(
            area.x + column,
            area.y + (middle / CELL_HEIGHT as i64) as u16,
            &commit.subject,
            usize::from(area.width - column),
            Style::default().fg(self.palette.text),
        );
    }
}
