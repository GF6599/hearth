use std::sync::Arc;

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;

use crate::corpus::Corpus;
use crate::scene::{QUARTERS_PER_TICK, Rate, Scene, SceneKind};
use crate::scenes::typing::Speed;
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// Every split is 1:1, so the panes keep their places relative to each
/// other on any screen. The focus moves on this plane, because a key
/// arrives with no frame size.
const PLANE: Rect = Rect::new(0, 0, 1 << 15, 1 << 15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Split {
    Beside,
    Below,
}

/// A leaf names its pane by its place in `Panes::panes`, so a split
/// replaces one leaf and moves no scene.
#[derive(Debug)]
enum Node {
    Leaf(usize),
    Split(Split, Box<Node>, Box<Node>),
}

/// The panes on screen as an i3 tree of splits, each split 1:1.
#[derive(Debug)]
pub struct Panes {
    tree: Node,
    panes: Vec<Pane>,
    focus: usize,
}

/// Where each pane and each rule goes in one area. `panes` is in the
/// order of `Panes::panes`.
#[derive(Debug)]
pub struct Layout {
    pub panes: Vec<Rect>,
    pub rules: Vec<(Split, Rect)>,
}

impl Panes {
    pub fn new(pane: Pane) -> Self {
        Self {
            tree: Node::Leaf(0),
            panes: vec![pane],
            focus: 0,
        }
    }

    pub fn focus(&self) -> usize {
        self.focus
    }

    pub fn is_split(&self) -> bool {
        self.panes.len() > 1
    }

    /// The nearest pane on that side that shares a row or a column with
    /// the focused one, and of two at the same distance, the one across
    /// from its middle. At an edge the focus stays.
    pub fn move_focus(&mut self, direction: Direction) {
        let panes = self.layout(PLANE).panes;
        let from = panes[self.focus];
        let middle = Position::new(from.x + from.width / 2, from.y + from.height / 2);
        let beside = |to: &Rect| from.top() < to.bottom() && to.top() < from.bottom();
        let above = |to: &Rect| from.left() < to.right() && to.left() < from.right();
        let gap = |to: &Rect| match direction {
            Direction::Left => {
                (to.right() <= from.left() && beside(to)).then(|| from.left() - to.right())
            }
            Direction::Right => {
                (from.right() <= to.left() && beside(to)).then(|| to.left() - from.right())
            }
            Direction::Up => {
                (to.bottom() <= from.top() && above(to)).then(|| from.top() - to.bottom())
            }
            Direction::Down => {
                (from.bottom() <= to.top() && above(to)).then(|| to.top() - from.bottom())
            }
        };
        let across = |to: &Rect| match direction {
            Direction::Left | Direction::Right => (to.top()..to.bottom()).contains(&middle.y),
            Direction::Up | Direction::Down => (to.left()..to.right()).contains(&middle.x),
        };
        let nearest = panes
            .iter()
            .enumerate()
            .filter_map(|(index, to)| Some((index, gap(to)?, !across(to))))
            .min_by_key(|&(_, gap, off)| (gap, off));
        if let Some((index, _, _)) = nearest {
            self.focus = index;
        }
    }

    pub fn focused(&self) -> &Pane {
        &self.panes[self.focus]
    }

    pub fn focused_mut(&mut self) -> &mut Pane {
        &mut self.panes[self.focus]
    }

    pub fn iter(&self) -> impl Iterator<Item = &Pane> {
        self.panes.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Pane> {
        self.panes.iter_mut()
    }

    /// The new pane goes after the focused one and takes the focus, as in
    /// i3.
    pub fn split(&mut self, split: Split, pane: Pane) {
        let new = self.panes.len();
        self.panes.push(pane);
        if let Some(leaf) = self.tree.leaf_mut(self.focus) {
            *leaf = Node::Split(
                split,
                Box::new(Node::Leaf(self.focus)),
                Box::new(Node::Leaf(new)),
            );
        }
        self.focus = new;
    }

    pub fn layout(&self, area: Rect) -> Layout {
        let mut layout = Layout {
            panes: vec![Rect::default(); self.panes.len()],
            rules: Vec::new(),
        };
        self.tree.place(area, &mut layout);
        layout
    }
}

impl Node {
    fn leaf_mut(&mut self, index: usize) -> Option<&mut Node> {
        match self {
            Node::Leaf(leaf) if *leaf == index => Some(self),
            Node::Leaf(_) => None,
            Node::Split(_, first, second) => match first.leaf_mut(index) {
                Some(leaf) => Some(leaf),
                None => second.leaf_mut(index),
            },
        }
    }

    fn place(&self, area: Rect, layout: &mut Layout) {
        match self {
            Node::Leaf(index) => layout.panes[*index] = area,
            Node::Split(split, first, second) => {
                let (before, rule, after) = halves(*split, area);
                layout.rules.push((*split, rule));
                first.place(before, layout);
                second.place(after, layout);
            }
        }
    }
}

/// The first half takes the smaller share of an odd length, and the rule
/// takes one cell between the two.
fn halves(split: Split, area: Rect) -> (Rect, Rect, Rect) {
    let length = match split {
        Split::Beside => area.width,
        Split::Below => area.height,
    };
    let first = length.saturating_sub(1) / 2;
    let rule = (length - first).min(1);
    let second = length - first - rule;
    match split {
        Split::Beside => (
            Rect::new(area.x, area.y, first, area.height),
            Rect::new(area.x + first, area.y, rule, area.height),
            Rect::new(area.x + first + rule, area.y, second, area.height),
        ),
        Split::Below => (
            Rect::new(area.x, area.y, area.width, first),
            Rect::new(area.x, area.y + first, area.width, rule),
            Rect::new(area.x, area.y + first + rule, area.width, second),
        ),
    }
}

/// Gold corner marks, the region form of the `[ ]` marker, so the frame
/// around the pane keeps its colour.
pub fn render_focus(pane: Rect, buf: &mut Buffer) {
    if pane.is_empty() {
        return;
    }
    let gold = Style::default().fg(theme::MARKER);
    buf[(pane.left(), pane.top())]
        .set_symbol(theme::RULE.top_left)
        .set_style(gold);
    buf[(pane.right() - 1, pane.bottom() - 1)]
        .set_symbol(theme::RULE.bottom_right)
        .set_style(gold);
}

/// A rule cell joins any rule that ends against its side, so two rules
/// meet in a tee and do not read as two frames drawn over each other.
pub fn render_rules(rules: &[(Split, Rect)], buf: &mut Buffer) {
    let on_rule = |x: Option<u16>, y: Option<u16>| match (x, y) {
        (Some(x), Some(y)) => rules
            .iter()
            .any(|(_, rule)| rule.contains(Position::new(x, y))),
        _ => false,
    };
    let style = Style::default().fg(theme::MUTED).bg(theme::BASE);
    for (split, rule) in rules {
        for Position { x, y } in rule.positions() {
            let symbol = match split {
                Split::Beside => match (
                    on_rule(x.checked_sub(1), Some(y)),
                    on_rule(x.checked_add(1), Some(y)),
                ) {
                    (true, true) => theme::RULE.cross,
                    (false, true) => theme::RULE.vertical_right,
                    (true, false) => theme::RULE.vertical_left,
                    (false, false) => theme::RULE.vertical,
                },
                Split::Below => match (
                    on_rule(Some(x), y.checked_sub(1)),
                    on_rule(Some(x), y.checked_add(1)),
                ) {
                    (true, true) => theme::RULE.cross,
                    (false, true) => theme::RULE.horizontal_down,
                    (true, false) => theme::RULE.horizontal_up,
                    (false, false) => theme::RULE.horizontal,
                },
            };
            buf[(x, y)].set_symbol(symbol).set_style(style);
        }
    }
}

/// One scene and the seed it was built from. Each pane keeps its own seed,
/// so a restart in one pane replays nothing in another.
pub struct Pane {
    pub kind: SceneKind,
    pub seed: u64,
    scene: Box<dyn Scene>,
    /// Quarter ticks owed and not yet run, carried from frame to frame.
    credit: u32,
}

impl std::fmt::Debug for Pane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pane")
            .field("kind", &self.kind)
            .field("seed", &self.seed)
            .finish_non_exhaustive()
    }
}

