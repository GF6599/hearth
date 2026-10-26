use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::App;
use crate::pane::Direction;

/// The terminal is the only implementation that ships. The seam exists
/// because `event::poll` needs a live terminal, which a test has not got.
pub trait EventSource {
    fn poll(&mut self, timeout: Duration) -> std::io::Result<bool>;
    fn read(&mut self) -> std::io::Result<Event>;
}

#[derive(Debug)]
pub struct TerminalEvents;

impl EventSource for TerminalEvents {
    fn poll(&mut self, timeout: Duration) -> std::io::Result<bool> {
        event::poll(timeout)
    }

    fn read(&mut self) -> std::io::Result<Event> {
        event::read()
    }
}

/// Without a cap the drain below only exits when the producer runs out,
/// so a held key stalls the redraw.
const MAX_DRAINED_EVENTS: usize = 256;

/// Waits for at most `timeout` for an event, then drains the ones already
/// queued behind it, so a burst costs one redraw instead of one per key.
pub fn handle_events(
    app: &mut App,
    source: &mut impl EventSource,
    timeout: Duration,
) -> std::io::Result<()> {
    if !source.poll(timeout)? {
        return Ok(());
    }
    dispatch(app, source.read()?);

    let mut drained = 0;
    while drained < MAX_DRAINED_EVENTS && source.poll(Duration::ZERO)? {
        dispatch(app, source.read()?);
        drained += 1;
    }
    Ok(())
}

fn dispatch(app: &mut App, event: Event) {
    app.request_redraw();
    if let Event::Key(key) = event {
        dispatch_key(app, key);
    }
}

fn dispatch_key(app: &mut App, key: KeyEvent) {
    if key.kind != KeyEventKind::Press {
        return;
    }
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.quit();
        return;
    }
    if app.is_searching() {
        match key.code {
            KeyCode::Esc => app.end_search(),
            KeyCode::Enter => app.start_selected(),
            KeyCode::Backspace => app.erase_in_search(),
            KeyCode::Down => app.select_next(),
            KeyCode::Up => app.select_previous(),
            KeyCode::Tab => app.select_next_wrapped(),
            KeyCode::BackTab => app.select_previous_wrapped(),
            KeyCode::Char(letter) => app.type_in_search(letter),
            _ => {}
        }
    } else if app.is_picking() {
        match key.code {
            KeyCode::Char('q') => app.quit(),
            KeyCode::Char('j') | KeyCode::Down => app.select_next(),
            KeyCode::Char('k') | KeyCode::Up => app.select_previous(),
            KeyCode::Tab => app.select_next_wrapped(),
            KeyCode::BackTab => app.select_previous_wrapped(),
            KeyCode::Enter => app.start_selected(),
            KeyCode::Esc => app.cancel_picker(),
            KeyCode::Char('t') => app.next_theme(),
            KeyCode::Char('/') => app.start_search(),
            _ => {}
        }
    } else {
        match key.code {
            KeyCode::Char('q') => app.quit(),
            KeyCode::Char('r') => app.restart(),
            KeyCode::Char('+') => app.faster(),
            KeyCode::Char('-') => app.slower(),
            KeyCode::Tab => app.next_scene(),
            KeyCode::BackTab => app.previous_scene(),
            KeyCode::Esc => app.open_picker(),
            KeyCode::Char('v') => app.split_beside(),
            KeyCode::Char('h') | KeyCode::Left => app.move_focus(Direction::Left),
            KeyCode::Char('j') | KeyCode::Down => app.move_focus(Direction::Down),
            KeyCode::Char('k') | KeyCode::Up => app.move_focus(Direction::Up),
            KeyCode::Char('l') | KeyCode::Right => app.move_focus(Direction::Right),
            KeyCode::Char('s') => app.split_below(),
            KeyCode::Char('x') => app.close_pane(),
            KeyCode::Char('t') => app.next_theme(),
            _ => {}
        }
    }
}
