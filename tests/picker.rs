#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use hearth::app::App;
use hearth::config::Config;
use hearth::corpus::Corpus;
use hearth::mark;
use hearth::picker::{Entry, Picker};
use hearth::scene::{ALL, Rate, SceneKind};
use hearth::theme;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

const ENTRIES: &[Entry] = &[
    Entry {
        name: "noise",
        summary: "random glyphs",
        speed: "1×",
    },
    Entry {
        name: "matrix",
        summary: "katakana rain",
        speed: "2×",
    },
];

fn render(selected: usize, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| {
            f.render_widget(
                Picker {
                    entries: ENTRIES,
                    selected,
                },
                f.area(),
            );
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn row_text(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol().to_string())
        .collect()
}

fn locate(buffer: &Buffer, label: &str) -> (u16, u16) {
    (0..buffer.area.height)
        .find_map(|y| {
            let row = row_text(buffer, y);
            row.find(label)
                .map(|byte| (u16::try_from(row[..byte].chars().count()).unwrap(), y))
        })
        .unwrap_or_else(|| panic!("no row shows {label}"))
}

/// Given: a picker with the second scene current.
/// When: it renders.
/// Then: that row reads `[matrix]`, the brackets in the marker colour and the
/// label in the marked colour.
#[test]
fn the_current_row_carries_the_gold_pair_and_an_iris_label() {
    // Given, when:
    let buffer = render(1, 40, 10);
    let (x, y) = locate(&buffer, "matrix");
    let after = x + u16::try_from("matrix".len()).unwrap();

    // Then:
    assert_eq!(buffer[(x - 1, y)].symbol(), "[");
    assert_eq!(buffer[(after, y)].symbol(), "]");
    assert_eq!(buffer[(x - 1, y)].fg, theme::MARKER);
    assert_eq!(buffer[(after, y)].fg, theme::MARKER);
    assert_eq!(buffer[(x, y)].fg, theme::MARKED);
}

/// Given: a picker with the second scene current.
/// When: it renders.
/// Then: the first row keeps blank cells where its brackets would be, so
/// its label starts in the same column as the current one.
///
/// Why it matters: a pair that appears only on the current row shifts every
/// label by one cell as the selection moves.
#[test]
fn a_row_that_is_not_current_reserves_the_pair_as_blank_cells() {
    // Given, when:
    let buffer = render(1, 40, 10);
    let (x, y) = locate(&buffer, "noise");
    let (current_x, _) = locate(&buffer, "matrix");
    let after = x + u16::try_from("noise".len()).unwrap();

    // Then:
    assert_eq!(x, current_x);
    assert_eq!(buffer[(x - 1, y)].symbol(), " ");
    assert_eq!(buffer[(after, y)].symbol(), " ");
    assert_eq!(buffer[(x, y)].fg, theme::TEXT);
}

/// Given: terminal widths from 20 to 120 columns.
/// When: the picker renders at each.
/// Then: the top rule is one unbroken run of box runes around its title, and
/// the frame sits in the middle of the area.
///
/// Why it matters: a rule that under-budgets its title gap only breaks at
/// the widths where the title nearly fits, which three fixed widths miss.
#[test]
fn the_top_rule_has_no_gap_at_any_width() {
    for width in 20..=120 {
        // Given, when:
        let buffer = render(0, width, 8);
        let top = (0..buffer.area.height)
            .map(|y| row_text(&buffer, y))
            .find(|row| row.contains('┌'))
            .unwrap_or_else(|| panic!("no frame at width {width}"));

        // Then:
        let left_margin = top.chars().take_while(|c| *c == ' ').count();
        let right_margin = top.chars().rev().take_while(|c| *c == ' ').count();
        assert!(
            left_margin.abs_diff(right_margin) <= 1,
            "frame off centre at width {width}: {top:?}"
        );

        let rule = top.trim().replace(theme::PICKER_TITLE, "");
        assert!(
            rule.starts_with('┌') && rule.ends_with('┐'),
            "no corners at width {width}: {top:?}"
        );
        assert!(
            rule.trim_matches(|c| c == '┌' || c == '┐')
                .chars()
                .all(|c| c == '─'),
            "gap in the top rule at width {width}: {top:?}"
        );
    }
}

/// Given: a picker with two scenes and room for two rows each.
/// When: it renders.
/// Then: the row under each name holds its summary, from the name's column,
/// in the subtle colour.
#[test]
fn each_name_has_its_summary_on_the_row_below() {
    // Given, when:
    let buffer = render(1, 40, 10);

    // Then:
    for entry in ENTRIES {
        let (x, y) = locate(&buffer, entry.name);
        let below = row_text(&buffer, y + 1);
        let from_name: String = below.chars().skip(usize::from(x)).collect();
        assert!(from_name.starts_with(entry.summary), "{below:?}");
        assert_eq!(buffer[(x, y + 1)].fg, theme::SUBTLE);
    }
}

/// Given: areas one row short of two rows a scene, and one with room.
/// When: the picker renders at each.
/// Then: the short ones show every name and no summary, and the one with
/// room shows the summaries.
///
/// Why it matters: a frame cut at the bottom hides the last scenes, and a
/// scene that is not in the list cannot be started from it.
#[test]
fn a_short_area_drops_the_summaries_and_keeps_every_name() {
    let room = u16::try_from(ENTRIES.len() * 2 + 2).unwrap();
    for height in [room - 2, room - 1, room] {
        // Given, when:
        let buffer = render(0, 40, height);
        let text: String = (0..height).map(|y| row_text(&buffer, y)).collect();

        // Then:
        for entry in ENTRIES {
            assert!(text.contains(entry.name), "{height}: {text:?}");
            assert_eq!(
                text.contains(entry.summary),
                height == room,
                "{height}: {text:?}"
            );
        }
    }
}

/// Each row of the app's frame with its border, its marker pair and its
/// padding cut off, so a name row starts with the bare name.
fn bare_rows(buffer: &Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|y| {
            row_text(buffer, y)
                .replace(['[', ']'], " ")
                .trim_matches(|c| matches!(c, ' ' | '│'))
                .to_string()
        })
        .collect()
}

