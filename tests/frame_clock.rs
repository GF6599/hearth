#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use hearth::app::App;
use hearth::config::{Background, Config};
use hearth::corpus::Corpus;
use hearth::events::{EventSource, handle_events};
use hearth::mark;
use hearth::pane::advance;
use hearth::scene::{ALL, FRAMES_PER_TICK, QUARTERS_PER_TICK, Rate, Scene, SceneKind, Settings};
use hearth::scenes::typing::Speed;
use hearth::theme;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

const AREA: Rect = Rect::new(0, 0, 40, 12);
const WIDE: Rect = Rect::new(0, 0, 120, 30);

/// The ticks a changed speed stays on screen, as it did when the app drew
/// one frame a tick.
const NOTICE_TICKS: u32 = 16;

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

fn app_frame(app: &App, area: Rect) -> Buffer {
    let mut buffer = Buffer::empty(area);
    app.render(area, &mut buffer);
    buffer
}

fn row_text(buffer: &Buffer, y: u16) -> String {
    (buffer.area.x..buffer.area.right())
        .map(|x| buffer[(x, y)].symbol().to_string())
        .collect()
}

/// Every rate `+` and `-` step through, slowest first.
fn every_rate() -> Vec<Rate> {
    Rate::all_times()
        .split(", ")
        .map(|times| Rate::from_times(times.parse().unwrap()).unwrap())
        .collect()
}

fn at_rate(kind: SceneKind, rate: Rate) -> Config {
    let mut config = Config::default();
    let slot = ALL.iter().position(|each| *each == kind).unwrap();
    config.rates[slot] = Some(rate);
    config
}

/// The ticks a scene at `rate` has run after `frames` frames: a frame is a
/// quarter of a tick, and the rate counts quarters of a tick for each tick.
fn ticks_after(frames: u32, rate: Rate) -> u32 {
    frames * rate.quarters() / (QUARTERS_PER_TICK * FRAMES_PER_TICK)
}

/// Given: the fire scene in the app at each rate, and the same scene built
/// alone from the same seed.
/// When: the app draws 48 frames, and the lone scene ticks as many times
/// as the rate has paid for after each frame.
/// Then: after each frame the app shows the lone scene's picture.
///
/// Why it matters: at 1× a scene ticks once every four frames, so it runs
/// at the same speed in wall-clock time as when the app drew one frame a
/// tick. At ½× it ticks once every eight frames, and at 4× four times a
/// frame.
#[test]
fn a_scene_ticks_once_a_tick_at_every_rate() {
    let corpus = Arc::new(Corpus::embedded());
    let palette = theme::THEMES[0].1;
    for rate in every_rate() {
        // Given:
        let config = at_rate(SceneKind::Fire, rate);
        let mut app = App::configured(Some(SceneKind::Fire), 7, Arc::clone(&corpus), &config);
        let mut alone = SceneKind::Fire.build(
            7,
            palette,
            Background::Theme,
            &corpus,
            Speed::DEFAULT,
            Settings::DEFAULT,
        );
        let mut ticked = 0;

        for frame in 1..=48 {
            // When:
            app.frame(AREA);
            while ticked < ticks_after(frame, rate) {
                alone.tick(AREA);
                ticked += 1;
            }

            // Then:
            let mut expected = Buffer::empty(AREA);
            expected.set_style(AREA, Style::default().bg(palette.base));
            alone.render(AREA, &mut expected);
            assert_eq!(app_frame(&app, AREA), expected, "{rate} frame {frame}");
        }
    }
}

/// A scene that only counts its ticks and keeps the last progress it was
/// given.
#[derive(Default)]
struct Recorded {
    ticks: u32,
    progress: Option<f64>,
}

impl Scene for Recorded {
    fn tick(&mut self, _area: Rect) {
        self.ticks += 1;
    }

    fn render(&self, _area: Rect, _buf: &mut Buffer) {}

    fn set_progress(&mut self, progress: f64) -> bool {
        self.progress = Some(progress);
        true
    }
}

/// Given: a scene at each rate.
/// When: it runs 32 frames.
/// Then: after each frame it has ticked once for each whole tick its rate
/// has paid for, and holds the share of the next tick it has run, from 0
/// up to but not including 1.
///
/// Why it matters: a scene that knows how far it is towards its next tick
/// can glide between two ticks, and a share that drifts from the ticks
/// would make the glide jump back at each one.
#[test]
fn a_scene_knows_its_progress_towards_the_next_tick() {
    for rate in every_rate() {
        // Given:
        let mut scene = Recorded::default();
        let mut credit = 0;

        for frame in 1..=32 {
            // When:
            advance(&mut scene, SceneKind::Fire, rate, &mut credit, AREA);

            // Then:
            let owed = frame * rate.quarters();
            let per_tick = QUARTERS_PER_TICK * FRAMES_PER_TICK;
            assert_eq!(
                scene.ticks,
                ticks_after(frame, rate),
                "{rate} frame {frame}"
            );
            let progress = f64::from(owed % per_tick) / f64::from(per_tick);
            assert_eq!(scene.progress, Some(progress), "{rate} frame {frame}");
        }
    }
}

