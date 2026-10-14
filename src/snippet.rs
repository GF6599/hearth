//! Cuts a source file into snippets, one for each function.

use std::ops::Range;

/// A longer function is cut here, so a snippet fits a terminal.
pub const MOST_SNIPPET_LINES: usize = 40;

/// The comment and attribute lines kept above a definition. A quarter of
/// the cap, so a long doc comment cannot leave the body out.
const MOST_LEAD_LINES: usize = MOST_SNIPPET_LINES / 4;

/// The line range of each function in `lines`, in the order the
/// functions start. `functions` holds the name each line defines and
/// `comment` one flag for each line, both from the grammar.
pub fn snippets(
    lines: &[&str],
    functions: &[Option<String>],
    comment: &[bool],
) -> Vec<Range<usize>> {
    functions
        .iter()
        .enumerate()
        .filter(|(_, function)| function.is_some())
        .map(|(start, _)| {
            let first = lead(lines, comment, start).max(start.saturating_sub(MOST_LEAD_LINES));
            first..end(lines, start).min(first + MOST_SNIPPET_LINES)
        })
        .collect()
}

/// The first line of the comments and attributes directly above the
/// definition, at its indentation.
fn lead(lines: &[&str], comment: &[bool], start: usize) -> usize {
    let indent = indentation(lines[start]);
    let mut first = start;
    while let Some(above) = first.checked_sub(1) {
        let line = lines[above];
        let trimmed = line.trim_start();
        let attribute = trimmed.starts_with("#[") || trimmed.starts_with('@');
        let is_lead = comment.get(above).copied().unwrap_or(false) || attribute;
        if trimmed.is_empty() || indentation(line) != indent || !is_lead {
            break;
        }
        first = above;
    }
    first
}

/// One past the last line of the function that starts at `start`: the
/// body is every line indented deeper than the definition, and a line at
/// its indentation that closes a bracket or a block ends it. A closing
/// line that opens again, as `) -> u32 {` after a long parameter list
/// does, carries on.
fn end(lines: &[&str], start: usize) -> usize {
    let indent = indentation(lines[start]);
    let mut last = start;
    for (index, line) in lines.iter().enumerate().skip(start + 1) {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        if indentation(line) > indent {
            last = index;
            continue;
        }
        let closes = trimmed.starts_with(['}', ')', ']'])
            || trimmed.split_whitespace().next() == Some("end");
        if !closes {
            break;
        }
        last = index;
        if !trimmed.ends_with(['{', '(', '[', ':']) {
            break;
        }
    }
    last + 1
}

fn indentation(line: &str) -> usize {
    line.len() - line.trim_start().len()
}
