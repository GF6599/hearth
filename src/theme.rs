//! The colour themes, and the two roles the chrome draws with.

use ratatui::style::Color;
use ratatui::symbols::line;
use ratatui::widgets::BorderType;

/// Rose Pine's roles. Every surface takes its colours from one of these,
/// so a theme is one value, and another family maps its colours onto them.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub base: Color,
    pub surface: Color,
    pub overlay: Color,
    pub muted: Color,
    pub subtle: Color,
    pub text: Color,
    pub love: Color,
    pub gold: Color,
    pub rose: Color,
    pub pine: Color,
    pub foam: Color,
    pub iris: Color,
    pub see_through: bool,
}

impl Palette {
    /// The brackets around the current item are chrome saying "this one",
    /// and the item itself is the accent. Two colours on purpose.
    pub const fn marker(&self) -> Color {
        self.gold
    }

    pub const fn marked(&self) -> Color {
        self.iris
    }

    /// A light theme draws dark text on a light base.
    pub fn is_light(&self) -> bool {
        luma(self.base) > luma(self.text)
    }
}

/// Perceived brightness, with the weights of ITU-R BT.601.
pub fn luma(colour: Color) -> u32 {
    match colour {
        Color::Rgb(r, g, b) => 299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b),
        _ => 0,
    }
}

pub const ROSE_PINE: Palette = Palette {
    base: Color::Rgb(0x19, 0x17, 0x24),
    surface: Color::Rgb(0x1f, 0x1d, 0x2e),
    overlay: Color::Rgb(0x26, 0x23, 0x3a),
    muted: Color::Rgb(0x6e, 0x6a, 0x86),
    subtle: Color::Rgb(0x90, 0x8c, 0xaa),
    text: Color::Rgb(0xe0, 0xde, 0xf4),
    love: Color::Rgb(0xeb, 0x6f, 0x92),
    gold: Color::Rgb(0xf6, 0xc1, 0x77),
    rose: Color::Rgb(0xeb, 0xbc, 0xba),
    pine: Color::Rgb(0x31, 0x74, 0x8f),
    foam: Color::Rgb(0x9c, 0xcf, 0xd8),
    iris: Color::Rgb(0xc4, 0xa7, 0xe7),
    see_through: false,
};

pub const ROSE_PINE_MOON: Palette = Palette {
    base: Color::Rgb(0x23, 0x21, 0x36),
    surface: Color::Rgb(0x2a, 0x27, 0x3f),
    overlay: Color::Rgb(0x39, 0x35, 0x52),
    muted: Color::Rgb(0x6e, 0x6a, 0x86),
    subtle: Color::Rgb(0x90, 0x8c, 0xaa),
    text: Color::Rgb(0xe0, 0xde, 0xf4),
    love: Color::Rgb(0xeb, 0x6f, 0x92),
    gold: Color::Rgb(0xf6, 0xc1, 0x77),
    rose: Color::Rgb(0xea, 0x9a, 0x97),
    pine: Color::Rgb(0x3e, 0x8f, 0xb0),
    foam: Color::Rgb(0x9c, 0xcf, 0xd8),
    iris: Color::Rgb(0xc4, 0xa7, 0xe7),
    see_through: false,
};

pub const ROSE_PINE_DAWN: Palette = Palette {
    base: Color::Rgb(0xfa, 0xf4, 0xed),
    surface: Color::Rgb(0xff, 0xfa, 0xf3),
    overlay: Color::Rgb(0xf2, 0xe9, 0xe1),
    muted: Color::Rgb(0x98, 0x93, 0xa5),
    subtle: Color::Rgb(0x79, 0x75, 0x93),
    text: Color::Rgb(0x46, 0x42, 0x61),
    love: Color::Rgb(0xb4, 0x63, 0x7a),
    gold: Color::Rgb(0xea, 0x9d, 0x34),
    rose: Color::Rgb(0xd7, 0x82, 0x7e),
    pine: Color::Rgb(0x28, 0x69, 0x83),
    foam: Color::Rgb(0x56, 0x94, 0x9f),
    iris: Color::Rgb(0x90, 0x7a, 0xa9),
    see_through: false,
};

