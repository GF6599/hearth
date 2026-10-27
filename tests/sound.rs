#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use hearth::app::App;
use hearth::config::Config;
use hearth::corpus::Corpus;
use hearth::pane::Direction;
use hearth::scene::{SceneKind, TICK_RATE};
use hearth::sound::{Cue, Mixer};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

const RATE: u32 = 48_000;
const AREA: Rect = Rect::new(0, 0, 80, 20);

/// Samples per channel in one tick.
fn tick_samples() -> usize {
    (TICK_RATE.as_secs_f64() * f64::from(RATE)).round() as usize
}

fn seconds(seconds: f64) -> usize {
    (seconds * f64::from(RATE)).round() as usize
}

/// Renders `ticks` ticks of stereo frames, with the cue on each tick for
/// which `cue` is true, and returns the left channel.
fn rendered(mixer: &mut Mixer, ticks: usize, cue: impl Fn(usize) -> bool) -> Vec<f32> {
    let mut left = Vec::with_capacity(ticks * tick_samples());
    let mut frames = vec![0.0; tick_samples() * 2];
    for tick in 0..ticks {
        if cue(tick) {
            mixer.play(Cue::Typing);
        }
        mixer.render(&mut frames);
        left.extend(frames.iter().step_by(2));
    }
    left
}

fn ticks(seconds: f64) -> usize {
    (seconds / TICK_RATE.as_secs_f64()).round() as usize
}

fn decibels(samples: &[f32]) -> f64 {
    let power = samples.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / samples.len() as f64;
    10.0 * power.max(1e-12).log10()
}

fn loudest(samples: &[f32]) -> f32 {
    samples
        .iter()
        .fold(0.0, |most, sample| most.max(sample.abs()))
}

/// The longest run of samples too quiet to hear.
fn longest_silence(samples: &[f32]) -> usize {
    samples
        .split(|sample| sample.abs() >= 1e-4)
        .map(<[f32]>::len)
        .max()
        .unwrap_or(0)
}

/// Given: a mixer.
/// When: the typing cue comes on every tick for three seconds.
/// Then: after the fade in, the recording plays at about its own level,
/// from -45 to -25 dB, and no sample goes past full scale.
///
/// Why it matters: a recording far louder than the others startles, one
/// far quieter is not heard, and a sample past full scale clips.
#[test]
fn typing_plays_the_typing_recording() {
    // Given:
    let mut mixer = Mixer::new(RATE);

    // When:
    let samples = rendered(&mut mixer, ticks(3.0), |_| true);

    // Then:
    let heard = &samples[seconds(0.5)..];
    let level = decibels(heard);
    assert!((-45.0..=-25.0).contains(&level), "{level} dB");
    assert!(
        loudest(heard) <= 1.0,
        "{} is past full scale",
        loudest(heard)
    );
}

/// Given: a mixer that has played the typing cue on every tick for a
/// second.
/// When: the cues stop.
/// Then: the sound still plays for the next 0.15 seconds, and from one
/// second after the last cue it is silent.
///
/// Why it matters: the scene pauses at the end of each line, and a sound
/// that stopped on each pause would stutter. A sound that went on long
/// after the typing stopped would not follow the screen.
#[test]
fn the_typing_sound_bridges_a_line_pause_and_then_fades_out() {
    // Given:
    let mut mixer = Mixer::new(RATE);
    let typed = ticks(1.0);

    // When:
    let samples = rendered(&mut mixer, typed + ticks(2.0), |tick| tick < typed);

    // Then:
    let last_cue = (typed - 1) * tick_samples();
    let pause = &samples[last_cue..last_cue + seconds(0.15)];
    assert!(
        decibels(pause) > -50.0,
        "{} dB in the pause",
        decibels(pause)
    );
    let after = &samples[last_cue + seconds(1.0)..];
    assert_eq!(
        loudest(after),
        0.0,
        "still heard a second after the last cue"
    );
}

/// Given: a mixer.
/// When: the typing cue comes on every tick for a minute, which plays the
/// recording through more than twice.
/// Then: after the fade in, no stretch of silence is longer than five
/// milliseconds.
///
/// Why it matters: an MP3 starts and ends with a few milliseconds of
/// padding, and a loop that kept it would tick at each turn.
#[test]
fn the_recording_loops_with_no_gap() {
    // Given:
    let mut mixer = Mixer::new(RATE);

    // When:
    let samples = rendered(&mut mixer, ticks(60.0), |_| true);

    // Then:
    let gap = longest_silence(&samples[seconds(0.5)..]);
    assert!(gap <= seconds(0.005), "{gap} silent samples in a row");
}

/// Given: a mixer that is turned off.
/// When: the typing cue comes, then the mixer is turned on again.
/// Then: it renders silence before and after it is turned on.
///
/// Why it matters: a muted hearth must be silent, and a cue sent while it
/// was muted must not play late when the sound comes back.
#[test]
fn a_mixer_that_is_off_is_silent_and_forgets_its_cues() {
    // Given:
    let mut mixer = Mixer::new(RATE);
    mixer.set_on(false);

    // When:
    let off = rendered(&mut mixer, 1, |_| true);
    mixer.set_on(true);
    let on = rendered(&mut mixer, 2, |_| false);

    // Then:
    assert_eq!(loudest(&off), 0.0);
    assert_eq!(loudest(&on), 0.0);
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

/// Given: typing in the left pane and noise in the right pane, which has
/// the focus.
/// When: both run for fifty ticks, and then the focus moves left for one.
/// Then: no cue comes while noise has the focus, and the tick after the
/// move sends the one cue of that tick only.
///
/// Why it matters: only the focused pane plays, and a pane without the
/// focus must not save up its cues and play them all when it gets it.
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
    assert_eq!(focused, [Cue::Typing]);
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

/// Given: each kind of error the audio device reports.
/// When: the speaker reads it.
/// Then: a glitch and a move to another device keep the sound playing, and
/// a lost device or a stream that must be built again stops it.
///
/// Why it matters: macOS reports a glitch whenever the machine is busy, and
/// a speaker that stops on one stays silent until hearth starts again.
#[cfg(target_os = "macos")]
#[test]
fn a_glitch_or_a_new_device_keeps_the_sound_playing() {
    use cpal::ErrorKind;
    use hearth::sound::stops_the_sound;

    assert!(!stops_the_sound(ErrorKind::Xrun));
    assert!(!stops_the_sound(ErrorKind::DeviceChanged));
    assert!(stops_the_sound(ErrorKind::DeviceNotAvailable));
    assert!(stops_the_sound(ErrorKind::StreamInvalidated));
}
