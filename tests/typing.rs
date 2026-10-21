#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::collections::HashSet;
use std::fs;
use std::sync::Arc;

use clap::Parser;
use hearth::app::App;
use hearth::cli::Cli;
use hearth::config::Config;
use hearth::corpus::Corpus;
use hearth::scene::{FRAME_RATE, Rng, Scene};
use hearth::scenes::typing::{FINDING, MOST_SNIPPET_LINES, Shortest, Speed, Typing};
use hearth::theme;
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;
use unicode_width::UnicodeWidthStr;

const PROGRAM: &str = "fn main() {\n    println!(\"hi\");\n}\n";

fn corpus(text: &str) -> Arc<Corpus> {
    Arc::new(Corpus::from_files([("a.rs".to_string(), text.to_string())]))
}

fn scene(seed: u64, text: &str) -> Typing {
    Typing::new(
        Rng::seed_from_u64(seed),
        &theme::ROSE_PINE,
        corpus(text),
        Speed::DEFAULT,
    )
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

/// The editor frame: the rectangle from its top left corner to its bottom
/// right corner.
fn frame(buffer: &Buffer) -> Rect {
    let symbol = |p: Position| buffer[p].symbol().to_string();
    let top_left = buffer
        .area
        .positions()
        .find(|&p| symbol(p) == "┌")
        .expect("no frame");
    let right = (top_left.x..buffer.area.right())
        .find(|&x| symbol(Position::new(x, top_left.y)) == "┐")
        .expect("no top right corner");
    let bottom = (top_left.y..buffer.area.bottom())
        .find(|&y| symbol(Position::new(top_left.x, y)) == "└")
        .expect("no bottom left corner");
    Rect::new(
        top_left.x,
        top_left.y,
        right - top_left.x + 1,
        bottom - top_left.y + 1,
    )
}

fn frame_row(buffer: &Buffer, y: u16) -> String {
    let frame = frame(buffer);
    (frame.x..frame.right())
        .map(|x| buffer[(x, y)].symbol().to_string())
        .collect()
}

fn breadcrumb(buffer: &Buffer) -> String {
    let frame = frame(buffer);
    if frame.y == buffer.area.y {
        return String::new();
    }
    frame_row(buffer, frame.y - 1).trim_end().to_string()
}

/// Trailing blanks stay, so a test can tell where the text ends.
fn status(buffer: &Buffer) -> String {
    let frame = frame(buffer);
    if frame.bottom() == buffer.area.bottom() {
        return String::new();
    }
    frame_row(buffer, frame.bottom())
}

/// One typed line inside the frame: its number in the gutter, the column
/// its code starts at, and the code.
#[derive(Debug)]
struct CodeRow {
    number: usize,
    x: u16,
    y: u16,
    text: String,
}

/// The typed lines inside the frame. A row with no number in the gutter is
/// not typed yet.
fn code_rows(buffer: &Buffer) -> Vec<CodeRow> {
    let frame = frame(buffer);
    let mut rows = Vec::new();
    for y in frame.y + 1..frame.bottom() - 1 {
        let cells: Vec<String> = (frame.x + 1..frame.right() - 1)
            .map(|x| buffer[(x, y)].symbol().to_string())
            .collect();
        let Some(start) = cells.iter().position(|c| c != " ") else {
            break;
        };
        let digits = cells[start..]
            .iter()
            .take_while(|c| c.chars().all(|g| g.is_ascii_digit()))
            .count();
        if digits == 0 {
            break;
        }
        let number = cells[start..start + digits].concat().parse().unwrap();
        let code = (start + digits + 1).min(cells.len());
        rows.push(CodeRow {
            number,
            x: frame.x + 1 + u16::try_from(code).unwrap(),
            y,
            text: cells[code..].concat().trim_end().to_string(),
        });
    }
    rows
}

/// The typed text inside the frame, as the rows read, without the gutter.
fn typed(buffer: &Buffer) -> String {
    code_rows(buffer)
        .iter()
        .map(|row| row.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
        .trim_end()
        .to_string()
}

/// The file line number of the first line on screen.
fn first_number(buffer: &Buffer) -> usize {
    code_rows(buffer).first().expect("no typed line").number
}

/// The breadcrumb and the first line number, which together name a snippet.
fn snippet_name(buffer: &Buffer) -> String {
    format!("{}:{}", breadcrumb(buffer), first_number(buffer))
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
        .filter(|&p| buffer[p].bg == theme::ROSE_PINE.text && buffer[p].symbol() == " ")
        .collect()
}

/// Given: two scenes on one seed over one file.
/// When: each ticks eight times.
/// Then: the frames are equal, the breadcrumb names the file, and the rows
/// inside the frame hold a non-empty prefix of the file.
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
    assert_eq!(snippet_name(&first), "a.rs › fn main:1");
    let text = typed(&first);
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
    let area = Rect::new(0, 0, 50, 6);
    let buffer = settled(&mut scene(7, "fn f() { let s = \"日本\"; } // ok\n"), area);

    // Then:
    let row = &code_rows(&buffer)[0];
    assert_eq!(row.text, "fn f() { let s = \"\"; } // ok");
    for x in 0..area.width {
        assert!(buffer[(x, row.y)].symbol().width() <= 1, "wide cell at {x}");
    }
    let expected = row.x + u16::try_from(row.text.len()).unwrap();
    assert_eq!(
        cursors(&buffer),
        [Position::new(expected, row.y)],
        "cursor not after the line"
    );
}

/// Given: a file of one long line, so no newline pauses the typing.
/// When: the scene ticks once a frame for three seconds of wall time at 50
/// and at 100 keystrokes per second.
/// Then: it types about 150 and about 300 glyphs.
///
/// Why it matters: the speed is a promise in keystrokes per second, so
/// the count follows the clock and not the number of ticks.
#[test]
fn the_scene_types_at_the_speed_it_is_given() {
    let area = Rect::new(0, 0, 1100, 5);
    let frames = (3.0 / FRAME_RATE.as_secs_f64()).round() as usize;
    for (speed, expected) in [("50", 150), ("100", 300)] {
        // Given:
        let scene = Typing::new(
            Rng::seed_from_u64(7),
            &theme::ROSE_PINE,
            corpus(&format!("fn f() {{ {} }}\n", "x".repeat(1000))),
            speed.parse().unwrap(),
        );

        // When:
        let buffer = render(&ticked(scene, area, frames), area, area);

        // Then:
        let count = typed(&buffer).len();
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
        &theme::ROSE_PINE,
        Arc::new(Corpus::from_files([(path.to_string(), text.to_string())])),
        "200".parse().unwrap(),
    );
    render(&ticked(scene, area, 10), area, area)
}

fn colour_at(buffer: &Buffer, needle: &str) -> Color {
    let row = &code_rows(buffer)[0];
    let x = u16::try_from(row.text.find(needle).unwrap()).unwrap();
    buffer[(row.x + x, row.y)].fg
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
    let area = Rect::new(0, 0, 50, 6);

    // When:
    let buffer = typed_file("a.rs", &format!("{line}\n"), area);

    // Then:
    assert_eq!(typed(&buffer), line);
    assert_eq!(colour_at(&buffer, "fn"), theme::ROSE_PINE.pine);
    assert_eq!(colour_at(&buffer, "main"), theme::ROSE_PINE.rose);
    assert_eq!(colour_at(&buffer, "x\""), theme::ROSE_PINE.gold);
    assert_eq!(colour_at(&buffer, "//"), theme::ROSE_PINE.muted);
}

/// Given: a Python file.
/// When: the scene has typed it.
/// Then: `def` is a keyword and the function name is a function, so the
/// grammar came from the extension and not from Rust.
#[test]
fn the_grammar_follows_the_extension() {
    let line = "def greet():";
    let area = Rect::new(0, 0, 40, 8);
    let buffer = typed_file("a.py", &format!("{line}\n    pass\n"), area);
    assert_eq!(colour_at(&buffer, "def"), theme::ROSE_PINE.pine);
    assert_eq!(colour_at(&buffer, "greet"), theme::ROSE_PINE.rose);
}

/// Given: a file no grammar claims and a Markdown file, beside a Rust
/// file.
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
        ("README.md".to_string(), "# fn z() {}\n".to_string()),
        ("a.rs".to_string(), "fn y() {}\n".to_string()),
    ];
    for seed in 0..8 {
        let mut scene = Typing::new(
            Rng::seed_from_u64(seed),
            &theme::ROSE_PINE,
            Arc::new(Corpus::from_files(files.clone())),
            Speed::DEFAULT,
        );
        let buffer = settled(&mut scene, area);
        assert_eq!(snippet_name(&buffer), "a.rs › fn y:1", "seed {seed}");
    }
}

