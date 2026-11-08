#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod events;
mod frames;

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyModifiers};
use hearth::app::App;
use hearth::corpus::Corpus;
use hearth::scene::{ALL, SceneKind};
use ratatui::layout::Rect;

use frames::{alone, app_frame, mark, marked, pane_inner};

use events::{back_tab, key, press, send, tab, typed};

const WIDE: Rect = Rect::new(0, 0, 120, 30);

fn corpus() -> Arc<Corpus> {
    Arc::new(Corpus::embedded())
}

fn place(kind: SceneKind) -> usize {
    ALL.iter().position(|each| *each == kind).unwrap()
}

fn assert_pane_runs(app: &App, kind: SceneKind) {
    let shown = app_frame(app, WIDE);
    let inner = pane_inner(&shown).unwrap_or_else(|| panic!("no pane for {kind:?}"));
    let expected = alone(kind, 7, 5, inner, WIDE);
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
    mark(&mut app, SceneKind::Hourglass, WIDE);

    // When:
    send(&mut app, vec![tab()]);
    for _ in 0..5 {
        app.tick(WIDE);
    }

    // Then:
    let next = ALL[place(SceneKind::Hourglass) + 1];
    assert_eq!(marked(&app, WIDE).as_deref(), Some(next.name()));
    assert_pane_runs(&app, next);

    // When:
    mark(&mut app, ALL[ALL.len() - 1], WIDE);
    send(&mut app, vec![tab()]);

    // Then:
    assert_eq!(marked(&app, WIDE).as_deref(), Some(ALL[0].name()));
    assert!(app.is_picking());
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
    mark(&mut app, SceneKind::Study, WIDE);

    // When:
    send(&mut app, vec![back_tab()]);
    for _ in 0..5 {
        app.tick(WIDE);
    }

    // Then:
    let before = ALL[place(SceneKind::Study) - 1];
    assert_eq!(marked(&app, WIDE).as_deref(), Some(before.name()));
    assert_pane_runs(&app, before);

    // When:
    let mut first = App::picker(7, corpus());
    send(&mut first, vec![back_tab()]);

    // Then:
    assert_eq!(
        marked(&first, WIDE).as_deref(),
        Some(ALL[ALL.len() - 1].name())
    );
    assert!(first.is_picking());
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
    assert_eq!(marked(&app, WIDE).as_deref(), Some("drive"));
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
    assert_eq!(marked(&app, WIDE).as_deref(), Some(expected[0]));
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
    assert_eq!(marked(&app, WIDE).as_deref(), Some(second.as_str()));
    assert!(app.is_picking());
    assert!(!list_rows(&app).iter().any(|row| row.contains("/rain")));
    let shown = listed(&app);
    assert!(
        shown.len() >= 5 && all.windows(shown.len()).any(|run| run == shown.as_slice()),
        "{shown:?}"
    );
}

