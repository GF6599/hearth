#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use hearth::app::App;
use hearth::corpus::Corpus;
use hearth::events::{EventSource, handle_events};
use hearth::scene::{ALL, SceneKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

const WIDE: Rect = Rect::new(0, 0, 120, 30);

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

fn press(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

fn tab() -> Event {
    press(KeyCode::Tab, KeyModifiers::NONE)
}

fn send(app: &mut App, events: Vec<Event>) {
    let mut source = QueuedEvents {
        queued: events.into(),
    };
    handle_events(app, &mut source, Duration::ZERO).unwrap();
}

fn corpus() -> Arc<Corpus> {
    Arc::new(Corpus::embedded())
}

fn app_frame(app: &App, area: Rect) -> Buffer {
    let mut buffer = Buffer::empty(area);
    app.render(area, &mut buffer);
    buffer
}

fn place(kind: SceneKind) -> usize {
    ALL.iter().position(|each| *each == kind).unwrap()
}

/// The inside of the top-most frame whose top rule is a plain run of
/// `─`, which is the preview pane: the list's rule has its title cut in.
fn pane_inner(buffer: &Buffer) -> Option<Rect> {
    let area = buffer.area;
    let symbol = |x: u16, y: u16| buffer[(x, y)].symbol();
    (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| (x, y)))
        .filter(|&(x, y)| symbol(x, y) == "┌")
        .filter_map(|(x, y)| {
            let right = (x + 1..area.width).find(|&r| symbol(r, y) != "─")?;
            let bottom = (y + 1..area.height).find(|&b| symbol(x, b) != "│")?;
            (right > x + 1 && symbol(right, y) == "┐" && symbol(x, bottom) == "└")
                .then(|| Rect::new(x + 1, y + 1, right - x - 1, bottom - y - 1))
        })
        .min_by_key(|inner| (inner.y, std::cmp::Reverse(inner.x)))
}

/// The name in the marked row of the list, read left of the preview pane,
/// because a scene in the pane can draw a `[` of its own.
fn marked(app: &App) -> Option<String> {
    let buffer = app_frame(app, WIDE);
    let end = pane_inner(&buffer).map_or(WIDE.width, |inner| inner.x - 1);
    let list = |y: u16| {
        (0..end)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
    };
    let row = (0..WIDE.height).map(list).find(|row| row.contains('['))?;
    let open = row.find('[')?;
    let close = row[open..].find(']')? + open;
    Some(row[open + 1..close].to_string())
}

/// Moves the mark down to `kind` in one pass of the list at most, so a mark
/// that never arrives fails the test and does not hang it.
fn mark(app: &mut App, kind: SceneKind) {
    for _ in 0..ALL.len() {
        if marked(app).as_deref() == Some(kind.name()) {
            return;
        }
        app.select_next();
    }
    panic!("the mark never reached {kind:?}");
}

/// What `kind` draws in `inner` after `ticks` ticks when it runs alone.
fn alone(kind: SceneKind, ticks: usize, inner: Rect) -> Buffer {
    let mut app = App::new(kind, 7, corpus());
    for _ in 0..ticks {
        app.tick(inner);
    }
    let mut buffer = Buffer::empty(WIDE);
    app.render(inner, &mut buffer);
    buffer
}

fn assert_pane_runs(app: &App, kind: SceneKind) {
    let shown = app_frame(app, WIDE);
    let inner = pane_inner(&shown).unwrap_or_else(|| panic!("no pane for {kind:?}"));
    let expected = alone(kind, 5, inner);
    for y in inner.top()..inner.bottom() {
        for x in inner.left()..inner.right() {
            assert_eq!(
                shown[(x, y)],
                expected[(x, y)],
                "{} at ({x}, {y})",
                kind.name()
            );
        }
    }
}

/// Tab in the picker marks the next scene, and the pane runs it.
/// Given: the picker with the hourglass marked and running in the pane.
/// When: Tab is pressed and the app ticks five times, then the mark moves
/// to the last scene and Tab is pressed again.
/// Then: the mark is on the scene after the hourglass and the pane runs
/// that scene from its first frame, and from the last scene Tab wraps to
/// the first.
///
/// Why it matters: the pane shows a running scene, so Tab there reads as
/// the key that moves to the next one, and a Tab that does nothing reads
/// as a frozen app.
#[test]
fn tab_in_the_picker_marks_the_next_scene() {
    // Given:
    let mut app = App::picker(7, corpus());
    mark(&mut app, SceneKind::Hourglass);

    // When:
    send(&mut app, vec![tab()]);
    for _ in 0..5 {
        app.tick(WIDE);
    }

    // Then:
    let next = ALL[place(SceneKind::Hourglass) + 1];
    assert_eq!(marked(&app).as_deref(), Some(next.name()));
    assert_pane_runs(&app, next);

    // When:
    mark(&mut app, ALL[ALL.len() - 1]);
    send(&mut app, vec![tab()]);

    // Then:
    assert_eq!(marked(&app).as_deref(), Some(ALL[0].name()));
    assert!(app.is_picking());
}

fn back_tab() -> Event {
    press(KeyCode::BackTab, KeyModifiers::SHIFT)
}

/// Shift-Tab goes back one scene, round from the first to the last.
/// Given: the second scene, ticked once, and then the first scene.
/// When: Shift-Tab is pressed and the app ticks ten times.
/// Then: each frame is that of the scene before it in the list, started on
/// the next seed, and before the first scene comes the last.
///
/// Why it matters: Tab alone makes a scene one step back a walk round the
/// whole list.
#[test]
fn shift_tab_goes_back_one_scene() {
    for (from, to) in [(ALL[1], ALL[0]), (ALL[0], ALL[ALL.len() - 1])] {
        // Given:
        let mut app = App::new(from, 7, corpus());
        app.tick(WIDE);

        // When:
        send(&mut app, vec![back_tab()]);
        let mut before = App::new(to, 8, corpus());
        for _ in 0..10 {
            app.tick(WIDE);
            before.tick(WIDE);
        }

        // Then:
        assert!(
            app_frame(&app, WIDE) == app_frame(&before, WIDE),
            "Shift-Tab from {from:?} did not show {to:?}"
        );
    }
}

/// Shift-Tab in the picker marks the scene before, and the pane runs it.
/// Given: the picker with the study marked.
/// When: Shift-Tab is pressed and the app ticks five times, then the mark
/// moves to the first scene and Shift-Tab is pressed again.
/// Then: the mark is on the scene before the study and the pane runs it,
/// and from the first scene Shift-Tab wraps to the last.
///
/// Why it matters: Shift-Tab undoes a Tab in the picker as it does outside
/// it, so the two keys read as one pair.
#[test]
fn shift_tab_in_the_picker_marks_the_scene_before() {
    // Given:
    let mut app = App::picker(7, corpus());
    mark(&mut app, SceneKind::Study);

    // When:
    send(&mut app, vec![back_tab()]);
    for _ in 0..5 {
        app.tick(WIDE);
    }

    // Then:
    let before = ALL[place(SceneKind::Study) - 1];
    assert_eq!(marked(&app).as_deref(), Some(before.name()));
    assert_pane_runs(&app, before);

    // When:
    let mut first = App::picker(7, corpus());
    send(&mut first, vec![back_tab()]);

    // Then:
    assert_eq!(marked(&first).as_deref(), Some(ALL[ALL.len() - 1].name()));
    assert!(first.is_picking());
}
