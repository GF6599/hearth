#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::HashSet;
use std::sync::Arc;

use hearth::corpus::Corpus;
use hearth::scene::{Rng, Scene, TICK_RATE};
use hearth::scenes::typing::{MOST_SNIPPET_LINES, Speed, Typing};
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
    Typing::new(Rng::seed_from_u64(seed), corpus(text), Speed::DEFAULT)
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

/// Ticks until the frame holds still for twelve ticks, which only the hold
/// after a finished snippet does, and returns that frame.
fn settled(scene: &mut Typing, area: Rect) -> Buffer {
    let mut last = render(scene, area, area);
    let mut still = 0;
    for _ in 0..5000 {
        scene.tick(area);
        let now = render(scene, area, area);
        if now == last {
            still += 1;
            if still == 12 {
                return now;
            }
        } else {
            still = 0;
            last = now;
        }
    }
    panic!("the frame never held still");
}

/// Ticks past the hold of the snippet on screen, then settles on the next.
fn next_snippet(scene: &mut Typing, area: Rect) -> Buffer {
    let held = render(scene, area, area);
    while render(scene, area, area) == held {
        scene.tick(area);
    }
    settled(scene, area)
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
    assert_eq!(row_text(&first, 0), "a.rs:1");
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
    let long = format!(
        "fn f() {{\n{}}}\n",
        "    let value = compute(alpha, beta, gamma, delta);\n".repeat(12)
    );
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
/// When: the scene has typed the whole line.
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
    let buffer = settled(&mut scene(7, "fn f() { let s = \"日本\"; } // ok\n"), area);

    // Then:
    let row = row_text(&buffer, 1);
    assert_eq!(row, "fn f() { let s = \"\"; } // ok");
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

/// Given: a file of one long line, so no newline pauses the typing.
/// When: the scene ticks for three seconds of wall time at 50 and at 100
/// keystrokes per second.
/// Then: it types about 150 and about 300 glyphs.
///
/// Why it matters: the speed is a promise in keystrokes per second, so
/// the count follows the clock and not the number of ticks.
#[test]
fn the_scene_types_at_the_speed_it_is_given() {
    let area = Rect::new(0, 0, 400, 3);
    let ticks = (3.0 / TICK_RATE.as_secs_f64()).round() as usize;
    for (speed, expected) in [("50", 150), ("100", 300)] {
        // Given:
        let scene = Typing::new(
            Rng::seed_from_u64(7),
            corpus(&format!("fn f() {{ {} }}\n", "x".repeat(1000))),
            speed.parse().unwrap(),
        );

        // When:
        let buffer = render(&ticked(scene, area, ticks), area, area);

        // Then:
        let count = row_text(&buffer, 1).len();
        assert!(
            count.abs_diff(expected) <= expected / 10,
            "{count} glyphs at {speed} keystrokes per second"
        );
    }
}

/// A file typed at 200 keystrokes per second for ten ticks: between 60 and
/// 180 keystrokes, which types a short file and stays inside the hold.
fn typed_file(path: &str, text: &str, area: Rect) -> Buffer {
    let scene = Typing::new(
        Rng::seed_from_u64(7),
        Arc::new(Corpus::from_files([(path.to_string(), text.to_string())])),
        "200".parse().unwrap(),
    );
    render(&ticked(scene, area, 10), area, area)
}

fn colour_at(buffer: &Buffer, line: &str, needle: &str, y: u16) -> ratatui::style::Color {
    let x = u16::try_from(line.find(needle).unwrap()).unwrap();
    buffer[(x, y)].fg
}

/// Given: a Rust file with a keyword, a function name, a string and a
/// comment on one line.
/// When: the scene has typed it.
/// Then: each takes its colour from the Rust grammar.
///
/// Why it matters: colour by line, as before, paints a trailing comment
/// in the code colour and a string in the comment colour.
#[test]
fn rust_is_highlighted_by_its_grammar() {
    // Given:
    let line = "fn main() { let s = \"x\"; } // c";
    let area = Rect::new(0, 0, 40, 4);

    // When:
    let buffer = typed_file("a.rs", &format!("{line}\n"), area);

    // Then:
    assert_eq!(row_text(&buffer, 1), line);
    assert_eq!(colour_at(&buffer, line, "fn", 1), theme::PINE);
    assert_eq!(colour_at(&buffer, line, "main", 1), theme::ROSE);
    assert_eq!(colour_at(&buffer, line, "x\"", 1), theme::GOLD);
    assert_eq!(colour_at(&buffer, line, "//", 1), theme::MUTED);
}

/// Given: a Python file.
/// When: the scene has typed it.
/// Then: `def` is a keyword and the function name is a function, so the
/// grammar came from the extension and not from Rust.
#[test]
fn the_grammar_follows_the_extension() {
    let line = "def greet():";
    let area = Rect::new(0, 0, 40, 4);
    let buffer = typed_file("a.py", &format!("{line}\n    pass\n"), area);
    assert_eq!(colour_at(&buffer, line, "def", 1), theme::PINE);
    assert_eq!(colour_at(&buffer, line, "greet", 1), theme::ROSE);
}

/// Given: a file no grammar claims, beside a Rust file.
/// When: the scene types its first snippet, on several seeds.
/// Then: every snippet comes from the Rust file.
///
/// Why it matters: a file with no grammar has no function to start from,
/// and typing it from the top is the behaviour this scene replaced.
#[test]
fn a_file_with_no_function_is_skipped() {
    let area = Rect::new(0, 0, 40, 6);
    let files = [
        ("notes.zzz".to_string(), "fn x() {}\n".to_string()),
        ("a.rs".to_string(), "fn y() {}\n".to_string()),
    ];
    for seed in 0..8 {
        let mut scene = Typing::new(
            Rng::seed_from_u64(seed),
            Arc::new(Corpus::from_files(files.clone())),
            Speed::DEFAULT,
        );
        let buffer = settled(&mut scene, area);
        assert_eq!(row_text(&buffer, 0), "a.rs:1", "seed {seed}");
    }
}

const TWO_FUNCTIONS: &str = "//! A header.\nuse std::fmt;\n\n/// Adds one.\nfn add(a: u32) -> u32 {\n    a + 1\n}\n\nfn other() {}\n";

/// Given: a file with a header comment, an import, a documented function
/// and a one-line function.
/// When: the scene types its first snippet, on several seeds.
/// Then: each snippet is one whole function with its doc comment, the
/// header names the line it starts on, and both functions come up.
///
/// Why it matters: a snippet that starts at the top of the file types
/// imports, and one that stops early or late cuts a function in half or
/// runs into the next.
#[test]
fn a_snippet_is_one_function_with_its_doc_comment() {
    let area = Rect::new(0, 0, 40, 12);
    let mut seen = HashSet::new();
    for seed in 0..12 {
        // Given:
        let mut scene = scene(seed, TWO_FUNCTIONS);

        // When:
        let buffer = settled(&mut scene, area);

        // Then:
        seen.insert((row_text(&buffer, 0), typed(&buffer, area)));
    }
    let expected = HashSet::from([
        (
            "a.rs:4".to_string(),
            "/// Adds one.\nfn add(a: u32) -> u32 {\n    a + 1\n}".to_string(),
        ),
        ("a.rs:9".to_string(), "fn other() {}".to_string()),
    ]);
    assert_eq!(seen, expected);
}

/// Given: a Python function followed by a statement at the top level.
/// When: the scene types the snippet.
/// Then: the snippet stops before the statement, because Python closes a
/// block by its indentation and has no closing line.
#[test]
fn a_python_snippet_ends_where_the_indentation_does() {
    let area = Rect::new(0, 0, 40, 8);
    let file = "def greet():\n    print('hi')\n\nvalue = 1\n";
    let mut scene = Typing::new(
        Rng::seed_from_u64(7),
        Arc::new(Corpus::from_files([("a.py".to_string(), file.to_string())])),
        Speed::DEFAULT,
    );
    let buffer = settled(&mut scene, area);
    assert_eq!(typed(&buffer, area), "def greet():\n    print('hi')");
}

/// Given: a method inside an `impl` block.
/// When: the scene types the snippet.
/// Then: the snippet is the method at its own indentation, without the
/// `impl` line and the brace that closes the block.
#[test]
fn a_nested_function_is_its_own_snippet() {
    let area = Rect::new(0, 0, 40, 8);
    let file = "struct S;\n\nimpl S {\n    fn a(&self) -> u8 {\n        1\n    }\n}\n";
    let buffer = settled(&mut scene(7, file), area);
    assert_eq!(
        typed(&buffer, area),
        "    fn a(&self) -> u8 {\n        1\n    }"
    );
}

/// Given: a function longer than the snippet cap.
/// When: the scene types the snippet on an area tall enough for all of it.
/// Then: it types the cap's number of lines and stops.
#[test]
fn a_long_function_is_cut_at_the_cap() {
    let area = Rect::new(0, 0, 30, 80);
    let file = format!("fn long() {{\n{}}}\n", "    x();\n".repeat(60));
    let buffer = settled(&mut scene(7, &file), area);
    assert_eq!(typed(&buffer, area).lines().count(), MOST_SNIPPET_LINES);
}

/// Given: a file with two functions.
/// When: the scene types one snippet, holds it, and types the next.
/// Then: the second is the other function, on every seed.
#[test]
fn a_snippet_does_not_come_twice_in_a_row() {
    let area = Rect::new(0, 0, 40, 12);
    for seed in 0..6 {
        let mut scene = scene(seed, TWO_FUNCTIONS);
        let first = row_text(&settled(&mut scene, area), 0);
        let second = row_text(&next_snippet(&mut scene, area), 0);
        assert_ne!(first, second, "seed {seed}");
    }
}
