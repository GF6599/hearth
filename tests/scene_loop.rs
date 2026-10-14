#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::VecDeque;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use hearth::app::App;
use hearth::events::{EventSource, handle_events};
use hearth::scene::SceneKind;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

const WIDTH: u16 = 24;
const HEIGHT: u16 = 6;

struct QueuedEvents {
    queued: VecDeque<Event>,
}

impl QueuedEvents {
    fn new(events: Vec<Event>) -> Self {
        Self {
            queued: events.into(),
        }
    }
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

fn key(c: char) -> Event {
    press(KeyCode::Char(c), KeyModifiers::NONE)
}

fn send(app: &mut App, events: Vec<Event>) {
    let mut source = QueuedEvents::new(events);
    handle_events(app, &mut source, Duration::ZERO).unwrap();
}

fn frame(app: &App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| app.render(f.area(), f.buffer_mut()))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn ticked(seed: u64, ticks: usize) -> App {
    let mut app = App::new(SceneKind::Noise, seed);
    for _ in 0..ticks {
        app.tick(Rect::new(0, 0, WIDTH, HEIGHT));
    }
    app
}

fn is_blank(buffer: &Buffer) -> bool {
    buffer.content().iter().all(|cell| cell.symbol() == " ")
}

/// Given: two apps built from the same seed.
/// When: each ticks once and renders.
/// Then: the frames are equal, and neither is blank.
///
/// Why it matters: every scene test rests on a seed drawing one frame, and a
/// blank frame would make two broken scenes look equal.
#[test]
fn the_same_seed_draws_the_same_frame() {
    // Given, when:
    let first = frame(&ticked(7, 1), WIDTH, HEIGHT);
    let second = frame(&ticked(7, 1), WIDTH, HEIGHT);

    // Then:
    assert_eq!(first, second);
    assert!(!is_blank(&first));
}

/// Given: two apps built from different seeds.
/// When: each ticks once and renders.
/// Then: the frames differ.
#[test]
fn a_different_seed_draws_a_different_frame() {
    // Given, when:
    let first = frame(&ticked(7, 1), WIDTH, HEIGHT);
    let second = frame(&ticked(8, 1), WIDTH, HEIGHT);

    // Then:
    assert_ne!(first, second);
}

/// Given: an app that has ticked once.
/// When: `r` is pressed and it ticks again.
/// Then: its frame differs from an untouched app at the same tick count.
///
/// Why it matters: a restart that keeps the seed replays the same scene, which
/// reads as the key doing nothing.
#[test]
fn r_restarts_the_scene_with_a_new_seed() {
    // Given:
    let mut restarted = ticked(7, 1);
    let untouched = ticked(7, 2);

    // When:
    send(&mut restarted, vec![key('r')]);
    restarted.tick(Rect::new(0, 0, WIDTH, HEIGHT));

    // Then:
    assert_ne!(
        frame(&restarted, WIDTH, HEIGHT),
        frame(&untouched, WIDTH, HEIGHT)
    );
}

#[test]
fn q_quits() {
    let mut app = ticked(7, 1);
    send(&mut app, vec![key('q')]);
    assert!(app.should_quit);
}

#[test]
fn ctrl_c_quits() {
    let mut app = ticked(7, 1);
    send(&mut app, vec![press(KeyCode::Char('c'), KeyModifiers::CONTROL)]);
    assert!(app.should_quit);
}

/// Tab and Esc belong to the picker, which does not exist yet. Until it
/// does they must neither quit nor change the frame.
#[test]
fn tab_and_esc_leave_the_scene_running() {
    // Given:
    let mut app = ticked(7, 1);
    let before = frame(&app, WIDTH, HEIGHT);

    // When:
    send(
        &mut app,
        vec![
            press(KeyCode::Tab, KeyModifiers::NONE),
            press(KeyCode::Esc, KeyModifiers::NONE),
        ],
    );

    // Then:
    assert!(!app.should_quit);
    assert_eq!(frame(&app, WIDTH, HEIGHT), before);
}

/// A key release must not count as a second press, or a terminal that
/// reports releases would restart the scene twice per `r`.
#[test]
fn a_key_release_is_ignored() {
    let mut app = ticked(7, 1);
    send(
        &mut app,
        vec![Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        })],
    );
    assert!(!app.should_quit);
}

/// Given: a scene ticked at one size.
/// When: it renders at one cell, and at no cells.
/// Then: it draws without a panic and stays inside the area.
#[test]
fn a_scene_renders_inside_any_area() {
    // Given:
    let app = ticked(7, 1);

    // When:
    let one_cell = frame(&app, 1, 1);
    let no_cells = frame(&app, 0, 0);

    // Then:
    assert_eq!(one_cell.area, Rect::new(0, 0, 1, 1));
    assert_eq!(no_cells.area, Rect::new(0, 0, 0, 0));
}

/// Held keys queue one event per auto-repeat. The loop has to hand control
/// back to the redraw before the queue is empty, or a held key freezes
/// the scene.
#[test]
fn an_unbounded_burst_does_not_starve_the_redraw() {
    // Given:
    let mut app = ticked(7, 1);
    let flood: Vec<Event> = (0..10_000)
        .map(|_| press(KeyCode::Tab, KeyModifiers::NONE))
        .collect();
    let mut source = QueuedEvents::new(flood);

    // When:
    handle_events(&mut app, &mut source, Duration::ZERO).unwrap();

    // Then:
    assert!(!source.queued.is_empty());
}
