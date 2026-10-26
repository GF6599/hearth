#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::cell::RefCell;
use std::io::{self, Write};
use std::rc::Rc;
use std::sync::Arc;

use hearth::app::App;
use hearth::corpus::Corpus;
use hearth::scene::SceneKind;
use hearth::screen;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
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
