use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::App;

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
    if let Event::Key(key) = event {
        dispatch_key(app, key);
    }
}

fn dispatch_key(app: &mut App, key: KeyEvent) {
    if key.kind != KeyEventKind::Press {
        return;
    }
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => app.quit(),
        KeyCode::Char('q') => app.quit(),
        KeyCode::Char('r') => app.restart(),
        _ => {}
    }
}
