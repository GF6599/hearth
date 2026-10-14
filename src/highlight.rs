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
    })
}

/// One colour for each `char` of `text`, in order, so a caller indexes
/// it by glyph.
pub fn colours(path: &str, text: &str) -> Vec<Color> {
    let grammars = grammars();
    let mut state = ParseState::new(syntax_for(&grammars.syntaxes, path, text));
    let mut stack = ScopeStack::new();
    let mut out = Vec::with_capacity(text.len());
    let mut consumed = 0;
    for line in LinesWithEndings::from(text) {
        // A grammar that fails on a line leaves the rest of the file plain,
        // because its state after the failure is unknown.
        let Ok(ops) = state.parse_line(line, &grammars.syntaxes) else {
            out.extend(text[consumed..].chars().map(|_| PLAIN));
            return out;
        };
        let mut start = 0;
        for (offset, op) in ops {
            let colour = grammars.colour(&stack);
            out.extend(line[start..offset].chars().map(|_| colour));
            let _ = stack.apply(&op);
            start = offset;
        }
        let colour = grammars.colour(&stack);
        out.extend(line[start..].chars().map(|_| colour));
        consumed += line.len();
    }
    out
}

impl Grammars {
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
