#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use hearth::app::App;
use hearth::corpus::Corpus;
use hearth::events::{EventSource, handle_events};
use hearth::scene::SceneKind;
use hearth::theme;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

const AREA: Rect = Rect::new(0, 0, 40, 12);
const LEFT: Rect = Rect::new(0, 0, 19, 12);
const RIGHT: Rect = Rect::new(20, 0, 20, 12);
const TOP: Rect = Rect::new(0, 0, 40, 5);

struct QueuedEvents {
    queued: VecDeque<Event>,
}

impl EventSource for QueuedEvents {
    fn poll(&mut self, _timeout: Duration) -> std::io::Result<bool> {
        Ok(!self.queued.is_empty())
    }

    fn read(&mut self) -> std::io::Result<Event> {
        self.queued
            .pop_front()
            .ok_or_else(|| std::io::Error::other("polled ready with an empty queue"))
    }
}

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

fn send(app: &mut App, keys: &[KeyCode]) {
    let mut source = QueuedEvents {
        queued: keys.iter().map(|code| press(*code)).collect(),
    };
    handle_events(app, &mut source, Duration::ZERO).unwrap();
}

fn fire() -> App {
    App::new(SceneKind::Fire, 7, Arc::new(Corpus::embedded()))
}

fn frame(app: &App) -> Buffer {
    let mut buffer = Buffer::empty(AREA);
    app.render(AREA, &mut buffer);
    buffer
}

/// What `kind` draws from `seed` in `pane` after `ticks` ticks when it
/// runs alone.
fn alone(kind: SceneKind, seed: u64, ticks: usize, pane: Rect) -> Buffer {
    let mut app = App::new(kind, seed, Arc::new(Corpus::embedded()));
    for _ in 0..ticks {
        app.tick(pane);
    }
    let mut buffer = Buffer::empty(AREA);
    app.render(pane, &mut buffer);
    buffer
}

fn assert_same_inside(shown: &Buffer, expected: &Buffer, pane: Rect, what: &str) {
    for position in pane.positions() {
        assert_eq!(
            shown[position], expected[position],
            "{what} at {position:?}"
        );
    }
}

fn text_inside(buffer: &Buffer, pane: Rect) -> String {
    pane.rows()
        .map(|row| {
            row.columns()
                .map(|cell| buffer[(cell.x, cell.y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_rule(buffer: &Buffer, rule: Rect, symbol: &str) {
    for position in rule.positions() {
        let cell = &buffer[position];
        assert_eq!(cell.symbol(), symbol, "rule at {position:?}");
        assert_eq!(cell.fg, theme::MUTED, "rule colour at {position:?}");
    }
}

/// Given: the fire on a forty by twelve terminal.
/// When: `v` splits it, and the app ticks three times.
/// Then: a muted rule runs down column 19, the fire runs alone in the
/// nineteen columns left of it, and the picker opens in the twenty columns
/// right of it.
///
/// Why it matters: a scene or the picker that draws across the rule tears
/// the pane beside it.
#[test]
fn a_side_by_side_split_puts_the_picker_beside_the_scene() {
    // Given:
    let mut app = fire();

    // When:
    send(&mut app, &[KeyCode::Char('v')]);
    for _ in 0..3 {
        app.tick(AREA);
    }

    // Then:
    let shown = frame(&app);
    assert_rule(&shown, Rect::new(19, 0, 1, 12), "│");
    assert_same_inside(&shown, &alone(SceneKind::Fire, 7, 3, LEFT), LEFT, "fire");
    assert!(text_inside(&shown, RIGHT).contains("scenes"));
}

/// Given: the fire on a forty by twelve terminal.
/// When: `s` splits it, and the app ticks three times.
/// Then: a muted rule runs across row 5, the fire runs alone in the five
/// rows above it, and the picker opens in the rows below it.
///
/// Why it matters: the stacked split is the second half of the i3 layout,
/// and it must keep the scene above inside its own rows.
#[test]
fn a_stacked_split_puts_the_picker_below_the_scene() {
    // Given:
    let mut app = fire();

    // When:
    send(&mut app, &[KeyCode::Char('s')]);
    for _ in 0..3 {
        app.tick(AREA);
    }

    // Then:
    let shown = frame(&app);
    assert_rule(&shown, Rect::new(0, 5, 40, 1), "─");
    assert_same_inside(&shown, &alone(SceneKind::Fire, 7, 3, TOP), TOP, "fire");
    assert!(text_inside(&shown, Rect::new(0, 6, 40, 6)).contains("scenes"));
}

/// Given: the fire split side by side, with the picker in the new pane.
/// When: Enter starts the marked scene, and the app ticks four times.
/// Then: the new pane runs the scene after the fire, life, from seed 9,
/// because a new pane takes the seed after the fire's 7 and a start takes
/// the next one. The fire still runs alone on the left.
///
/// Why it matters: Enter in a pane's picker must change that pane only,
/// and a fixed seed must replay every pane.
#[test]
fn enter_starts_the_next_scene_in_the_new_pane_only() {
    // Given:
    let mut app = fire();
    send(&mut app, &[KeyCode::Char('v')]);

    // When:
    send(&mut app, &[KeyCode::Enter]);
    for _ in 0..4 {
        app.tick(AREA);
    }

    // Then:
    let shown = frame(&app);
    assert_same_inside(&shown, &alone(SceneKind::Fire, 7, 4, LEFT), LEFT, "fire");
    assert_same_inside(&shown, &alone(SceneKind::Life, 9, 4, RIGHT), RIGHT, "life");
}

/// Given: the fire split side by side, and a scene started on the right.
/// When: `s` splits the right pane.
/// Then: the new rule meets the first one in a `├` at column 19, row 5.
///
/// Why it matters: two rules that cross without a joint read as two frames
/// drawn over each other.
#[test]
fn the_rules_join_where_they_meet() {
    // Given:
    let mut app = fire();
    send(&mut app, &[KeyCode::Char('v'), KeyCode::Enter]);

    // When:
    send(&mut app, &[KeyCode::Char('s')]);

    // Then:
    let shown = frame(&app);
    assert_eq!(shown[(19, 5)].symbol(), "├");
    assert_rule(&shown, Rect::new(20, 5, 20, 1), "─");
    assert_rule(&shown, Rect::new(19, 0, 1, 5), "│");
    assert_rule(&shown, Rect::new(19, 6, 1, 6), "│");
}