/// Catppuccin by the same names in both variants: mantle fills a frame,
/// surface0 is the overlay, overlay0 and overlay2 are muted and subtle, and
/// red, yellow, flamingo, blue, teal and mauve are the accents.
pub const CATPPUCCIN_MOCHA: Palette = Palette {
    base: Color::Rgb(0x1e, 0x1e, 0x2e),
    surface: Color::Rgb(0x18, 0x18, 0x25),
    overlay: Color::Rgb(0x31, 0x32, 0x44),
    muted: Color::Rgb(0x6c, 0x70, 0x86),
    subtle: Color::Rgb(0x93, 0x99, 0xb2),
    text: Color::Rgb(0xcd, 0xd6, 0xf4),
    love: Color::Rgb(0xf3, 0x8b, 0xa8),
    gold: Color::Rgb(0xf9, 0xe2, 0xaf),
    rose: Color::Rgb(0xf2, 0xcd, 0xcd),
    pine: Color::Rgb(0x89, 0xb4, 0xfa),
    foam: Color::Rgb(0x94, 0xe2, 0xd5),
    iris: Color::Rgb(0xcb, 0xa6, 0xf7),
    see_through: false,
};

pub const CATPPUCCIN_LATTE: Palette = Palette {
    base: Color::Rgb(0xef, 0xf1, 0xf5),
    surface: Color::Rgb(0xe6, 0xe9, 0xef),
    overlay: Color::Rgb(0xcc, 0xd0, 0xda),
    muted: Color::Rgb(0x9c, 0xa0, 0xb0),
    subtle: Color::Rgb(0x7c, 0x7f, 0x93),
    text: Color::Rgb(0x4c, 0x4f, 0x69),
    love: Color::Rgb(0xd2, 0x0f, 0x39),
    gold: Color::Rgb(0xdf, 0x8e, 0x1d),
    rose: Color::Rgb(0xdd, 0x78, 0x78),
    pine: Color::Rgb(0x1e, 0x66, 0xf5),
    foam: Color::Rgb(0x17, 0x92, 0x99),
    iris: Color::Rgb(0x88, 0x39, 0xef),
    see_through: false,
};

/// Tokyo Night by the same names in both variants: bg_dark fills a frame,
/// bg_highlight is the overlay, comment and dark5 are muted and subtle, and
/// red, yellow, orange, blue, cyan and magenta are the accents. Orange is
/// rose so that rose and iris are not two violets.
pub const TOKYO_NIGHT: Palette = Palette {
    base: Color::Rgb(0x1a, 0x1b, 0x26),
    surface: Color::Rgb(0x16, 0x16, 0x1e),
    overlay: Color::Rgb(0x29, 0x2e, 0x42),
    muted: Color::Rgb(0x56, 0x5f, 0x89),
    subtle: Color::Rgb(0x73, 0x7a, 0xa2),
    text: Color::Rgb(0xc0, 0xca, 0xf5),
    love: Color::Rgb(0xf7, 0x76, 0x8e),
    gold: Color::Rgb(0xe0, 0xaf, 0x68),
    rose: Color::Rgb(0xff, 0x9e, 0x64),
    pine: Color::Rgb(0x7a, 0xa2, 0xf7),
    foam: Color::Rgb(0x7d, 0xcf, 0xff),
    iris: Color::Rgb(0xbb, 0x9a, 0xf7),
    see_through: false,
};

pub const TOKYO_NIGHT_DAY: Palette = Palette {
    base: Color::Rgb(0xe1, 0xe2, 0xe7),
    surface: Color::Rgb(0xd0, 0xd5, 0xe3),
    overlay: Color::Rgb(0xc4, 0xc8, 0xda),
    muted: Color::Rgb(0x84, 0x8c, 0xb5),
    subtle: Color::Rgb(0x68, 0x70, 0x9a),
    text: Color::Rgb(0x37, 0x60, 0xbf),
    love: Color::Rgb(0xf5, 0x2a, 0x65),
    gold: Color::Rgb(0x8c, 0x6c, 0x3e),
    rose: Color::Rgb(0xb1, 0x5c, 0x00),
    pine: Color::Rgb(0x2e, 0x7d, 0xe9),
    foam: Color::Rgb(0x00, 0x71, 0x97),
    iris: Color::Rgb(0x98, 0x54, 0xf1),
    see_through: false,
};