/// Given: the typing scene at each rate.
/// When: it runs 32 frames.
/// Then: it ticks on the last frame of each tick, whatever the rate.
///
/// Why it matters: typing counts its speed in keystrokes per second, so the
/// rate does not change it. A tick spends a tick's share of the keystrokes
/// at once, so the terminal draws typing once a tick and not on each frame.
#[test]
fn typing_ticks_once_a_tick() {
    for rate in every_rate() {
        let mut scene = Recorded::default();
        let mut credit = 0;
        for frame in 1..=32 {
            advance(&mut scene, SceneKind::Typing, rate, &mut credit, AREA);
            assert_eq!(scene.ticks, frame / FRAMES_PER_TICK, "{rate} frame {frame}");
        }
    }
}

/// Given: the fire scene with `+` pressed once.
/// When: the app draws frames.
/// Then: the new speed shows until the frame before 16 ticks have passed,
/// and is gone on that frame.
///
/// Why it matters: the notice shows for about one second. A countdown in
/// frames that kept its old count would cut it to a quarter of a second.
#[test]
fn the_notice_lasts_as_long_as_before() {
    // Given:
    let mut app = App::new(SceneKind::Fire, 7, Arc::new(Corpus::embedded()));
    send(&mut app, &[KeyCode::Char('+')]);
    let header = |app: &App| row_text(&app_frame(app, AREA), 0);
    assert!(header(&app).contains("1.5×"), "{}", header(&app));

    // When, then:
    let frames = NOTICE_TICKS * FRAMES_PER_TICK;
    for frame in 1..frames {
        app.frame(AREA);
        assert!(header(&app).contains("1.5×"), "frame {frame}");
    }
    app.frame(AREA);
    assert!(!header(&app).contains("1.5×"), "{}", header(&app));
}

/// The colour of every cell of the title art, row by row.
fn title(buffer: &Buffer) -> Vec<Color> {
    let first = (0..buffer.area.height)
        .find(|&y| row_text(buffer, y).contains(mark::ART[0]))
        .expect("no title art");
    let rows = u16::try_from(mark::ART.len()).unwrap();
    (first..first + rows)
        .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
        .map(|position| buffer[position].fg)
        .collect()
}

/// Given: the picker at 120 by 30.
/// When: it draws the frames of two `GLOW_TICKS` periods.
/// Then: each frame of the second period shows the title in the colours
/// of the frame one period before, and in other colours than the frames a
/// quarter and a half of a period before.
///
/// Why it matters: the title glows at the pace of ticks, so it keeps the
/// pace it had when the app drew one frame a tick. A glow that counted
/// frames as ticks would pulse four times as fast.
#[test]
fn the_title_glows_at_the_pace_of_ticks() {
    // Given:
    let mut app = App::picker(7, Arc::new(Corpus::embedded()));
    let period = usize::try_from(mark::GLOW_TICKS * FRAMES_PER_TICK).unwrap();
    let mut titles = Vec::new();

    for _ in 0..2 * period {
        // When:
        app.frame(WIDE);
        titles.push(title(&app_frame(&app, WIDE)));
    }

    // Then:
    for frame in period..2 * period {
        assert_eq!(titles[frame], titles[frame - period], "frame {frame}");
        for shift in [period / 4, period / 2] {
            assert_ne!(
                titles[frame],
                titles[frame - shift],
                "frame {frame}: the title repeats after {shift} frames"
            );
        }
    }
}

/// Given: two apps on the same seed, for fire, matrix and typing.
/// When: each draws 40 frames.
/// Then: the two show the same picture after every frame.
///
/// Why it matters: a seed fixes every frame, so the frame clock steps by a
/// fixed share of a tick and never by the time a frame took.
#[test]
fn a_seed_fixes_every_frame() {
    let corpus = Arc::new(Corpus::from_files([(
        "a.rs".to_string(),
        format!("fn f() {{\n{}}}\n", "    step();\n".repeat(20)),
    )]));
    for kind in [SceneKind::Fire, SceneKind::Matrix, SceneKind::Typing] {
        // Given:
        let mut first = App::new(kind, 7, Arc::clone(&corpus));
        let mut second = App::new(kind, 7, Arc::clone(&corpus));

        for frame in 1..=40 {
            // When:
            first.frame(WIDE);
            std::thread::sleep(Duration::from_millis(1));
            second.frame(WIDE);

            // Then:
            assert_eq!(
                app_frame(&first, WIDE),
                app_frame(&second, WIDE),
                "{} frame {frame}",
                kind.name()
            );
        }
    }
}