/// Given: the app opened on the picker in a tall terminal.
/// When: it renders.
/// Then: every scene's name has that scene's summary on the row below it,
/// and no summary is empty.
///
/// Why it matters: a summary the widget can draw but the app never passes
/// in leaves the list as bare as before.
#[test]
fn the_app_lists_every_scene_with_its_summary() {
    // Given:
    let app = App::picker(7, Arc::new(Corpus::embedded()));

    // When:
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|f| app.render(f.area(), f.buffer_mut()))
        .unwrap();
    let rows = bare_rows(terminal.backend().buffer());

    // Then:
    for kind in ALL {
        assert!(!kind.summary().is_empty(), "{}", kind.name());
        let at = rows
            .iter()
            .position(|row| row.split_whitespace().next() == Some(kind.name()))
            .unwrap_or_else(|| panic!("no row for {}: {rows:#?}", kind.name()));
        assert_eq!(rows[at + 1], kind.summary(), "{rows:#?}");
    }
}

/// Given: a picker with room for every name and its speed.
/// When: it renders.
/// Then: each speed is on its name's row, in the subtle colour, with one
/// blank cell between its last character and the right border.
#[test]
fn each_speed_sits_at_the_right_of_its_name_row() {
    // Given, when:
    let buffer = render(1, 40, 10);

    // Then:
    for entry in ENTRIES {
        let (_, name_y) = locate(&buffer, entry.name);
        let (x, y) = locate(&buffer, entry.speed);
        let after = x + u16::try_from(entry.speed.chars().count()).unwrap();
        assert_eq!(y, name_y, "{}", entry.name);
        assert_eq!(buffer[(x, y)].fg, theme::SUBTLE);
        assert_eq!(buffer[(after, y)].symbol(), " ");
        assert_eq!(buffer[(after + 1, y)].symbol(), "│");
    }
}

/// Given: terminal widths from 6 to 40 columns.
/// When: the picker renders at each.
/// Then: the cells after the marked name hold its whole speed or nothing,
/// some width shows the name with no speed, and at 40 the speed shows.
///
/// Why it matters: a speed cut at the border reads as another speed, `2`
/// for `2×` or `1` for `1.5×`.
#[test]
fn a_speed_shows_whole_or_not_at_all() {
    let entry = ENTRIES[1];
    let mut name_alone = false;
    for width in 6..=40 {
        // Given, when:
        let buffer = render(1, width, 10);
        let row = (0..10)
            .map(|y| row_text(&buffer, y))
            .find(|row| row.contains('['))
            .unwrap_or_else(|| panic!("no marked row at width {width}"));

        // Then:
        let (_, after_name) = row.split_once(']').unwrap();
        let speed = after_name.trim_matches(|c| matches!(c, ' ' | '│'));
        assert!(
            speed.is_empty() || speed == entry.speed,
            "cut speed at width {width}: {row:?}"
        );
        name_alone |= speed.is_empty() && row.contains(&format!("[{}]", entry.name));
        if width == 40 {
            assert_eq!(speed, entry.speed, "{row:?}");
        }
    }
    assert!(name_alone, "no width shows the name without its speed");
}