/// Kanagawa by the role each theme gives a colour, the same in both
/// variants: ui.bg, the float background, ui.bg_p1, syn.comment, ui.special
/// and ui.fg, then syn.preproc, vcs.changed, syn.number, syn.fun, syn.type
/// and syn.keyword as the accents. Gold is vcs.changed because Lotus draws
/// syn.identifier in a dark olive.
pub const KANAGAWA_WAVE: Palette = Palette {
    base: Color::Rgb(0x1f, 0x1f, 0x28),
    surface: Color::Rgb(0x16, 0x16, 0x1d),
    overlay: Color::Rgb(0x2a, 0x2a, 0x37),
    muted: Color::Rgb(0x72, 0x71, 0x69),
    subtle: Color::Rgb(0x93, 0x8a, 0xa9),
    text: Color::Rgb(0xdc, 0xd7, 0xba),
    love: Color::Rgb(0xe4, 0x68, 0x76),
    gold: Color::Rgb(0xdc, 0xa5, 0x61),
    rose: Color::Rgb(0xd2, 0x7e, 0x99),
    pine: Color::Rgb(0x7e, 0x9c, 0xd8),
    foam: Color::Rgb(0x7a, 0xa8, 0x9f),
    iris: Color::Rgb(0x95, 0x7f, 0xb8),
    see_through: false,
};

pub const KANAGAWA_LOTUS: Palette = Palette {
    base: Color::Rgb(0xf2, 0xec, 0xbc),
    surface: Color::Rgb(0xd5, 0xce, 0xa3),
    overlay: Color::Rgb(0xe7, 0xdb, 0xa0),
    muted: Color::Rgb(0x8a, 0x89, 0x80),
    subtle: Color::Rgb(0x76, 0x6b, 0x90),
    text: Color::Rgb(0x54, 0x54, 0x64),
    love: Color::Rgb(0xc8, 0x40, 0x53),
    gold: Color::Rgb(0xde, 0x98, 0x00),
    rose: Color::Rgb(0xb3, 0x5b, 0x79),
    pine: Color::Rgb(0x4d, 0x69, 0x9b),
    foam: Color::Rgb(0x59, 0x7b, 0x75),
    iris: Color::Rgb(0x62, 0x4c, 0x83),
    see_through: false,
};

/// Dracula and Alucard by the names of the Dracula specification: Background,
/// the UI's Background Dark, Selection, Comment and Foreground, then Red,
/// Yellow, Pink, Green, Cyan and Purple as the accents. The specification
/// has no tone between Comment and Foreground, so Comment is both muted and
/// subtle.
pub const DRACULA: Palette = Palette {
    base: Color::Rgb(0x28, 0x2a, 0x36),
    surface: Color::Rgb(0x21, 0x22, 0x2c),
    overlay: Color::Rgb(0x44, 0x47, 0x5a),
    muted: Color::Rgb(0x62, 0x72, 0xa4),
    subtle: Color::Rgb(0x62, 0x72, 0xa4),
    text: Color::Rgb(0xf8, 0xf8, 0xf2),
    love: Color::Rgb(0xff, 0x55, 0x55),
    gold: Color::Rgb(0xf1, 0xfa, 0x8c),
    rose: Color::Rgb(0xff, 0x79, 0xc6),
    pine: Color::Rgb(0x50, 0xfa, 0x7b),
    foam: Color::Rgb(0x8b, 0xe9, 0xfd),
    iris: Color::Rgb(0xbd, 0x93, 0xf9),
    see_through: false,
};

pub const DRACULA_ALUCARD: Palette = Palette {
    base: Color::Rgb(0xff, 0xfb, 0xeb),
    surface: Color::Rgb(0xce, 0xcc, 0xc0),
    overlay: Color::Rgb(0xcf, 0xcf, 0xde),
    muted: Color::Rgb(0x6c, 0x66, 0x4b),
    subtle: Color::Rgb(0x6c, 0x66, 0x4b),
    text: Color::Rgb(0x1f, 0x1f, 0x1f),
    love: Color::Rgb(0xcb, 0x3a, 0x2a),
    gold: Color::Rgb(0x84, 0x6e, 0x15),
    rose: Color::Rgb(0xa3, 0x14, 0x4d),
    pine: Color::Rgb(0x14, 0x71, 0x0a),
    foam: Color::Rgb(0x03, 0x6a, 0x96),
    iris: Color::Rgb(0x64, 0x4a, 0xc9),
    see_through: false,
};

