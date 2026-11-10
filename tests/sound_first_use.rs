#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::thread;
use std::time::{Duration, Instant};

use hearth::sound::{self, Cue, Mixer};

const RATE: u32 = 48_000;
const CUES: [Cue; 4] = [Cue::Typing, Cue::Fire, Cue::River, Cue::Tide { swell: 1.0 }];

fn decoded() -> Vec<Cue> {
    CUES.into_iter()
        .filter(|cue| sound::decoded(*cue))
        .collect()
}

/// A recording decodes when its cue first comes, and no other does.
/// Given: a new mixer, in a process that has played nothing.
/// When: the river is prepared, and then the fire cue plays in the mixer.
/// Then: the mixer decodes nothing by itself. The call to prepare returns
/// before the river is decoded, and the river is decoded soon after. The
/// fire is heard and decoded on its first cue. The typing and the tide
/// stay as they were.
///
/// Why it matters: a decoded recording takes up to 17 MB, so a start that decodes
/// each one pays for scenes that never run, and a decode on the audio
/// thread stops every sound for its length.
#[test]
fn a_recording_decodes_when_its_cue_first_comes() {
    // Given:
    let mut mixer = Mixer::new(RATE);
    assert_eq!(decoded(), [], "decoded by a new mixer");

    // When:
    sound::prepare(Cue::River);

    // Then:
    assert_eq!(decoded(), [], "decoded when prepare returns");
    let deadline = Instant::now() + Duration::from_secs(30);
    while !sound::decoded(Cue::River) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(decoded(), [Cue::River]);

    // And when:
    let mut frames = vec![0.0; RATE as usize * 2];
    mixer.play(Cue::Fire);
    mixer.render(&mut frames);

    // Then:
    let loudest = frames.iter().fold(0.0_f32, |most, s| most.max(s.abs()));
    assert!(loudest >= 1e-3, "the fire plays at {loudest} at the most");
    assert_eq!(decoded(), [Cue::Fire, Cue::River]);
}
