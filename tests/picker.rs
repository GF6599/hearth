#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use hearth::picker::Picker;
use hearth::theme;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;

const LABELS: &[&str] = &["noise", "matrix"];

fn render(selected: usize, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| {
            f.render_widget(
                Picker {
                    labels: LABELS,
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
            row_text(buffer, y)
                .find(label)
                .map(|x| (u16::try_from(x).unwrap(), y))
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
