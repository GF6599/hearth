#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use hearth::app::App;
use hearth::corpus::Corpus;
use hearth::picker::{Entry, Picker};
use hearth::scene::ALL;
use hearth::theme;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

const ENTRIES: &[Entry] = &[
    Entry {
        name: "noise",
        summary: "random glyphs",
    },
    Entry {
        name: "matrix",
        summary: "katakana rain",
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
/// padding cut off, so a name row reads as the bare name.
fn bare_rows(buffer: &Buffer) -> Vec<String> {
    (0..buffer.area.height)
        .map(|y| {
            row_text(buffer, y)
                .trim_matches(|c| matches!(c, ' ' | '│' | '[' | ']'))
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
            .position(|row| row == kind.name())
            .unwrap_or_else(|| panic!("no row for {}: {rows:#?}", kind.name()));
        assert_eq!(rows[at + 1], kind.summary(), "{rows:#?}");
    }
}
