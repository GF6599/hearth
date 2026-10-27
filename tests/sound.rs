#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use hearth::app::App;
use hearth::config::Config;
use hearth::corpus::Corpus;
use hearth::pane::Direction;
use hearth::scene::{SceneKind, TICK_RATE};
use hearth::sound::{Cue, Synth};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

const RATE: u32 = 48_000;
const AREA: Rect = Rect::new(0, 0, 80, 20);
/// Wide enough that the typing frame holds every glyph the tests type.
const WIDE: Rect = Rect::new(0, 0, 300, 20);

fn tick_samples() -> usize {
    (TICK_RATE.as_secs_f64() * f64::from(RATE)).round() as usize
}

fn rendered(seed: u64, cues: &[Cue], samples: usize) -> Vec<f32> {
    let mut synth = Synth::new(RATE, seed);
    for cue in cues {
        synth.play(*cue);
    }
    let mut out = vec![0.0; samples];
    synth.render(&mut out);
    out
}

fn keys(places: &[f32]) -> Vec<Cue> {
    places.iter().map(|&at| Cue::Key { at }).collect()
}

fn loudest(samples: &[f32]) -> f32 {
    samples
        .iter()
        .fold(0.0, |most, sample| most.max(sample.abs()))
}

fn first_heard(samples: &[f32]) -> Option<usize> {
    samples.iter().position(|sample| sample.abs() > 1e-4)
}

fn config_file(name: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hearth-sound-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    fs::write(&path, text).unwrap();
    path
}

fn typing_app() -> App {
    let corpus = Corpus::from_files([(
        "a.rs".to_string(),
        format!("fn f() {{ {} }}\n", "x".repeat(5000)),
    )]);
    App::new(SceneKind::Typing, 7, Arc::new(corpus))
}

fn header(app: &App) -> String {
    let mut buffer = Buffer::empty(AREA);
    app.render(AREA, &mut buffer);
    (0..AREA.width)
        .map(|x| buffer[(x, 0)].symbol().to_string())
        .collect()
}

/// Given: three keys at the start, the middle and near the end of a tick.
/// When: one synth renders them twice from seed 7, and once from seed 8.
/// Then: the two renders from seed 7 are the same, the one from seed 8
/// differs, and each is heard.
///
/// Why it matters: `--seed` replays a run, so it replays its sound too,
/// and the seed gives each click its own small variation.
#[test]
fn a_seed_renders_the_same_clicks() {
    // Given:
    let cues = keys(&[0.0, 0.5, 0.9]);

    // When:
    let first = rendered(7, &cues, RATE as usize);
    let again = rendered(7, &cues, RATE as usize);
    let other = rendered(8, &cues, RATE as usize);

    // Then:
    assert_eq!(first, again);
    assert_ne!(first, other);
    assert!(loudest(&first) > 0.01, "the clicks are not heard");
}

/// Given: two hundred keys on the same place in a tick.
/// When: the synth renders them.
/// Then: no sample goes past full scale.
///
/// Why it matters: a sample past full scale clips in the output, which is
/// heard as a crack.
#[test]
fn many_clicks_at_once_stay_under_full_scale() {
    // Given:
    let cues = keys(&[0.0; 200]);

    // When:
    let samples = rendered(7, &cues, tick_samples() * 2);

    // Then:
    assert!(
        loudest(&samples) <= 1.0,
        "{} is past full scale",
        loudest(&samples)
    );
    assert!(loudest(&samples) > 0.01, "the clicks are not heard");
}

/// Given: one key at the start of a tick and one at its middle.
/// When: each renders alone.
/// Then: the first is heard at once, and the second half a tick later.
///
/// Why it matters: a tick holds several keystrokes, and keys that sound at
/// once are heard as one chord and not as typing.
#[test]
fn a_key_sounds_at_its_place_in_the_tick() {
    // When:
    let start = first_heard(&rendered(7, &keys(&[0.0]), tick_samples()));
    let middle = first_heard(&rendered(7, &keys(&[0.5]), tick_samples()));

    // Then:
    let half = tick_samples() / 2;
    assert!(start.unwrap() < 10, "the start is heard at {start:?}");
    let middle = middle.unwrap();
    assert!(
        middle.abs_diff(half) < 10,
        "the middle is heard at {middle}, not {half}"
    );
}

/// Given: a synth that is turned off.
/// When: a key plays, then the synth is turned on again.
/// Then: it renders silence before and after it is turned on.
///
/// Why it matters: a muted hearth must be silent, and a key typed while it
/// was muted must not play late when the sound comes back.
#[test]
fn a_synth_that_is_off_is_silent_and_forgets_its_keys() {
    // Given:
    let mut synth = Synth::new(RATE, 7);
    synth.set_on(false);

    // When:
    synth.play(Cue::Key { at: 0.0 });
    let mut off = vec![0.0; tick_samples()];
    synth.render(&mut off);
    synth.set_on(true);
    let mut on = vec![0.0; tick_samples()];
    synth.render(&mut on);

    // Then:
    assert_eq!(first_heard(&off), None);
    assert_eq!(first_heard(&on), None);
}

