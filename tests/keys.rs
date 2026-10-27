#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use hearth::app::App;
use hearth::corpus::Corpus;
use hearth::events::{EventSource, handle_events};
use hearth::scene::{ALL, SceneKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

const WIDE: Rect = Rect::new(0, 0, 120, 30);

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

fn press(code: KeyCode, modifiers: KeyModifiers) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

fn tab() -> Event {
    press(KeyCode::Tab, KeyModifiers::NONE)
}

fn send(app: &mut App, events: Vec<Event>) {
    let mut source = QueuedEvents {
        queued: events.into(),
    };
    handle_events(app, &mut source, Duration::ZERO).unwrap();
}

fn corpus() -> Arc<Corpus> {
    Arc::new(Corpus::embedded())
}

fn app_frame(app: &App, area: Rect) -> Buffer {
    let mut buffer = Buffer::empty(area);
    app.render(area, &mut buffer);
    buffer
}

fn place(kind: SceneKind) -> usize {
    ALL.iter().position(|each| *each == kind).unwrap()
}

/// The inside of the top-most frame whose top rule is a plain run of
/// `─`, which is the preview pane: the list's rule has its title cut in.
fn pane_inner(buffer: &Buffer) -> Option<Rect> {
    let area = buffer.area;
    let symbol = |x: u16, y: u16| buffer[(x, y)].symbol();
    (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| (x, y)))
        .filter(|&(x, y)| symbol(x, y) == "┌")
        .filter_map(|(x, y)| {
            let right = (x + 1..area.width).find(|&r| symbol(r, y) != "─")?;
            let bottom = (y + 1..area.height).find(|&b| symbol(x, b) != "│")?;
            (right > x + 1 && symbol(right, y) == "┐" && symbol(x, bottom) == "└")
                .then(|| Rect::new(x + 1, y + 1, right - x - 1, bottom - y - 1))
        })
        .min_by_key(|inner| (inner.y, std::cmp::Reverse(inner.x)))
}

/// The name in the marked row of the list, read left of the preview pane,
/// because a scene in the pane can draw a `[` of its own.
fn marked(app: &App) -> Option<String> {
    let buffer = app_frame(app, WIDE);
    let end = pane_inner(&buffer).map_or(WIDE.width, |inner| inner.x - 1);
    let list = |y: u16| {
        (0..end)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
    };
    let row = (0..WIDE.height).map(list).find(|row| row.contains('['))?;
    let open = row.find('[')?;
    let close = row[open..].find(']')? + open;
    Some(row[open + 1..close].to_string())
}

/// Moves the mark down to `kind` in one pass of the list at most, so a mark
/// that never arrives fails the test and does not hang it.
fn mark(app: &mut App, kind: SceneKind) {
    for _ in 0..ALL.len() {
        if marked(app).as_deref() == Some(kind.name()) {
            return;
        }
        app.select_next();
    }
    panic!("the mark never reached {kind:?}");
}

/// What `kind` draws in `inner` after `ticks` ticks when it runs alone.
fn alone(kind: SceneKind, ticks: usize, inner: Rect) -> Buffer {
    let mut app = App::new(kind, 7, corpus());
    for _ in 0..ticks {
        app.tick(inner);
    }
    let mut buffer = Buffer::empty(WIDE);
    app.render(inner, &mut buffer);
    buffer
}

fn assert_pane_runs(app: &App, kind: SceneKind) {
    let shown = app_frame(app, WIDE);
    let inner = pane_inner(&shown).unwrap_or_else(|| panic!("no pane for {kind:?}"));
    let expected = alone(kind, 5, inner);
    for y in inner.top()..inner.bottom() {
        for x in inner.left()..inner.right() {
            assert_eq!(
                shown[(x, y)],
                expected[(x, y)],
                "{} at ({x}, {y})",
                kind.name()
            );
        }
    }
}

