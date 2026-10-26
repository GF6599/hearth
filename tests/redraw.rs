#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use hearth::app::App;
use hearth::config::Config;
use hearth::corpus::Corpus;
use hearth::events::{EventSource, handle_events};
use hearth::scene::{ALL, FRAMES_PER_TICK, Rate, SceneKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

const AREA: Rect = Rect::new(0, 0, 60, 20);

/// The frames the speed notice stays on screen.
const NOTICE_FRAMES: usize = 16 * FRAMES_PER_TICK as usize;

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

fn send(app: &mut App, events: Vec<Event>) {
    let mut source = QueuedEvents {
        queued: events.into(),
    };
    handle_events(app, &mut source, Duration::ZERO).unwrap();
}

fn key(c: char) -> Event {
    Event::Key(KeyEvent {
        code: KeyCode::Char(c),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

fn picture(app: &App, area: Rect) -> Buffer {
    let mut buffer = Buffer::empty(area);
    app.render(area, &mut buffer);
    buffer
}

/// The app with `kind` at 1×, so it ticks on every fourth frame.
fn at_one(kind: SceneKind) -> App {
    let mut config = Config::default();
    let slot = ALL.iter().position(|each| *each == kind).unwrap();
    config.rates[slot] = Some(Rate::from_times(1.0).unwrap());
    App::configured(Some(kind), 7, Arc::new(Corpus::embedded()), &config)
}

/// Draws as the terminal loop does: one frame, the first draw, then each
/// frame draws only when it asks to.
///
/// A frame that asks for no draw leaves the last picture on the screen, so
/// it must render that same picture. A frame that does not is a stale
/// screen, and this panics on it.
fn run(app: &mut App, area: Rect, frames: usize) -> Vec<bool> {
    app.frame(area);
    assert!(app.take_redraw(), "the first frame asks for no draw");
    let mut shown = picture(app, area);
    (1..=frames)
        .map(|frame| {
            app.frame(area);
            let asked = app.take_redraw();
            let now = picture(app, area);
            if asked {
                shown = now;
            } else {
                assert_eq!(now, shown, "frame {frame} asked for no draw and changed");
            }
            asked
        })
        .collect()
}

fn count(asked: &[bool]) -> usize {
    asked.iter().filter(|each| **each).count()
}

/// Given: each scene at 1×.
/// When: it runs 240 frames, drawing only the frames that ask.
/// Then: no frame that asks for no draw shows a picture other than the
/// last one drawn.
///
/// Why it matters: a frame that is not drawn leaves the last picture on
/// the screen. A scene that moved without asking would freeze or stutter,
/// and no error would show it.
#[test]
fn a_frame_that_asks_for_no_draw_shows_the_last_picture() {
    for &kind in ALL {
        // Given:
        let mut app = at_one(kind);

        // When, then:
        run(&mut app, AREA, 240);
    }
}

/// Given: each scene that steps and does not glide, at 1×.
/// When: it runs 64 frames.
/// Then: it asks for a draw on the 16 frames it ticks and on no other.
///
/// Why it matters: these scenes draw the same picture on the three frames
/// between two ticks, and the render, the diff and the compare of those
/// frames were most of what they cost.
#[test]
fn a_scene_that_does_not_glide_asks_for_one_draw_a_tick() {
    let stepping = [
        SceneKind::Fire,
        SceneKind::Life,
        SceneKind::Noise,
        SceneKind::Matrix,
        SceneKind::Pipes,
        SceneKind::Bonsai,
        SceneKind::Aquarium,
        SceneKind::Hourglass,
    ];
    for kind in stepping {
        // Given:
        let mut app = at_one(kind);

        // When:
        let asked = run(&mut app, AREA, 64);

        // Then:
        let ticks: Vec<bool> = (1..=64)
            .map(|frame| (frame + 1) % FRAMES_PER_TICK == 0)
            .collect();
        assert_eq!(asked, ticks, "{kind:?}");
    }
}

/// Given: the city and the embers at 1×.
/// When: each runs 480 frames.
/// Then: the city asks for a draw on fewer than half of them, and the
/// embers on fewer than three in four.
///
/// Why it matters: in the city only the train glides, and only a whole
/// pixel at a time. In the embers only the sparks and the falling logs
/// glide. Between ticks, most frames move none of them, and a scene that
/// asked on each frame its progress changed would draw them all.
#[test]
fn a_glide_that_moves_nothing_asks_for_no_draw() {
    for (kind, most) in [(SceneKind::City, 240), (SceneKind::Embers, 360)] {
        // Given:
        let mut app = at_one(kind);

        // When:
        let asked = run(&mut app, AREA, 480);

        // Then:
        assert!(count(&asked) < most, "{kind:?} asked on {}", count(&asked));
    }
}

/// Given: the fire, on a frame between two ticks.
/// When: a key comes in, then a resize.
/// Then: each asks for a draw.
///
/// Why it matters: a key changes the theme, the scene or the speed. The
/// terminal learns its new size only in a draw, so a resize that asked for
/// none would leave the app at the old size.
#[test]
fn a_key_and_a_resize_ask_for_a_draw() {
    // Given:
    let mut app = at_one(SceneKind::Fire);
    run(&mut app, AREA, 1);

    // When, then:
    send(&mut app, vec![key('t')]);
    assert!(app.take_redraw(), "a key asked for no draw");
    send(&mut app, vec![Event::Resize(80, 24)]);
    assert!(app.take_redraw(), "a resize asked for no draw");
}

/// Given: the fire, on a frame between two ticks.
/// When: the next frame has a larger area.
/// Then: it asks for a draw.
///
/// Why it matters: the scenes size themselves from the area, and the
/// whole screen has to be drawn again at the new size.
#[test]
fn a_new_area_asks_for_a_draw() {
    // Given:
    let mut app = at_one(SceneKind::Fire);
    run(&mut app, AREA, 1);

    // When:
    app.frame(Rect::new(0, 0, 80, 24));

    // Then:
    assert!(app.take_redraw());
}

/// Given: the fire with `+` pressed on a frame that puts the end of the
/// speed notice between two ticks.
/// When: it runs past the end of the notice.
/// Then: the frame the notice ends on asks for a draw, though the fire
/// does not tick on it.
///
/// Why it matters: the notice changes nothing while it counts down, so
/// only its end needs a draw, and a missed end would leave it on screen.
#[test]
fn the_notice_asks_for_a_draw_when_it_ends() {
    // Given:
    let mut app = at_one(SceneKind::Fire);
    run(&mut app, AREA, 2);
    send(&mut app, vec![key('+')]);

    // When:
    let asked = run(&mut app, AREA, NOTICE_FRAMES + 8);

    // Then:
    assert!(asked[NOTICE_FRAMES - 1], "the notice ended with no draw");
}

/// Given: the picker.
/// When: it runs 16 frames.
/// Then: every frame asks for a draw.
///
/// Why it matters: the title's glow moves and the preview runs on every
/// frame while the picker is open.
#[test]
fn the_picker_asks_for_a_draw_on_every_frame() {
    // Given:
    let mut app = App::picker(7, Arc::new(Corpus::embedded()));

    // When:
    let asked = run(&mut app, AREA, 16);

    // Then:
    assert!(asked.iter().all(|each| *each));
}
