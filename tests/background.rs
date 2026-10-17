#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use hearth::app::App;
use hearth::cli::Cli;
use hearth::config::Config;
use hearth::corpus::Corpus;
use hearth::scene::{ALL, SceneKind};
use hearth::theme::{self, Palette};
use ratatui::buffer::{Buffer, Cell};
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);
const THEMES: [&str; 2] = ["rose-pine", "rose-pine-dawn"];

/// Each quadrant glyph by its bits: the upper left, upper right, lower left
/// and lower right quarters, from the lowest bit. A set bit draws in the
/// foreground.
const QUADRANTS: [&str; 16] = [
    " ", "▘", "▝", "▀", "▖", "▌", "▞", "▛", "▗", "▚", "▐", "▜", "▄", "▙", "▟", "█",
];

fn config(name: &str, text: &str) -> Config {
    let dir = std::env::temp_dir().join(format!("hearth-bg-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path: PathBuf = dir.join("config.toml");
    fs::write(&path, text).unwrap();
    Config::load(&path).unwrap()
}

fn palette(name: &str) -> &'static Palette {
    theme::THEMES[theme::position(name).unwrap()].1
}

/// The picker, then each scene, at one seed.
fn starts() -> Vec<Vec<&'static str>> {
    let mut starts = vec![vec!["hearth", "--seed", "7"]];
    starts.extend(
        ALL.iter()
            .map(|kind| vec!["hearth", kind.name(), "--seed", "7"]),
    );
    starts
}

fn frame(args: &[&str], config: &Config) -> Buffer {
    let cli = Cli::try_parse_from(args).unwrap();
    let mut app: App = cli.app(config, Arc::new(Corpus::embedded()));
    for _ in 0..40 {
        app.tick(AREA);
    }
    let mut buffer = Buffer::empty(AREA);
    app.render(AREA, &mut buffer);
    buffer
}

/// The colours a cell shows, a quarter at a time for a quadrant glyph and
/// as its glyph and two colours for any other, with every colour in
/// `clear` read as the terminal's.
fn seen(cell: &Cell, clear: &[Color]) -> (Option<[Color; 4]>, String, Color, Color) {
    let shown = |colour: Color| {
        if clear.contains(&colour) {
            Color::Reset
        } else {
            colour
        }
    };
    match QUADRANTS.iter().position(|glyph| *glyph == cell.symbol()) {
        Some(bits) => {
            let quarters = std::array::from_fn(|quarter| {
                shown(if bits & (1 << quarter) != 0 {
                    cell.fg
                } else {
                    cell.bg
                })
            });
            (Some(quarters), String::new(), Color::Reset, Color::Reset)
        }
        None => (
            None,
            cell.symbol().to_string(),
            shown(cell.fg),
            shown(cell.bg),
        ),
    }
}

/// With the terminal's background, hearth paints none of its own.
/// Given: a file that asks for the terminal's background, on Rose Pine and
/// on Dawn.
/// When: hearth starts the picker, and then each scene, and runs a while.
/// Then: no cell has the theme's base or surface as its background, and no
/// glyph draws in either.
///
/// Why it matters: one cell in the theme's background shows as a solid
/// block over a transparent terminal.
#[test]
fn the_terminal_background_leaves_no_theme_background() {
    for name in THEMES {
        // Given:
        let config = config(
            name,
            &format!("theme = \"{name}\"\nbackground = \"terminal\"\n"),
        );
        let palette = palette(name);
        let theirs = [palette.base, palette.surface];

        for args in starts() {
            // When:
            let buffer = frame(&args, &config);

            // Then:
            for position in AREA.positions() {
                let cell = &buffer[position];
                let at = format!("{name} {args:?} at {position:?}");
                assert!(!theirs.contains(&cell.bg), "bg {:?} {at}", cell.bg);
                if cell.symbol() != " " {
                    assert!(
                        !theirs.contains(&cell.fg),
                        "fg {:?} on {:?} {at}",
                        cell.fg,
                        cell.symbol()
                    );
                }
            }
        }
    }
}

/// The terminal's background changes no picture, only what lies behind it.
/// Given: two files on Rose Pine and on Dawn, one with the theme's
/// background and one with the terminal's.
/// When: hearth starts the picker, and then each scene, at one seed under
/// each file, and runs a while.
/// Then: every cell shows the same glyph and colours, and every quarter of
/// a block glyph the same colour, when the theme's base and surface read as
/// the terminal's, and murmuration's overlay too, because its whole sky
/// clears.
///
/// Why it matters: a half block whose top was the base, cleared and not
/// turned over, draws its lower colour over the whole cell and doubles the
/// fire.
#[test]
fn the_terminal_background_keeps_every_picture() {
    for name in THEMES {
        // Given:
        let painted = config(&format!("{name}-theme"), &format!("theme = \"{name}\"\n"));
        let clear = config(
            &format!("{name}-terminal"),
            &format!("theme = \"{name}\"\nbackground = \"terminal\"\n"),
        );
        let palette = palette(name);
        let theirs = [palette.base, palette.surface];

        for args in starts() {
            // When:
            let before = frame(&args, &painted);
            let after = frame(&args, &clear);

            // Then:
            let mut behind = theirs.to_vec();
            if args.contains(&SceneKind::Murmuration.name()) {
                behind.push(palette.overlay);
            }
            for position in AREA.positions() {
                assert_eq!(
                    seen(&after[position], &[]),
                    seen(&before[position], &behind),
                    "{name} {args:?} at {position:?}"
                );
            }
        }
    }
}

fn shows(buffer: &Buffer, colour: Color) -> bool {
    AREA.positions().any(|position| {
        let cell = &buffer[position];
        cell.bg == colour || (cell.symbol() != " " && cell.fg == colour)
    })
}

/// The overlay stays painted on every theme, even where it shares its
/// colour with the surface.
/// Given: a file that asks for the terminal's background, on each theme.
/// When: hearth starts fire and typing, and runs a while.
/// Then: each frame still shows the overlay: the smoke of the fire, and the
/// outline of the code around the typed function in the minimap.
///
/// Why it matters: Solarized gives its surface and its overlay one colour,
/// and a background known by its colour alone takes the smoke and the
/// minimap away with the panels.
#[test]
fn the_overlay_stays_painted_on_every_theme() {
    for (name, palette) in theme::THEMES {
        // Given:
        let config = config(
            &format!("overlay-{name}"),
            &format!("theme = \"{name}\"\nbackground = \"terminal\"\n"),
        );

        for kind in [SceneKind::Fire, SceneKind::Typing] {
            // When:
            let buffer = frame(&["hearth", kind.name(), "--seed", "7"], &config);

            // Then:
            assert!(shows(&buffer, palette.overlay), "{name} {}", kind.name());
        }
    }
}

/// Murmuration's whole sky shows the terminal.
/// Given: a file that asks for the terminal's background, on Rose Pine and
/// on Dawn.
/// When: hearth starts murmuration, and runs a while.
/// Then: no cell shows the overlay, the lightest third of the dusk sky.
///
/// Why it matters: a sky cleared in two thirds leaves the last third as a
/// solid band over the trees, which reads as a fault and not as a dusk.
#[test]
fn the_whole_murmuration_sky_shows_the_terminal() {
    for name in THEMES {
        // Given:
        let config = config(
            &format!("sky-{name}"),
            &format!("theme = \"{name}\"\nbackground = \"terminal\"\n"),
        );

        // When:
        let buffer = frame(&["hearth", "murmuration", "--seed", "7"], &config);

        // Then:
        assert!(!shows(&buffer, palette(name).overlay), "{name}");
    }
}
