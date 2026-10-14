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

fn corpus() -> Arc<Corpus> {
    Arc::new(Corpus::embedded())
}

fn ticked(seed: u64, ticks: usize) -> App {
    let mut app = App::new(SceneKind::Noise, seed, corpus());
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
/// Then: its frame differs from the first frame of the seed it started on.
///
/// Why it matters: a restart that keeps the seed replays the same scene, which
/// reads as the key doing nothing. A fresh scene on the old seed draws that
/// first frame again, so that is the frame to compare against.
#[test]
fn r_restarts_the_scene_with_a_new_seed() {
    // Given:
    let mut restarted = ticked(7, 1);
    let first_frame_of_seed_7 = frame(&ticked(7, 1), WIDTH, HEIGHT);

    // When:
    send(&mut restarted, vec![key('r')]);
    restarted.tick(Rect::new(0, 0, WIDTH, HEIGHT));

    // Then:
    assert_ne!(frame(&restarted, WIDTH, HEIGHT), first_frame_of_seed_7);
}

/// Given: the noise scene, ticked once.
/// When: Tab is pressed and the app ticks ten times.
/// Then: the frame is the matrix scene's, started on the next seed, and no
/// longer the noise scene's.
///
/// Why it matters: Tab is the only way to walk the scenes without the
/// picker, and a cycle of one scene cannot show that it moves.
#[test]
fn tab_moves_to_the_next_scene() {
    // Given:
    let mut app = ticked(7, 1);
    let noise = frame(&app, WIDTH, HEIGHT);

    // When:
    send(&mut app, vec![press(KeyCode::Tab, KeyModifiers::NONE)]);
    let mut matrix = App::new(SceneKind::Matrix, 8, corpus());
    for _ in 0..10 {
        app.tick(Rect::new(0, 0, WIDTH, HEIGHT));
        matrix.tick(Rect::new(0, 0, WIDTH, HEIGHT));
    }

    // Then:
    let after = frame(&app, WIDTH, HEIGHT);
    assert_ne!(after, noise);
    assert_eq!(after, frame(&matrix, WIDTH, HEIGHT));
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
    send(
        &mut app,
        vec![press(KeyCode::Char('c'), KeyModifiers::CONTROL)],
    );
    assert!(app.should_quit);
}

fn shows_picker(buffer: &Buffer) -> bool {
    (0..buffer.area.height).any(|y| {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol().to_string())
            .collect::<String>()
            .contains(theme::PICKER_TITLE)
    })
}

/// Given: a running scene.
/// When: Esc is pressed.
/// Then: the picker shows, and the app has not quit.
#[test]
fn esc_returns_to_the_picker() {
    // Given:
    let mut app = ticked(7, 1);
    assert!(!shows_picker(&frame(&app, WIDTH, HEIGHT)));

    // When:
    send(&mut app, vec![press(KeyCode::Esc, KeyModifiers::NONE)]);

    // Then:
    assert!(!app.should_quit);
    assert!(shows_picker(&frame(&app, WIDTH, HEIGHT)));
}

/// Given: the app opened on the picker.
/// When: Enter is pressed and the app ticks.
/// Then: the picker is gone and a scene is drawing.
#[test]
fn enter_starts_the_selected_scene() {
    // Given:
    let mut app = App::picker(7, corpus());
    assert!(shows_picker(&frame(&app, WIDTH, HEIGHT)));

    // When:
    send(&mut app, vec![press(KeyCode::Enter, KeyModifiers::NONE)]);
    app.tick(Rect::new(0, 0, WIDTH, HEIGHT));

    // Then:
    let buffer = frame(&app, WIDTH, HEIGHT);
    assert!(!shows_picker(&buffer));
    assert!(!is_blank(&buffer));
}

/// Given: the picker on its last row.
/// When: Down and `j` are pressed, then Up and `k` past the first row.
/// Then: the marker stays on a row, and the app has not quit.
#[test]
fn the_picker_selection_stays_inside_the_list() {
    // Given:
    let mut app = App::picker(7, corpus());

    // When:
    send(
        &mut app,
        vec![
            press(KeyCode::Down, KeyModifiers::NONE),
            key('j'),
            press(KeyCode::Up, KeyModifiers::NONE),
            key('k'),
            key('k'),
        ],
    );

    // Then:
    let buffer = frame(&app, WIDTH, HEIGHT);
    let marked = (0..buffer.area.height)
        .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
        .filter(|&(x, y)| buffer[(x, y)].symbol() == "[")
        .count();
    assert_eq!(marked, 1);
    assert!(!app.should_quit);
}

#[test]
fn the_picker_quits_on_q() {
    let mut app = App::picker(7, corpus());
    send(&mut app, vec![key('q')]);
    assert!(app.should_quit);
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

/// Given: a scene ticked at one size, and the picker.
/// When: each renders at one cell, and at no cells.
/// Then: each draws without a panic and stays inside the area.
#[test]
fn a_scene_and_the_picker_render_inside_any_area() {
    // Given:
    let scene = ticked(7, 1);
    let picker = App::picker(7, corpus());

    // When, then:
    for app in [&scene, &picker] {
        assert_eq!(frame(app, 1, 1).area, Rect::new(0, 0, 1, 1));
        assert_eq!(frame(app, 0, 0).area, Rect::new(0, 0, 0, 0));
    }
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