/// Tab in the picker marks the next scene, and the pane runs it.
/// Given: the picker with the hourglass marked and running in the pane.
/// When: Tab is pressed and the app ticks five times, then the mark moves
/// to the last scene and Tab is pressed again.
/// Then: the mark is on the scene after the hourglass and the pane runs
/// that scene from its first frame, and from the last scene Tab wraps to
/// the first.
///
/// Why it matters: the pane shows a running scene, so Tab there reads as
/// the key that moves to the next one, and a Tab that does nothing reads
/// as a frozen app.
#[test]
fn tab_in_the_picker_marks_the_next_scene() {
    // Given:
    let mut app = App::picker(7, corpus());
    mark(&mut app, SceneKind::Hourglass);

    // When:
    send(&mut app, vec![tab()]);
    for _ in 0..5 {
        app.tick(WIDE);
    }

    // Then:
    let next = ALL[place(SceneKind::Hourglass) + 1];
    assert_eq!(marked(&app).as_deref(), Some(next.name()));
    assert_pane_runs(&app, next);

    // When:
    mark(&mut app, ALL[ALL.len() - 1]);
    send(&mut app, vec![tab()]);

    // Then:
    assert_eq!(marked(&app).as_deref(), Some(ALL[0].name()));
    assert!(app.is_picking());
}

fn back_tab() -> Event {
    press(KeyCode::BackTab, KeyModifiers::SHIFT)
}

/// Shift-Tab goes back one scene, round from the first to the last.
/// Given: the second scene, ticked once, and then the first scene.
/// When: Shift-Tab is pressed and the app ticks ten times.
/// Then: each frame is that of the scene before it in the list, started on
/// the next seed, and before the first scene comes the last.
///
/// Why it matters: Tab alone makes a scene one step back a walk round the
/// whole list.
#[test]
fn shift_tab_goes_back_one_scene() {
    for (from, to) in [(ALL[1], ALL[0]), (ALL[0], ALL[ALL.len() - 1])] {
        // Given:
        let mut app = App::new(from, 7, corpus());
        app.tick(WIDE);

        // When:
        send(&mut app, vec![back_tab()]);
        let mut before = App::new(to, 8, corpus());
        for _ in 0..10 {
            app.tick(WIDE);
            before.tick(WIDE);
        }

        // Then:
        assert!(
            app_frame(&app, WIDE) == app_frame(&before, WIDE),
            "Shift-Tab from {from:?} did not show {to:?}"
        );
    }
}

/// Shift-Tab in the picker marks the scene before, and the pane runs it.
/// Given: the picker with the study marked.
/// When: Shift-Tab is pressed and the app ticks five times, then the mark
/// moves to the first scene and Shift-Tab is pressed again.
/// Then: the mark is on the scene before the study and the pane runs it,
/// and from the first scene Shift-Tab wraps to the last.
///
/// Why it matters: Shift-Tab undoes a Tab in the picker as it does outside
/// it, so the two keys read as one pair.
#[test]
fn shift_tab_in_the_picker_marks_the_scene_before() {
    // Given:
    let mut app = App::picker(7, corpus());
    mark(&mut app, SceneKind::Study);

    // When:
    send(&mut app, vec![back_tab()]);
    for _ in 0..5 {
        app.tick(WIDE);
    }

    // Then:
    let before = ALL[place(SceneKind::Study) - 1];
    assert_eq!(marked(&app).as_deref(), Some(before.name()));
    assert_pane_runs(&app, before);

    // When:
    let mut first = App::picker(7, corpus());
    send(&mut first, vec![back_tab()]);

    // Then:
    assert_eq!(marked(&first).as_deref(), Some(ALL[ALL.len() - 1].name()));
    assert!(first.is_picking());
}

fn typed(text: &str) -> Vec<Event> {
    text.chars()
        .map(|c| press(KeyCode::Char(c), KeyModifiers::NONE))
        .collect()
}

fn key(code: KeyCode) -> Event {
    press(code, KeyModifiers::NONE)
}