/// Given: terminal widths from 6 to 120 columns.
/// When: the picker renders at each.
/// Then: the bottom rule holds the whole key hint, in the subtle colour, or
/// none of it, and at 40 columns it holds the hint.
///
/// Why it matters: a hint cut at the corner reads as a key that does not
/// exist, and a hint that breaks the rule leaves a gap in the frame.
#[test]
fn the_bottom_rule_holds_the_whole_hint_or_none() {
    for width in 6..=120 {
        // Given, when:
        let buffer = render(0, width, 8);
        let (y, bottom) = (0..buffer.area.height)
            .map(|y| (y, row_text(&buffer, y)))
            .find(|(_, row)| row.contains('└'))
            .unwrap_or_else(|| panic!("no frame at width {width}"));

        // Then:
        let shown = bottom.contains(theme::PICKER_HINT);
        let rule = bottom.trim().replacen(theme::PICKER_HINT, "", 1);
        assert!(
            rule.starts_with('└') && rule.ends_with('┘'),
            "no corners at width {width}: {bottom:?}"
        );
        assert!(
            rule.trim_matches(|c| c == '└' || c == '┘')
                .chars()
                .all(|c| c == '─'),
            "part of the hint at width {width}: {bottom:?}"
        );
        if shown {
            let x = bottom.chars().position(|c| c == '↵').unwrap();
            let x = u16::try_from(x).unwrap();
            assert_eq!(buffer[(x, y)].fg, theme::SUBTLE, "{width}");
        }
        if width == 40 {
            assert!(shown, "{bottom:?}");
        }
    }
}

const WIDE: Rect = Rect::new(0, 0, 120, 30);

fn app_frame(app: &App, area: Rect) -> Buffer {
    let mut buffer = Buffer::empty(area);
    app.render(area, &mut buffer);
    buffer
}

/// The inside of the top-most frame whose top rule is a plain run of
/// `─`, which is the preview pane when there is one. The list's rule has
/// its title cut in, the mark's braces have no rule, and a scene in the
/// pane draws its own frames lower than the pane's top rule.
fn pane_inner(buffer: &Buffer) -> Option<Rect> {
    let area = buffer.area;
    let symbol = |x: u16, y: u16| buffer[(x, y)].symbol();
    let boxes = (0..area.height)
        .flat_map(|y| (0..area.width).map(move |x| (x, y)))
        .filter(|&(x, y)| symbol(x, y) == "┌")
        .filter_map(|(x, y)| {
            let right = (x + 1..area.width).find(|&r| symbol(r, y) != "─")?;
            let bottom = (y + 1..area.height).find(|&b| symbol(x, b) != "│")?;
            (right > x + 1 && symbol(right, y) == "┐" && symbol(x, bottom) == "└")
                .then(|| Rect::new(x + 1, y + 1, right - x - 1, bottom - y - 1))
        });
    boxes.min_by_key(|inner| (inner.y, std::cmp::Reverse(inner.x)))
}

/// What `kind` draws in `inner` after `ticks` ticks when it runs alone.
fn alone(kind: SceneKind, ticks: usize, inner: Rect) -> Buffer {
    let mut app = App::new(kind, 7, Arc::new(Corpus::embedded()));
    for _ in 0..ticks {
        app.tick(inner);
    }
    let mut buffer = Buffer::empty(WIDE);
    app.render(inner, &mut buffer);
    buffer
}

fn assert_same_inside(shown: &Buffer, expected: &Buffer, inner: Rect, what: &str) {
    for y in inner.top()..inner.bottom() {
        for x in inner.left()..inner.right() {
            assert_eq!(shown[(x, y)], expected[(x, y)], "{what} at ({x}, {y})");
        }
    }
}

