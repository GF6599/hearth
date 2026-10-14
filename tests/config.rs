#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use hearth::app::App;
use hearth::cli::Cli;
use hearth::config::Config;
use hearth::corpus::Corpus;
use hearth::scene::SceneKind;
use hearth::scenes::typing::Speed;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

const AREA: Rect = Rect::new(0, 0, 60, 6);

/// A path in a fresh folder, holding `text` when there is some.
fn config_file(name: &str, text: Option<&str>) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hearth-config-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    if let Some(text) = text {
        fs::write(&path, text).unwrap();
    }
    path
}

fn app(args: &[&str], config: &Config) -> App {
    let cli = Cli::try_parse_from(args).unwrap();
    cli.app(config, Arc::new(Corpus::embedded()))
}

fn frame(app: &App) -> Buffer {
    let mut buffer = Buffer::empty(AREA);
    app.render(AREA, &mut buffer);
    buffer
}

fn header(app: &App) -> String {
    let buffer = frame(app);
    (0..AREA.width)
        .map(|x| buffer[(x, 0)].symbol().to_string())
        .collect()
}

/// Given: a file that sets fire to 2 and typing to 80.
/// When: hearth starts fire, and `-` is pressed in fire, in life, and `+`
/// in typing.
/// Then: fire steps down from 2×, life from 1×, and typing from 80.
///
/// Why it matters: a speed in the file that does not reach the scene is
/// a preference the user saved for nothing.
#[test]
fn a_speed_in_the_file_is_the_speed_a_scene_starts_at() {
    // Given:
    let path = config_file("speeds", Some("[speed]\nfire = 2.0\ntyping = 80\n"));
    let config = Config::load(&path).unwrap();

    // When, then:
    let mut app = app(&["hearth", "fire"], &config);
    app.slower();
    assert!(header(&app).contains("1.5×"), "{}", header(&app));
    app.next_scene();
    app.slower();
    assert!(header(&app).contains("¾×"), "{}", header(&app));
    for _ in 0..4 {
        app.next_scene();
    }
    app.faster();
    let typing: Speed = "80".parse().unwrap();
    let shown = format!("{} cps", typing.faster());
    assert!(header(&app).contains(&shown), "{}", header(&app));
}

/// Given: a path where no file exists.
/// When: the config loads from it and hearth starts fire.
/// Then: fire starts at 1×, and the file still does not exist.
///
/// Why it matters: a read that creates the file writes to the user's
/// home on a run that changed nothing.
#[test]
fn a_missing_file_gives_the_defaults_and_is_not_created() {
    let path = config_file("missing", None);
    let config = Config::load(&path).unwrap();
    let mut app = app(&["hearth", "fire"], &config);
    app.slower();
    assert!(header(&app).contains("¾×"), "{}", header(&app));
    assert!(!path.exists());
}

/// Given: a file whose scene is fire.
/// When: hearth starts with no scene named, and with life named.
/// Then: the first is fire and the second is life, frame for frame.
///
/// Why it matters: the command line is the user's choice for this run,
/// and the file only fills in what it leaves out.
#[test]
fn the_command_line_comes_before_the_file() {
    let path = config_file("scene", Some("scene = \"fire\"\n"));
    let config = Config::load(&path).unwrap();
    let corpus = Arc::new(Corpus::embedded());
    for (args, kind) in [
        (&["hearth", "--seed", "7"][..], SceneKind::Fire),
        (&["hearth", "life", "--seed", "7"], SceneKind::Life),
    ] {
        let mut started = app(args, &config);
        let mut expected = App::new(kind, 7, Arc::clone(&corpus));
        for _ in 0..5 {
            started.tick(AREA);
            expected.tick(AREA);
        }
        assert_eq!(frame(&started), frame(&expected), "{args:?}");
    }
}

/// Given: files with a word for a speed, a speed between two steps, a
/// typing speed of zero, a key no scene has, a scene that does not exist,
/// and text that is not TOML.
/// When: each loads.
/// Then: each fails with a message that names the file and, where there
/// is one, the key.
#[test]
fn a_bad_value_names_the_file_and_the_key() {
    let cases = [
        ("word", "[speed]\nfire = \"fast\"\n", "speed.fire"),
        ("between", "[speed]\nfire = 1.7\n", "speed.fire"),
        ("zero", "[speed]\ntyping = 0\n", "speed.typing"),
        ("unknown", "[speed]\nfyre = 2\n", "speed.fyre"),
        ("scene", "scene = \"nope\"\n", "scene"),
        ("broken", "[speed\n", ""),
    ];
    for (name, text, key) in cases {
        let path = config_file(name, Some(text));
        let error = Config::load(&path).unwrap_err();
        assert!(
            error.contains(&path.display().to_string()),
            "{name}: {error}"
        );
        assert!(error.contains(key), "{name}: {error}");
    }
}
