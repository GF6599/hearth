#![allow(
    dead_code,
    reason = "each test binary builds this module and uses only some of it"
)]

use std::collections::VecDeque;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use hearth::app::App;
use hearth::events::{EventSource, handle_events};

pub struct QueuedEvents {
    queued: VecDeque<Event>,
}

impl QueuedEvents {
    pub fn new(events: Vec<Event>) -> Self {
        Self {
            queued: events.into(),
        }
    }

    pub fn left(&self) -> usize {
        self.queued.len()
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

pub fn press(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

pub fn key(code: KeyCode) -> Event {
    press(code, KeyModifiers::NONE)
}

pub fn release(code: KeyCode) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Release,
        state: KeyEventState::NONE,
    })
}

pub fn letter(c: char) -> Event {
    key(KeyCode::Char(c))
}

pub fn typed(text: &str) -> Vec<Event> {
    text.chars().map(letter).collect()
}

pub fn tab() -> Event {
    key(KeyCode::Tab)
}

pub fn back_tab() -> Event {
    press(KeyCode::BackTab, KeyModifiers::SHIFT)
}

/// Sends `events` to the app in one pass of its event loop.
pub fn send(app: &mut App, events: Vec<Event>) {
    let mut source = QueuedEvents::new(events);
    handle_events(app, &mut source, Duration::ZERO).unwrap();
}

pub fn send_keys(app: &mut App, keys: &[KeyCode]) {
    send(app, keys.iter().copied().map(key).collect());
}
