use std::io::{self, Stdout, Write, stdout};

use crossterm::cursor::MoveTo;
use crossterm::style::{
    Attribute, Color as CrosstermColor, Print, SetAttribute, SetAttributes, SetBackgroundColor,
    SetForegroundColor,
};
use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use crossterm::{execute, queue};
use ratatui::Terminal;
use ratatui::backend::{Backend, ClearType, CrosstermBackend, IntoCrossterm, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};
use ratatui::style::{Color, Modifier, Style};

use crate::app::App;

/// Opens the terminal as `ratatui::try_init` does, with its raw mode, its
/// alternate screen and its panic hook, but drawing through `Output`.
/// `ratatui::restore` closes it.
pub fn open() -> io::Result<Terminal<Output<Stdout>>> {
    drop(ratatui::try_init()?);
    Terminal::new(Output::new(stdout()))
}

/// Draws the app as one synchronized update. A terminal that knows the mode
/// shows nothing of the frame until all of it has arrived, so a large frame
/// never shows half written. A terminal that does not know it ignores it.
///
/// The end goes out even when the draw fails, because a terminal holds its
/// screen still while an update is open: Ghostty waits a second for it.
pub fn draw<B>(terminal: &mut Terminal<B>, app: &App) -> io::Result<()>
where
    B: Backend<Error = io::Error> + Write,
{
    queue!(terminal.backend_mut(), BeginSynchronizedUpdate)?;
    let drawn = terminal
        .draw(|frame| app.render(frame.area(), frame.buffer_mut()))
        .map(drop);
    execute!(terminal.backend_mut(), EndSynchronizedUpdate)?;
    drawn
}

/// ratatui's crossterm output, except that a cell writes only the colour
/// that changed from the cell before it. ratatui writes the foreground and
/// the background together whenever either changes, and in a scene of
/// blended pixels those escapes are most of what the terminal reads.
#[derive(Debug)]
pub struct Output<W: Write>(CrosstermBackend<W>);

impl<W: Write> Output<W> {
    pub fn new(writer: W) -> Self {
        Self(CrosstermBackend::new(writer))
    }
}

impl<W: Write> Write for Output<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        Write::flush(&mut self.0)
    }
}

impl<W: Write> Backend for Output<W> {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let (mut fg, mut bg) = (Color::Reset, Color::Reset);
        let mut modifier = Modifier::empty();
        let mut last: Option<Position> = None;
        for (x, y, cell) in content {
            if !matches!(last, Some(place) if x == place.x + 1 && y == place.y) {
                queue!(self.0, MoveTo(x, y))?;
            }
            last = Some(Position { x, y });
            if cell.modifier != modifier {
                // A reset of the attributes resets both colours with them,
                // so both are written again after it.
                let attributes = Style::new().add_modifier(cell.modifier).into_crossterm();
                queue!(
                    self.0,
                    SetAttribute(Attribute::Reset),
                    SetAttributes(attributes.attributes)
                )?;
                modifier = cell.modifier;
                (fg, bg) = (Color::Reset, Color::Reset);
            }
            if cell.fg != fg {
                queue!(self.0, SetForegroundColor(cell.fg.into_crossterm()))?;
                fg = cell.fg;
            }
            if cell.bg != bg {
                queue!(self.0, SetBackgroundColor(cell.bg.into_crossterm()))?;
                bg = cell.bg;
            }
            queue!(self.0, Print(cell.symbol()))?;
        }
        queue!(
            self.0,
            SetForegroundColor(CrosstermColor::Reset),
            SetBackgroundColor(CrosstermColor::Reset),
            SetAttribute(Attribute::Reset),
        )
    }

    fn append_lines(&mut self, n: u16) -> io::Result<()> {
        self.0.append_lines(n)
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.0.hide_cursor()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.0.show_cursor()
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        self.0.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        self.0.set_cursor_position(position)
    }

    fn clear(&mut self) -> io::Result<()> {
        self.0.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        self.0.clear_region(clear_type)
    }

    fn size(&self) -> io::Result<Size> {
        self.0.size()
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        self.0.window_size()
    }

    fn flush(&mut self) -> io::Result<()> {
        Backend::flush(&mut self.0)
    }
}
