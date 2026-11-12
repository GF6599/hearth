#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use hearth::app::App;
use hearth::config::Config;
use hearth::corpus::Corpus;
use hearth::pane::Direction;
use hearth::scene::{QUARTERS_PER_TICK, Rate, SceneKind, TICK_RATE};
use hearth::sound::{Cue, Mixer, Volume};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

const RATE: u32 = 48_000;
const AREA: Rect = Rect::new(0, 0, 80, 20);
/// The recordings that loop while their scene runs, as the typing stops and
/// starts with its keys.
const SCENE_RECORDINGS: [Cue; 3] = [Cue::Fire, Cue::River, Cue::Tide { swell: 1.0 }];
/// The recordings of the weather, one for each sky, with the wind at the
/// strength of a mean gust.
const WEATHER_RECORDINGS: [Cue; 8] = [
    Cue::ClearDay,
    Cue::ClearNight,
    Cue::Cloud,
    Cue::Rain,
    Cue::Snow,
    Cue::Storm,
    Cue::Fog,
    Cue::Wind { gust: 0.5 },
];
/// The seconds the recordings of the thunder and of the foghorn last.
const THUNDER_SECONDS: f64 = 18.5;
const FOGHORN_SECONDS: f64 = 14.0;

/// Every recording that loops while its cue comes.
fn looping() -> Vec<Cue> {
    SCENE_RECORDINGS
        .into_iter()
        .chain(WEATHER_RECORDINGS)
        .collect()
}

/// Samples per channel in one tick.
fn tick_samples() -> usize {
    (TICK_RATE.as_secs_f64() * f64::from(RATE)).round() as usize
}

fn seconds(seconds: f64) -> usize {
    (seconds * f64::from(RATE)).round() as usize
}

/// Renders `ticks` ticks of stereo frames, with `cue` on each tick for
/// which `when` is true.
fn played(
    mixer: &mut Mixer,
    ticks: usize,
    cue: Cue,
    when: impl Fn(usize) -> bool,
) -> Vec<[f32; 2]> {
    let mut played = Vec::with_capacity(ticks * tick_samples());
    let mut frames = vec![0.0; tick_samples() * 2];
    for tick in 0..ticks {
        if when(tick) {
            mixer.play(cue);
        }
        mixer.render(&mut frames);
        played.extend(frames.as_chunks::<2>().0);
    }
    played
}

