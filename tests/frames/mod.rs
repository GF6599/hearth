#![allow(
    dead_code,
    reason = "each test binary builds this module and uses only some of it"
)]

use std::sync::Arc;

use hearth::app::App;
use hearth::corpus::Corpus;
use hearth::mark;
use hearth::scene::{ALL, SceneKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

pub fn app_frame(app: &App, area: Rect) -> Buffer {
    let mut buffer = Buffer::empty(area);
    app.render(area, &mut buffer);
    buffer
}

/// The inside of the top-most frame whose top rule is a plain run of
/// `─`, which is the preview pane when there is one. The list's rule has
/// its title cut in, the mark's braces have no rule, and a scene in the
/// pane draws its own frames lower than the pane's top rule.
pub fn pane_inner(buffer: &Buffer) -> Option<Rect> {
    let area = buffer.area;
    let symbol = |x: u16, y: u16| buffer[(x, y)].symbol();
    let boxes = (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| (x, y)))
        .filter(|&(x, y)| symbol(x, y) == "┌")
        .filter_map(|(x, y)| {
            let right = (x + 1..area.width).find(|&r| symbol(r, y) != "─")?;
            let bottom = (y + 1..area.height).find(|&b| symbol(x, b) != "│")?;
            (right > x + 1 && symbol(right, y) == "┐" && symbol(x, bottom) == "└")
                .then(|| Rect::new(x + 1, y + 1, right - x - 1, bottom - y - 1))
        });
    boxes.min_by_key(|inner| (inner.y, std::cmp::Reverse(inner.x)))
}

pub fn alone(kind: SceneKind, seed: u64, ticks: usize, pane: Rect, area: Rect) -> Buffer {
    let mut app = App::new(kind, seed, Arc::new(Corpus::embedded()));
    for _ in 0..ticks {
        app.tick(pane);
    }
    let mut buffer = Buffer::empty(area);
    app.render(pane, &mut buffer);
    buffer
}

pub fn assert_same_inside(shown: &Buffer, expected: &Buffer, pane: Rect, what: &str) {
    for position in pane.positions() {
        assert_eq!(
            shown[position], expected[position],
            "{what} at {position:?}"
        );
    }
}

/// The name in the marked row of the list, read left of the preview pane,
/// because a scene in the pane can draw a `[` of its own.
pub fn marked(app: &App, area: Rect) -> Option<String> {
    let buffer = app_frame(app, area);
    let end = pane_inner(&buffer).map_or(area.width, |inner| inner.x - 1);
    let list = |y: u16| {
        (0..end)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
    };
    let row = (0..area.height).map(list).find(|row| row.contains('['))?;
    let open = row.find('[')?;
    let close = row[open..].find(']')? + open;
    Some(row[open + 1..close].to_string())
}

/// Moves the mark down to `kind` in one pass of the list at most, so a mark
/// that never arrives fails the test and does not hang it.
pub fn mark(app: &mut App, kind: SceneKind, area: Rect) {
    for _ in 0..ALL.len() {
        if marked(app, area).as_deref() == Some(kind.name()) {
            return;
        }
        app.select_next();
    }
    panic!("the mark never reached {kind:?}");
}

/// Rows that show every scene with its summary: two a scene, the mark and
/// the row under it, the frame's two rules, and two to spare.
pub fn tall() -> u16 {
    u16::try_from(ALL.len() * 2 + mark::ART.len() + 1 + 2 + 2).unwrap()
}

/// The Tab presses from one scene to another, read from the order of `ALL`,
/// which wraps from the last scene to the first.
pub fn tabs(from: SceneKind, to: SceneKind) -> usize {
    let place = |kind| ALL.iter().position(|each| *each == kind).unwrap();
    (place(to) + ALL.len() - place(from)) % ALL.len()
}
