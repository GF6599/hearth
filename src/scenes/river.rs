use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::corpus::Corpus;
use crate::history::{Commit, History};
use crate::octant::{self, CELL_HEIGHT, CELL_WIDTH, Canvas};
use crate::scene::{self, Rng, Scene};
use crate::splitmix;
use crate::theme::Palette;

pub const NOTICE: &str = "no git history to draw here";
pub const READING: &str = "reading the history";

/// The pixel rows from one commit to the next. Three cells, so a lane that
/// moves across has room to curve.
const ROW_GAP: i64 = 12;
/// The most pixels from one lane to the next. A history with many lanes at
/// once packs them closer, down to one cell.
const LANE_GAP: i64 = 12;
/// The widest a lane of water grows, in pixels, and the pixels it leaves
/// clear of the next lane, so lanes packed close still read as two.
const WATER: i64 = 8;
const LANE_CLEAR: i64 = 4;
/// The share of the pane's pixels across that the lanes may take, and the
/// share of its cells the water takes at the least.
const LANE_ROOM: (i64, i64) = (3, 5);
const RIVER: (i64, i64) = (3, 5);
/// The cells of water between the outer lanes and each bank, at the
/// least.
const SHORE: i64 = 3;
/// The cells from the rightmost lane at the subject's row to the subject.
const SUBJECT_GAP: i64 = 2;
/// The pixel rows the river moves down in a tick: a commit about every one
/// and a half seconds, slow enough to read its subject.
const SCROLL: f64 = 0.5;
/// The pixel rows the river runs on past the newest commit before it starts
/// again from the oldest, so the newest is not cut off the moment it is
/// read.
const PAUSE: i64 = 2 * ROW_GAP;
/// The pixel rows the foam moves down its lane in a tick, on top of the
/// river's own move, so the water flows past the stones.
const FLOW: f64 = 1.0;
/// The pixel rows from one band of foam to the next in a lane, and the pixel
/// rows one band covers.
const FOAM_PERIOD: i64 = 20;
const FOAM: i64 = 2;
/// How many lanes have their own phase of foam before the phases repeat.
const PHASES: usize = 16;
/// The pixel rows from one row of ripples to the next, and the share of the
/// places on such a row that hold a ripple, one in this many. A ripple is
/// two to four pixels long, so it stays thinner than a lane's full cells.
const RIPPLE_ROWS: i64 = 4;
const RIPPLE_ODDS: u64 = 3;
/// How fast each bank wanders, in radians a pixel row, and how far, in
/// cells. Each bank has its own phase, so the river widens and narrows.
const MEANDER: f64 = 0.045;
const MEANDER_REACH: f64 = 1.4;

/// What a pixel shows when more than one thing falls on it, from the
/// lowest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Layer {
    Bank,
    Ripple,
    Water,
    Foam,
    Stone,
}

/// The bed or the ground shows behind a cell with an empty pixel, so a
/// stone covers the water in a cell with the bed in it. Water takes the
/// colour of foam in front of it, so a band of foam leaves no gap in its
/// lane.
impl octant::Layer for Layer {
    fn joins(self, top: Self) -> bool {
        self.is_liquid() && top.is_liquid()
    }
}

impl Layer {
    fn is_liquid(self) -> bool {
        matches!(self, Layer::Water | Layer::Foam)
    }
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
    /// The history on its way from a thread that runs git, until it lands.
    /// A large repository takes a while, and the app must not wait on it.
    reading: Option<Receiver<History>>,
    history: History,
    rows: Vec<Row>,
    /// The most lanes the history holds at once.
    widest: usize,
    ticks: u64,
    /// How far the scene has run towards its next tick.
    progress: f64,
    /// Where the foam of each lane starts, so the lanes do not foam in step.
    phases: Vec<i64>,
    /// Seeds the ripples and the sand.
    salt: u64,
    /// Where the wander of each bank starts, as an angle.
    banks: [f64; 2],
}

impl River {
    /// Starts the read of the corpus's history on its own thread. A corpus
    /// with nothing to read from, or a thread that cannot start, gives no
    /// history, and the pane shows the notice.
    pub fn new(mut rng: Rng, palette: &'static Palette, corpus: Arc<Corpus>) -> Self {
        let reading = corpus.history_root().and_then(|root| {
            let root = root.to_path_buf();
            let (sender, receiver) = mpsc::channel();
            thread::Builder::new()
                .name("river history".into())
                .spawn(move || {
                    let _ = sender.send(History::read(&root));
                })
                .ok()
                .map(|_| receiver)
        });
        let phases = (0..PHASES)
            .map(|_| rng.random_range(0..FOAM_PERIOD))
            .collect();
        Self {
            palette,
            reading,
            history: History::default(),
            rows: Vec::new(),
            widest: 0,
            ticks: 0,
            progress: 0.0,
            phases,
            salt: rng.random(),
            banks: [
                rng.random_range(0.0..std::f64::consts::TAU),
                rng.random_range(0.0..std::f64::consts::TAU),
            ],
        }
    }

