#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use clap::Parser;
use hearth::app::App;
use hearth::cli::Cli;
use hearth::config::Config;
use hearth::corpus::Corpus;
use hearth::octant;
use hearth::scene::{ALL, SceneKind};
use hearth::scenes::typing::FINDING;
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

fn render(app: &App) -> Buffer {
    let mut buffer = Buffer::empty(AREA);
    app.render(AREA, &mut buffer);
    buffer
}

fn shows_text(buffer: &Buffer, text: &str) -> bool {
    (AREA.y..AREA.bottom()).any(|y| {
        (AREA.x..AREA.right())
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
            .contains(text)
    })
}

/// The frame after 40 ticks. The typing scene measures this corpus off the
/// UI thread, so its 40 ticks start once it has found a function.
fn frame(args: &[&str], config: &Config) -> Buffer {
    let cli = Cli::try_parse_from(args).unwrap();
    let mut app: App = cli.app(config, Arc::new(Corpus::embedded()));
    for _ in 0..2000 {
        if !shows_text(&render(&app), FINDING) {
            break;
        }
        app.tick(AREA);
        thread::sleep(Duration::from_millis(5));
    }
    for _ in 0..40 {
        app.tick(AREA);
    }
    render(&app)
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
/// Given: a file that asks for the terminal's background on Dawn, and a
/// file that asks for it alone, on Rose Pine as the default theme.
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
        let text = if name == THEMES[0] {
            "background = \"terminal\"\n".to_string()
        } else {
            format!("theme = \"{name}\"\nbackground = \"terminal\"\n")
        };
        let config = config(name, &text);
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
/// clears. The one other change is the typing scene's minimap: each cell
/// it draws in blocks shows dots in one of the block's colours, because
/// blocks over the terminal's background read as a solid panel.
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
            let typing = args.contains(&SceneKind::Typing.name());
            for position in AREA.positions() {
                let at = format!("{name} {args:?} at {position:?}");
                let painted = seen(&before[position], &behind);
                if let (true, (Some(quarters), ..)) = (typing, &painted)
                    && before[position].symbol() != " "
                    && quarters.iter().any(|colour| *colour != Color::Reset)
                {
                    let cell = &after[position];
                    assert!(is_dots(cell.symbol()), "{:?} {at}", cell.symbol());
                    assert!(quarters.contains(&cell.fg), "fg {:?} {at}", cell.fg);
                    assert_eq!(cell.bg, Color::Reset, "{at}");
                    continue;
                }
                assert_eq!(seen(&after[position], &[]), painted, "{at}");
            }
        }
    }
}

/// A braille glyph that holds at least one dot.
fn is_dots(symbol: &str) -> bool {
    symbol
        .chars()
        .next()
        .is_some_and(|glyph| ('\u{2801}'..='\u{28ff}').contains(&glyph))
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

/// The typing scene's minimap shows the terminal between its lines.
/// Given: a file that asks for the terminal's background, on Rose Pine and
/// on Dawn.
/// When: hearth starts the typing scene, and runs a while.
/// Then: the minimap draws its lines in dots, no cell of the scene is a
/// block glyph, and no cell paints a background but the cursor's.
///
/// Why it matters: lines of blocks two to a cell fill every cell they
/// cross, so the outline of a long file reads as a solid panel over a
/// transparent terminal.
#[test]
fn the_typing_minimap_shows_the_terminal_between_its_lines() {
    for name in THEMES {
        // Given:
        let config = config(
            &format!("minimap-{name}"),
            &format!("theme = \"{name}\"\nbackground = \"terminal\"\n"),
        );
        let cursor = palette(name).text;

        // When:
        let buffer = frame(&["hearth", "typing", "--seed", "7"], &config);

        // Then:
        let mut dots = 0;
        for position in AREA.positions() {
            let cell = &buffer[position];
            let at = format!("{name} at {position:?}");
            assert!(
                cell.symbol() == " " || !QUADRANTS.contains(&cell.symbol()),
                "block {:?} {at}",
                cell.symbol()
            );
            assert!(
                [Color::Reset, cursor].contains(&cell.bg),
                "bg {:?} {at}",
                cell.bg
            );
            dots += usize::from(is_dots(cell.symbol()));
        }
        assert!(dots > 20, "{name}: {dots} cells of dots");
    }
}

/// An octant glyph drawn in the base turns over, so the other colour stays
/// in front.
/// Given: on each theme, an octant glyph in the base over the pine, and the
/// same glyph in the base over the base.
/// When: each cell clears to the terminal's background.
/// Then: the first shows the pixels the glyph leaves out in the pine over
/// the terminal, and the second shows the terminal alone.
///
/// Why it matters: an octant glyph left in the base paints the theme's base
/// over a transparent terminal, and its pine fills the whole cell.
#[test]
fn an_octant_glyph_in_the_base_turns_over() {
    let glyph = octant::glyph(0b0110_1001);
    let turned = octant::glyph(0b1001_0110);
    for (name, palette) in theme::THEMES {
        for (back, expected) in [
            (palette.pine, (turned.to_string(), palette.pine)),
            (palette.base, (" ".to_string(), Color::Reset)),
        ] {
            // Given:
            let mut cell = Cell::default();
            cell.set_char(glyph).set_fg(palette.base).set_bg(back);

            // When:
            octant::clear(&mut cell, palette.base);

            // Then:
            assert_eq!(
                (cell.symbol().to_string(), cell.fg, cell.bg),
                (expected.0, expected.1, Color::Reset),
                "{name} over {back:?}"
            );
        }
    }
}
