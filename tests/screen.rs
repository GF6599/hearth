#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::cell::RefCell;
use std::io::{self, Write};
use std::num::NonZeroU16;
use std::rc::Rc;
use std::sync::Arc;

use hearth::app::App;
use hearth::corpus::Corpus;
use hearth::scene::{ALL, SceneKind};
use hearth::screen::{self, Output};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::{Terminal, TerminalOptions, Viewport};

const AREA: Rect = Rect::new(0, 0, 40, 12);
const BEGIN: &[u8] = b"\x1b[?2026h";
const END: &[u8] = b"\x1b[?2026l";

/// A terminal that keeps every byte it is sent, shared with the test.
#[derive(Clone, Default)]
struct Kept(Rc<RefCell<Vec<u8>>>);

impl Write for Kept {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn count(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

/// Given: the fire and a terminal that keeps the bytes it is sent.
/// When: two frames are drawn, the fire ticking between them.
/// Then: each frame's bytes open with the start of a synchronized update,
/// close with its end, and hold no other start or end.
///
/// Why it matters: Ghostty draws nothing while an update is open, so a
/// frame sent between the pair shows whole and never half written.
#[test]
fn each_frame_is_one_synchronized_update() {
    // Given:
    let kept = Kept::default();
    let options = TerminalOptions {
        viewport: Viewport::Fixed(AREA),
    };
    let mut terminal =
        Terminal::with_options(CrosstermBackend::new(kept.clone()), options).unwrap();
    let mut app = App::new(SceneKind::Fire, 7, Arc::new(Corpus::embedded()));
    app.frame(AREA);

    for frame in 1..=2 {
        // When:
        kept.0.borrow_mut().clear();
        screen::draw(&mut terminal, &app).unwrap();
        app.tick(AREA);

        // Then:
        let bytes = kept.0.borrow();
        assert!(
            bytes.len() > BEGIN.len() + END.len(),
            "frame {frame} drew nothing"
        );
        assert!(
            bytes.starts_with(BEGIN),
            "frame {frame} opens with no start"
        );
        assert!(bytes.ends_with(END), "frame {frame} closes with no end");
        assert_eq!(count(&bytes, BEGIN), 1, "frame {frame}");
        assert_eq!(count(&bytes, END), 1, "frame {frame}");
    }
}

fn terminal<B: Backend>(backend: B) -> Terminal<B> {
    let options = TerminalOptions {
        viewport: Viewport::Fixed(AREA),
    };
    Terminal::with_options(backend, options).unwrap()
}

fn parse(bytes: &[u8]) -> vt100::Parser {
    let size = |cells: u16| NonZeroU16::new(cells).unwrap();
    let mut parser = vt100::Parser::new(size(AREA.height), size(AREA.width), 0);
    parser.process(bytes);
    parser
}

/// Each cell a terminal shows after it reads `bytes`: its text and its two
/// colours.
fn shown(bytes: &[u8]) -> Vec<(String, vt100::Color, vt100::Color)> {
    let parser = parse(bytes);
    let screen = parser.screen();
    (0..AREA.height)
        .flat_map(|row| (0..AREA.width).map(move |column| (row, column)))
        .map(|(row, column)| {
            let cell = screen.cell(row, column).unwrap();
            (cell.contents().to_string(), cell.fgcolor(), cell.bgcolor())
        })
        .collect()
}

/// Given: each scene and the picker, drawn through ratatui's own output
/// and through hearth's, as the terminal loop draws them.
/// When: each runs 40 frames.
/// Then: after every frame a terminal that read either stream shows the
/// same text in the same colours in every cell.
///
/// Why it matters: an escape that leaves out a colour that did change
/// keeps the last one on the screen, and only the terminal's state shows
/// it, not the escapes one at a time.
#[test]
fn the_output_shows_what_ratatui_shows() {
    let corpus = Arc::new(Corpus::embedded());
    let apps = ALL
        .iter()
        .map(|kind| (format!("{kind:?}"), App::new(*kind, 7, Arc::clone(&corpus))))
        .chain([("picker".to_string(), App::picker(7, Arc::clone(&corpus)))]);
    for (name, mut app) in apps {
        // Given:
        let (theirs, ours) = (Kept::default(), Kept::default());
        let mut reference = terminal(CrosstermBackend::new(theirs.clone()));
        let mut output = terminal(Output::new(ours.clone()));

        for frame in 1..=40 {
            // When:
            app.frame(AREA);
            if app.take_redraw() {
                reference
                    .draw(|f| app.render(f.area(), f.buffer_mut()))
                    .unwrap();
                output
                    .draw(|f| app.render(f.area(), f.buffer_mut()))
                    .unwrap();
            }

            // Then:
            assert!(
                shown(&ours.0.borrow()) == shown(&theirs.0.borrow()),
                "{name} frame {frame}"
            );
        }
    }
}

/// Given: a row of cells that turn bold, reversed and plain again, each in
/// its own colours.
/// When: it is drawn through ratatui's output and through hearth's.
/// Then: a terminal that read either stream shows the same text, colours
/// and attributes.
///
/// Why it matters: a terminal resets its colours with its attributes, so
/// an output that resets one and keeps the other draws the next cell in
/// the terminal's own colours.
#[test]
fn the_output_keeps_the_colours_through_a_change_of_attributes() {
    // Given:
    let styles = [
        Style::new()
            .fg(Color::Rgb(200, 10, 10))
            .bg(Color::Rgb(0, 0, 40)),
        Style::new()
            .fg(Color::Rgb(200, 10, 10))
            .bg(Color::Rgb(0, 0, 40))
            .bold(),
        Style::new()
            .fg(Color::Rgb(10, 200, 10))
            .bg(Color::Rgb(0, 0, 40))
            .bold()
            .reversed(),
        Style::new()
            .fg(Color::Rgb(10, 200, 10))
            .bg(Color::Rgb(0, 0, 40)),
        Style::new().fg(Color::Reset).bg(Color::Rgb(0, 0, 40)),
    ];
    let row = |buffer: &mut Buffer| {
        for (x, style) in (0..).zip(styles) {
            buffer[(x, 0)].set_char('x').set_style(style);
        }
    };
    let (theirs, ours) = (Kept::default(), Kept::default());

    // When:
    terminal(CrosstermBackend::new(theirs.clone()))
        .draw(|f| row(f.buffer_mut()))
        .unwrap();
    terminal(Output::new(ours.clone()))
        .draw(|f| row(f.buffer_mut()))
        .unwrap();

    // Then:
    let attributes = |bytes: &[u8]| {
        let parser = parse(bytes);
        (0..styles.len() as u16)
            .map(|column| {
                let cell = parser.screen().cell(0, column).unwrap().clone();
                (cell.fgcolor(), cell.bgcolor(), cell.bold(), cell.inverse())
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(attributes(&ours.0.borrow()), attributes(&theirs.0.borrow()));
}

/// Given: the fire, drawn through ratatui's output and through hearth's.
/// When: each draws the same 40 frames.
/// Then: hearth's output writes fewer bytes.
///
/// Why it matters: the colour escapes are most of what the terminal reads
/// in the fire, and ratatui writes both colours whenever either changes.
#[test]
fn the_output_writes_fewer_bytes_than_ratatui() {
    // Given:
    let corpus = Arc::new(Corpus::embedded());
    let (theirs, ours) = (Kept::default(), Kept::default());
    let mut reference = terminal(CrosstermBackend::new(theirs.clone()));
    let mut output = terminal(Output::new(ours.clone()));
    let mut app = App::new(SceneKind::Fire, 7, corpus);

    // When:
    for _ in 1..=40 {
        app.frame(AREA);
        reference
            .draw(|f| app.render(f.area(), f.buffer_mut()))
            .unwrap();
        output
            .draw(|f| app.render(f.area(), f.buffer_mut()))
            .unwrap();
    }

    // Then:
    let (ours, theirs) = (ours.0.borrow().len(), theirs.0.borrow().len());
    assert!(ours < theirs, "{ours} bytes against ratatui's {theirs}");
}
