#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use hearth::app::App;
use hearth::corpus::Corpus;
use hearth::events::{EventSource, handle_events};
use hearth::scene::SceneKind;
use hearth::theme;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};

const AREA: Rect = Rect::new(0, 0, 40, 12);
const LEFT: Rect = Rect::new(0, 0, 19, 12);
const RIGHT: Rect = Rect::new(20, 0, 20, 12);
const TOP: Rect = Rect::new(0, 0, 40, 5);

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

fn press(code: KeyCode) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

fn send(app: &mut App, keys: &[KeyCode]) {
    let mut source = QueuedEvents {
        queued: keys.iter().map(|code| press(*code)).collect(),
    };
    handle_events(app, &mut source, Duration::ZERO).unwrap();
}

fn fire() -> App {
    App::new(SceneKind::Fire, 7, Arc::new(Corpus::embedded()))
}

fn frame(app: &App) -> Buffer {
    let mut buffer = Buffer::empty(AREA);
    app.render(AREA, &mut buffer);
    buffer
}

/// What `kind` draws from `seed` in `pane` after `ticks` ticks when it
/// runs alone.
fn alone(kind: SceneKind, seed: u64, ticks: usize, pane: Rect) -> Buffer {
    let mut app = App::new(kind, seed, Arc::new(Corpus::embedded()));
    for _ in 0..ticks {
        app.tick(pane);
    }
    let mut buffer = Buffer::empty(AREA);
    app.render(pane, &mut buffer);
    buffer
}

fn assert_same_inside(shown: &Buffer, expected: &Buffer, pane: Rect, what: &str) {
    for position in pane.positions() {
        assert_eq!(
            shown[position], expected[position],
            "{what} at {position:?}"
        );
    }
}

/// The same check for a focused pane, whose top-left and bottom-right
/// cells carry the focus marks.
fn assert_same_but_marks(shown: &Buffer, expected: &Buffer, pane: Rect, what: &str) {
    let marks = marks_of(pane);
    for position in pane.positions().filter(|p| !marks.contains(p)) {
        assert_eq!(
            shown[position], expected[position],
            "{what} at {position:?}"
        );
    }
}

fn marks_of(pane: Rect) -> [Position; 2] {
    [
        Position::new(pane.left(), pane.top()),
        Position::new(pane.right() - 1, pane.bottom() - 1),
    ]
}

/// The pane whose corners carry the gold marks, read from the screen.
fn focused(buffer: &Buffer, panes: &[Rect]) -> Option<Rect> {
    let marked = |pane: &&Rect| {
        let [top_left, bottom_right] = marks_of(**pane);
        let (a, b) = (&buffer[top_left], &buffer[bottom_right]);
        (a.symbol(), b.symbol()) == ("┌", "┘") && a.fg == theme::MARKER && b.fg == theme::MARKER
    };
    let found: Vec<Rect> = panes.iter().filter(marked).copied().collect();
    assert!(found.len() <= 1, "more than one pane marked: {found:?}");
    found.first().copied()
}