fn rendered(mixer: &mut Mixer, ticks: usize, when: impl Fn(usize) -> bool) -> Vec<f32> {
    played(mixer, ticks, Cue::Typing, when)
        .iter()
        .map(|[left, _]| *left)
        .collect()
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

/// Each scene recording plays in stereo at about the level of the others.
/// Given: a mixer for each recording that plays while its scene runs.
/// When: its cue comes on every tick for three seconds.
/// Then: after the fade in, it plays from -45 to -25 dB with no sample past
/// full scale, and its left and right channels differ by more than -60 dB.
///
/// Why it matters: each of these is a stereo recording, and copied to one
/// channel it loses the place it was recorded in.
#[test]
fn each_scene_recording_plays_in_stereo() {
    for cue in SCENE_RECORDINGS {
        // Given:
        let mut mixer = Mixer::new(RATE);

        // When:
        let frames = played(&mut mixer, ticks(3.0), cue, |_| true);

        // Then:
        let heard = &frames[seconds(0.5)..];
        let both: Vec<f32> = heard.iter().flatten().copied().collect();
        let level = decibels(&both);
        assert!((-45.0..=-25.0).contains(&level), "{cue:?}: {level} dB");
        assert!(
            loudest(&both) <= 1.0,
            "{cue:?}: {} is past full scale",
            loudest(&both)
        );
        let side: Vec<f32> = heard
            .iter()
            .map(|[left, right]| (left - right) / 2.0)
            .collect();
        assert!(
            decibels(&side) > -60.0,
            "{cue:?}: {} dB between the channels",
            decibels(&side)
        );
    }
}

/// Each looping recording plays near the level of the others with no drop.
/// Given: a mixer for each recording that loops: the fire, the river, the
/// tide and each sky of the weather.
/// When: its cue comes on every tick for a minute, which plays the recording
/// through more than once.
/// Then: after the fade in, the minute plays from -37 to -29 dB, no sample
/// goes past full scale, and no two milliseconds are quieter than -70 dB.
///
/// Why it matters: a focus move or a change of weather fades one recording
/// into the next, and a jump in level startles. The sources differ by more
/// than 30 dB. A fire, the water and the weather go on with no break, and
/// an MP3 starts and ends with padding that a loop must not keep.
#[test]
fn each_looping_recording_plays_near_one_level_with_no_drop() {
    for cue in looping() {
        // Given:
        let mut mixer = Mixer::new(RATE);

        // When:
        let frames = played(&mut mixer, ticks(60.0), cue, |_| true);

        // Then:
        let heard = frames[seconds(0.5)..].as_flattened();
        let level = decibels(heard);
        assert!((-37.0..=-29.0).contains(&level), "{cue:?}: {level} dB");
        assert!(
            loudest(heard) <= 1.0,
            "{cue:?}: {} is past full scale",
            loudest(heard)
        );
        let left: Vec<f32> = frames[seconds(0.5)..]
            .iter()
            .map(|[left, _]| *left)
            .collect();
        let quietest = left
            .chunks_exact(seconds(0.002))
            .map(decibels)
            .fold(f64::INFINITY, f64::min);
        assert!(quietest >= -70.0, "{cue:?}: {quietest} dB");
    }
}

/// Each looping cue plays its own recording.
/// Given: a mixer for each recording that loops: the fire, the river, the
/// tide and each sky of the weather.
/// When: its cue comes on every tick for three seconds.
/// Then: the frames of each cue differ from the frames of every other cue.
///
/// Why it matters: a cue wired to the wrong recording passes every level
/// check, and the river would crackle, or the snow would sound of rain.
#[test]
fn each_looping_cue_plays_its_own_recording() {
    // When:
    let sounds: Vec<(Cue, Vec<[f32; 2]>)> = looping()
        .into_iter()
        .map(|cue| {
            let frames = played(&mut Mixer::new(RATE), ticks(3.0), cue, |_| true);
            (cue, frames)
        })
        .collect();

    // Then:
    for (index, (cue, frames)) in sounds.iter().enumerate() {
        for (other, other_frames) in &sounds[index + 1..] {
            assert!(
                frames != other_frames,
                "{cue:?} plays the same as {other:?}"
            );
        }
    }
}

/// Given: two mixers.
/// When: one gets the fire cue on every tick, and the other only on the
/// ticks of a fire at its slowest speed, for three seconds.
/// Then: both render the same frames.
///
/// Why it matters: the fire sends its cue when it ticks, and a sound that
/// faded between two slow ticks would pulse.
#[test]
fn the_fire_plays_on_at_its_slowest_speed() {
    // Given:
    let mut slowest = Rate::NORMAL;
    while slowest.slower() != slowest {
        slowest = slowest.slower();
    }
    let every = (QUARTERS_PER_TICK / slowest.quarters()) as usize;

    // When:
    let steady = played(&mut Mixer::new(RATE), ticks(3.0), Cue::Fire, |_| true);
    let slow = played(&mut Mixer::new(RATE), ticks(3.0), Cue::Fire, |tick| {
        tick % every == 0
    });

    // Then:
    let differ = steady.iter().zip(&slow).filter(|(a, b)| a != b).count();
    assert_eq!(differ, 0, "{differ} frames differ");
}

/// The cues whose recording follows a value from its scene: the tide its
/// swell, and the wind its gust.
const FOLLOWING: [fn(f32) -> Cue; 2] = [|swell| Cue::Tide { swell }, |gust| Cue::Wind { gust }];

/// Renders `ticks` ticks, with the cue that `cue` makes on each tick.
fn followed(mixer: &mut Mixer, ticks: usize, cue: impl Fn(usize) -> Cue) -> Vec<f32> {
    let mut played = Vec::with_capacity(ticks * tick_samples() * 2);
    let mut frames = vec![0.0; tick_samples() * 2];
    for tick in 0..ticks {
        mixer.play(cue(tick));
        mixer.render(&mut frames);
        played.extend_from_slice(&frames);
    }
    played
}

/// The tide is louder at the top of a run up than at the bottom of a draw
/// back, and the wind is louder in its strongest gust than in its weakest.
/// Given: two mixers, for the tide and then for the wind.
/// When: one plays at the most its scene sends and the other at the least,
/// for three seconds.
/// Then: after the fade in, the first plays at least 3 dB louder.
///
/// Why it matters: the sound must rise and fall with the water or the gust
/// lines on the screen, and a smaller change is lost under the waves and
/// the gusts in the recording.
#[test]
fn the_swell_and_the_gust_change_how_loud_they_play() {
    for cue in FOLLOWING {
        // When:
        let most = followed(&mut Mixer::new(RATE), ticks(3.0), |_| cue(1.0));
        let least = followed(&mut Mixer::new(RATE), ticks(3.0), |_| cue(0.0));

        // Then:
        let heard = seconds(1.0) * 2;
        let rise = decibels(&most[heard..]) - decibels(&least[heard..]);
        assert!(rise >= 3.0, "{:?}: {rise} dB", cue(1.0));
    }
}

/// A jump in the swell or in the gust reaches the level as a glide.
/// Given: two mixers that play at the least their scene sends, for the tide
/// and then for the wind.
/// When: one stays there, and the other jumps to the most after one second.
/// Then: over the next 5 ms the two stay within 1 dB, and from half a second
/// after the jump the second is at least 3 dB louder.
///
/// Why it matters: the swell and the gust change only on a tick, and a gain
/// that jumps on each tick is heard as a click many times a second.
#[test]
fn a_jump_in_the_swell_or_the_gust_glides_between_ticks() {
    for cue in FOLLOWING {
        // Given:
        let jump = ticks(1.0);
        let steady = followed(&mut Mixer::new(RATE), ticks(2.0), |_| cue(0.0));

        // When:
        let jumped = followed(&mut Mixer::new(RATE), ticks(2.0), |tick| {
            cue(if tick < jump { 0.0 } else { 1.0 })
        });

        // Then:
        let at_jump = jump * tick_samples() * 2;
        let glide = at_jump..at_jump + seconds(0.005) * 2;
        let start = decibels(&jumped[glide.clone()]) - decibels(&steady[glide]);
        assert!(
            start.abs() <= 1.0,
            "{:?}: {start} dB over the first 5 ms",
            cue(1.0)
        );
        let settled = at_jump + seconds(0.5) * 2;
        let rise = decibels(&jumped[settled..]) - decibels(&steady[settled..]);
        assert!(rise >= 3.0, "{:?}: {rise} dB after the glide", cue(1.0));
    }
}

/// Renders `ticks` ticks with `weather` on each tick below `until`, and
/// `sound` also on each tick in `at`.
fn sounded(ticks: usize, weather: Cue, until: usize, sound: Cue, at: &[usize]) -> Vec<f32> {
    let mut mixer = Mixer::new(RATE);
    let mut played = Vec::with_capacity(ticks * tick_samples() * 2);
    let mut frames = vec![0.0; tick_samples() * 2];
    for tick in 0..ticks {
        if tick < until {
            mixer.play(weather);
        }
        if at.contains(&tick) {
            mixer.play(sound);
        }
        mixer.render(&mut frames);
        played.extend_from_slice(&frames);
    }
    played
}

/// What `with` plays over `without`, sample by sample.
fn added(with: &[f32], without: &[f32]) -> Vec<f32> {
    with.iter().zip(without).map(|(a, b)| a - b).collect()
}

/// The sample at which tick `tick` starts.
fn sample_of(tick: usize) -> usize {
    tick * tick_samples() * 2
}

/// The thunder of a bolt and the horn in the fog each sound once over
/// their weather.
/// Given: two mixers that play the storm on every tick, and then two that
/// play the fog.
/// When: one of the two also gets the thunder cue, or the foghorn cue,
/// once, after a second.
/// Then: the two play the same up to that cue. Over the next five seconds
/// the one with the cue adds at least -45 dB. It still adds sound a second
/// before the recording ends, and it adds nothing from a second after.
///
/// Why it matters: a thunder that looped would roll with no bolt, and one
/// cut short, or wired to the recording of the horn, passes a check of its
/// start alone.
#[test]
fn the_thunder_and_the_foghorn_each_sound_once_over_their_weather() {
    for (weather, sound, length) in [
        (Cue::Storm, Cue::Thunder, THUNDER_SECONDS),
        (Cue::Fog, Cue::Foghorn, FOGHORN_SECONDS),
    ] {
        // Given:
        let (once, total) = (ticks(1.0), ticks(1.0 + length + 3.0));
        let without = sounded(total, weather, total, sound, &[]);

        // When:
        let with = sounded(total, weather, total, sound, &[once]);

        // Then:
        let added = added(&with, &without);
        let after = |seconds_after: f64| sample_of(once) + seconds(seconds_after) * 2;
        assert_eq!(
            loudest(&added[..after(0.0)]),
            0.0,
            "{sound:?} before its cue"
        );
        let start = decibels(&added[after(0.0)..after(5.0)]);
        assert!(start >= -45.0, "{sound:?} starts at {start} dB");
        assert!(
            loudest(&added[after(length - 1.5)..after(length - 0.5)]) > 0.0,
            "{sound:?} stops before its recording ends"
        );
        assert_eq!(
            loudest(&added[after(length + 1.0)..]),
            0.0,
            "{sound:?} after its recording ends"
        );
    }
}

/// A second bolt leaves the roll of the first to go on.
/// Given: two mixers that play the storm on every tick and get a thunder
/// cue after four seconds.
/// When: one of the two also got a thunder cue after one second.
/// Then: from half a second to five seconds after the second cue, that one
/// adds at least -60 dB: the first roll under the second.
///
/// Why it matters: bolts come seconds apart and a roll lasts far longer, so
/// a roll that started again on each bolt would cut the one before it.
#[test]
fn a_second_bolt_leaves_the_first_roll_to_go_on() {
    // Given:
    let (first, second, total) = (ticks(1.0), ticks(4.0), ticks(10.0));
    let one = sounded(total, Cue::Storm, total, Cue::Thunder, &[second]);

    // When:
    let two = sounded(total, Cue::Storm, total, Cue::Thunder, &[first, second]);

    // Then:
    let under = sample_of(second) + seconds(0.5) * 2..sample_of(second) + seconds(5.0) * 2;
    let first_roll = decibels(&added(&two, &one)[under]);
    assert!(
        first_roll >= -60.0,
        "the first roll plays at {first_roll} dB"
    );
}

/// Many bolts at once stay under full scale.
/// Given: a mixer that plays the storm on every tick.
/// When: a thunder cue comes on each of twenty ticks in a row, as a storm
/// at its fastest speed can send them, and the mixer plays for ten seconds.
/// Then: no sample goes past full scale.
///
/// Why it matters: every roll that starts adds to the ones that sound, and
/// a sum past full scale clips in the speaker.
#[test]
fn many_bolts_at_once_stay_under_full_scale() {
    // Given:
    let (first, total) = (ticks(1.0), ticks(10.0));
    let bolts: Vec<usize> = (first..first + 20).collect();

    // When:
    let played = sounded(total, Cue::Storm, total, Cue::Thunder, &bolts);

    // Then:
    assert!(
        loudest(&played) <= 1.0,
        "{} is past full scale",
        loudest(&played)
    );
}

/// A roll of thunder stops with its storm.
/// Given: a mixer that plays the storm on every tick for two seconds, with
/// a thunder cue after one second.
/// When: the storm cues stop.
/// Then: the roll is heard while the storm plays, and from one second after
/// the last storm cue the next three seconds are silent, though the roll
/// has more than ten seconds left.
///
/// Why it matters: a focus move or a change of weather fades the storm
/// out, and thunder over another scene is a sound with no picture.
#[test]
fn a_roll_of_thunder_stops_with_its_storm() {
    // Given:
    let (bolt, last, total) = (ticks(1.0), ticks(2.0), ticks(6.0));

    // When:
    let played = sounded(total, Cue::Storm, last, Cue::Thunder, &[bolt]);
    let storm = sounded(total, Cue::Storm, last, Cue::Thunder, &[]);

    // Then:
    let roll = decibels(&added(&played, &storm)[sample_of(bolt)..sample_of(last)]);
    assert!(roll >= -45.0, "the roll plays at {roll} dB in the storm");
    let after = sample_of(last) + seconds(1.0) * 2;
    assert_eq!(loudest(&played[after..]), 0.0);
}

/// A change of weather fades one sound into the next.
/// Given: a mixer that plays the rain on every tick for two seconds.
/// When: the cue changes to the snow.
/// Then: over the next 0.3 seconds the mixer adds at least -50 dB to the
/// snow alone, which is the rain on its way out. From one second after
/// the change it plays the same as a mixer that got only the snow, from
/// the tick of the change.
///
/// Why it matters: a refresh can change the weather, and a sound that cut
/// off, or one that played on under the next, would not follow the sky.
#[test]
fn a_change_of_weather_fades_one_sound_into_the_next() {
    // Given:
    let (change, total) = (ticks(2.0), ticks(4.0));
    let weather = |tick: usize| if tick < change { Cue::Rain } else { Cue::Snow };

    // When:
    let changed = followed(&mut Mixer::new(RATE), total, weather);
    let mut alone = Mixer::new(RATE);
    let mut silent = vec![0.0; sample_of(change)];
    alone.render(&mut silent);
    let snow = followed(&mut alone, total - change, |_| Cue::Snow);

    // Then:
    let after = &changed[sample_of(change)..];
    let out = seconds(0.3) * 2;
    let rain = decibels(&added(&after[..out], &snow[..out]));
    assert!(rain >= -50.0, "the rain plays out at {rain} dB");
    let settled = seconds(1.0) * 2;
    let differ = after[settled..]
        .iter()
        .zip(&snow[settled..])
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(differ, 0, "{differ} samples differ");
}

/// A scene recording stops at once when the mixer is turned off.
/// Given: a mixer that plays a scene's recording.
/// When: it is turned off.
/// Then: the next tick is silent.
///
/// Why it matters: `m` must stop the sound at once, not after its fade.
#[test]
fn turning_the_mixer_off_silences_a_scene_recording_at_once() {
    for cue in SCENE_RECORDINGS {
        // Given:
        let mut mixer = Mixer::new(RATE);
        played(&mut mixer, ticks(1.0), cue, |_| true);

        // When:
        mixer.set_on(false);
        let after = played(&mut mixer, 1, cue, |_| false);

        // Then:
        assert_eq!(loudest(after.as_flattened()), 0.0, "{cue:?}");
    }
}

/// Given: a mixer at full volume and a mixer at 50%.
/// When: both get the fire cue on every tick for three seconds.
/// Then: after the fade in, the one at 50% plays about 6 dB lower.
///
/// Why it matters: a volume must change how loud hearth plays and nothing
/// else, so 50% is half the amplitude of the same sound.
#[test]
fn half_volume_plays_the_same_cues_six_db_lower() {
    // Given:
    let mut full = Mixer::new(RATE);
    let mut half = Mixer::new(RATE);
    half.set_volume(Volume::from_percent(50).unwrap());

    // When:
    let loud = played(&mut full, ticks(3.0), Cue::Fire, |_| true);
    let quiet = played(&mut half, ticks(3.0), Cue::Fire, |_| true);

    // Then:
    let drop = decibels(quiet[seconds(0.5)..].as_flattened())
        - decibels(loud[seconds(0.5)..].as_flattened());
    assert!((-6.2..=-5.8).contains(&drop), "{drop} dB");
}

/// A volume of 0% silences every recording.
/// Given: a mixer at 0% volume.
/// When: the typing cue and each scene's cue come on every tick for three
/// seconds.
/// Then: every frame is silent.
///
/// Why it matters: 0% is the quietest level `[` reaches, and a user who
/// chooses it expects no sound.
#[test]
fn zero_volume_is_silent() {
    // Given:
    let mut mixer = Mixer::new(RATE);
    mixer.set_volume(Volume::from_percent(0).unwrap());

    // When:
    let mut frames = vec![0.0; tick_samples() * 2];
    let mut heard = 0.0_f32;
    for _ in 0..ticks(3.0) {
        mixer.play(Cue::Typing);
        for cue in SCENE_RECORDINGS {
            mixer.play(cue);
        }
        mixer.render(&mut frames);
        heard = heard.max(loudest(&frames));
    }

    // Then:
    assert_eq!(heard, 0.0);
}

/// Given: the whole numbers from -10 to 110.
/// When: each is read as a volume.
/// Then: only 0, 10, 20 and so on up to 100 are volumes.
///
/// Why it matters: `[` and `]` step the volume by 10%, so a level off that
/// grid could never come back to a round one.
#[test]
fn a_volume_is_a_step_of_ten_from_0_to_100() {
    let volumes: Vec<i64> = (-10..=110)
        .filter(|percent| Volume::from_percent(*percent).is_ok())
        .collect();

    assert_eq!(volumes, vec![0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100]);
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

/// Given: hearth started from a config file with `volume = 60` and a
/// comment.
/// When: the volume goes up five times, then down eleven times, saving to
/// the file, and hearth starts again from it.
/// Then: it stops at 100% and at 0%, the notice names each level, the file
/// keeps the comment, and the new start plays at 0%.
///
/// Why it matters: a volume that does not last until the next start makes
/// the user set it on every run.
#[test]
fn the_volume_steps_stops_at_the_ends_and_is_saved() {
    // Given:
    let path = config_file("volume", "# my settings\nvolume = 60\n");
    let start = |path: &PathBuf| {
        let config = Config::load(path).unwrap();
        App::configured(
            Some(SceneKind::Fire),
            7,
            Arc::new(Corpus::embedded()),
            &config,
        )
        .saving_to(path.clone())
    };
    let mut app = start(&path);
    assert_eq!(app.volume().percent(), 60);

    // When:
    for _ in 0..5 {
        app.louder();
    }

    // Then:
    assert_eq!(app.volume().percent(), 100);
    assert!(header(&app).contains("volume 100%"), "{}", header(&app));
    assert_eq!(Config::load(&path).unwrap().volume.percent(), 100);

    // When:
    for _ in 0..11 {
        app.quieter();
    }
    let again = start(&path);

    // Then:
    assert_eq!(app.volume().percent(), 0);
    assert!(header(&app).contains("volume 0%"), "{}", header(&app));
    assert!(fs::read_to_string(&path).unwrap().contains("# my settings"));
    assert_eq!(again.volume().percent(), 0);
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