/// Given: the picker in a wide terminal, ticked three times on noise.
/// When: the mark moves to matrix, and later to typing, and the app ticks
/// five times after each move.
/// Then: the pane right of the list holds what that scene draws alone
/// after five ticks in a rectangle the pane's size.
///
/// Why it matters: a preview that is not rebuilt on a move shows the last
/// scene under the new name, and one that differs from the scene alone
/// shows the user something Enter does not start.
#[test]
fn the_pane_runs_the_marked_scene_from_the_start() {
    // Given:
    let mut app = App::picker(7, Arc::new(Corpus::embedded()));
    for _ in 0..3 {
        app.tick(WIDE);
    }

    for kind in [SceneKind::Matrix, SceneKind::Typing] {
        // When:
        while ALL[app_selected(&app)] != kind {
            app.select_next();
        }
        for _ in 0..5 {
            app.tick(WIDE);
        }

        // Then:
        let shown = app_frame(&app, WIDE);
        let inner = pane_inner(&shown).unwrap_or_else(|| panic!("no pane for {kind:?}"));
        assert_same_inside(&shown, &alone(kind, 5, inner), inner, kind.name());
    }
}

/// The index of the marked row, read from the screen.
fn app_selected(app: &App) -> usize {
    let buffer = app_frame(app, WIDE);
    let marked = (0..WIDE.height)
        .find(|&y| row_text(&buffer, y).contains('['))
        .unwrap();
    let rows = bare_rows(&buffer);
    let name = rows[usize::from(marked)].split_whitespace().next().unwrap();
    ALL.iter().position(|kind| kind.name() == name).unwrap()
}

/// Given: a config that sets fire to 2×, and the picker marking fire.
/// When: the app ticks five times.
/// Then: the pane holds what fire draws alone after ten ticks.
///
/// Why it matters: a preview at the normal speed shows a scene slower than
/// the one Enter starts.
#[test]
fn the_pane_runs_the_scene_at_its_own_speed() {
    // Given:
    let mut config = Config::default();
    let fire = ALL
        .iter()
        .position(|kind| *kind == SceneKind::Fire)
        .unwrap();
    config.rates[fire] = Rate::from_times(2.0);
    let mut app = App::picker(7, Arc::new(Corpus::embedded())).with_config(&config);
    while app_selected(&app) != fire {
        app.select_next();
    }

    // When:
    for _ in 0..5 {
        app.tick(WIDE);
    }

    // Then:
    let shown = app_frame(&app, WIDE);
    let inner = pane_inner(&shown).unwrap();
    assert_same_inside(&shown, &alone(SceneKind::Fire, 10, inner), inner, "fire");
}

/// Given: the picker in a terminal 60 columns wide, and one 120 wide.
/// When: each renders.
/// Then: the narrow one has no pane and its list sits in the middle, and
/// the wide one has a pane clear of the list.
///
/// Why it matters: a pane squeezed into a narrow terminal cuts the list or
/// shows a scene too small to read.
#[test]
fn a_narrow_terminal_shows_the_list_alone() {
    // Given, when:
    let app = App::picker(7, Arc::new(Corpus::embedded()));
    let narrow = app_frame(&app, Rect::new(0, 0, 60, 30));
    let wide = app_frame(&app, WIDE);

    // Then:
    assert_eq!(pane_inner(&narrow), None);
    let top = (0..30)
        .map(|y| row_text(&narrow, y))
        .find(|row| row.contains(theme::PICKER_TITLE))
        .unwrap();
    let left = top.chars().take_while(|c| *c == ' ').count();
    let right = top.chars().rev().take_while(|c| *c == ' ').count();
    assert!(left.abs_diff(right) <= 1, "{top:?}");

    let inner = pane_inner(&wide).unwrap();
    let list_right = (0..30)
        .filter_map(|y| {
            row_text(&wide, y)
                .find('┐')
                .map(|byte| row_text(&wide, y)[..byte].chars().count())
        })
        .min()
        .unwrap();
    assert!(usize::from(inner.x) > list_right + 1, "pane over the list");
}

/// The column and row of the list frame's top-left corner.
fn list_corner(buffer: &Buffer) -> (u16, u16) {
    let lead = format!("┌{}", theme::PICKER_TITLE);
    (0..buffer.area.height)
        .find_map(|y| {
            let row = row_text(buffer, y);
            row.find(&lead)
                .map(|byte| (u16::try_from(row[..byte].chars().count()).unwrap(), y))
        })
        .expect("no list frame")
}

fn row_of(buffer: &Buffer, text: &str) -> Option<u16> {
    (0..buffer.area.height).find(|&y| row_text(buffer, y).contains(text))
}