fn text_inside(buffer: &Buffer, pane: Rect) -> String {
    pane.rows()
        .map(|row| {
            row.columns()
                .map(|cell| buffer[(cell.x, cell.y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_rule(buffer: &Buffer, rule: Rect, symbol: &str) {
    for position in rule.positions() {
        let cell = &buffer[position];
        assert_eq!(cell.symbol(), symbol, "rule at {position:?}");
        assert_eq!(cell.fg, theme::MUTED, "rule colour at {position:?}");
    }
}

/// Given: the fire on a forty by twelve terminal.
/// When: `v` splits it, and the app ticks three times.
/// Then: a muted rule runs down column 19, the fire runs alone in the
/// nineteen columns left of it, and the picker opens in the twenty columns
/// right of it.
///
/// Why it matters: a scene or the picker that draws across the rule tears
/// the pane beside it.
#[test]
fn a_side_by_side_split_puts_the_picker_beside_the_scene() {
    // Given:
    let mut app = fire();

    // When:
    send(&mut app, &[KeyCode::Char('v')]);
    for _ in 0..3 {
        app.tick(AREA);
    }

    // Then:
    let shown = frame(&app);
    assert_rule(&shown, Rect::new(19, 0, 1, 12), "│");
    assert_same_inside(&shown, &alone(SceneKind::Fire, 7, 3, LEFT), LEFT, "fire");
    assert!(text_inside(&shown, RIGHT).contains("scenes"));
}

/// Given: the fire on a forty by twelve terminal.
/// When: `s` splits it, and the app ticks three times.
/// Then: a muted rule runs across row 5, the fire runs alone in the five
/// rows above it, and the picker opens in the rows below it.
///
/// Why it matters: the stacked split is the second half of the i3 layout,
/// and it must keep the scene above inside its own rows.
#[test]
fn a_stacked_split_puts_the_picker_below_the_scene() {
    // Given:
    let mut app = fire();

    // When:
    send(&mut app, &[KeyCode::Char('s')]);
    for _ in 0..3 {
        app.tick(AREA);
    }

    // Then:
    let shown = frame(&app);
    assert_rule(&shown, Rect::new(0, 5, 40, 1), "─");
    assert_same_inside(&shown, &alone(SceneKind::Fire, 7, 3, TOP), TOP, "fire");
    assert!(text_inside(&shown, Rect::new(0, 6, 40, 6)).contains("scenes"));
}

/// Given: the fire split side by side, with the picker in the new pane.
/// When: Enter starts the marked scene, and the app ticks four times.
/// Then: the new pane runs the scene after the fire, life, from seed 9,
/// because a new pane takes the seed after the fire's 7 and a start takes
/// the next one. The fire still runs alone on the left.
///
/// Why it matters: Enter in a pane's picker must change that pane only,
/// and a fixed seed must replay every pane.
#[test]
fn enter_starts_the_next_scene_in_the_new_pane_only() {
    // Given:
    let mut app = fire();
    send(&mut app, &[KeyCode::Char('v')]);

    // When:
    send(&mut app, &[KeyCode::Enter]);
    for _ in 0..4 {
        app.tick(AREA);
    }

    // Then:
    let shown = frame(&app);
    assert_same_inside(&shown, &alone(SceneKind::Fire, 7, 4, LEFT), LEFT, "fire");
    assert_same_but_marks(&shown, &alone(SceneKind::Life, 9, 4, RIGHT), RIGHT, "life");
}

/// Given: the fire split side by side, and a scene started on the right.
/// When: `s` splits the right pane.
/// Then: the new rule meets the first one in a `├` at column 19, row 5.
///
/// Why it matters: two rules that cross without a joint read as two frames
/// drawn over each other.
#[test]
fn the_rules_join_where_they_meet() {
    // Given:
    let mut app = fire();
    send(&mut app, &[KeyCode::Char('v'), KeyCode::Enter]);

    // When:
    send(&mut app, &[KeyCode::Char('s')]);

    // Then:
    let shown = frame(&app);
    assert_eq!(shown[(19, 5)].symbol(), "├");
    assert_rule(&shown, Rect::new(20, 5, 20, 1), "─");
    assert_rule(&shown, Rect::new(19, 0, 1, 5), "│");
    assert_rule(&shown, Rect::new(19, 6, 1, 6), "│");
}

const RIGHT_TOP: Rect = Rect::new(20, 0, 20, 5);
const RIGHT_BOTTOM: Rect = Rect::new(20, 6, 20, 6);

/// Given: the fire alone on the screen.
/// When: it ticks and renders.
/// Then: no pane carries the focus marks.
///
/// Why it matters: one pane has nothing to tell apart, and marks there
/// would cover two cells of every scene for no reason.
#[test]
fn one_pane_shows_no_focus_marks() {
    // Given:
    let mut app = fire();

    // When:
    for _ in 0..3 {
        app.tick(AREA);
    }

    // Then:
    assert_eq!(focused(&frame(&app), &[AREA]), None);
}

/// Given: the fire beside a second scene, and the right pane split again.
/// When: `h`, `j`, `k`, `l` and the arrows are pressed in turn.
/// Then: the gold marks follow each key to the pane on that side, stay
/// put at an edge, and from the tall left pane `l` picks the right-hand
/// pane that holds the left pane's middle row.
///
/// Why it matters: a key that moves the focus to the wrong pane makes
/// every other key act on a scene the user is not looking at.
#[test]
fn h_j_k_l_and_the_arrows_move_the_focus() {
    // Given:
    let mut app = fire();
    send(&mut app, &[KeyCode::Char('v'), KeyCode::Enter]);
    let two = [LEFT, RIGHT];
    assert_eq!(focused(&frame(&app), &two), Some(RIGHT));

    // When, then:
    for (key, pane) in [
        (KeyCode::Char('h'), LEFT),
        (KeyCode::Char('h'), LEFT),
        (KeyCode::Char('l'), RIGHT),
        (KeyCode::Left, LEFT),
        (KeyCode::Right, RIGHT),
    ] {
        send(&mut app, &[key]);
        assert_eq!(focused(&frame(&app), &two), Some(pane), "after {key:?}");
    }

    send(&mut app, &[KeyCode::Char('s'), KeyCode::Enter]);
    let three = [LEFT, RIGHT_TOP, RIGHT_BOTTOM];
    for (key, pane) in [
        (KeyCode::Char('k'), RIGHT_TOP),
        (KeyCode::Char('j'), RIGHT_BOTTOM),
        (KeyCode::Up, RIGHT_TOP),
        (KeyCode::Down, RIGHT_BOTTOM),
        (KeyCode::Char('j'), RIGHT_BOTTOM),
        (KeyCode::Char('h'), LEFT),
        (KeyCode::Char('l'), RIGHT_BOTTOM),
    ] {
        send(&mut app, &[key]);
        assert_eq!(focused(&frame(&app), &three), Some(pane), "after {key:?}");
    }
}

/// Given: the fire beside life, with the focus moved back to the fire.
/// When: Tab is pressed, and the app ticks four times.
/// Then: the left pane runs life from seed 8, the fire's next seed, and
/// the right pane still runs its own life from seed 9.
///
/// Why it matters: Tab, `r`, `+` and `-` must change the pane the user
/// looks at and leave every other pane running as it was.
#[test]
fn tab_changes_the_focused_pane_only() {
    // Given:
    let mut app = fire();
    send(
        &mut app,
        &[KeyCode::Char('v'), KeyCode::Enter, KeyCode::Char('h')],
    );

    // When:
    send(&mut app, &[KeyCode::Tab]);
    for _ in 0..4 {
        app.tick(AREA);
    }

    // Then:
    let shown = frame(&app);
    assert_same_but_marks(&shown, &alone(SceneKind::Life, 8, 4, LEFT), LEFT, "left");
    assert_same_inside(&shown, &alone(SceneKind::Life, 9, 4, RIGHT), RIGHT, "right");
}

/// Given: the fire beside life, with the focus moved back to the fire.
/// When: Esc is pressed.
/// Then: the picker opens in the left pane, and the right pane shows no
/// picker.
///
/// Why it matters: a picker over the whole screen would hide every pane
/// to change one.
#[test]
fn esc_opens_the_picker_in_the_focused_pane() {
    // Given:
    let mut app = fire();
    send(
        &mut app,
        &[KeyCode::Char('v'), KeyCode::Enter, KeyCode::Char('h')],
    );

    // When:
    send(&mut app, &[KeyCode::Esc]);

    // Then:
    let shown = frame(&app);
    assert!(text_inside(&shown, LEFT).contains("scenes"));
    assert!(!text_inside(&shown, RIGHT).contains("scenes"));
}

/// Given: the fire beside life, with the focus moved back to the fire.
/// When: `+` makes the fire faster.
/// Then: the new speed shows on the top row of the left pane, and not on
/// the right pane.
///
/// Why it matters: a speed shown in the corner of the screen names the
/// wrong pane when the focus is on the left.
#[test]
fn a_new_speed_shows_in_the_focused_pane() {
    // Given:
    let mut app = fire();
    send(
        &mut app,
        &[KeyCode::Char('v'), KeyCode::Enter, KeyCode::Char('h')],
    );

    // When:
    send(&mut app, &[KeyCode::Char('+')]);

    // Then:
    let shown = frame(&app);
    let row = |pane: Rect| text_inside(&shown, Rect { height: 1, ..pane });
    assert!(row(LEFT).ends_with("1.5×"), "{}", row(LEFT));
    assert!(!row(RIGHT).contains('×'), "{}", row(RIGHT));
}

/// Given: the fire beside life, with the focus on life.
/// When: `x` closes life, and the app ticks three times.
/// Then: the fire runs alone over the whole screen, with no rule and no
/// marks.
///
/// Why it matters: a closed pane that leaves its rule or its area behind
/// shrinks the screen for good.
#[test]
fn x_gives_the_closed_pane_s_area_to_its_sibling() {
    // Given:
    let mut app = fire();
    send(&mut app, &[KeyCode::Char('v'), KeyCode::Enter]);

    // When:
    send(&mut app, &[KeyCode::Char('x')]);
    for _ in 0..3 {
        app.tick(AREA);
    }

    // Then:
    assert_same_inside(
        &frame(&app),
        &alone(SceneKind::Fire, 7, 3, AREA),
        AREA,
        "fire",
    );
}

/// Given: the fire beside life, and life split again below.
/// When: `x` closes the bottom-right pane, and the app ticks three times.
/// Then: life takes the whole right half and the focus, and the fire on
/// the left runs on as it was.
///
/// Why it matters: a close must give the area to the closed pane's own
/// sibling and leave the rest of the tree as it was.
#[test]
fn a_close_in_a_stack_changes_that_stack_only() {
    // Given:
    let mut app = fire();
    send(
        &mut app,
        &[
            KeyCode::Char('v'),
            KeyCode::Enter,
            KeyCode::Char('s'),
            KeyCode::Enter,
        ],
    );

    // When:
    send(&mut app, &[KeyCode::Char('x')]);
    for _ in 0..3 {
        app.tick(AREA);
    }

    // Then:
    let shown = frame(&app);
    assert_eq!(focused(&shown, &[LEFT, RIGHT]), Some(RIGHT));
    assert_same_inside(&shown, &alone(SceneKind::Fire, 7, 3, LEFT), LEFT, "fire");
    assert_same_but_marks(&shown, &alone(SceneKind::Life, 9, 3, RIGHT), RIGHT, "life");
    assert_rule(&shown, Rect::new(19, 0, 1, 12), "│");
}

/// Given: the fire alone on the screen.
/// When: `x` is pressed, and the app ticks three times.
/// Then: the fire still runs over the whole screen.
///
/// Why it matters: a screensaver with no pane has nothing to draw and no
/// pane to open the picker in.
#[test]
fn the_last_pane_does_not_close() {
    // Given:
    let mut app = fire();

    // When:
    send(&mut app, &[KeyCode::Char('x')]);
    for _ in 0..3 {
        app.tick(AREA);
    }

    // Then:
    assert!(!app.should_quit);
    assert_same_inside(
        &frame(&app),
        &alone(SceneKind::Fire, 7, 3, AREA),
        AREA,
        "fire",
    );
}

/// Given: the fire split side by side, with the picker in the new pane.
/// When: Esc is pressed, and the app ticks three times.
/// Then: the new pane is gone and the fire runs over the whole screen.
///
/// Why it matters: a split made by mistake must cost one key to undo.
#[test]
fn esc_in_a_new_pane_s_picker_closes_the_pane() {
    // Given:
    let mut app = fire();
    send(&mut app, &[KeyCode::Char('v')]);

    // When:
    send(&mut app, &[KeyCode::Esc]);
    for _ in 0..3 {
        app.tick(AREA);
    }

    // Then:
    assert!(!app.is_picking());
    assert_same_inside(
        &frame(&app),
        &alone(SceneKind::Fire, 7, 3, AREA),
        AREA,
        "fire",
    );
}

/// Given: the fire beside life, with the focus moved back to the fire.
/// When: `x` closes the fire, the first pane made, and the app ticks three
/// times.
/// Then: life runs alone over the whole screen.
///
/// Why it matters: every pane after a closed one moves down one place,
/// and a pane left at its old place points past the end of the list.
#[test]
fn closing_the_first_pane_keeps_the_ones_after_it() {
    // Given:
    let mut app = fire();
    send(
        &mut app,
        &[KeyCode::Char('v'), KeyCode::Enter, KeyCode::Char('h')],
    );

    // When:
    send(&mut app, &[KeyCode::Char('x')]);
    for _ in 0..3 {
        app.tick(AREA);
    }

    // Then:
    assert_same_inside(
        &frame(&app),
        &alone(SceneKind::Life, 9, 3, AREA),
        AREA,
        "life",
    );
}

/// Given: the fire beside life, and life split again below, on a forty by
/// twelve terminal.
/// When: the terminal grows to eighty by twenty-four.
/// Then: the first rule runs down column 39 and the second across row 11
/// of the right half, the middle of each split at the new size.
///
/// Why it matters: a layout fixed at the size it was made in leaves a
/// grown terminal half empty and cuts a shrunk one.
#[test]
fn a_resize_keeps_each_split_in_the_middle() {
    // Given:
    let mut app = fire();
    send(
        &mut app,
        &[
            KeyCode::Char('v'),
            KeyCode::Enter,
            KeyCode::Char('s'),
            KeyCode::Enter,
        ],
    );

    // When:
    let grown = Rect::new(0, 0, 80, 24);
    app.tick(grown);
    let mut shown = Buffer::empty(grown);
    app.render(grown, &mut shown);

    // Then:
    assert_rule(&shown, Rect::new(39, 0, 1, 11), "│");
    assert_eq!(shown[(39, 11)].symbol(), "├");
    assert_rule(&shown, Rect::new(39, 12, 1, 12), "│");
    assert_rule(&shown, Rect::new(40, 11, 40, 1), "─");
}

/// Given: every scene, split into four panes.
/// When: the app ticks and renders in a corner of a larger canvas at
/// sizes down to one cell and none.
/// Then: nothing panics, and no cell outside the area is written.
///
/// Why it matters: a terminal dragged small leaves panes with no cells,
/// and a scene that panics on one takes the whole screensaver down.
#[test]
fn panes_too_small_to_draw_never_panic() {
    let canvas = Rect::new(0, 0, 30, 12);
    for &kind in hearth::scene::ALL {
        // Given:
        let mut app = App::new(kind, 7, Arc::new(Corpus::embedded()));
        send(
            &mut app,
            &[
                KeyCode::Char('v'),
                KeyCode::Enter,
                KeyCode::Char('s'),
                KeyCode::Enter,
                KeyCode::Char('v'),
                KeyCode::Enter,
            ],
        );

        // When, then:
        for area in [
            Rect::new(2, 2, 20, 8),
            Rect::new(2, 2, 5, 3),
            Rect::new(2, 2, 2, 1),
            Rect::new(2, 2, 1, 1),
            Rect::new(2, 2, 0, 0),
        ] {
            app.tick(area);
            let mut shown = Buffer::empty(canvas);
            app.render(area, &mut shown);
            for position in canvas.positions() {
                if !area.contains(position) {
                    assert_eq!(
                        shown[position],
                        Buffer::empty(canvas)[position],
                        "{kind:?} in {area:?} wrote {position:?}"
                    );
                }
            }
        }
    }
}