/// Given: the typing scene past the head of a function of one long line,
/// so each key it types is an `x`.
/// When: it runs one tick.
/// Then: it sends one key cue for each glyph it typed, each at a place in
/// the tick from 0 up to 1, in order.
///
/// Why it matters: a click with no glyph, or a glyph with no click, is
/// heard as sound out of step with the screen.
#[test]
fn typing_sends_one_key_for_each_glyph() {
    // Given:
    let mut app = typing_app();
    for _ in 0..20 {
        if typed(&app) > 0 {
            break;
        }
        app.tick(WIDE);
    }
    assert!(typed(&app) > 0, "no x typed");
    let _ = app.take_cues();

    for _ in 0..20 {
        // When:
        let before = typed(&app);
        app.tick(WIDE);
        let cues = app.take_cues();

        // Then:
        assert_eq!(cues.len(), typed(&app) - before);
        let places: Vec<f32> = cues
            .iter()
            .map(|cue| match cue {
                Cue::Key { at } => *at,
            })
            .collect();
        assert!(
            places.iter().all(|at| (0.0..1.0).contains(at)),
            "{places:?}"
        );
        assert!(
            places.windows(2).all(|pair| pair[0] < pair[1]),
            "{places:?}"
        );
    }
}

/// The count of `x` the typing scene shows.
fn typed(app: &App) -> usize {
    let mut buffer = Buffer::empty(WIDE);
    app.render(WIDE, &mut buffer);
    buffer
        .content()
        .iter()
        .filter(|cell| cell.symbol() == "x")
        .count()
}

/// Given: typing in the left pane and noise in the right pane, which has
/// the focus.
/// When: both run for fifty ticks, and then the focus moves left for one.
/// Then: no cue comes while noise has the focus, and the tick after the
/// move sends the keys of that tick only.
///
/// Why it matters: only the focused pane plays, and a pane without the
/// focus must not save up its keys and play them all when it gets it.
#[test]
fn only_the_focused_pane_sends_cues() {
    // Given:
    let mut app = typing_app();
    app.tick(AREA);
    app.split_beside();
    app.start_selected();
    let _ = app.take_cues();

    // When:
    for _ in 0..50 {
        app.tick(AREA);
    }
    let unfocused = app.take_cues();
    app.move_focus(Direction::Left);
    app.tick(AREA);
    let focused = app.take_cues();

    // Then:
    assert!(
        unfocused.is_empty(),
        "{} cues from the pane without focus",
        unfocused.len()
    );
    assert!(
        (1..=10).contains(&focused.len()),
        "{} cues in one tick",
        focused.len()
    );
}

/// Given: hearth with no config file setting `sound`.
/// When: `m` is toggled twice, saving to a file.
/// Then: sound starts on, the first toggle turns it off and saves
/// `sound = false`, the second turns it on and saves `sound = true`, and
/// the notice names each state.
///
/// Why it matters: a mute that does not last until the next start makes
/// the user mute hearth on every run.
#[test]
fn a_toggle_turns_sound_off_and_on_and_saves_it() {
    // Given:
    let path = config_file("toggle", "# my settings\ntheme = \"rose-pine\"\n");
    let mut app =
        App::new(SceneKind::Fire, 7, Arc::new(Corpus::embedded())).saving_to(path.clone());
    assert!(app.sound_on());

    // When:
    app.toggle_sound();

    // Then:
    assert!(!app.sound_on());
    assert_eq!(Config::load(&path).unwrap().sound, Some(false));
    assert!(header(&app).contains("sound off"), "{}", header(&app));
    assert!(fs::read_to_string(&path).unwrap().contains("# my settings"));

    // When:
    app.toggle_sound();

    // Then:
    assert!(app.sound_on());
    assert_eq!(Config::load(&path).unwrap().sound, Some(true));
    assert!(header(&app).contains("sound on"), "{}", header(&app));
}

/// Given: a config file with `sound = false`.
/// When: hearth starts from it.
/// Then: sound is off.
#[test]
fn a_config_with_sound_off_starts_muted() {
    // Given:
    let path = config_file("off", "sound = false\n");
    let config = Config::load(&path).unwrap();

    // When:
    let app = App::configured(
        Some(SceneKind::Fire),
        7,
        Arc::new(Corpus::embedded()),
        &config,
    );

    // Then:
    assert!(!app.sound_on());
}

/// Given: a config file with `sound = "loud"`.
/// When: it loads.
/// Then: the error names the key and its two values.
#[test]
fn a_sound_that_is_not_true_or_false_stops_startup() {
    let path = config_file("loud", "sound = \"loud\"\n");

    let error = Config::load(&path).unwrap_err();

    assert!(error.contains("sound must be true or false"), "{error}");
}

/// Given: hearth with sound on, on a machine where no sound can play.
/// When: it starts.
/// Then: the notice says why there is no sound.
///
/// Why it matters: a sound that is on and silent with no reason reads as a
/// fault in hearth.
#[test]
fn the_notice_says_why_there_is_no_sound() {
    // Given:
    let mut app = App::new(SceneKind::Fire, 7, Arc::new(Corpus::embedded()));

    // When:
    app.without_sound("no output device");

    // Then:
    assert!(
        header(&app).contains("no sound: no output device"),
        "{}",
        header(&app)
    );
}

/// Given: hearth with sound off, on a machine where no sound can play.
/// When: it starts.
/// Then: the notice stays empty.
///
/// Why it matters: a user who turned sound off has no use for the reason
/// it cannot play.
#[test]
fn a_muted_start_says_nothing_about_sound() {
    // Given:
    let path = config_file("quiet", "sound = false\n");
    let config = Config::load(&path).unwrap();
    let mut app = App::configured(
        Some(SceneKind::Fire),
        7,
        Arc::new(Corpus::embedded()),
        &config,
    );

    // When:
    app.without_sound("no output device");

    // Then:
    assert!(!header(&app).contains("no sound"), "{}", header(&app));
}
