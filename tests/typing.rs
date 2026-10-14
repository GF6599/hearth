#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use hearth::corpus::Corpus;
use hearth::scene::{Rng, Scene};
use hearth::scenes::typing::Typing;
use hearth::theme;
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use unicode_width::UnicodeWidthStr;

const PROGRAM: &str = "fn main() {\n    println!(\"hi\");\n}\n";

fn corpus(text: &str) -> Arc<Corpus> {
    Arc::new(Corpus::from_files([("a.rs".to_string(), text.to_string())]))
}

fn scene(seed: u64, text: &str) -> Typing {
    Typing::new(Rng::seed_from_u64(seed), corpus(text))
}

fn ticked(mut scene: Typing, area: Rect, ticks: usize) -> Typing {
    for _ in 0..ticks {
        scene.tick(area);
    }
    scene
}

fn render(scene: &Typing, area: Rect, canvas: Rect) -> Buffer {
    let mut buffer = Buffer::empty(canvas);
    scene.render(area, &mut buffer);
    buffer
}

fn row_text(buffer: &Buffer, y: u16) -> String {
    (buffer.area.x..buffer.area.right())
        .map(|x| buffer[(x, y)].symbol().to_string())
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// The typed text under the path header, as the rows read.
fn typed(buffer: &Buffer, area: Rect) -> String {
    (area.y + 1..area.bottom())
        .map(|y| row_text(buffer, y))
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

fn cursors(buffer: &Buffer) -> Vec<Position> {
    buffer
        .area
        .positions()
        .filter(|&p| buffer[p].bg == theme::TEXT)
        .collect()
}

/// Given: two scenes on one seed over one file.
/// When: each ticks eight times.
/// Then: the frames are equal, the header names the file, and the rows
/// under it hold a non-empty prefix of the file.
///
/// Why it matters: a replay with `--seed` has to type the same text, and
/// typed text that is not a prefix of the source is a scene showing
/// something that is not in the repo.
#[test]
fn a_seed_types_the_same_prefix_of_the_file() {
    // Given, when:
    let area = Rect::new(0, 0, 30, 10);
    let first = render(&ticked(scene(7, PROGRAM), area, 8), area, area);
    let second = render(&ticked(scene(7, PROGRAM), area, 8), area, area);

    // Then:
    assert_eq!(first, second);
    assert_eq!(row_text(&first, 0), "a.rs");
    let text = typed(&first, area);
    assert!(!text.is_empty());
    assert!(PROGRAM.starts_with(&text), "{text:?} is not a prefix");
}

/// Given: a file of long lines, on an eight by three area inside a larger
/// canvas.
/// When: the scene ticks two hundred times, with a render after each.
/// Then: every frame shows exactly one cursor, inside the area, and no
/// cell outside the area is touched.
#[test]
fn the_cursor_never_leaves_the_area() {
    let canvas = Rect::new(0, 0, 14, 9);
    let area = Rect::new(2, 2, 8, 3);
    let long = "let value = compute(alpha, beta, gamma, delta);\n".repeat(12);
    let mut scene = scene(7, &long);
    for tick in 0..200 {
        scene.tick(area);
        let buffer = render(&scene, area, canvas);
        let cursors = cursors(&buffer);
        assert_eq!(cursors.len(), 1, "tick {tick}: {cursors:?}");
        assert!(area.contains(cursors[0]), "tick {tick}: {cursors:?}");
        for position in canvas.positions().filter(|p| !area.contains(*p)) {
            assert_eq!(
                buffer[position].symbol(),
                " ",
                "tick {tick} at {position:?}"
            );
        }
    }
}

/// Given: a file whose one line holds two-cell glyphs.
/// When: the scene types the whole line.
/// Then: the row reads the line without those glyphs, every cell is one
/// column wide, and the cursor sits right after the last glyph.
///
/// Why it matters: a two-cell glyph pushes every cell after it one column
/// right, and the terminal and the buffer then disagree on where the
/// cursor is.
#[test]
fn a_wide_glyph_is_dropped_so_the_row_does_not_tear() {
    // Given, when:
    let area = Rect::new(0, 0, 30, 4);
    let scene = ticked(scene(7, "let s = \"日本\"; // ok\n"), area, 60);
    let buffer = render(&scene, area, area);

    // Then:
    let row = row_text(&buffer, 1);
    assert_eq!(row, "let s = \"\"; // ok");
    for x in 0..area.width {
        assert!(buffer[(x, 1)].symbol().width() <= 1, "wide cell at {x}");
    }
    let expected = u16::try_from(row.len()).unwrap();
    assert!(
        cursors(&buffer).iter().any(|c| c.y == 1 && c.x == expected)
            || cursors(&buffer).iter().any(|c| c.y == 2 && c.x == 0),
        "cursor not after the line: {:?}",
        cursors(&buffer)
    );
}
