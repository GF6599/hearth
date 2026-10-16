#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use hearth::app::App;
use hearth::corpus::Corpus;
use hearth::events::{EventSource, handle_events};
use hearth::scene::SceneKind;
use hearth::theme::{self, Palette};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);

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

fn send(app: &mut App, keys: &[KeyCode]) {
    let mut source = QueuedEvents {
        queued: keys
            .iter()
            .map(|code| {
                Event::Key(KeyEvent {
                    code: *code,
                    modifiers: KeyModifiers::NONE,
                    kind: KeyEventKind::Press,
                    state: KeyEventState::NONE,
                })
            })
            .collect(),
    };
    handle_events(app, &mut source, Duration::ZERO).unwrap();
}

fn frame(app: &mut App, ticks: usize) -> Buffer {
    for _ in 0..ticks {
        app.tick(AREA);
    }
    let mut buffer = Buffer::empty(AREA);
    app.render(AREA, &mut buffer);
    buffer
}

fn colours(palette: &Palette) -> [Color; 12] {
    [
        palette.base,
        palette.surface,
        palette.overlay,
        palette.muted,
        palette.subtle,
        palette.text,
        palette.love,
        palette.gold,
        palette.rose,
        palette.pine,
        palette.foam,
        palette.iris,
    ]
}

/// The colours of `before` that `after` does not have.
fn left_behind(before: &Palette, after: &Palette) -> Vec<Color> {
    let kept = colours(after);
    colours(before)
        .into_iter()
        .filter(|colour| !kept.contains(colour))
        .collect()
}

fn assert_none_of(buffer: &Buffer, foreign: &[Color], what: &str) {
    for position in buffer.area.positions() {
        let cell = &buffer[position];
        assert!(
            !foreign.contains(&cell.fg) && !foreign.contains(&cell.bg),
            "{what}: {:?} on {:?} at {position:?}",
            cell.fg,
            cell.bg
        );
    }
}

fn row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

/// Given: fire beside life in two panes, on the default theme.
/// When: `t` is pressed once for each theme, with a frame after each.
/// Then: each frame shows no colour of the theme before it that the new
/// theme does not have, in either pane or the rule between them.
///
/// Why it matters: a pane that keeps the old palette shows two themes on
/// one screen, and on a light theme it reads as a dark hole.
#[test]
fn t_moves_every_pane_to_the_next_theme() {
    // Given:
    let mut app = App::new(SceneKind::Fire, 7, Arc::new(Corpus::embedded()));
    send(&mut app, &[KeyCode::Char('v'), KeyCode::Enter]);
    frame(&mut app, 5);

    for pair in theme::THEMES
        .iter()
        .cycle()
        .take(theme::THEMES.len() + 1)
        .collect::<Vec<_>>()
        .windows(2)
    {
        let ((before, from), (after, to)) = (pair[0], pair[1]);

        // When:
        send(&mut app, &[KeyCode::Char('t')]);

        // Then:
        let shown = frame(&mut app, 10);
        assert_none_of(
            &shown,
            &left_behind(from, to),
            &format!("{before} to {after}"),
        );
    }
}

/// Given: two fire scenes from one seed.
/// When: `t` is pressed in one once for each theme, so it wraps to the
/// default, and both run twenty ticks, past the notice.
/// Then: the two frames are the same.
///
/// Why it matters: the key cycles, so the last theme leads back to the
/// first and not to an end.
#[test]
fn t_wraps_from_the_last_theme_to_the_first() {
    // Given:
    let corpus = Arc::new(Corpus::embedded());
    let mut cycled = App::new(SceneKind::Fire, 7, Arc::clone(&corpus));
    let mut still = App::new(SceneKind::Fire, 7, corpus);

    // When:
    let presses = vec![KeyCode::Char('t'); theme::THEMES.len()];
    send(&mut cycled, &presses);

    // Then:
    assert_eq!(frame(&mut cycled, 20), frame(&mut still, 20));
}

/// Given: the picker on the default theme, marking fire, which draws in
/// base and overlay, two colours the next theme changes.
/// When: `t` is pressed.
/// Then: the picker stays open, and no cell, the preview included, shows
/// a default colour that the next theme does not have.
///
/// Why it matters: the picker is where a user compares themes, so the
/// list and the scene beside it must change together.
#[test]
fn t_in_the_picker_moves_the_list_and_the_preview_to_the_next_theme() {
    // Given:
    let mut app = App::picker(7, Arc::new(Corpus::embedded()));
    send(&mut app, &[KeyCode::Char('j'), KeyCode::Char('j')]);
    frame(&mut app, 5);

    // When:
    send(&mut app, &[KeyCode::Char('t')]);

    // Then:
    assert!(app.is_picking());
    let (_, next) = theme::THEMES[1];
    assert_none_of(
        &frame(&mut app, 10),
        &left_behind(&theme::ROSE_PINE, next),
        "picker",
    );
}

/// Given: fire on the default theme.
/// When: `t` is pressed.
/// Then: the top row of the frame names the next theme.
///
/// Why it matters: two dark themes can look alike at a glance, and the
/// name says which one the key chose.
#[test]
fn t_names_the_new_theme_in_the_notice() {
    // Given:
    let mut app = App::new(SceneKind::Fire, 7, Arc::new(Corpus::embedded()));

    // When:
    send(&mut app, &[KeyCode::Char('t')]);

    // Then:
    let (name, _) = theme::THEMES[1];
    let top = row(&frame(&mut app, 1), 0);
    assert!(top.contains(name), "{top:?}");
}