impl Pane {
    pub fn new(kind: SceneKind, seed: u64, corpus: &Arc<Corpus>, speed: Speed) -> Self {
        Self {
            kind,
            seed,
            scene: kind.build(seed, corpus, speed),
            credit: 0,
        }
    }

    /// Rebuilds the scene from the next seed in the chain, so a run with
    /// `--seed` replays the same sequence of restarts.
    pub fn restart(&mut self, corpus: &Arc<Corpus>, speed: Speed) {
        self.seed = self.seed.wrapping_add(1);
        self.rebuild(corpus, speed);
    }

    pub fn rebuild(&mut self, corpus: &Arc<Corpus>, speed: Speed) {
        self.credit = 0;
        self.scene = self.kind.build(self.seed, corpus, speed);
    }

    pub fn set_speed(&mut self, speed: Speed) {
        self.scene.set_speed(speed);
    }

    pub fn tick(&mut self, rate: Rate, area: Rect) {
        advance(&mut *self.scene, self.kind, rate, &mut self.credit, area);
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        self.scene.render(area, buf);
    }
}

/// Typing keeps one tick a frame and counts its own speed in keystrokes.
/// Every other scene ticks once for each whole tick its rate has paid for.
pub fn advance(scene: &mut dyn Scene, kind: SceneKind, rate: Rate, credit: &mut u32, area: Rect) {
    if kind == SceneKind::Typing {
        scene.tick(area);
        return;
    }
    *credit += rate.quarters();
    while *credit >= QUARTERS_PER_TICK {
        *credit -= QUARTERS_PER_TICK;
        scene.tick(area);
    }
}