const TWO_FUNCTIONS: &str = "//! A header.\nuse std::fmt;\n\n/// Adds one.\nfn add(a: u32) -> u32 {\n    a + 1\n}\n\nfn other() {}\n";

/// Given: a file with a header comment, an import, a documented function
/// and a one-line function.
/// When: the scene types its first snippet, on several seeds.
/// Then: each snippet is one whole function with its doc comment, the
/// gutter numbers the line it starts on, and both functions come up.
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
        seen.insert((snippet_name(&buffer), typed(&buffer)));
    }
    let expected = HashSet::from([
        (
            "a.rs › fn add:4".to_string(),
            "/// Adds one.\nfn add(a: u32) -> u32 {\n    a + 1\n}".to_string(),
        ),
        ("a.rs › fn other:9".to_string(), "fn other() {}".to_string()),
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
        &theme::ROSE_PINE,
        Arc::new(Corpus::from_files([("a.py".to_string(), file.to_string())])),
        Speed::DEFAULT,
    );
    let buffer = settled(&mut scene, area);
    assert_eq!(typed(&buffer), "def greet():\n    print('hi')");
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
    assert_eq!(typed(&buffer), "    fn a(&self) -> u8 {\n        1\n    }");
}

/// Given: a function longer than the snippet cap.
/// When: the scene types the snippet on an area tall enough for all of it.
/// Then: it types the cap's number of lines and stops.
#[test]
fn a_long_function_is_cut_at_the_cap() {
    let area = Rect::new(0, 0, 30, 80);
    let file = format!("fn long() {{\n{}}}\n", "    x();\n".repeat(60));
    let buffer = settled(&mut scene(7, &file), area);
    assert_eq!(typed(&buffer).lines().count(), MOST_SNIPPET_LINES);
}

/// Given: a file with two functions.
/// When: the scene types one snippet, holds it, and types the next.
/// Then: the second is the other function, on every seed.
#[test]
fn a_snippet_does_not_come_twice_in_a_row() {
    let area = Rect::new(0, 0, 40, 12);
    for seed in 0..6 {
        let mut scene = scene(seed, TWO_FUNCTIONS);
        let first = snippet_name(&settled(&mut scene, area));
        let second = snippet_name(&next_snippet(&mut scene, area));
        assert_ne!(first, second, "seed {seed}");
    }
}

/// Given: a Rust function whose parameters wrap onto their own lines, the
/// way rustfmt writes a long signature, and a Ruby method.
/// When: the scene types each snippet.
/// Then: each is the whole function, the body after `) -> u32 {` and the
/// `end` that closes the method included.
///
/// Why it matters: the line that closes a parameter list sits at the
/// definition's indentation, and reading it as the end of the function
/// cuts every long signature off before its body.
#[test]
fn a_snippet_runs_to_the_line_that_closes_the_function() {
    let area = Rect::new(0, 0, 40, 10);
    let cases = [
        (
            "a.rs",
            "fn add(\n    a: u32,\n) -> u32 {\n    a + 1\n}\n",
            "fn add(\n    a: u32,\n) -> u32 {\n    a + 1\n}",
        ),
        (
            "a.rb",
            "def greet\n  puts 'hi'\nend\n\nputs 1\n",
            "def greet\n  puts 'hi'\nend",
        ),
    ];
    for (path, file, snippet) in cases {
        let mut scene = Typing::new(
            Rng::seed_from_u64(7),
            &theme::ROSE_PINE,
            Arc::new(Corpus::from_files([(path.to_string(), file.to_string())])),
            Speed::DEFAULT,
        );
        assert_eq!(typed(&settled(&mut scene, area)), snippet, "{path}");
    }
}