/// Solarized by its tone names: Dark takes base03, base02, base01, base00
/// and base0, and Light inverts them. Both keep red, yellow, magenta, blue,
/// cyan and violet as the accents. Solarized has one tone between the
/// background and the comments, so surface and overlay share it.
pub const SOLARIZED_DARK: Palette = Palette {
    base: Color::Rgb(0x00, 0x2b, 0x36),
    surface: Color::Rgb(0x07, 0x36, 0x42),
    overlay: Color::Rgb(0x07, 0x36, 0x42),
    muted: Color::Rgb(0x58, 0x6e, 0x75),
    subtle: Color::Rgb(0x65, 0x7b, 0x83),
    text: Color::Rgb(0x83, 0x94, 0x96),
    love: Color::Rgb(0xdc, 0x32, 0x2f),
    gold: Color::Rgb(0xb5, 0x89, 0x00),
    rose: Color::Rgb(0xd3, 0x36, 0x82),
    pine: Color::Rgb(0x26, 0x8b, 0xd2),
    foam: Color::Rgb(0x2a, 0xa1, 0x98),
    iris: Color::Rgb(0x6c, 0x71, 0xc4),
    see_through: false,
};

pub const SOLARIZED_LIGHT: Palette = Palette {
    base: Color::Rgb(0xfd, 0xf6, 0xe3),
    surface: Color::Rgb(0xee, 0xe8, 0xd5),
    overlay: Color::Rgb(0xee, 0xe8, 0xd5),
    muted: Color::Rgb(0x93, 0xa1, 0xa1),
    subtle: Color::Rgb(0x83, 0x94, 0x96),
    text: Color::Rgb(0x65, 0x7b, 0x83),
    love: Color::Rgb(0xdc, 0x32, 0x2f),
    gold: Color::Rgb(0xb5, 0x89, 0x00),
    rose: Color::Rgb(0xd3, 0x36, 0x82),
    pine: Color::Rgb(0x26, 0x8b, 0xd2),
    foam: Color::Rgb(0x2a, 0xa1, 0x98),
    iris: Color::Rgb(0x6c, 0x71, 0xc4),
    see_through: false,
};

/// Each theme by the name the config file gives it. A family comes first
/// in the name, so another family can join without a clash.
pub const THEMES: [(&str, &Palette); 13] = [
    ("rose-pine", &ROSE_PINE),
    ("rose-pine-moon", &ROSE_PINE_MOON),
    ("rose-pine-dawn", &ROSE_PINE_DAWN),
    ("catppuccin-mocha", &CATPPUCCIN_MOCHA),
    ("catppuccin-latte", &CATPPUCCIN_LATTE),
    ("tokyo-night", &TOKYO_NIGHT),
    ("tokyo-night-day", &TOKYO_NIGHT_DAY),
    ("kanagawa-wave", &KANAGAWA_WAVE),
    ("kanagawa-lotus", &KANAGAWA_LOTUS),
    ("dracula", &DRACULA),
    ("dracula-alucard", &DRACULA_ALUCARD),
    ("solarized-dark", &SOLARIZED_DARK),
    ("solarized-light", &SOLARIZED_LIGHT),
];

/// Each theme of `THEMES` with its surface in its base colour, for the
/// terminal's background. A panel then shows the terminal with the rest of
/// the background, and a role that shares the surface's colour, as
/// Solarized's overlay does, stays painted.
pub static FLAT_THEMES: [Palette; THEMES.len()] = {
    let mut flat = [ROSE_PINE; THEMES.len()];
    let mut index = 0;
    while index < THEMES.len() {
        let palette = THEMES[index].1;
        flat[index] = Palette {
            surface: palette.base,
            ..*palette
        };
        index += 1;
    }
    flat
};

/// The place of a theme in `THEMES`, which is the first theme for a
/// bare start.
pub fn position(name: &str) -> Option<usize> {
    THEMES.iter().position(|(each, _)| *each == name)
}

/// Every frame takes its corners from here, so a rounded surface can only
/// come back deliberately.
pub const BORDER: BorderType = BorderType::Plain;

/// The rules between panes, in the weight `BORDER` sets.
pub const RULE: line::Set = match BORDER {
    BorderType::Double => line::DOUBLE,
    BorderType::Thick => line::THICK,
    _ => line::NORMAL,
};

/// A title is cut into the top rule as `┌─ title ─…`, with the lead dash
/// and the gap after the label as part of the text.
pub const PICKER_TITLE: &str = "─ scenes ";

/// The picker's keys, cut into its bottom rule the same way.
pub const PICKER_HINT: &str = "─ ↵ start  j/k move  / search  t theme  q quit ";

/// The picker's keys while a search is on, in the same place.
pub const PICKER_SEARCH_HINT: &str = "─ ↵ start  ↑/↓ move  esc back ";
