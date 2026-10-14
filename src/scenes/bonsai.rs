use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::symbols::border;

use crate::scene::{Rng, Scene};
use crate::theme;

/// Steps a trunk may take. The shoots it sends out get a little more.
const LIFE: u32 = 32;

/// The cbonsai multiplier: it paces shoots, the young-trunk phase and the
/// point where a branch starts to die.
const MULTIPLIER: u32 = 5;

const MOST_BRANCHES: usize = 200;

/// A branch steps once in this many ticks, so the tree grows on screen.
const GROWTH_EVERY: u32 = 2;

/// Ticks a finished tree stays before a new one grows.
pub const HOLD_TICKS: usize = 160;

pub const POT_HEIGHT: u16 = 3;
const POT_WIDTH: usize = 15;

const WOOD: Color = theme::GOLD;
const LEAVES: [Color; 2] = [theme::PINE, theme::FOAM];
const POT: Color = theme::MUTED;

const RUNES: border::Set<'static> = theme::BORDER.to_border_set();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Trunk,
    ShootLeft,
    ShootRight,
    Dying,
    Dead,
}

#[derive(Debug)]
struct Branch {
    x: i32,
    y: i32,
    kind: Kind,
    life: u32,
    age: u32,
}

#[derive(Debug)]
pub struct Bonsai {
    rng: Rng,
    size: (u16, u16),
    cells: Vec<Option<(char, Color)>>,
    branches: Vec<Branch>,
    spawned: usize,
    shoots: u32,
    cooldown: i32,
    ticks: u32,
    held: usize,
}

impl Bonsai {
    pub fn new(rng: Rng) -> Self {
        Self {
            rng,
            size: (0, 0),
            cells: Vec::new(),
            branches: Vec::new(),
            spawned: 0,
            shoots: 0,
            cooldown: 0,
            ticks: 0,
            held: 0,
        }
    }

    /// The row the trunk starts on, just above the pot.
    fn ground(&self) -> i32 {
        i32::from(self.size.1) - i32::from(POT_HEIGHT) - 1
    }

    fn plant(&mut self) {
        let (width, height) = self.size;
        self.cells = vec![None; usize::from(width) * usize::from(height)];
        self.branches.clear();
        self.spawned = 0;
        self.shoots = 0;
        self.cooldown = 0;
        self.held = 0;
        self.draw_pot();
        let centre = i32::from(width) / 2;
        self.sprout(centre, self.ground(), Kind::Trunk, LIFE);
    }

    fn draw_pot(&mut self) {
        let width = i32::from(self.size.0);
        let left = (width - i32::try_from(POT_WIDTH).unwrap_or(0)) / 2;
        let inner = POT_WIDTH.saturating_sub(4);
        let rows = [
            format!(
                "{}{}{}",
                RUNES.top_left,
                RUNES.horizontal_top.repeat(POT_WIDTH.saturating_sub(2)),
                RUNES.top_right
            ),
            format!(
                "{}{}{}{}{}",
                RUNES.bottom_left,
                RUNES.top_right,
                " ".repeat(inner),
                RUNES.top_left,
                RUNES.bottom_right
            ),
            format!(
                " {}{}{} ",
                RUNES.bottom_left,
                RUNES.horizontal_bottom.repeat(inner),
                RUNES.bottom_right
            ),
        ];
        let top = self.ground() + 1;
        for (offset, row) in rows.iter().enumerate() {
            let y = top + i32::try_from(offset).unwrap_or(0);
            for (column, glyph) in row.chars().enumerate() {
                if glyph != ' ' {
                    let x = left + i32::try_from(column).unwrap_or(0);
                    self.set(x, y, glyph, POT);
                }
            }
        }
    }

    fn set(&mut self, x: i32, y: i32, glyph: char, colour: Color) {
        let (width, height) = (i32::from(self.size.0), i32::from(self.size.1));
        if x < 0 || y < 0 || x >= width || y >= height {
            return;
        }
        let index = usize::try_from(y * width + x).unwrap_or(usize::MAX);
        if let Some(cell) = self.cells.get_mut(index) {
            *cell = Some((glyph, colour));
        }
    }

    fn sprout(&mut self, x: i32, y: i32, kind: Kind, life: u32) {
        if self.spawned >= MOST_BRANCHES || life == 0 {
            return;
        }
        self.spawned += 1;
        self.branches.push(Branch {
            x,
            y,
            kind,
            life,
            age: 0,
        });
    }