/// Ten lines of thirty glyphs each, numbered from line 1.
fn block_of_ten() -> String {
    let body = format!("    let value = compute({});\n", "x".repeat(6));
    format!("fn f() {{\n{}}}\n", body.repeat(8))
}

/// Given: a ten line snippet on a 120 by 40 area.
/// When: the scene has typed one key, and when it has typed the snippet.
/// Then: the frame is the same in both, it is smaller than the area, and
/// the padding on each side differs from the opposite side by one at most.
///
/// Why it matters: a frame sized from the typed text grows as it types,
/// and a frame that is not centred leaves the other padding uneven.
#[test]
fn the_frame_is_centred_and_does_not_grow() {
    // Given:
    let area = Rect::new(0, 0, 120, 40);
    let early = ticked(scene(7, &block_of_ten()), area, 2);
    let mut done = scene(7, &block_of_ten());

    // When:
    let early = frame(&render(&early, area, area));
    let done = frame(&settled(&mut done, area));

    // Then:
    assert_eq!(early, done);
    assert!(
        done.width < area.width && done.height < area.height,
        "{done:?}"
    );
    let left = done.x - area.x;
    let right = area.right() - done.right();
    let top = done.y - area.y;
    let bottom = area.bottom() - done.bottom();
    assert!(left.abs_diff(right) <= 1, "left {left}, right {right}");
    assert!(top.abs_diff(bottom) <= 1, "top {top}, bottom {bottom}");
}

fn settled_file(path: &str, text: &str, area: Rect) -> Buffer {
    let mut scene = Typing::new(
        Rng::seed_from_u64(7),
        &theme::ROSE_PINE,
        Arc::new(Corpus::from_files([(path.to_string(), text.to_string())])),
        Speed::DEFAULT,
    );
    settled(&mut scene, area)
}

/// Whether the rule on row `y` runs unbroken from one corner to the other.
fn rule_is_bare(buffer: &Buffer, y: u16) -> bool {
    let row = frame_row(buffer, y);
    let inside: Vec<char> = row.chars().collect();
    inside[1..inside.len() - 1].iter().all(|&c| c == '─')
}

/// Given: a typed function in a Rust file and in a Python file, on a 120 by
/// 40 area.
/// When: each snippet is typed whole.
/// Then: the row above the frame starts at its left edge and reads the
/// path, then `fn` and the function's name, the top rule carries no text,
/// and the row below the frame holds the language at its left edge and
/// the cursor's line and column and the speed at its right edge.
///
/// Why it matters: a name read from a keyword list knows `fn` and not
/// `def`, and a status line that is not flush with the frame's edges reads
/// as a stray line of text.
#[test]
fn the_breadcrumb_and_the_status_line_frame_the_snippet() {
    let area = Rect::new(0, 0, 120, 40);
    let cases = [
        (
            "src/scenes/typing.rs",
            "fn tick() {\n    self.notice = self.notice.saturating_sub(1);\n}\n",
            "src › scenes › typing.rs › fn tick",
            "Rust",
            "Ln 3, Col 2  50 cps",
        ),
        (
            "a.py",
            "def greet():\n    print('hello, world')\n",
            "a.py › fn greet",
            "Python",
            "Ln 2, Col 26  50 cps",
        ),
    ];
    for (path, text, trail, language, cursor) in cases {
        // Given, when:
        let buffer = settled_file(path, text, area);

        // Then:
        let frame = frame(&buffer);
        assert_eq!(breadcrumb(&buffer), trail);
        assert!(rule_is_bare(&buffer, frame.y), "{path}");
        let status = status(&buffer);
        assert!(status.starts_with(&format!("{language} ")), "{status:?}");
        assert!(status.ends_with(cursor), "{status:?}");
    }
}

/// Given: a typed snippet on a 120 by 40 area.
/// When: the scene ticks through the snippet, with a render after each.
/// Then: the status line gives the gutter number of the cursor's row as
/// `Ln` and the cursor's column after the gutter, counted from 1, as `Col`.
#[test]
fn the_status_line_follows_the_cursor() {
    let area = Rect::new(0, 0, 120, 40);
    let mut scene = scene(7, &block_of_ten());
    for tick in 0..60 {
        // When:
        scene.tick(area);
        let buffer = render(&scene, area, area);

        // Then:
        let cursor = cursors(&buffer)[0];
        let rows = code_rows(&buffer);
        let row = rows.iter().find(|row| row.y == cursor.y).unwrap();
        let place = format!("Ln {}, Col {}", row.number, cursor.x - row.x + 1);
        assert!(status(&buffer).contains(&place), "tick {tick}: {place}");
    }
}

/// Given: a one line function on areas 40, 28 and 20 columns wide, so
/// each frame is narrower than the one before.
/// When: it is typed whole on each.
/// Then: the widest status line holds everything, the next drops the
/// language, and the narrowest also drops the speed, each flush with the
/// frame's right edge.
///
/// Why it matters: a status line cut at the frame edge shows `50 cp`, a
/// word cut in half, where dropping a whole part keeps the rest readable.
#[test]
fn a_narrow_status_line_drops_the_language_and_then_the_speed() {
    let cases = [
        (40, "Rust", "Ln 1, Col 10  50 cps"),
        (28, "", "Ln 1, Col 10  50 cps"),
        (20, "", "Ln 1, Col 10"),
    ];
    for (width, language, place) in cases {
        // Given, when:
        let area = Rect::new(0, 0, width, 10);
        let buffer = settled_file("a.rs", "fn f() {}\n", area);

        // Then:
        let status = status(&buffer);
        assert!(status.starts_with(language), "{status:?}");
        assert!(status.ends_with(place), "{status:?}");
        let left = status[..status.len() - place.len()].trim();
        assert_eq!(left, language, "{status:?}");
    }
}

