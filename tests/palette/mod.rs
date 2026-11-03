#![allow(
    dead_code,
    reason = "each test binary builds this module and uses only some of it"
)]

use hearth::theme::Palette;
use ratatui::style::Color;

pub const DARK: Color = Color::Rgb(0, 0, 0);
pub const LIGHT: Color = Color::Rgb(255, 255, 255);

pub fn colours(palette: &Palette) -> [Color; 12] {
    [
        palette.base,
        palette.surface,
        palette.overlay,
        palette.muted,
        palette.subtle,
        palette.text,
        palette.love,
        palette.gold,
        palette.rose,
        palette.pine,
        palette.foam,
        palette.iris,
    ]
}

/// The colours a picture draws in: every colour but the base and the
/// surface.
pub fn roles(palette: &Palette) -> [Color; 10] {
    let [_, _, rest @ ..] = colours(palette);
    rest
}

/// `palette` with the colours of `set` in light and every other colour in
/// dark, so a two-colour cell shows which side of the set each pixel is on.
pub fn two_tone(palette: &Palette, set: &[Color]) -> &'static Palette {
    let tone = |colour: Color| if set.contains(&colour) { LIGHT } else { DARK };
    Box::leak(Box::new(Palette {
        base: tone(palette.base),
        surface: tone(palette.surface),
        overlay: tone(palette.overlay),
        muted: tone(palette.muted),
        subtle: tone(palette.subtle),
        text: tone(palette.text),
        love: tone(palette.love),
        gold: tone(palette.gold),
        rose: tone(palette.rose),
        pine: tone(palette.pine),
        foam: tone(palette.foam),
        iris: tone(palette.iris),
        ..*palette
    }))
}
