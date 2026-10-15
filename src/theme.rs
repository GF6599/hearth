//! The Rose Pine palette, and the two roles the chrome draws with.

use ratatui::style::Color;
use ratatui::widgets::BorderType;

pub const BASE: Color = Color::Rgb(0x19, 0x17, 0x24);
pub const SURFACE: Color = Color::Rgb(0x1f, 0x1d, 0x2e);
pub const OVERLAY: Color = Color::Rgb(0x26, 0x23, 0x3a);
pub const MUTED: Color = Color::Rgb(0x6e, 0x6a, 0x86);
pub const SUBTLE: Color = Color::Rgb(0x90, 0x8c, 0xaa);
pub const TEXT: Color = Color::Rgb(0xe0, 0xde, 0xf4);
pub const LOVE: Color = Color::Rgb(0xeb, 0x6f, 0x92);
pub const GOLD: Color = Color::Rgb(0xf6, 0xc1, 0x77);
pub const ROSE: Color = Color::Rgb(0xeb, 0xbc, 0xba);
pub const PINE: Color = Color::Rgb(0x31, 0x74, 0x8f);
pub const FOAM: Color = Color::Rgb(0x9c, 0xcf, 0xd8);
pub const IRIS: Color = Color::Rgb(0xc4, 0xa7, 0xe7);

/// The brackets around the current item are chrome saying "this one", and
/// the item itself is the accent. Two colours on purpose.
pub const MARKER: Color = GOLD;
pub const MARKED: Color = IRIS;

/// Every frame takes its corners from here, so a rounded surface can only
/// come back deliberately.
pub const BORDER: BorderType = BorderType::Plain;

/// A title is cut into the top rule as `┌─ title ─…`, with the lead dash
/// and the gap after the label as part of the text.
pub const PICKER_TITLE: &str = "─ scenes ";

/// The picker's keys, cut into its bottom rule the same way.
pub const PICKER_HINT: &str = "─ ↵ start  j/k move  q quit ";