/// Given: a function in a file whose path is wider than the frame, on an
/// area whose padded frame holds the file and the function, and on one
/// whose padded frame would be narrower than those two.
/// When: the snippet is typed whole on each.
/// Then: both breadcrumbs drop the leading folders, keep the whole file
/// and function, and start at the frame's left edge.
///
/// Why it matters: a breadcrumb cut at its right end loses the function
/// name, which is the part the reader cannot get from anywhere else.
#[test]
fn a_narrow_frame_drops_the_leading_folders() {
    for width in [28, 20] {
        let area = Rect::new(0, 0, width, 10);
        let buffer = settled_file("src/scenes/typing.rs", "fn tick() {}\n", area);
        let frame = frame(&buffer);
        assert_eq!(breadcrumb(&buffer), "typing.rs › fn tick", "{width}");
        assert_eq!(
            row_text(&buffer, frame.y - 1).trim_start(),
            "typing.rs › fn tick",
            "{width}"
        );
    }
}

/// Given: a one line function on an area one row too short for padding.
/// When: the snippet is typed whole.
/// Then: the frame fills the area's height and both rules run unbroken, so
/// neither the breadcrumb nor the status line is drawn over the frame.
///
/// Why it matters: a breadcrumb moved inside the frame for want of a row
/// writes over the rule, and one that moves the frame down to make room
/// breaks its centring.
#[test]
fn an_area_with_no_padding_has_no_breadcrumb_or_status_line() {
    let area = Rect::new(0, 0, 40, 4);
    let buffer = settled_file("a.rs", "fn f() {}\n", area);
    let frame = frame(&buffer);
    assert_eq!((frame.y, frame.height), (area.y, area.height));
    assert!(rule_is_bare(&buffer, frame.y));
    assert!(rule_is_bare(&buffer, frame.bottom() - 1));
}

/// Given: a file whose documented function runs from line 3 to line 6.
/// When: the scene has typed it.
/// Then: the gutter numbers the rows 3 to 6 in the muted colour, and the
/// code starts after the gutter.
///
/// Why it matters: a gutter that counts from 1 says the snippet is the top
/// of the file, which it is not.
#[test]
fn the_gutter_shows_the_line_numbers_of_the_file() {
    let area = Rect::new(0, 0, 40, 12);
    let file = "use std::fmt;\n\n/// Adds one.\nfn add(a: u32) -> u32 {\n    a + 1\n}\n";
    let mut scene = scene(7, file);
    let buffer = settled(&mut scene, area);
    let rows = code_rows(&buffer);
    let numbers: Vec<usize> = rows.iter().map(|row| row.number).collect();
    assert_eq!(numbers, [3, 4, 5, 6]);
    for row in &rows {
        assert_eq!(
            buffer[(row.x - 2, row.y)].fg,
            theme::ROSE_PINE.muted,
            "{row:?}"
        );
    }
}

/// Given: a ten line snippet whose lines are wider than most areas, inside
/// a larger canvas.
/// When: the area sweeps every width from 10 to 200, at heights 5 and 30,
/// with the snippet half typed.
/// Then: nothing is drawn outside the area, the frame keeps its right
/// rule, and the one cursor sits inside the frame.
///
/// Why it matters: a line wider than the frame that is not cut at the
/// frame edge writes over the rule, and past the area into the padding a
/// later card fills.
#[test]
fn every_width_keeps_the_frame_inside_the_area() {
    let half = ticked(scene(7, &block_of_ten()), Rect::new(0, 0, 200, 40), 60);
    for height in [5, 30] {
        for width in 10..=200 {
            let area = Rect::new(3, 2, width, height);
            let canvas = Rect::new(0, 0, width + 6, height + 4);
            let buffer = render(&half, area, canvas);
            for position in canvas.positions().filter(|p| !area.contains(*p)) {
                assert_eq!(
                    buffer[position].symbol(),
                    " ",
                    "{width}x{height} at {position:?}"
                );
                assert_eq!(buffer[position].bg, Color::Reset);
            }
            let frame = frame(&buffer);
            assert!(area.contains(Position::new(frame.right() - 1, frame.bottom() - 1)));
            for y in frame.y + 1..frame.bottom() - 1 {
                assert_eq!(
                    buffer[(frame.right() - 1, y)].symbol(),
                    "│",
                    "{width}x{height} row {y}"
                );
            }
            let inner = Rect::new(frame.x + 1, frame.y + 1, frame.width - 2, frame.height - 2);
            let cursors = cursors(&buffer);
            assert_eq!(cursors.len(), 1, "{width}x{height}: {cursors:?}");
            assert!(inner.contains(cursors[0]), "{width}x{height}: {cursors:?}");
        }
    }
}

/// Given: a ten line snippet on an area eight rows tall.
/// When: the scene has typed it.
/// Then: the frame leaves one row above it for the breadcrumb and one below
/// it for the status line, and the gutter numbers the last four lines,
/// which are the ones still on screen.
///
/// Why it matters: a snippet taller than the area is the common case on a
/// terminal, and a frame that fills the height then hides both rows.
#[test]
fn a_tall_snippet_keeps_its_breadcrumb_and_status_line() {
    let area = Rect::new(0, 0, 40, 8);
    let buffer = settled(&mut scene(7, &block_of_ten()), area);
    let frame = frame(&buffer);
    assert_eq!((frame.y, frame.bottom()), (1, 7));
    assert_eq!(breadcrumb(&buffer), "a.rs › fn f");
    assert!(status(&buffer).contains("Ln 10,"), "{:?}", status(&buffer));
    let numbers: Vec<usize> = code_rows(&buffer).iter().map(|row| row.number).collect();
    assert_eq!(numbers, [7, 8, 9, 10]);
}

