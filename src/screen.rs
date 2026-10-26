use std::io::{self, Write};

use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use crossterm::{execute, queue};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::app::App;

/// Draws the app as one synchronized update. A terminal that knows the mode
/// shows nothing of the frame until all of it has arrived, so a large frame
/// never shows half written. A terminal that does not know it ignores it.
///
/// The end goes out even when the draw fails, because a terminal holds its
/// screen still while an update is open: Ghostty waits a second for it.
pub fn draw<W: Write>(terminal: &mut Terminal<CrosstermBackend<W>>, app: &App) -> io::Result<()> {
    queue!(terminal.backend_mut(), BeginSynchronizedUpdate)?;
    let drawn = terminal
        .draw(|frame| app.render(frame.area(), frame.buffer_mut()))
        .map(drop);
    execute!(terminal.backend_mut(), EndSynchronizedUpdate)?;
    drawn
}
