#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use hearth::app::App;
use hearth::config::Config;
use hearth::corpus::Corpus;
use hearth::mark;
use hearth::picker::{Entry, LEAST_SCENES_IN_VIEW, Picker};
use hearth::scene::{ALL, Rate, SceneKind};
use hearth::theme;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const ENTRIES: &[Entry] = &[
    Entry {
        name: "noise",
        summary: "random glyphs",
        speed: "1×",
        credit: None,
    },
    Entry {
        name: "matrix",
        summary: "katakana rain",
        speed: "2×",
        credit: Some("after cmatrix by Chris Allegretta"),
    },
];

fn render(selected: usize, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| {
            f.render_widget(
                Picker {
                    palette: &theme::ROSE_PINE,
                    entries: ENTRIES,
                    selected,
                    top: 0,
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
    assert_eq!(buffer[(x - 1, y)].fg, theme::ROSE_PINE.marker());
    assert_eq!(buffer[(after, y)].fg, theme::ROSE_PINE.marker());
    assert_eq!(buffer[(x, y)].fg, theme::ROSE_PINE.marked());
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
    assert_eq!(buffer[(x, y)].fg, theme::ROSE_PINE.text);
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
        assert_eq!(buffer[(x, y + 1)].fg, theme::ROSE_PINE.subtle);
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
    let tall = u16::try_from(ALL.len() * 2 + mark::ART.len() + 5).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(100, tall)).unwrap();
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
        assert_eq!(buffer[(x, y)].fg, theme::ROSE_PINE.subtle);
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
            assert_eq!(buffer[(x, y)].fg, theme::ROSE_PINE.subtle, "{width}");
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
        select(&mut app, kind);
        for _ in 0..5 {
            app.tick(WIDE);
        }

        // Then:
        let shown = app_frame(&app, WIDE);
        let inner = pane_inner(&shown).unwrap_or_else(|| panic!("no pane for {kind:?}"));
        assert_same_inside(&shown, &alone(kind, 5, inner), inner, kind.name());
    }
}

/// Moves the mark down to `kind` in one pass of the list at most, so an
/// app whose mark does not arrive fails the test and does not hang it.
fn select(app: &mut App, kind: SceneKind) {
    for _ in 0..ALL.len() {
        if ALL[app_selected(app)] == kind {
            return;
        }
        app.select_next();
    }
    panic!("the mark never reached {kind:?}");
}

/// The index of the marked row, read from the columns left of the pane,
/// because a scene in the pane can draw a `[` of its own.
fn app_selected(app: &App) -> usize {
    let buffer = app_frame(app, WIDE);
    let end = pane_inner(&buffer).map_or(WIDE.width, |inner| inner.x - 1);
    let list = |y: u16| {
        (0..end)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
    };
    let marked = (0..WIDE.height).find(|&y| list(y).contains('[')).unwrap();
    let row = list(marked);
    let name = row
        .split(|c: char| c.is_whitespace() || "[]│".contains(c))
        .find(|word| !word.is_empty())
        .unwrap();
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
    select(&mut app, SceneKind::Fire);

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

/// The braces of the art, row by row, drawn in the art's own style.
const LEFT_BRACE: [&str; 6] = [" ███╗", " ██╔╝", "██╔╝ ", "╚██╗ ", " ███╗", " ╚══╝"];
const RIGHT_BRACE: [&str; 6] = ["███╗ ", "╚██╗ ", " ╚██╗", " ██╔╝", "███╔╝", "╚══╝ "];

/// Given: the picker at 120 by 30, with the preview, and at 80 by 30,
/// without it.
/// When: each renders.
/// Then: from the list frame's left column each art row reads as a block
/// brace, a blank, the art row, a blank and the closing brace, the braces
/// in gold, and every art row is above the list and the pane.
///
/// Why it matters: a mark that floats off the content column reads as a
/// separate thing, one that overlaps the list hides a scene, and a thin
/// brace beside block letters reads as faint.
#[test]
fn the_mark_sits_above_the_content_at_its_left_edge() {
    let app = App::picker(7, Arc::new(Corpus::embedded()));
    let width = mark::ART
        .iter()
        .map(|row| row.chars().count())
        .max()
        .unwrap();
    for area in [WIDE, Rect::new(0, 0, 80, 30)] {
        // Given, when:
        let buffer = app_frame(&app, area);

        // Then:
        let (list_x, list_y) = list_corner(&buffer);
        let first = row_of(&buffer, mark::ART[0]).unwrap_or_else(|| panic!("no art at {area}"));
        for (offset, line) in (0..).zip(mark::ART) {
            let y = first + offset;
            let row = usize::from(offset);
            let expected = format!("{} {line:<width$} {}", LEFT_BRACE[row], RIGHT_BRACE[row]);
            let shown: String = row_text(&buffer, y)
                .chars()
                .skip(usize::from(list_x))
                .take(expected.chars().count())
                .collect();
            assert_eq!(shown, expected, "{area}: row {offset}");
            let right = list_x + u16::try_from(expected.chars().count() - 5).unwrap();
            for (x, glyph) in (list_x..)
                .zip(LEFT_BRACE[row].chars())
                .chain((right..).zip(RIGHT_BRACE[row].chars()))
            {
                if glyph != ' ' {
                    assert_eq!(
                        buffer[(x, y)].fg,
                        theme::ROSE_PINE.marker(),
                        "{area}: brace at {x}, {y}"
                    );
                }
            }
        }
        let last = first + u16::try_from(mark::ART.len()).unwrap() - 1;
        assert!(last < list_y, "{area}: art over the list");
        if let Some(inner) = pane_inner(&buffer) {
            assert!(last < inner.y - 1, "{area}: art over the pane");
        }
    }
}

/// Given: the picker 60 columns wide, too narrow for the art and its
/// block braces.
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
    let buffer = app_frame(&app, Rect::new(0, 0, 60, 30));

    // Then:
    let (list_x, _) = list_corner(&buffer);
    let y = row_of(&buffer, mark::WORD).expect("no braced word");
    assert_eq!(buffer[(list_x, y)].symbol(), "{");
    assert_eq!(row_of(&buffer, mark::ART[0]), None);
}

/// The colour of each art column that has ink, read from the frame, with
/// the colours of the brace cells beside them.
fn mark_colours(buffer: &Buffer) -> (Vec<Vec<Color>>, Vec<Color>) {
    let (list_x, _) = list_corner(buffer);
    let first = row_of(buffer, mark::ART[0]).expect("no art");
    let art_x = list_x + u16::try_from(LEFT_BRACE[0].chars().count() + 1).unwrap();
    let width = mark::ART
        .iter()
        .map(|row| row.chars().count())
        .max()
        .unwrap();
    let columns = (0..width)
        .map(|column| {
            let x = art_x + u16::try_from(column).unwrap();
            (first..first + 6)
                .filter(|&y| buffer[(x, y)].symbol() != " ")
                .map(|y| buffer[(x, y)].fg)
                .collect()
        })
        .collect();
    let right_x = art_x + u16::try_from(width + 1).unwrap();
    let braces = (first..first + 6)
        .flat_map(|y| {
            (list_x..art_x - 1)
                .chain(right_x..right_x + 5)
                .map(move |x| (x, y))
        })
        .filter(|&position| buffer[position].symbol() != " ")
        .map(|position| buffer[position].fg)
        .collect();
    (columns, braces)
}

/// Given: the picker at 120 by 30.
/// When: it ticks through one `GLOW_TICKS` period, with a frame after each
/// tick.
/// Then: each art column is one colour from top to bottom, every colour is
/// iris, rose or love and each of the three shows, on every frame at least
/// one column glows and at least one rests in iris, and the braces stay
/// gold.
///
/// Why it matters: the title is a hearth, so it glows like embers, and
/// embers glow out of step. A title that pulses as one block reads as a
/// warning light.
#[test]
fn the_title_glows_like_embers_out_of_step() {
    // Given:
    let mut app = App::picker(7, Arc::new(Corpus::embedded()));
    let rest = theme::ROSE_PINE.marked();
    let glow = [rest, theme::ROSE_PINE.rose, theme::ROSE_PINE.love];

    let mut seen = Vec::new();
    for tick in 0..mark::GLOW_TICKS {
        // When:
        app.tick(WIDE);
        let (columns, braces) = mark_colours(&app_frame(&app, WIDE));

        // Then:
        let mut resting = false;
        let mut glowing = false;
        for (column, colours) in columns.iter().enumerate() {
            let Some(&colour) = colours.first() else {
                continue;
            };
            assert!(
                colours.iter().all(|c| *c == colour),
                "tick {tick}: column {column} {colours:?}"
            );
            assert!(
                glow.contains(&colour),
                "tick {tick}: column {column} {colour:?}"
            );
            resting |= colour == rest;
            glowing |= colour != rest;
            if !seen.contains(&colour) {
                seen.push(colour);
            }
        }
        assert!(resting && glowing, "tick {tick}: the columns glow in step");
        assert!(
            braces.iter().all(|c| *c == theme::ROSE_PINE.marker()),
            "tick {tick}: {braces:?}"
        );
    }
    assert_eq!(seen.len(), glow.len(), "{seen:?}");
}

/// Given: the picker 60 columns wide, which shows the braced word.
/// When: it renders, ticks through one `GLOW_TICKS` period, and renders
/// again at every tick.
/// Then: every frame shows the word in the same colours as the first.
///
/// Why it matters: the word is the fallback for a cramped terminal, and a
/// few glowing letters there read as flicker, not as embers.
#[test]
fn the_braced_word_stays_still() {
    // Given:
    let area = Rect::new(0, 0, 60, 30);
    let mut app = App::picker(7, Arc::new(Corpus::embedded()));
    let word = |buffer: &Buffer| {
        let (list_x, _) = list_corner(buffer);
        let y = row_of(buffer, mark::WORD).expect("no braced word");
        (list_x..list_x + u16::try_from(mark::WORD.len()).unwrap())
            .map(|x| buffer[(x, y)].fg)
            .collect::<Vec<Color>>()
    };
    let still = word(&app_frame(&app, area));

    for tick in 0..mark::GLOW_TICKS {
        // When:
        app.tick(area);

        // Then:
        assert_eq!(word(&app_frame(&app, area)), still, "tick {tick}");
    }
}

/// The scenes in the list of an app frame, top first, each with whether
/// its summary is on the row below. The pipes summary starts with the
/// name pipes, so a row that is a whole summary is never a name row.
fn in_view(buffer: &Buffer) -> Vec<(SceneKind, bool)> {
    let end = pane_inner(buffer).map_or(buffer.area.width, |inner| inner.x - 1);
    let rows: Vec<String> = (0..buffer.area.height)
        .map(|y| {
            (0..end)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .replace(['[', ']'], " ")
                .trim_matches(|c| matches!(c, ' ' | '│' | '┃'))
                .to_string()
        })
        .collect();
    rows.iter()
        .enumerate()
        .filter(|(_, row)| !ALL.iter().any(|kind| kind.summary() == row.as_str()))
        .filter_map(|(y, row)| {
            let kind = SceneKind::from_name(row.split_whitespace().next()?)?;
            let summary = rows.get(y + 1).is_some_and(|below| below == kind.summary());
            Some((kind, summary))
        })
        .collect()
}

/// Given: terminal heights from 4 to 40 rows, 120 columns wide.
/// When: the picker renders at each.
/// Then: a height that shows the art or the word shows at least
/// `LEAST_SCENES_IN_VIEW` scenes, each with its summary, some height shows
/// that many with no mark, and at 30 rows the art shows.
///
/// Why it matters: the mark is the brand and the list is where a scene is
/// chosen, so the brand takes rows only while the list keeps a useful
/// share of the scenes.
#[test]
fn a_short_terminal_gives_up_the_mark_before_the_summaries() {
    let app = App::picker(7, Arc::new(Corpus::embedded()));
    let least = LEAST_SCENES_IN_VIEW.min(ALL.len());
    let mut summaries_alone = false;
    for height in 4..=40 {
        // Given, when:
        let buffer = app_frame(&app, Rect::new(0, 0, 120, height));

        // Then:
        let art = row_of(&buffer, mark::ART[0]).is_some();
        let word = row_of(&buffer, mark::WORD).is_some();
        let shown = in_view(&buffer);
        let summaries = shown.len() >= least && shown.iter().all(|(_, summary)| *summary);
        assert!(
            !(art || word) || summaries,
            "a mark over {shown:?} at {height}"
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

/// Given: the picker 120 columns wide, at a height that shows a share of
/// the scenes with their summaries, and at one that shows names only.
/// When: the mark moves down the list one scene at a time.
/// Then: at each step the marked scene is in view, and so is the scene
/// after it, and at the end the first scene is out of view.
///
/// Why it matters: a mark that moves past the bottom of the frame marks a
/// scene the user cannot see, and Enter starts a scene by surprise.
#[test]
fn the_list_scrolls_to_keep_the_marked_scene_in_view() {
    for height in [16, 8] {
        // Given:
        let area = Rect::new(0, 0, 120, height);
        let mut app = App::picker(7, Arc::new(Corpus::embedded()));
        let shown = in_view(&app_frame(&app, area));
        assert!(shown.len() < ALL.len(), "every scene fits at {height}");

        for (index, kind) in ALL.iter().enumerate() {
            // When:
            let buffer = app_frame(&app, area);

            // Then:
            let shown: Vec<SceneKind> = in_view(&buffer).into_iter().map(|(k, _)| k).collect();
            let marked = format!("[{}]", kind.name());
            assert!(
                row_of(&buffer, &marked).is_some(),
                "{height}: {marked} in {shown:?}"
            );
            if let Some(next) = ALL.get(index + 1) {
                assert!(shown.contains(next), "{height}: {next:?} in {shown:?}");
            }
            if index + 1 == ALL.len() {
                assert!(!shown.contains(&ALL[0]), "{height}: {shown:?}");
            }
            app.select_next();
            app.tick(area);
        }
    }
}

/// Given: the picker 120 by 16, scrolled to the last scene.
/// When: the mark moves up one scene at a time.
/// Then: the list holds still while the mark stays below the top scene
/// in view, and scrolls one scene when the mark reaches it.
///
/// Why it matters: a list that scrolls on every key pins the mark to one
/// row, and then the list moves while the mark appears to stand still.
#[test]
fn the_list_holds_still_while_the_mark_moves_inside_it() {
    // Given:
    let area = Rect::new(0, 0, 120, 16);
    let mut app = App::picker(7, Arc::new(Corpus::embedded()));
    for _ in 1..ALL.len() {
        app.select_next();
        app.tick(area);
    }
    let at_end = in_view(&app_frame(&app, area));
    let top = ALL.iter().position(|kind| *kind == at_end[0].0).unwrap();
    assert!(top > 0, "the list did not scroll: {at_end:?}");

    // When, then:
    for selected in (top + 1..ALL.len() - 1).rev() {
        app.select_previous();
        app.tick(area);
        assert_eq!(in_view(&app_frame(&app, area)), at_end, "at {selected}");
    }
    app.select_previous();
    app.tick(area);
    assert_eq!(in_view(&app_frame(&app, area))[0].0, ALL[top - 1]);
}

/// The right rule of the list frame between its corners: the rows inside
/// the frame, and the rows of them that do not show the plain rule.
fn right_rule(buffer: &Buffer) -> (Vec<u16>, Vec<u16>) {
    let (left, top) = list_corner(buffer);
    let right = (left + 1..buffer.area.width)
        .find(|&x| buffer[(x, top)].symbol() == "┐")
        .expect("no top-right corner");
    let bottom = (top + 1..buffer.area.height)
        .find(|&y| buffer[(right, y)].symbol() == "┘")
        .expect("no bottom-right corner");
    let inside: Vec<u16> = (top + 1..bottom).collect();
    let thumb = inside
        .iter()
        .copied()
        .filter(|&y| buffer[(right, y)].symbol() != "│")
        .inspect(|&y| assert_eq!(buffer[(right, y)].fg, theme::ROSE_PINE.subtle, "row {y}"))
        .collect();
    (inside, thumb)
}

/// Given: the picker 120 by 16, which shows a share of the scenes.
/// When: it renders with the first scene marked, and again with the mark
/// moved to the last.
/// Then: a thumb in subtle on the list's right rule, shorter than the
/// rule, starts on the first row inside the frame, and at the end it stops
/// on the last row.
///
/// Why it matters: a list cut at its frame looks like the whole list, so
/// without the thumb nobody knows that more scenes are below.
#[test]
fn a_thumb_on_the_right_rule_shows_where_the_list_is() {
    // Given:
    let area = Rect::new(0, 0, 120, 16);
    let mut app = App::picker(7, Arc::new(Corpus::embedded()));

    // When:
    let (inside, at_start) = right_rule(&app_frame(&app, area));
    for _ in 1..ALL.len() {
        app.select_next();
        app.tick(area);
    }
    let (_, at_end) = right_rule(&app_frame(&app, area));

    // Then:
    for thumb in [&at_start, &at_end] {
        assert!(
            !thumb.is_empty() && thumb.len() < inside.len(),
            "{thumb:?} in {inside:?}"
        );
        assert!(
            thumb.windows(2).all(|pair| pair[1] == pair[0] + 1),
            "{thumb:?}"
        );
    }
    assert_eq!(at_start.first(), inside.first());
    assert_eq!(at_end.last(), inside.last());
}

/// Given: the picker in a terminal tall enough for every scene.
/// When: it renders.
/// Then: the list's right rule is the plain rule from corner to corner.
///
/// Why it matters: a thumb that fills the whole rule says nothing, and it
/// reads as a heavier border on one side.
#[test]
fn a_list_that_shows_every_scene_has_no_thumb() {
    // Given:
    let tall = u16::try_from(ALL.len() * 2 + mark::ART.len() + 5).unwrap();
    let app = App::picker(7, Arc::new(Corpus::embedded()));

    // When:
    let (_, thumb) = right_rule(&app_frame(&app, Rect::new(0, 0, 120, tall)));

    // Then:
    assert_eq!(thumb, Vec::<u16>::new());
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

/// Given: the picker 120 columns wide, from 14 to 50 rows high, with the
/// first scene marked and then the last.
/// When: it renders at each height that shows the preview pane.
/// Then: the list frame's top and bottom rules are on the rows of the
/// pane's top and bottom rules.
///
/// Why it matters: the list and the pane read as one band, and a list that
/// stops short of the pane looks cut off.
#[test]
fn the_list_and_the_pane_are_one_height() {
    // Given:
    let first = App::picker(7, Arc::new(Corpus::embedded()));
    let mut last = App::picker(7, Arc::new(Corpus::embedded()));
    for _ in 1..ALL.len() {
        last.select_next();
    }

    let mut shown = 0;
    for height in 14..=50 {
        for app in [&first, &last] {
            // When:
            let buffer = app_frame(app, Rect::new(0, 0, 120, height));
            let Some(inner) = pane_inner(&buffer) else {
                continue;
            };
            shown += 1;

            // Then:
            let (x, top) = list_corner(&buffer);
            let bottom = (top + 1..height)
                .find(|&y| buffer[(x, y)].symbol() == "└")
                .unwrap_or_else(|| panic!("no bottom rule at {height}"));
            assert_eq!((top, bottom), (inner.y - 1, inner.bottom()), "{height}");
        }
    }
    assert!(shown > 0, "no height shows the pane");
}

/// The bottom rule of the preview pane, from corner to corner.
fn pane_bottom(buffer: &Buffer, inner: Rect) -> String {
    (inner.left() - 1..=inner.right())
        .map(|x| buffer[(x, inner.bottom())].symbol())
        .collect()
}

/// Given: the picker marking a scene with a credit, at widths from 40 to
/// 160 columns.
/// When: it renders at each.
/// Then: wherever the preview pane shows, its bottom rule holds the whole
/// credit, in the subtle colour, or none of it, and at 120 columns it holds
/// the credit.
///
/// Why it matters: a credit cut at the corner names a work or a person
/// wrongly, and a credit that breaks the rule leaves a gap in the frame.
#[test]
fn the_preview_rule_holds_the_whole_credit_or_none() {
    let credit = ENTRIES[1].credit.unwrap();
    for width in 40..=160 {
        // Given, when:
        let buffer = render(1, width, 20);
        let Some(inner) = pane_inner(&buffer) else {
            continue;
        };

        // Then:
        let bottom = pane_bottom(&buffer, inner);
        let cut = format!("─ {credit} ");
        let shown = bottom.contains(&cut);
        let rule = bottom.replacen(&cut, "", 1);
        assert!(
            rule.starts_with('└') && rule.ends_with('┘'),
            "no corners at width {width}: {bottom:?}"
        );
        assert!(
            rule.trim_matches(|c| c == '└' || c == '┘')
                .chars()
                .all(|c| c == '─'),
            "part of the credit at width {width}: {bottom:?}"
        );
        if shown {
            let x = inner.left() + 2;
            assert_eq!(buffer[(x, inner.bottom())].symbol(), "a", "{width}");
            assert_eq!(
                buffer[(x, inner.bottom())].fg,
                theme::ROSE_PINE.subtle,
                "{width}"
            );
        }
        if width == 120 {
            assert!(shown, "{bottom:?}");
        }
    }
}

/// Given: the picker in a wide terminal.
/// When: the mark moves down the list, one scene at a time.
/// Then: the preview's bottom rule holds the marked scene's credit, the
/// typing scene credits gittype, and a scene with no credit has a plain
/// rule.
///
/// Why it matters: each scene reuses an idea from another program or
/// person, and the picker is where a user meets the scene first.
#[test]
fn the_preview_credits_the_marked_scene() {
    // Given:
    let mut app = App::picker(7, Arc::new(Corpus::embedded()));

    for &kind in ALL {
        // When:
        select(&mut app, kind);

        // Then:
        let shown = app_frame(&app, WIDE);
        let inner = pane_inner(&shown).unwrap_or_else(|| panic!("no pane for {kind:?}"));
        let bottom = pane_bottom(&shown, inner);
        match kind.credit() {
            Some(credit) => assert!(bottom.contains(&format!("─ {credit} ")), "{bottom:?}"),
            None => assert!(
                bottom
                    .trim_matches(|c| c == '└' || c == '┘')
                    .chars()
                    .all(|c| c == '─'),
                "{kind:?}: {bottom:?}"
            ),
        }
    }
    assert!(
        SceneKind::Typing
            .credit()
            .is_some_and(|credit| credit.contains("gittype"))
    );
}