/// Given: snippets of one, ten and forty lines on one 120 by 40 area.
/// When: each is typed whole.
/// Then: each sits in the same frame.
///
/// Why it matters: a frame sized from the snippet moves its edges with
/// every snippet, so the padding beside it changes width each time.
#[test]
fn every_snippet_sits_in_the_same_frame() {
    let area = Rect::new(0, 0, 120, 40);
    let forty = format!("fn long() {{\n{}}}\n", "    x();\n".repeat(38));
    let frames: Vec<Rect> = ["fn f() {}\n", &block_of_ten(), &forty]
        .into_iter()
        .map(|text| frame(&settled(&mut scene(7, text), area)))
        .collect();
    assert_eq!(frames[0], frames[1]);
    assert_eq!(frames[0], frames[2]);
}

/// Given: a half typed snippet.
/// When: it renders on every area from 0 by 0 to 9 by 4, too small for a
/// frame with a line in it.
/// Then: nothing panics and nothing is drawn outside the area.
#[test]
fn a_tiny_area_draws_nothing_outside_it() {
    let half = ticked(scene(7, &block_of_ten()), Rect::new(0, 0, 200, 40), 60);
    for height in 0..=4 {
        for width in 0..=9 {
            let area = Rect::new(3, 2, width, height);
            let canvas = Rect::new(0, 0, width + 6, height + 4);
            let buffer = render(&half, area, canvas);
            for position in canvas.positions().filter(|p| !area.contains(*p)) {
                assert_eq!(
                    buffer[position],
                    Buffer::empty(canvas)[(0, 0)],
                    "{width}x{height}"
                );
            }
        }
    }
}

fn settled_files(files: &[(&str, &str)], area: Rect) -> Buffer {
    let files = files
        .iter()
        .map(|&(path, text)| (path.to_string(), text.to_string()));
    let mut scene = Typing::new(
        Rng::seed_from_u64(7),
        &theme::ROSE_PINE,
        Arc::new(Corpus::from_files(files)),
        Speed::DEFAULT,
    );
    settled(&mut scene, area)
}

fn tree(buffer: &Buffer) -> Vec<String> {
    let frame = frame(buffer);
    let mut rows: Vec<String> = (frame.y..frame.bottom())
        .map(|y| {
            (buffer.area.x + 1..frame.x)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect();
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    rows
}

/// Given: `src/a.rs`, `src/b/c.rs` and `README.md`, where only `c.rs`
/// holds a function.
/// When: the scene types it on a 120 by 40 area.
/// Then: the left padding lists `src` and `b` open, `c.rs` in gold
/// brackets with an iris name, then `a.rs`, then `README.md`, folders above
/// files at each level.
#[test]
fn the_tree_opens_the_folders_on_the_path_and_marks_the_file() {
    // Given:
    let files = [
        ("src/a.rs", "const A: u8 = 1;\n"),
        ("src/b/c.rs", "fn c() {}\n"),
        ("README.md", "# hearth\n"),
    ];

    // When:
    let buffer = settled_files(&files, Rect::new(0, 0, 120, 40));

    // Then:
    assert_eq!(
        tree(&buffer),
        ["▾ src", " ▾ b", "   [c.rs]", "   a.rs", "  README.md"]
    );
    let y = frame(&buffer).y + 2;
    let at = |x: u16| (buffer[(x, y)].symbol().to_string(), buffer[(x, y)].fg);
    assert_eq!(at(4), ("[".to_string(), theme::ROSE_PINE.marker()));
    assert_eq!(at(5), ("c".to_string(), theme::ROSE_PINE.marked()));
    assert_eq!(at(9), ("]".to_string(), theme::ROSE_PINE.marker()));
}

/// Given: one file, on a 120 column area and on a 40 column area, whose
/// left padding is five columns.
/// When: the snippet is typed whole on each.
/// Then: the wide area shows the file in the tree, and the narrow one
/// draws nothing left of the frame.
///
/// Why it matters: a tree squeezed into a few columns is cut to marks
/// nobody can read.
#[test]
fn a_narrow_padding_has_no_tree() {
    let wide = settled_files(&[("a.rs", "fn f() {}\n")], Rect::new(0, 0, 120, 10));
    assert_eq!(tree(&wide), [" [a.rs]"]);
    let narrow = settled_files(&[("a.rs", "fn f() {}\n")], Rect::new(0, 0, 40, 10));
    let frame = frame(&narrow);
    for y in 0..narrow.area.height {
        for x in 0..frame.x {
            assert_eq!(narrow[(x, y)].symbol(), " ", "{x},{y}");
        }
    }
}

/// Given: sixty files in one folder, where only the last holds a function.
/// When: the scene types it on a 120 by 40 area, whose frame is 30 rows.
/// Then: the tree fills the frame's rows and one of them is the marked
/// file.
///
/// Why it matters: a tree drawn from its top shows the first thirty files
/// and never the one being typed.
#[test]
fn a_tall_tree_scrolls_to_the_current_file() {
    let names: Vec<String> = (0..60).map(|n| format!("src/f{n:02}.rs")).collect();
    let files: Vec<(&str, &str)> = names
        .iter()
        .map(|name| {
            let text = if name.ends_with("59.rs") {
                "fn f() {}\n"
            } else {
                "const A: u8 = 1;\n"
            };
            (name.as_str(), text)
        })
        .collect();
    let buffer = settled_files(&files, Rect::new(0, 0, 120, 40));
    let rows = tree(&buffer);
    assert_eq!(rows.len(), usize::from(frame(&buffer).height));
    assert!(rows.contains(&"  [f59.rs]".to_string()), "{rows:?}");
}

/// Given: a current file whose name is wider than the tree.
/// When: the scene types it on a 120 by 40 area.
/// Then: the name is cut so the closing bracket still shows, and the
/// column before the frame stays blank.
#[test]
fn a_long_name_is_cut_inside_the_tree() {
    let buffer = settled_files(
        &[("src/a_very_long_module_name.rs", "fn f() {}\n")],
        Rect::new(0, 0, 120, 40),
    );
    assert_eq!(tree(&buffer), ["▾ src", "  [a_very_lo]"]);
    let frame = frame(&buffer);
    for y in frame.y..frame.bottom() {
        assert_eq!(buffer[(frame.x - 1, y)].symbol(), " ", "row {y}");
    }
}

/// `None` is a half with no glyph in it.
fn minimap(buffer: &Buffer) -> Vec<Vec<(Option<Color>, Option<Color>)>> {
    let frame = frame(buffer);
    (frame.y..frame.bottom())
        .map(|y| {
            (frame.right() + 1..buffer.area.right() - 1)
                .map(|x| {
                    let cell = &buffer[(x, y)];
                    let lower_bg = (cell.bg != Color::Reset).then_some(cell.bg);
                    match cell.symbol() {
                        "▀" => (Some(cell.fg), lower_bg),
                        "▄" => (None, Some(cell.fg)),
                        _ => (None, None),
                    }
                })
                .collect()
        })
        .collect()
}

/// A hundred lines whose only function runs from line 41 to line 51.
fn function_in_the_middle() -> String {
    let filler = "const A: u8 = 1;\n";
    format!(
        "{}fn f() {{\n{}}}\n{}",
        filler.repeat(40),
        "    x();\n".repeat(9),
        filler.repeat(49)
    )
}

/// Given: a hundred line file whose function runs from line 41 to 51, on an
/// area whose frame is 20 rows tall.
/// When: the scene types the function.
/// Then: the minimap rows that hold anything but the outline colour all
/// sit in the middle third, and the first row holds the outline colour.
///
/// Why it matters: a minimap that does not scale a tall file down shows
/// the first forty lines, and the snippet's place in the file is lost.
#[test]
fn the_minimap_shows_the_snippet_in_its_place_in_the_file() {
    // Given, when:
    let buffer = settled_files(
        &[("a.rs", &function_in_the_middle())],
        Rect::new(0, 0, 120, 26),
    );

    // Then:
    let rows = minimap(&buffer);
    assert_eq!(rows.len(), 20);
    let lit: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            row.iter()
                .flat_map(|&(upper, lower)| [upper, lower])
                .flatten()
                .any(|colour| colour != theme::ROSE_PINE.overlay)
        })
        .map(|(index, _)| index)
        .collect();
    assert!(!lit.is_empty());
    assert!(lit.iter().all(|row| (6..=13).contains(row)), "{lit:?}");
    assert!(
        rows[0]
            .iter()
            .any(|&(upper, _)| upper == Some(theme::ROSE_PINE.overlay))
    );
}

