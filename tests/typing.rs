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
use ratatui::style::Color;
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

/// The text cut into the top rule of the frame.
fn title(buffer: &Buffer) -> String {
    let frame = frame(buffer);
    (frame.x + 1..frame.right() - 1)
        .map(|x| buffer[(x, frame.y)].symbol().to_string())
        .collect::<String>()
        .trim_matches('─')
        .trim()
        .to_string()
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

/// The title and the first line number, which together name a snippet.
fn snippet_name(buffer: &Buffer) -> String {
    format!("{}:{}", title(buffer), first_number(buffer))
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
/// Then: the frames are equal, the title names the file, and the rows
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
    assert_eq!(snippet_name(&first), "a.rs:1");
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
/// When: the scene ticks for three seconds of wall time at 50 and at 100
/// keystrokes per second.
/// Then: it types about 150 and about 300 glyphs.
///
/// Why it matters: the speed is a promise in keystrokes per second, so
/// the count follows the clock and not the number of ticks.
#[test]
fn the_scene_types_at_the_speed_it_is_given() {
    let area = Rect::new(0, 0, 1100, 5);
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
    assert_eq!(colour_at(&buffer, "fn"), theme::PINE);
    assert_eq!(colour_at(&buffer, "main"), theme::ROSE);
    assert_eq!(colour_at(&buffer, "x\""), theme::GOLD);
    assert_eq!(colour_at(&buffer, "//"), theme::MUTED);
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
    assert_eq!(colour_at(&buffer, "def"), theme::PINE);
    assert_eq!(colour_at(&buffer, "greet"), theme::ROSE);
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
            Arc::new(Corpus::from_files(files.clone())),
            Speed::DEFAULT,
        );
        let buffer = settled(&mut scene, area);
        assert_eq!(snippet_name(&buffer), "a.rs:1", "seed {seed}");
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

/// Given: a one line function in a file with a long path.
/// When: the scene has typed it.
/// Then: the top rule reads the corner, a dash, the path, a gap, at least
/// one more dash, and the other corner, so the frame is wide enough for
/// its title.
#[test]
fn the_path_is_cut_into_the_top_rule() {
    let area = Rect::new(0, 0, 80, 8);
    let path = "src/scenes/a_long_module_name.rs";
    let mut scene = Typing::new(
        Rng::seed_from_u64(7),
        Arc::new(Corpus::from_files([(
            path.to_string(),
            "fn f() {}\n".to_string(),
        )])),
        Speed::DEFAULT,
    );
    let buffer = settled(&mut scene, area);
    let frame = frame(&buffer);
    let row = row_text(&buffer, frame.y);
    let rule = row.trim_start();
    let lead = format!("┌─ {path} ─");
    assert!(rule.starts_with(&lead), "{rule:?}");
    assert!(rule.ends_with('┐'), "{rule:?}");
    assert!(
        rule[lead.len()..].chars().all(|c| c == '─' || c == '┐'),
        "{rule:?}"
    );
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
        assert_eq!(buffer[(row.x - 2, row.y)].fg, theme::MUTED, "{row:?}");
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

/// Given: a ten line snippet on an area too small for it and its margin.
/// When: the scene has typed it.
/// Then: the frame fills the area, so no padding is left, and the gutter
/// numbers the last six lines, which are the ones still on screen.
#[test]
fn a_small_area_is_filled_by_the_frame() {
    let area = Rect::new(0, 0, 24, 8);
    let buffer = settled(&mut scene(7, &block_of_ten()), area);
    assert_eq!(frame(&buffer), area);
    let numbers: Vec<usize> = code_rows(&buffer).iter().map(|row| row.number).collect();
    assert_eq!(numbers, [5, 6, 7, 8, 9, 10]);
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