/// The rows of the list frame, left of the preview pane.
fn list_rows(app: &App) -> Vec<String> {
    let buffer = app_frame(app, WIDE);
    let end = pane_inner(&buffer).map_or(WIDE.width, |inner| inner.x - 1);
    (0..WIDE.height)
        .map(|y| {
            (0..end)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn list_corners(app: &App) -> (usize, usize) {
    let row = list_rows(app)
        .into_iter()
        .find(|row| row.contains("┌─ scenes"))
        .expect("no list frame");
    let left = row.chars().position(|c| c == '┌').unwrap();
    let right = row.chars().position(|c| c == '┐').unwrap();
    (left, right)
}

/// The names the list shows, top to bottom. A row that is a whole summary
/// is never a name row, because a summary can start with a scene's name.
fn listed(app: &App) -> Vec<String> {
    list_rows(app)
        .iter()
        .filter_map(|row| {
            let inside = row.split('│').nth(1)?;
            if ALL
                .iter()
                .any(|kind| inside.trim_end() == format!(" {}", kind.summary()))
            {
                return None;
            }
            let word = inside
                .split(|c: char| c.is_whitespace() || "[]".contains(c))
                .find(|word| !word.is_empty())?;
            ALL.iter()
                .any(|kind| kind.name() == word)
                .then(|| word.to_string())
        })
        .collect()
}

/// `/` and letters narrow the list to the scenes that match them loosely.
/// Given: the picker.
/// When: `/` is pressed and `drv` typed, then the query is cleared and
/// `the` typed.
/// Then: `drv` lists only the drive, whose name holds the letters in
/// order, and marks it, and the pane runs it. `the`, which no name holds in
/// order, lists the scenes whose summary holds the word, in the order of
/// the list. The query shows in the list's top rule, and the list frame
/// keeps its corners.
///
/// Why it matters: a list of eighteen scenes is long to walk with `j`, and
/// a user who knows part of a name or what a scene shows should reach it in
/// a few keys.
#[test]
fn slash_and_letters_narrow_the_list_to_the_loose_matches() {
    // Given:
    let mut app = App::picker(7, corpus());
    let corners = list_corners(&app);

    // When:
    let mut keys = vec![key(KeyCode::Char('/'))];
    keys.extend(typed("drv"));
    send(&mut app, keys);
    for _ in 0..5 {
        app.tick(WIDE);
    }

    // Then:
    assert_eq!(listed(&app), vec!["drive"]);
    assert_eq!(marked(&app).as_deref(), Some("drive"));
    assert_pane_runs(&app, SceneKind::Drive);
    assert!(list_rows(&app).iter().any(|row| row.contains("/drv")));
    assert_eq!(list_corners(&app), corners);

    // When:
    send(&mut app, vec![key(KeyCode::Backspace); 3]);
    send(&mut app, typed("the"));

    // Then:
    let by_name = |kind: &&SceneKind| {
        let mut letters = kind.name().chars();
        "the".chars().all(|wanted| letters.any(|c| c == wanted))
    };
    assert!(!ALL.iter().any(|kind| by_name(&kind)), "a name holds `the`");
    let expected: Vec<&str> = ALL
        .iter()
        .filter(|kind| kind.summary().contains("the"))
        .map(|kind| kind.name())
        .collect();
    assert!(expected.len() >= 2, "{expected:?}");
    assert_eq!(listed(&app), expected);
    assert_eq!(marked(&app).as_deref(), Some(expected[0]));
    assert_eq!(list_corners(&app), corners);
}

/// A scene whose name matches comes before one whose summary matches.
/// Given: the picker.
/// When: `/` is pressed and `ri` typed.
/// Then: every scene whose name holds `r` then `i` is listed before every
/// scene that matches by its summary alone.
///
/// Why it matters: the name is what a user types first, and a summary
/// match at the top would hide the scene they named.
#[test]
fn a_name_match_comes_before_a_summary_match() {
    // Given:
    let mut app = App::picker(7, corpus());

    // When:
    let mut keys = vec![key(KeyCode::Char('/'))];
    keys.extend(typed("ri"));
    send(&mut app, keys);

    // Then:
    let names = listed(&app);
    let by_name = |name: &String| {
        let mut letters = name.chars();
        "ri".chars().all(|wanted| letters.any(|c| c == wanted))
    };
    let first_summary = names.iter().position(|name| !by_name(name));
    let last_name = names.iter().rposition(by_name);
    assert!(last_name.is_some(), "{names:?}");
    if let (Some(first_summary), Some(last_name)) = (first_summary, last_name) {
        assert!(last_name < first_summary, "{names:?}");
    }
}

/// In a search every letter types, and Esc gives back the whole list.
/// Given: the picker.
/// When: `/` is pressed, `qt` typed, and Enter pressed, then Backspace
/// clears the query, `rain` is typed, Down moves the mark, and Esc leaves
/// the search.
/// Then: `q` does not quit, `qt` lists nothing, the pane is blank once
/// the app ticks, and Enter starts nothing. Down marks the second match,
/// and after Esc the whole list shows with the mark on that scene and the
/// picker still open.
///
/// Why it matters: a search that quits on `q` or changes the theme on `t`
/// loses the user's place, and a search that cannot be left traps them.
#[test]
fn letters_type_in_a_search_and_esc_leaves_it() {
    // Given:
    let saved =
        std::env::temp_dir().join(format!("hearth-keys-{}-search.toml", std::process::id()));
    let _ = std::fs::remove_file(&saved);
    let mut app = App::picker(7, corpus()).saving_to(saved.clone());

    // When:
    let mut keys = vec![key(KeyCode::Char('/'))];
    keys.extend(typed("qt"));
    keys.push(key(KeyCode::Enter));
    send(&mut app, keys);

    // Then:
    assert!(!app.should_quit);
    let theme = std::fs::read_to_string(&saved).unwrap_or_default();
    assert!(!theme.contains("theme"), "t changed the theme: {theme:?}");
    let _ = std::fs::remove_file(&saved);
    assert!(app.is_picking());
    assert!(listed(&app).is_empty(), "{:?}", listed(&app));
    for _ in 0..5 {
        app.tick(WIDE);
    }
    let shown = app_frame(&app, WIDE);
    let inner = pane_inner(&shown).expect("no pane");
    assert!(
        inner.positions().all(|at| shown[at].symbol() == " "),
        "the pane runs a scene with no match"
    );

    // When:
    send(&mut app, vec![key(KeyCode::Backspace); 2]);
    send(&mut app, typed("rain"));
    send(&mut app, vec![key(KeyCode::Down)]);
    let second = listed(&app)[1].clone();
    send(&mut app, vec![key(KeyCode::Esc)]);

    // Then:
    let all: Vec<&str> = ALL.iter().map(|kind| kind.name()).collect();
    assert_eq!(marked(&app).as_deref(), Some(second.as_str()));
    assert!(app.is_picking());
    assert!(!list_rows(&app).iter().any(|row| row.contains("/rain")));
    let shown = listed(&app);
    assert!(
        shown.len() >= 5 && all.windows(shown.len()).any(|run| run == shown.as_slice()),
        "{shown:?}"
    );
}

/// Enter in a search starts the marked match and ends the search.
/// Given: the picker with `/` pressed and `drv` typed.
/// When: Enter is pressed, the app ticks ten times, and Esc opens the
/// picker again.
/// Then: the drive runs as Tab would start it, and the picker shows the
/// whole list.
///
/// Why it matters: a search that stays on after its scene starts would
/// greet the user with a list of one when they come back.
#[test]
fn enter_in_a_search_starts_the_match() {
    // Given:
    let mut app = App::picker(7, corpus());
    let mut keys = vec![key(KeyCode::Char('/'))];
    keys.extend(typed("drv"));
    send(&mut app, keys);

    // When:
    send(&mut app, vec![key(KeyCode::Enter)]);
    let mut drive = App::new(SceneKind::Drive, 8, corpus());
    for _ in 0..10 {
        app.tick(WIDE);
        drive.tick(WIDE);
    }

    // Then:
    assert!(!app.is_picking());
    assert!(
        app_frame(&app, WIDE) == app_frame(&drive, WIDE),
        "drive did not start"
    );

    // When:
    send(&mut app, vec![key(KeyCode::Esc)]);

    // Then:
    assert!(listed(&app).len() >= 5, "{:?}", listed(&app));
}

/// Given: a scene running with sound on.
/// When: `m` is pressed, then pressed again.
/// Then: sound turns off, then on.
///
/// Why it matters: `m` is the one way to silence hearth while it runs.
#[test]
fn m_turns_the_sound_off_and_on() {
    // Given:
    let mut app = App::new(SceneKind::Fire, 7, corpus());
    assert!(app.sound_on());

    // When:
    send(&mut app, vec![key(KeyCode::Char('m'))]);

    // Then:
    assert!(!app.sound_on());

    // When:
    send(&mut app, vec![key(KeyCode::Char('m'))]);

    // Then:
    assert!(app.sound_on());
}