    fn deltas(&mut self, kind: Kind, life: u32, age: u32) -> (i32, i32) {
        let rng = &mut self.rng;
        match kind {
            Kind::Trunk if age <= 2 || life < 4 => (rng.random_range(-1..=1), 0),
            Kind::Trunk if age < MULTIPLIER * 3 => {
                let dy = if age.is_multiple_of((MULTIPLIER / 2).max(1)) {
                    -1
                } else {
                    0
                };
                let dx = match rng.random_range(0..10) {
                    0 => -2,
                    1..=3 => -1,
                    4..=5 => 0,
                    6..=8 => 1,
                    _ => 2,
                };
                (dx, dy)
            }
            Kind::Trunk => {
                let dy = if rng.random_range(0..10) > 2 { -1 } else { 0 };
                (rng.random_range(-1..=1), dy)
            }
            Kind::ShootLeft | Kind::ShootRight => {
                let dy = match rng.random_range(0..10) {
                    0..=1 => -1,
                    2..=7 => 0,
                    _ => 1,
                };
                let outward = match rng.random_range(0..10) {
                    0..=1 => 2,
                    2..=5 => 1,
                    6..=8 => 0,
                    _ => -1,
                };
                let dx = if kind == Kind::ShootLeft {
                    -outward
                } else {
                    outward
                };
                (dx, dy)
            }
            Kind::Dying => {
                let dy = match rng.random_range(0..10) {
                    0..=1 => -1,
                    2..=8 => 0,
                    _ => 1,
                };
                let dx = match rng.random_range(0..15) {
                    0 => -3,
                    1..=2 => -2,
                    3..=5 => -1,
                    6..=8 => 0,
                    9..=11 => 1,
                    12..=13 => 2,
                    _ => 3,
                };
                (dx, dy)
            }
            Kind::Dead => {
                let dy = match rng.random_range(0..10) {
                    0..=2 => -1,
                    3..=6 => 0,
                    _ => 1,
                };
                (rng.random_range(-1..=1), dy)
            }
        }
    }

    fn step(&mut self, mut branch: Branch) -> Option<Branch> {
        branch.life -= 1;
        branch.age += 1;
        let (dx, mut dy) = self.deltas(branch.kind, branch.life, branch.age);
        if dy > 0 && branch.y + dy > self.ground() {
            dy = 0;
        }

        let life = branch.life;
        let dying_soon = life < MULTIPLIER + 2;
        match branch.kind {
            _ if life < 3 => self.sprout(branch.x, branch.y, Kind::Dead, life),
            Kind::Trunk | Kind::ShootLeft | Kind::ShootRight if dying_soon => {
                self.sprout(branch.x, branch.y, Kind::Dying, life);
            }
            Kind::Trunk if self.rng.random_range(0..3) == 0 || life.is_multiple_of(MULTIPLIER) => {
                if self.rng.random_range(0..8) == 0 && life > 7 {
                    self.cooldown = i32::try_from(MULTIPLIER * 2).unwrap_or(0);
                    let extra = self.rng.random_range(0..5);
                    self.sprout(
                        branch.x,
                        branch.y,
                        Kind::Trunk,
                        (life + extra).saturating_sub(2),
                    );
                } else if self.cooldown <= 0 {
                    self.cooldown = i32::try_from(MULTIPLIER * 2).unwrap_or(0);
                    self.shoots += 1;
                    let kind = if self.shoots.is_multiple_of(2) {
                        Kind::ShootLeft
                    } else {
                        Kind::ShootRight
                    };
                    self.sprout(branch.x, branch.y, kind, life + MULTIPLIER);
                }
            }
            _ => {}
        }
        self.cooldown -= 1;

        branch.x += dx;
        branch.y += dy;
        let colour = match branch.kind {
            Kind::Dying | Kind::Dead => LEAVES[self.rng.random_range(0..LEAVES.len())],
            _ => WOOD,
        };
        for (offset, glyph) in glyphs(branch.kind, life, dx, dy).chars().enumerate() {
            let x = branch.x + i32::try_from(offset).unwrap_or(0);
            self.set(x, branch.y, glyph, colour);
        }

        (branch.life > 0).then_some(branch)
    }
}

/// The cbonsai strings: wood leans with the step, leaves are `&`.
fn glyphs(kind: Kind, life: u32, dx: i32, dy: i32) -> &'static str {
    match kind {
        Kind::Dying | Kind::Dead => "&",
        _ if life < 4 => "&",
        Kind::Trunk if dy == 0 => "/~",
        Kind::Trunk if dx < 0 => "\\|",
        Kind::Trunk if dx == 0 => "/|\\",
        Kind::Trunk => "|/",
        Kind::ShootLeft if dy > 0 => "\\",
        Kind::ShootLeft if dy == 0 => "\\_",
        Kind::ShootRight if dy > 0 => "/",
        Kind::ShootRight if dy == 0 => "_/",
        _ if dx < 0 => "\\|",
        _ if dx == 0 => "/|",
        _ => "/",
    }
}

impl Scene for Bonsai {
    fn tick(&mut self, area: Rect) {
        if (area.width, area.height) != self.size {
            self.size = (area.width, area.height);
            self.plant();
        }
        if area.width == 0 || area.height == 0 {
            return;
        }
        if self.branches.is_empty() {
            self.held += 1;
            if self.held > HOLD_TICKS {
                self.plant();
            }
            return;
        }
        self.ticks = self.ticks.wrapping_add(1);
        if !self.ticks.is_multiple_of(GROWTH_EVERY) {
            return;
        }
        let branches = std::mem::take(&mut self.branches);
        let mut alive = Vec::with_capacity(branches.len());
        for branch in branches {
            if let Some(branch) = self.step(branch) {
                alive.push(branch);
            }
        }
        alive.append(&mut self.branches);
        self.branches = alive;
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let (width, height) = self.size;
        for y in 0..area.height.min(height) {
            for x in 0..area.width.min(width) {
                let index = usize::from(y) * usize::from(width) + usize::from(x);
                if let Some((glyph, colour)) = self.cells[index] {
                    buf[(area.x + x, area.y + y)].set_char(glyph).set_fg(colour);
                }
            }
        }
    }
}