    fn land(&mut self, history: History) {
        let (rows, widest) = lay_out(history.commits());
        (self.rows, self.widest, self.history) = (rows, widest, history);
        self.reading = None;
    }

    /// The ticks since the history landed, with the share of the next.
    fn time(&self) -> f64 {
        self.ticks as f64 + self.progress
    }

    /// The pixel rows the river has moved down.
    fn travelled(&self) -> i64 {
        (self.time() * SCROLL) as i64
    }

    /// The world pixel row at the middle of the pane. It starts at the oldest
    /// commit, with the newer commits above it, and moves up the history,
    /// so the river flows down the pane. Past the newest commit it starts
    /// again.
    fn focus(&self) -> i64 {
        let oldest = (self.rows.len() as i64 - 1) * ROW_GAP;
        oldest - self.travelled().rem_euclid(oldest + PAUSE)
    }

    /// The commit whose stone is nearest the middle row, if one is within
    /// half the gap to the next.
    fn at_middle(&self) -> Option<usize> {
        let focus = self.focus();
        if focus < -ROW_GAP / 2 {
            return None;
        }
        Some(((focus + ROW_GAP / 2) / ROW_GAP) as usize).filter(|index| *index < self.rows.len())
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

    /// A number from `place` that looks random and is the same every frame,
    /// so the ripples and the sand stay put on the water and the shore.
    fn noise(&self, place: (i64, i64)) -> u64 {
        splitmix::mix(
            self.salt
                .wrapping_add((place.0 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15))
                .wrapping_add((place.1 as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f)),
        )
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
    fn tick(&mut self, _area: Rect) {
        if let Some(reading) = &self.reading {
            match reading.try_recv() {
                Ok(history) => self.land(history),
                Err(TryRecvError::Disconnected) => self.land(History::default()),
                Err(TryRecvError::Empty) => {}
            }
            return;
        }
        self.ticks += 1;
    }

    fn set_progress(&mut self, progress: f64) {
        self.progress = progress;
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if self.reading.is_some() {
            scene::draw_notice(READING, area, self.palette, buf);
            return;
        }
        if self.rows.is_empty() {
            scene::draw_notice(NOTICE, area, self.palette, buf);
            return;
        }
        let (columns, lines) = (usize::from(area.width), usize::from(area.height));
        if columns == 0 || lines == 0 {
            return;
        }
        let (across, down) = ((columns * CELL_WIDTH) as i64, (lines * CELL_HEIGHT) as i64);
        let middle = (lines / 2 * CELL_HEIGHT) as i64 + 2;
        let top = self.focus() - middle;

        let widest = self.widest.max(1) as i64;
        let room = across * LANE_ROOM.0 / LANE_ROOM.1;
        let gap = ((room / widest).min(LANE_GAP) & !1).max(2);
        let water = (gap - LANE_CLEAR).clamp(2, WATER);
        let span = (widest - 1) * gap + water;
        let left = ((across - span) / 2) & !1;
        let lane_x = |lane: usize| left + lane as i64 * gap;

        // The water's first and last column on each row. Wide at the
        // least, so one lane still reads as a river, and never so wide that
        // a bank leaves the pane.
        let centre = columns as i64 / 2;
        let half = (columns as i64 * RIVER.0 / RIVER.1 / 2)
            .max(span / CELL_WIDTH as i64 / 2 + SHORE)
            .min(columns as i64 / 2 - 3)
            .max(0);
        let shores: Vec<(i64, i64)> = (0..lines as i64)
            .map(|line| {
                let wander = |side: usize| {
                    let at = (top + line * CELL_HEIGHT as i64) as f64 * MEANDER + self.banks[side];
                    (at.sin() * MEANDER_REACH).round() as i64
                };
                (centre - half + wander(0), centre + half + wander(1))
            })
            .collect();

        let width = columns * CELL_WIDTH;
        let mut layers: Vec<Option<(Layer, Color)>> = vec![None; width * lines * CELL_HEIGHT];
        let mut plot = |x: i64, y: i64, layer: Layer, colour: Color| {
            if x < 0 || y < 0 || x >= across || y >= down {
                return;
            }
            let pixel = &mut layers[y as usize * width + x as usize];
            if pixel.is_none_or(|(under, _)| layer > under) {
                *pixel = Some((layer, colour));
            }
        };

        // The shore: a line of sand beside the water and loose grains
        // beyond it, both fixed to the ground so they flow with the river.
        for (line, &(first, last)) in shores.iter().enumerate() {
            for y in line as i64 * CELL_HEIGHT as i64..(line as i64 + 1) * CELL_HEIGHT as i64 {
                let ground = top + y;
                for (edge, outward) in [
                    (first * CELL_WIDTH as i64 - 1, -1),
                    (last * CELL_WIDTH as i64 + CELL_WIDTH as i64, 1),
                ] {
                    plot(edge, y, Layer::Bank, self.palette.gold);
                    for step in 1..6 {
                        let x = edge + outward * step;
                        if self
                            .noise((x * outward, ground))
                            .is_multiple_of(step as u64 + 1)
                        {
                            plot(x, y, Layer::Bank, self.palette.gold);
                        }
                    }
                }
            }
        }

        // The water moves with the river and on down its lanes. The two
        // moves count as one, so the water steps a pixel at a time between
        // ticks. Counted apart, they step on the same tick and the water
        // jumps two pixels.
        let flow = (self.time() * (SCROLL + FLOW)) as i64 - self.travelled();

        // Ripples on the open water, short dashes across the flow that move
        // down with the foam, faster than the banks.
        for y in 0..down {
            let water = top + y - flow;
            if water.rem_euclid(RIPPLE_ROWS) != 0 {
                continue;
            }
            let (first, last) = shores[(y / CELL_HEIGHT as i64) as usize];
            let (from, to) = (first * CELL_WIDTH as i64, (last + 1) * CELL_WIDTH as i64);
            for place in (from / 6)..=(to / 6) {
                let noise = self.noise((place, water.div_euclid(RIPPLE_ROWS)));
                if !noise.is_multiple_of(RIPPLE_ODDS) {
                    continue;
                }
                let start = place * 6 + (noise >> 8) as i64 % 2;
                for x in start..start + 2 + (noise >> 16) as i64 % 3 {
                    if (from..to).contains(&x) {
                        plot(x, y, Layer::Ripple, self.palette.pine);
                    }
                }
            }
        }

        let first = ((top - ROW_GAP) / ROW_GAP).max(0) as usize;
        let last = ((top + down) / ROW_GAP + 1).max(0) as usize;
        let shown = first..=last.min(self.rows.len() - 1);
        let mut wet = |x: i64, y: i64, lane: usize| {
            let phase = self.phases[lane % PHASES];
            let (layer, colour) = if (top + y - flow + phase).rem_euclid(FOAM_PERIOD) < FOAM {
                (Layer::Foam, self.palette.foam)
            } else {
                (Layer::Water, self.palette.pine)
            };
            for across in x..x + water {
                plot(across, y, layer, colour);
            }
        };
        for index in shown.clone() {
            let row = &self.rows[index];
            let y = index as i64 * ROW_GAP - top;
            let oldest = index + 1 == self.rows.len();
            for &(from, to) in &row.lines {
                let (from_x, to_x) = (lane_x(from) as f64, lane_x(to) as f64);
                for step in 0..=ROW_GAP {
                    let x = from_x + (to_x - from_x) * ease(step as f64 / ROW_GAP as f64);
                    wet(x.round() as i64, y + step, to);
                }
                if oldest {
                    for below in y + ROW_GAP..down {
                        wet(lane_x(to), below, to);
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
                for dx in -2..=water + 1 {
                    let corner = (dx == -2 || dx == water + 1) && (dy == -2 || dy == 1);
                    if !corner {
                        plot(x + dx, y + dy, Layer::Stone, colour);
                    }
                }
            }
        }

        let mut canvas = Canvas::new(area.width, area.height, self.palette);
        for (line, &(first, last)) in shores.iter().enumerate() {
            for column in 0..columns {
                let empty = if (first..=last).contains(&(column as i64)) {
                    self.palette.overlay
                } else {
                    self.palette.base
                };
                let at = |pixel: usize| {
                    (
                        column * CELL_WIDTH + pixel % CELL_WIDTH,
                        line * CELL_HEIGHT + pixel / CELL_WIDTH,
                    )
                };
                let pixels = std::array::from_fn(|pixel| {
                    let (x, y) = at(pixel);
                    layers[y * width + x]
                });
                for (pixel, colour) in octant::settle(pixels, empty).into_iter().enumerate() {
                    let (x, y) = at(pixel);
                    canvas.set(x, y, colour);
                }
            }
        }
        canvas.render(area, buf);

        let Some(at_middle) = self.at_middle() else {
            return;
        };
        let Some(commit) = self.history.commits().get(at_middle) else {
            return;
        };
        let line = (at_middle as i64 * ROW_GAP - top).div_euclid(CELL_HEIGHT as i64);
        let Ok(line) = u16::try_from(line) else {
            return;
        };
        if line >= area.height {
            return;
        }
        let column = (lane_x(self.rightmost(at_middle)) + water) / CELL_WIDTH as i64 + SUBJECT_GAP;
        let Ok(column) = u16::try_from(column) else {
            return;
        };
        if column >= area.width {
            return;
        }
        buf.set_stringn(
            area.x + column,
            area.y + line,
            &commit.subject,
            usize::from(area.width - column),
            Style::default().fg(self.palette.text),
        );
    }
}