/// Given: the picker at 120 by 30, with the preview, and at 60 by 30,
/// without it.
/// When: each renders.
/// Then: the art rows read in order from the list frame's left column,
/// braces first and the art in iris, and every art row is above the list
/// and the pane.
///
/// Why it matters: a mark that floats off the content column reads as a
/// separate thing, and one that overlaps the list hides a scene.
#[test]
fn the_mark_sits_above_the_content_at_its_left_edge() {
    let app = App::picker(7, Arc::new(Corpus::embedded()));
    for area in [WIDE, Rect::new(0, 0, 60, 30)] {
        // Given, when:
        let buffer = app_frame(&app, area);

        // Then:
        let (list_x, list_y) = list_corner(&buffer);
        let first = row_of(&buffer, mark::ART[0]).unwrap_or_else(|| panic!("no art at {area}"));
        for (offset, line) in (0..).zip(mark::ART) {
            let y = first + offset;
            assert!(row_text(&buffer, y).contains(line), "{area}: row {offset}");
        }
        assert_eq!(buffer[(list_x, first)].symbol(), "╭", "{area}");
        assert_eq!(buffer[(list_x + 2, first)].fg, theme::MARKED, "{area}");
        let last = first + u16::try_from(mark::ART.len()).unwrap() - 1;
        assert!(last < list_y, "{area}: art over the list");
        if let Some(inner) = pane_inner(&buffer) {
            assert!(last < inner.y - 1, "{area}: art over the pane");
        }
    }
}

/// Given: the picker 52 columns wide, too narrow for the art.
/// When: it renders.
/// Then: the braced word shows from the list frame's left column, and no
/// art row shows.
///
/// Why it matters: block art cut at the edge shreds into fragments that
/// read as noise, not as a name.
#[test]
fn a_narrow_terminal_shows_the_braced_word() {
    // Given, when:
    let app = App::picker(7, Arc::new(Corpus::embedded()));
    let buffer = app_frame(&app, Rect::new(0, 0, 52, 30));

    // Then:
    let (list_x, _) = list_corner(&buffer);
    let y = row_of(&buffer, mark::WORD).expect("no braced word");
    assert_eq!(buffer[(list_x, y)].symbol(), "{");
    assert_eq!(row_of(&buffer, mark::ART[0]), None);
}

/// Given: terminal heights from 4 to 40 rows, 120 columns wide.
/// When: the picker renders at each.
/// Then: a height that shows the art or the word shows every summary too,
/// some height shows the summaries with no mark, and at 30 rows the art
/// shows.
///
/// Why it matters: the mark is the brand and the summaries help choose a
/// scene, so a short terminal gives up the brand first.
#[test]
fn a_short_terminal_gives_up_the_mark_before_a_summary() {
    let app = App::picker(7, Arc::new(Corpus::embedded()));
    let mut summaries_alone = false;
    for height in 4..=40 {
        // Given, when:
        let buffer = app_frame(&app, Rect::new(0, 0, 120, height));

        // Then:
        let art = row_of(&buffer, mark::ART[0]).is_some();
        let word = row_of(&buffer, mark::WORD).is_some();
        let summaries = ALL
            .iter()
            .all(|kind| row_of(&buffer, kind.summary()).is_some());
        assert!(
            !(art || word) || summaries,
            "a mark without summaries at {height}"
        );
        summaries_alone |= summaries && !art && !word;
        if height == 30 {
            assert!(art, "no art at 30 rows");
        }
    }
    assert!(
        summaries_alone,
        "no height shows the summaries with no mark"
    );
}

/// Given: the picker 60 and 120 columns wide, from 10 to 40 rows high.
/// When: it renders at each size.
/// Then: the list frame's bottom-left corner is in the area, in the
/// column of its top-left corner, and a rule runs from it.
///
/// Why it matters: a list one row taller than its room is clipped, so its
/// bottom rule lands on the last summary and the key hint is lost.
#[test]
fn the_list_frame_stays_whole_at_every_height() {
    let app = App::picker(7, Arc::new(Corpus::embedded()));
    for width in [60, 120] {
        for height in 10..=40 {
            // Given, when:
            let buffer = app_frame(&app, Rect::new(0, 0, width, height));

            // Then:
            let (x, y) = list_corner(&buffer);
            let bottom = (y + 1..height)
                .find(|&b| buffer[(x, b)].symbol() != "│")
                .unwrap_or_else(|| panic!("no bottom rule at {width}x{height}"));
            assert_eq!(buffer[(x, bottom)].symbol(), "└", "{width}x{height}");
            assert_eq!(buffer[(x + 1, bottom)].symbol(), "─", "{width}x{height}");
        }
    }
}
