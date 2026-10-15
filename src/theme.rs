//! The Rose Pine palettes, and the two roles the chrome draws with.

use ratatui::style::Color;
use ratatui::symbols::line;
use ratatui::widgets::BorderType;

/// The roles of a Rose Pine variant. Every surface takes its colours
/// from one of these, so a theme is one value.
#[derive(Debug, PartialEq, Eq)]
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
fn luma(colour: Color) -> u32 {
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
};

/// Each theme by the name the config file gives it. A family comes first
/// in the name, so another family can join without a clash.
pub const THEMES: [(&str, &Palette); 3] = [
    ("rose-pine", &ROSE_PINE),
    ("rose-pine-moon", &ROSE_PINE_MOON),
    ("rose-pine-dawn", &ROSE_PINE_DAWN),
];

pub fn by_name(name: &str) -> Option<&'static Palette> {
    THEMES
        .iter()
        .find(|(each, _)| *each == name)
        .map(|&(_, palette)| palette)
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
pub const PICKER_HINT: &str = "─ ↵ start  j/k move  q quit ";
