//! Colours source text by its language's grammar, in the Rose Pine roles.

use std::path::Path;
use std::sync::OnceLock;

use ratatui::style::Color;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use crate::theme;

/// The innermost scope that has a role decides the colour, and within one
/// scope the first matching row wins, so a narrow prefix sits above a
/// wider one.
const ROLES: &[(&str, Color)] = &[
    ("comment", theme::MUTED),
    ("string", theme::GOLD),
    ("constant", theme::IRIS),
    ("entity.name.function", theme::ROSE),
    ("variable.function", theme::ROSE),
    ("support.function", theme::ROSE),
    ("entity.name", theme::FOAM),
    ("support.type", theme::FOAM),
    ("support.class", theme::FOAM),
    ("keyword.operator", theme::SUBTLE),
    ("keyword", theme::PINE),
    ("storage", theme::PINE),
];

const PLAIN: Color = theme::TEXT;

#[derive(Debug)]
struct Grammars {
    syntaxes: SyntaxSet,
    roles: Vec<(Scope, Color)>,
    definition: Option<Scope>,
    comment: Option<Scope>,
}

/// A file as its grammar reads it.
#[derive(Debug, Default)]
pub struct Highlighted {
    /// One colour for each `char` of the text, in order, so a caller indexes
    /// it by glyph.
    pub colours: Vec<Color>,
    /// For each line, whether it names a function where the function is
    /// defined. Grammars scope a definition `entity.name.function` and a
    /// call `variable.function`, so a call does not count.
    pub defines: Vec<bool>,
    /// For each line, whether every glyph on it that is not a space is in
    /// a comment.
    pub comment: Vec<bool>,
}

/// Loaded on the first file the typing scene opens, so the other scenes
/// never pay for it.
fn grammars() -> &'static Grammars {
    static GRAMMARS: OnceLock<Grammars> = OnceLock::new();
    GRAMMARS.get_or_init(|| Grammars {
        syntaxes: SyntaxSet::load_defaults_newlines(),
        roles: ROLES
            .iter()
            .filter_map(|&(name, colour)| Scope::new(name).ok().map(|scope| (scope, colour)))
            .collect(),
        definition: Scope::new("entity.name.function").ok(),
        comment: Scope::new("comment").ok(),
    })
}

pub fn highlight(path: &str, text: &str) -> Highlighted {
    let grammars = grammars();
    let mut state = ParseState::new(syntax_for(&grammars.syntaxes, path, text));
    let mut stack = ScopeStack::new();
    let mut out = Highlighted::default();
    let mut consumed = 0;
    for line in LinesWithEndings::from(text) {
        // A grammar that fails on a line leaves the rest of the file plain,
        // because its state after the failure is unknown.
        let Ok(ops) = state.parse_line(line, &grammars.syntaxes) else {
            out.colours.extend(text[consumed..].chars().map(|_| PLAIN));
            let rest = text[consumed..].lines().count();
            out.defines.extend(std::iter::repeat_n(false, rest));
            out.comment.extend(std::iter::repeat_n(false, rest));
            return out;
        };
        let mut defines = false;
        let mut words = 0;
        let mut commented = 0;
        let mut segment = |text: &str, stack: &ScopeStack, out: &mut Highlighted| {
            let colour = grammars.colour(stack);
            let in_definition = grammars.within(stack, grammars.definition);
            let in_comment = grammars.within(stack, grammars.comment);
            for glyph in text.chars() {
                out.colours.push(colour);
                if !glyph.is_whitespace() {
                    words += 1;
                    defines |= in_definition;
                    commented += usize::from(in_comment);
                }
            }
        };
        let mut start = 0;
        for (offset, op) in ops {
            segment(&line[start..offset], &stack, &mut out);
            let _ = stack.apply(&op);
            start = offset;
        }
        segment(&line[start..], &stack, &mut out);
        out.defines.push(defines);
        out.comment.push(words > 0 && commented == words);
        consumed += line.len();
    }
    out
}

impl Grammars {
    fn within(&self, stack: &ScopeStack, scope: Option<Scope>) -> bool {
        scope.is_some_and(|scope| stack.as_slice().iter().any(|s| scope.is_prefix_of(*s)))
    }

    fn colour(&self, stack: &ScopeStack) -> Color {
        stack
            .as_slice()
            .iter()
            .rev()
            .find_map(|scope| {
                self.roles
                    .iter()
                    .find(|(role, _)| role.is_prefix_of(*scope))
                    .map(|&(_, colour)| colour)
            })
            .unwrap_or(PLAIN)
    }
}

/// The extension first, then the whole file name for files such as
/// `Makefile`, then the first line for a script with a shebang.
fn syntax_for<'a>(syntaxes: &'a SyntaxSet, path: &str, text: &str) -> &'a SyntaxReference {
    let path = Path::new(path);
    let by_name = |name: Option<&std::ffi::OsStr>| {
        name.and_then(|name| name.to_str())
            .and_then(|name| syntaxes.find_syntax_by_extension(name))
    };
    by_name(path.extension())
        .or_else(|| by_name(path.file_name()))
        .or_else(|| syntaxes.find_syntax_by_first_line(text.lines().next().unwrap_or("")))
        .unwrap_or_else(|| syntaxes.find_syntax_plain_text())
}