/// Given: the same file, with the cursor on line 51, the closing brace.
/// When: the scene types the function.
/// Then: the minimap row of line 51 holds the brace in the full text
/// colour, and no other row holds that colour.
///
/// Why it matters: a cursor line dimmed like the rest of the snippet
/// still reads brighter than a dimmed keyword, so only the undimmed colour
/// tells the cursor line apart.
#[test]
fn the_cursor_line_is_the_only_one_at_full_colour() {
    let buffer = settled_files(
        &[("a.rs", &function_in_the_middle())],
        Rect::new(0, 0, 120, 26),
    );
    let holds_text: Vec<usize> = minimap(&buffer)
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            row.iter()
                .flat_map(|&(upper, lower)| [upper, lower])
                .any(|colour| colour == Some(theme::ROSE_PINE.text))
        })
        .map(|(index, _)| index)
        .collect();
    assert_eq!(holds_text, [10]);
}

/// Given: a function whose body holds an 80 glyph line, a 40 glyph line,
/// and a 40 glyph line indented by 40.
/// When: the scene types it on a 120 column area, whose minimap is 13
/// cells wide.
/// Then: the first line fills all 13 cells, the second the left 7, and the
/// indented one the right 7.
///
/// Why it matters: a line drawn from column 0 hides the indentation, which
/// is the shape a reader sees code by.
#[test]
fn the_minimap_draws_each_line_from_its_indent_to_its_end() {
    let file = format!(
        "fn f() {{\n    {}\n    {}\n{}{}\n}}\n",
        "x".repeat(76),
        "y".repeat(36),
        " ".repeat(40),
        "z".repeat(40)
    );
    let buffer = settled_files(&[("a.rs", &file)], Rect::new(0, 0, 120, 26));
    let rows = minimap(&buffer);
    let lit = |half: usize| -> Vec<usize> {
        let row = &rows[half / 2];
        (0..row.len())
            .filter(|&x| {
                let (upper, lower) = row[x];
                if half.is_multiple_of(2) {
                    upper.is_some()
                } else {
                    lower.is_some()
                }
            })
            .collect()
    };
    assert_eq!(lit(1), (0..13).collect::<Vec<_>>());
    assert_eq!(lit(2), (0..=6).collect::<Vec<_>>());
    assert_eq!(lit(3), (6..=12).collect::<Vec<_>>());
}

/// Given: one file, on a 120 column area and on a 40 column area, whose
/// right padding is five columns.
/// When: the snippet is typed whole on each.
/// Then: the wide area draws a minimap, and the narrow one draws nothing
/// right of the frame.
#[test]
fn a_narrow_padding_has_no_minimap() {
    let wide = settled_files(&[("a.rs", "fn f() {}\n")], Rect::new(0, 0, 120, 10));
    assert!(minimap(&wide)[0].iter().any(|&(upper, _)| upper.is_some()));
    let narrow = settled_files(&[("a.rs", "fn f() {}\n")], Rect::new(0, 0, 40, 10));
    let frame = frame(&narrow);
    for y in 0..narrow.area.height {
        for x in frame.right()..narrow.area.right() {
            assert_eq!(narrow[(x, y)].symbol(), " ", "{x},{y}");
        }
    }
}

/// Given: a function under 45 doc comment lines, and one under exactly 10.
/// When: the scene types each snippet.
/// Then: the first keeps the 10 doc lines nearest the definition, from
/// line 36, and the second keeps all 10, and both type the whole function
/// and name it in the breadcrumb.
///
/// Why it matters: a cap counted from the top of a long doc comment fills
/// the snippet with comments and never reaches the definition.
#[test]
fn a_long_doc_comment_keeps_the_lines_nearest_the_definition() {
    let area = Rect::new(0, 0, 60, 30);
    let function = "fn deep() {\n    x();\n}";
    for (doc_lines, first) in [(45, 36), (10, 1)] {
        let file = format!("{}{function}\n", "/// doc\n".repeat(doc_lines));
        let buffer = settled_files(&[("a.rs", &file)], area);
        assert_eq!(
            snippet_name(&buffer),
            format!("a.rs › fn deep:{first}"),
            "{doc_lines}"
        );
        assert_eq!(
            typed(&buffer),
            format!("{}{function}", "/// doc\n".repeat(10)),
            "{doc_lines}"
        );
    }
}

