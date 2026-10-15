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

/// Given: a file with a comment above the table, one above `fire`, one
/// after its value, and `matrix` after `fire`.
/// When: hearth saves to it, `+` is pressed in fire, and `+` in typing.
/// Then: the file reads fire at 3 and typing one step above the default,
/// every comment is still there, and `fire` still comes before `matrix`.
///
/// Why it matters: a save that rewrites the file from scratch throws away
/// the notes the user wrote in it.
#[test]
fn plus_saves_the_speed_and_keeps_the_comments() {
    // Given:
    let text = "# my speeds\n[speed]\n# the fire one\nfire = 2.0 # quick\nmatrix = 0.5\n";
    let path = config_file("save", Some(text));
    let config = Config::load(&path).unwrap();
    let mut app = app(&["hearth", "fire"], &config).saving_to(path.clone());

    // When:
    app.faster();
    for _ in 0..5 {
        app.next_scene();
    }
    app.faster();

    // Then:
    let saved = Config::load(&path).unwrap();
    let fire = app_with(&saved, "fire");
    assert!(fire.contains("3×"), "{fire}");
    assert_eq!(saved.typing, Some(Speed::DEFAULT.faster()));
    let text = fs::read_to_string(&path).unwrap();
    for comment in ["# my speeds", "# the fire one", "# quick"] {
        assert!(text.contains(comment), "{comment} lost: {text}");
    }
    assert!(
        text.find("fire").unwrap() < text.find("matrix").unwrap(),
        "{text}"
    );
}

/// The header after the scene steps to its own speed and back, which
/// shows the speed it started at.
fn app_with(config: &Config, scene: &str) -> String {
    let mut app = app(&["hearth", scene], config);
    app.faster();
    app.slower();
    header(&app)
}

/// Given: a path in a folder that does not exist yet.
/// When: `-` is pressed in fire.
/// Then: the folder and the file now exist, and the file reads fire at ¾×.
#[test]
fn the_first_save_creates_the_folder_and_the_file() {
    let path = config_file("first", None)
        .parent()
        .unwrap()
        .join("new")
        .join("config.toml");
    let mut app = app(&["hearth", "fire"], &Config::default()).saving_to(path.clone());
    app.slower();
    let saved = Config::load(&path).unwrap();
    assert!(app_with(&saved, "fire").contains("¾×"));
}

/// Given: a config file in a folder no one may write to.
/// When: `+` is pressed in fire.
/// Then: the notice shows the new speed and that it was not saved, fire
/// runs at that speed, and the file is unchanged.
///
/// Why it matters: a save that fails in silence lets the user think the
/// speed will be there next run, and one that stops the scene loses a
/// screensaver to a full disk.
#[test]
fn a_failed_save_shows_in_the_notice_and_keeps_the_speed() {
    use std::os::unix::fs::PermissionsExt;

    // Given:
    let path = config_file("locked", Some("[speed]\nfire = 2.0\n"));
    let folder = path.parent().unwrap().to_path_buf();
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();
    let config = Config::load(&path).unwrap();
    let mut app = app(&["hearth", "fire"], &config).saving_to(path.clone());

    // When:
    app.faster();

    // Then:
    let shown = header(&app);
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(shown.contains("3×"), "{shown}");
    assert!(shown.contains("not saved"), "{shown}");
    assert_eq!(fs::read_to_string(&path).unwrap(), "[speed]\nfire = 2.0\n");
}

/// Given: a config file that is a symlink to a file in another folder, as
/// a dotfiles manager makes.
/// When: `+` is pressed in fire.
/// Then: the link is still a link, and the file it points to holds the
/// new speed.
///
/// Why it matters: a save that renames over the link puts a plain file in
/// its place, and the user's dotfiles stop seeing every later change.
#[test]
fn a_save_through_a_symlink_keeps_the_link() {
    // Given:
    let real = config_file("real", Some("[speed]\nfire = 2.0\n"));
    let link = config_file("link", None);
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let config = Config::load(&link).unwrap();
    let mut app = app(&["hearth", "fire"], &config).saving_to(link.clone());

    // When:
    app.faster();

    // Then:
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let saved = Config::load(&real).unwrap();
    assert!(app_with(&saved, "fire").contains("3×"));
}

/// Given: a file that sets fire to 2 and typing to 80.
/// When: hearth starts with no scene and the picker renders.
/// Then: the fire row reads `fire 2×`, the typing row `typing 80 cps`, and
/// the life row `life 1×`.
///
/// Why it matters: a picker that shows the default speed for a scene the
/// user slowed down tells them the wrong thing about what Enter starts.
#[test]
fn the_picker_shows_each_scene_at_its_saved_speed() {
    // Given:
    let path = config_file("picker", Some("[speed]\nfire = 2.0\ntyping = 80\n"));
    let config = Config::load(&path).unwrap();
    let app = app(&["hearth"], &config);

    // When:
    let area = Rect::new(0, 0, 100, 30);
    let mut buffer = Buffer::empty(area);
    app.render(area, &mut buffer);
    let rows: Vec<String> = (0..area.height)
        .map(|y| {
            let row: String = (0..area.width)
                .map(|x| buffer[(x, y)].symbol().to_string())
                .collect();
            row.split_whitespace()
                .filter(|word| *word != "│")
                .collect::<Vec<_>>()
                .join(" ")
                .replace(['[', ']'], "")
        })
        .collect();

    // Then:
    for expected in ["fire 2×", "typing 80 cps", "life 1×"] {
        assert!(
            rows.iter().any(|row| row == expected),
            "{expected}: {rows:#?}"
        );
    }
}