/// Esc after a search that matched nothing brings the marked scene back
/// into the pane.
/// Given: the picker, a search for `zz`, which matches no scene.
/// When: Esc is pressed and the app ticks.
/// Then: the whole list shows, a scene is marked, and the pane runs it.
///
/// Why it matters: a search with no match empties the pane, and a pane
/// that stays empty after the search is over reads as a hang.
#[test]
fn esc_after_a_search_with_no_match_shows_the_marked_scene() {
    // Given:
    let mut app = App::picker(7, corpus());
    let mut keys = vec![key(KeyCode::Char('/'))];
    keys.extend(typed("zz"));
    send(&mut app, keys);
    assert!(listed(&app).is_empty(), "{:?}", listed(&app));

    // When:
    send(&mut app, vec![key(KeyCode::Esc)]);
    for _ in 0..5 {
        app.tick(WIDE);
    }

    // Then:
    assert!(app.is_picking());
    assert!(!listed(&app).is_empty(), "the list is still empty");
    assert!(marked(&app, WIDE).is_some());
    let shown = app_frame(&app, WIDE);
    let inner = pane_inner(&shown).expect("no pane");
    assert!(
        inner.positions().any(|at| shown[at].symbol() != " "),
        "the pane is empty after Esc"
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

/// Given: a scene running at full volume.
/// When: `[` is pressed, then `]`.
/// Then: the volume goes down to 90%, then back up to 100%.
///
/// Why it matters: `[` and `]` are the one way to set the volume while
/// hearth runs.
#[test]
fn brackets_lower_and_raise_the_volume() {
    // Given:
    let mut app = App::new(SceneKind::Fire, 7, corpus());
    assert_eq!(app.volume().percent(), 100);

    // When:
    send(&mut app, vec![key(KeyCode::Char('['))]);

    // Then:
    assert_eq!(app.volume().percent(), 90);

    // When:
    send(&mut app, vec![key(KeyCode::Char(']'))]);

    // Then:
    assert_eq!(app.volume().percent(), 100);
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

/// A letter with Ctrl or Alt held does not type into a search.
/// Given: the picker with a search for `ra`.
/// When: Ctrl-i and Alt-i are pressed, and then a plain `i`.
/// Then: the search reads `ra` after the two chords and `rai` after the
/// plain letter.
///
/// Why it matters: a chord is a command in most terminals, and one that
/// types its letter changes the search under the user.
#[test]
fn a_chord_does_not_type_into_a_search() {
    // Given:
    let mut app = App::picker(7, corpus());
    let mut keys = vec![key(KeyCode::Char('/'))];
    keys.extend(typed("ra"));
    send(&mut app, keys);

    // When:
    send(
        &mut app,
        vec![
            press(KeyCode::Char('i'), KeyModifiers::CONTROL),
            press(KeyCode::Char('i'), KeyModifiers::ALT),
        ],
    );
    let after_chords = list_rows(&app);
    send(&mut app, typed("i"));

    // Then:
    assert!(
        after_chords
            .iter()
            .any(|row| row.contains("/ra ") || row.ends_with("/ra")),
        "{after_chords:?}"
    );
    assert!(
        !after_chords.iter().any(|row| row.contains("/rai")),
        "{after_chords:?}"
    );
    assert!(list_rows(&app).iter().any(|row| row.contains("/rai")));
}

fn shows_text(app: &App, text: &str) -> bool {
    let shown = app_frame(app, WIDE);
    (0..WIDE.height).any(|y| {
        let row: String = (0..WIDE.width).map(|x| shown[(x, y)].symbol()).collect();
        row.contains(text)
    })
}

/// Space pauses the hourglass and space again resumes it.
/// Given: hearth on the hourglass, after one tick.
/// When: space is pressed, then pressed again.
/// Then: the screen says the timer is paused, then no longer says so.
///
/// Why it matters: space is the one way to stop the timer for an
/// interruption without losing the round.
#[test]
fn space_pauses_and_resumes_the_hourglass() {
    // Given:
    let mut app = App::new(SceneKind::Hourglass, 7, corpus());
    app.tick(WIDE);
    assert!(!shows_text(&app, "paused"));

    // When:
    send(&mut app, vec![key(KeyCode::Char(' '))]);

    // Then:
    assert!(shows_text(&app, "paused"));

    // When:
    send(&mut app, vec![key(KeyCode::Char(' '))]);

    // Then:
    assert!(!shows_text(&app, "paused"));
}

/// `n`, `+` and `-` reach the hourglass timer.
/// Given: hearth on the hourglass, after one tick.
/// When: `n` is pressed, then `+`, then `-`.
/// Then: the screen shows the first break with 5:00 left, then 6:00
/// left, then 5:00 left again.
///
/// Why it matters: these keys are the one way to skip a phase or change
/// its time while hearth runs.
#[test]
fn n_plus_and_minus_control_the_hourglass() {
    // Given:
    let mut app = App::new(SceneKind::Hourglass, 7, corpus());
    app.tick(WIDE);

    for (press, expected) in [
        ('n', "break 1/4 · 5:00 left"),
        ('+', "break 1/4 · 6:00 left"),
        ('-', "break 1/4 · 5:00 left"),
    ] {
        // When:
        send(&mut app, vec![key(KeyCode::Char(press))]);
        app.tick(WIDE);

        // Then:
        assert!(shows_text(&app, expected), "after {press}: {expected}");
    }
}