/// Two areas and the rows of code that the frame on each holds.
const SHORT_AREA: Rect = Rect::new(0, 0, 60, 12);
const SHORT_AREA_ROWS: usize = 8;
const TALL_AREA: Rect = Rect::new(0, 0, 60, 40);
const TALL_AREA_ROWS: usize = 28;

fn function(name: &str, lines: usize) -> String {
    match lines {
        1 => format!("fn {name}() {{}}\n"),
        _ => format!("fn {name}() {{\n{}}}\n", "    step();\n".repeat(lines - 2)),
    }
}

/// Functions of 1, 6 and 20 lines: under half of both areas, over half of
/// the short area only, and over half of both.
fn three_lengths() -> String {
    [function("tiny", 1), function("mid", 6), function("big", 20)].join("\n")
}

fn config(name: &str, text: &str) -> Config {
    let dir = std::env::temp_dir().join(format!("hearth-typing-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    fs::write(&path, text).unwrap();
    Config::load(&path).unwrap()
}

fn typing_app(config: &Config, seed: u64, files: &[(&str, &str)]) -> App {
    let seed = seed.to_string();
    let cli = Cli::try_parse_from(["hearth", "typing", "--seed", &seed]).unwrap();
    let files = files
        .iter()
        .map(|&(path, text)| (path.to_string(), text.to_string()));
    cli.app(config, Arc::new(Corpus::from_files(files)))
}

fn app_frame(app: &App, area: Rect) -> Buffer {
    let mut buffer = Buffer::empty(area);
    app.render(area, &mut buffer);
    buffer
}

/// The function the breadcrumb names, or an empty string with none.
fn function_on_screen(app: &App, area: Rect) -> String {
    let buffer = app_frame(app, area);
    (area.y..area.bottom())
        .find_map(|y| {
            let row = row_text(&buffer, y);
            let (_, after) = row.split_once("› fn ")?;
            after.split_whitespace().next().map(str::to_string)
        })
        .unwrap_or_default()
}

fn first_functions(config: &Config, files: &[(&str, &str)], area: Rect) -> HashSet<String> {
    (0..24)
        .map(|seed| {
            let mut app = typing_app(config, seed, files);
            app.tick(area);
            function_on_screen(&app, area)
        })
        .collect()
}

fn functions_over(app: &mut App, area: Rect, ticks: usize) -> HashSet<String> {
    (0..ticks)
        .map(|_| {
            app.tick(area);
            function_on_screen(app, area)
        })
        .collect()
}

/// A function shorter than half the frame is skipped, and the limit
/// follows the size of the frame.
/// Given: a file with functions of 1, 6 and 20 lines, and no config.
/// When: the scene opens its first function with 24 seeds, on an area whose
/// frame holds 8 rows of code and on one whose frame holds 28.
/// Then: the 8 row frame opens the 6 and the 20 line functions and never
/// the 1 line one, and the 28 row frame opens only the 20 line one. Two
/// runs with one seed draw the same frames.
///
/// Why it matters: a short function fills a corner of a tall frame and
/// ends at once, so the screen is mostly empty for most of the run.
#[test]
fn a_function_too_short_for_the_frame_is_skipped() {
    // Given:
    let file = three_lengths();
    let files = [("a.rs", file.as_str())];
    let config = config("default", "");
    for (area, rows) in [(SHORT_AREA, SHORT_AREA_ROWS), (TALL_AREA, TALL_AREA_ROWS)] {
        let mut app = typing_app(&config, 7, &files);
        app.tick(area);
        let code_rows = frame(&app_frame(&app, area)).height - 2;
        assert_eq!(usize::from(code_rows), rows, "{area:?}");
    }

    // When, then:
    let short = first_functions(&config, &files, SHORT_AREA);
    assert_eq!(short, HashSet::from(["mid".to_string(), "big".to_string()]));
    let tall = first_functions(&config, &files, TALL_AREA);
    assert_eq!(tall, HashSet::from(["big".to_string()]));
    let mut first = typing_app(&config, 7, &files);
    let mut second = typing_app(&config, 7, &files);
    for _ in 0..300 {
        first.tick(TALL_AREA);
        second.tick(TALL_AREA);
        assert_eq!(app_frame(&first, TALL_AREA), app_frame(&second, TALL_AREA));
    }
}

/// Given: a config whose `[typing]` table sets `shortest` to 0, and a file
/// with functions of 1, 6 and 20 lines.
/// When: the scene opens its first function with 24 seeds, on an area whose
/// frame holds 28 rows of code.
/// Then: it opens each of the three functions.
///
/// Why it matters: a repository of short functions is still worth typing
/// for a user who asks for every function.
#[test]
fn a_shortest_of_zero_skips_nothing() {
    let file = three_lengths();
    let config = config("zero", "[typing]\nshortest = 0\n");
    let seen = first_functions(&config, &[("a.rs", &file)], TALL_AREA);
    let all = ["tiny", "mid", "big"].map(str::to_string);
    assert_eq!(seen, HashSet::from(all));
}

/// Given: `a.rs` with functions of 1 and 6 lines, and `b.rs` with one of 20.
/// When: the scene types for 1000 ticks with each of 8 seeds, on an area
/// whose frame holds 28 rows of code.
/// Then: it only ever types the function in `b.rs`.
///
/// Why it matters: a file whose functions are all short would otherwise
/// show a short function whenever it is the file drawn.
#[test]
fn a_file_with_no_long_function_falls_back_to_the_next_file() {
    let short = [function("tiny", 1), function("mid", 6)].join("\n");
    let long = function("big", 20);
    let files = [("a.rs", short.as_str()), ("b.rs", long.as_str())];
    let config = config("next-file", "");
    for seed in 0..8 {
        let mut app = typing_app(&config, seed, &files);
        let seen = functions_over(&mut app, TALL_AREA, 1000);
        assert_eq!(seen, HashSet::from(["big".to_string()]), "seed {seed}");
    }
}

/// Given: a repository whose functions of 1 and 6 lines are both under half
/// of a frame that holds 28 rows of code.
/// When: the scene types for 1000 ticks, long enough for several functions.
/// Then: it types the 6 line function, again and again, and never the 1
/// line one.
///
/// Why it matters: a repository with no long function must still show
/// something, and the longest one fills the most of the frame.
#[test]
fn with_no_long_function_the_longest_is_typed() {
    let file = [function("tiny", 1), function("mid", 6)].join("\n");
    let config = config("longest", "");
    for seed in 0..8 {
        let mut app = typing_app(&config, seed, &[("a.rs", &file)]);
        let seen = functions_over(&mut app, TALL_AREA, 1000);
        assert_eq!(seen, HashSet::from(["mid".to_string()]), "seed {seed}");
    }
}

/// Given: a config whose `shortest` is 1, an area whose frame holds 58 rows
/// of code, and a file with a 1 line function and two functions longer
/// than the cap.
/// When: the scene opens its first function with 24 seeds.
/// Then: it opens each of the two long functions and never the short one.
///
/// Why it matters: no snippet is longer than the cap, so a limit past it
/// would find no function long enough and type the same one every time.
#[test]
fn a_function_at_the_cap_is_long_enough_for_any_frame() {
    let area = Rect::new(0, 0, 60, 80);
    let file = [
        function("tiny", 1),
        function("first", MOST_SNIPPET_LINES + 5),
        function("second", MOST_SNIPPET_LINES + 10),
    ]
    .join("\n");
    let config = config("cap", "[typing]\nshortest = 1\n");
    let seen = first_functions(&config, &[("a.rs", &file)], area);
    let long = ["first", "second"].map(str::to_string);
    assert_eq!(seen, HashSet::from(long));
}

/// A large repository is measured off the UI thread, so the first frame
/// comes at once and the scene then types a function long enough.
/// Given: the large corpus, whose only functions that fill half the frame
/// are the long ones.
/// When: the scene ticks once, and then until code shows.
/// Then: the first frame holds the notice and no code, and the code that
/// shows is a long function.
///
/// Why it matters: measuring every file on the UI thread kept the screen
/// of a large repository black for seconds, and no key could quit it.
#[test]
fn a_large_repository_is_measured_off_the_ui_thread() {
    // Given:
    let mut scene = Typing::new(
        Rng::seed_from_u64(7),
        &theme::ROSE_PINE,
        large_corpus(),
        Speed::DEFAULT,
    )
    .with_shortest(Shortest::from_share(0.5).unwrap());
    let has_frame = |buffer: &Buffer| buffer.content.iter().any(|cell| cell.symbol() == "┌");

    // When:
    scene.tick(TALL_AREA);
    let first = render(&scene, TALL_AREA, TALL_AREA);
    let mut shown = first.clone();
    for _ in 0..2000 {
        if has_frame(&shown) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        scene.tick(TALL_AREA);
        shown = render(&scene, TALL_AREA, TALL_AREA);
    }

    // Then:
    assert!(!has_frame(&first), "the first tick waited for the scan");
    let notice = (0..TALL_AREA.height).any(|y| row_text(&first, y).contains(FINDING));
    assert!(notice, "no notice on the first frame");
    assert!(
        breadcrumb(&shown).contains("fn long_"),
        "{}",
        breadcrumb(&shown)
    );
}

/// More text than the UI thread measures itself, so the scene scans it.
fn large_corpus() -> Arc<Corpus> {
    let mut files: Vec<(String, String)> = (0..500)
        .map(|index| {
            let text =
                format!("/// Returns {index}.\nfn short_{index}() -> u32 {{\n    {index}\n}}\n");
            (
                format!("short/s{index}.rs"),
                text + &format!("// {}\n", "-".repeat(40)),
            )
        })
        .collect();
    let body: String = (0..18)
        .map(|line| format!("    let v{line} = {line};\n"))
        .collect();
    for file in 0..4 {
        let text = (0..2)
            .map(|function| format!("fn long_{file}_{function}() {{\n{body}}}\n\n"))
            .collect::<String>();
        files.push((format!("long{file}.rs"), text));
    }
    Arc::new(Corpus::from_files(files))
}

fn snippets_typed(mut scene: Typing, count: usize, pause: std::time::Duration) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for _ in 0..20_000 {
        scene.tick(TALL_AREA);
        let buffer = render(&scene, TALL_AREA, TALL_AREA);
        if buffer.content.iter().any(|cell| cell.symbol() == "┌") {
            let name = breadcrumb(&buffer);
            if names.last() != Some(&name) {
                names.push(name);
                if names.len() == count {
                    return names;
                }
            }
        }
        std::thread::sleep(pause);
    }
    panic!("typed {names:?} and no more");
}

/// A seed fixes the functions the scene types, however long it waits for
/// the scan.
/// Given: two scenes with one seed over a large corpus, one ticked with a
/// pause that lets the scan keep ahead, and one ticked with none, which
/// runs into waits.
/// When: each types six functions.
/// Then: both type the same six, in the same order.
///
/// Why it matters: a wait adds ticks, and every tick draws from the scene's
/// random source, so a choice drawn from it would follow the speed of the
/// machine and not the seed.
#[test]
fn a_seed_fixes_the_functions_however_long_the_scan_takes() {
    // Given:
    let scene = || {
        Typing::new(
            Rng::seed_from_u64(7),
            &theme::ROSE_PINE,
            large_corpus(),
            Speed::from_cps(1000).unwrap(),
        )
        .with_shortest(Shortest::from_share(0.5).unwrap())
    };

    // When:
    let patient = snippets_typed(scene(), 6, std::time::Duration::from_millis(5));
    let hasty = snippets_typed(scene(), 6, std::time::Duration::ZERO);

    // Then:
    assert_eq!(patient, hasty);
}
