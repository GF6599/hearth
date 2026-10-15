//! Colours source text by its language's grammar, in the palette's roles.

use std::path::Path;
use std::sync::OnceLock;

use ratatui::style::Color;
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

use crate::theme::Palette;

/// The innermost scope that has a role decides the colour, and within one
/// scope the first matching row wins, so a narrow prefix sits above a
/// wider one.
const ROLES: &[(&str, Role)] = &[
    ("comment", |p| p.muted),
    ("string", |p| p.gold),
    ("constant", |p| p.iris),
    ("entity.name.function", |p| p.rose),
    ("variable.function", |p| p.rose),
    ("support.function", |p| p.rose),
    ("entity.name", |p| p.foam),
    ("support.type", |p| p.foam),
    ("support.class", |p| p.foam),
    ("keyword.operator", |p| p.subtle),
    ("keyword", |p| p.pine),
    ("storage", |p| p.pine),
];

/// Reads one role from a palette. The grammars are cached for the whole
/// run, so they hold the role and not the colour of one theme.
type Role = fn(&Palette) -> Color;

#[derive(Debug)]
struct Grammars {
    syntaxes: SyntaxSet,
    roles: Vec<(Scope, Role)>,
    definition: Option<Scope>,
    comment: Option<Scope>,
}

/// A file as its grammar reads it.
#[derive(Debug, Default)]
pub struct Highlighted {
    /// One colour for each `char` of the text, in order, so a caller indexes
    /// it by glyph.
    pub colours: Vec<Color>,
    /// For each line, the name of the function it defines, if it defines
    /// one. Grammars scope a definition `entity.name.function` and a call
    /// `variable.function`, so a call does not count.
    pub functions: Vec<Option<String>>,
    /// For each line, whether every glyph on it that is not a space is in
    /// a comment.
    pub comment: Vec<bool>,
    pub language: &'static str,
}

/// Loaded on the first file the typing scene opens, so the other scenes
/// never pay for it.
fn grammars() -> &'static Grammars {
    static GRAMMARS: OnceLock<Grammars> = OnceLock::new();
    GRAMMARS.get_or_init(|| Grammars {
        syntaxes: SyntaxSet::load_defaults_newlines(),
        roles: ROLES
            .iter()
            .filter_map(|&(name, role)| Scope::new(name).ok().map(|scope| (scope, role)))
            .collect(),
        definition: Scope::new("entity.name.function").ok(),
        comment: Scope::new("comment").ok(),
    })
}

pub fn highlight(path: &str, text: &str, palette: &Palette) -> Highlighted {
    let grammars = grammars();
    let syntax = syntax_for(&grammars.syntaxes, path, text);
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut out = Highlighted {
        language: &syntax.name,
        ..Highlighted::default()
    };
    let mut consumed = 0;
    for line in LinesWithEndings::from(text) {
        // A grammar that fails on a line leaves the rest of the file plain,
        // because its state after the failure is unknown.
        let Ok(ops) = state.parse_line(line, &grammars.syntaxes) else {
            out.colours
                .extend(text[consumed..].chars().map(|_| palette.text));
            let rest = text[consumed..].lines().count();
            out.functions.extend(std::iter::repeat_n(None, rest));
            out.comment.extend(std::iter::repeat_n(false, rest));
            return out;
        };
        let mut function: Option<String> = None;
        let mut words = 0;
        let mut commented = 0;
        let mut segment = |text: &str, stack: &ScopeStack, out: &mut Highlighted| {
            let colour = grammars.colour(stack, palette);
            let in_definition = grammars.within(stack, grammars.definition);
            let in_comment = grammars.within(stack, grammars.comment);
            let word = text.trim();
            if in_definition && function.is_none() && !word.is_empty() {
                function = Some(word.to_string());
            }
            for glyph in text.chars() {
                out.colours.push(colour);
                if !glyph.is_whitespace() {
                    words += 1;
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
        out.functions.push(function);
        out.comment.push(words > 0 && commented == words);
        consumed += line.len();
    }
    out
}

impl Grammars {
    fn within(&self, stack: &ScopeStack, scope: Option<Scope>) -> bool {
        scope.is_some_and(|scope| stack.as_slice().iter().any(|s| scope.is_prefix_of(*s)))
    }

    fn colour(&self, stack: &ScopeStack, palette: &Palette) -> Color {
        stack
            .as_slice()
            .iter()
            .rev()
            .find_map(|scope| {
                self.roles
                    .iter()
                    .find(|(role, _)| role.is_prefix_of(*scope))
                    .map(|&(_, role)| role(palette))
            })
            .unwrap_or(palette.text)
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
