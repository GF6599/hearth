//! The cues a scene sends, the mixer that turns them into the recordings
//! they stand for, and the speaker that plays them.

use std::sync::OnceLock;

/// What happened in a scene on its last tick, for the mixer to sound.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cue {
    /// The scene typed at least one key.
    Typing,
    Fire,
    River,
    Tide {
        /// How far the wave on screen has run up the sand, from 0 at the
        /// bottom of a draw back to 1 at the top of the strongest run up.
        swell: f32,
    },
}

/// Seconds the sound keeps playing after its last cue, longer than the
/// longest pause at the end of a line at the default speed, so the sound
/// does not stutter at each line.
const HOLD: f32 = 0.3;
const FADE_IN: f32 = 0.1;
const FADE_OUT: f32 = 0.4;

/// The gains of the tide at the bottom of a draw back and at the top of the
/// strongest run up. The tide was recorded close to the sand, and its cut
/// copies the stream with no re-encode, so these bring it down to the level
/// of the fire. A wave's mean swell is about one half.
const SWELL_QUIET: f32 = 0.1;
const SWELL_LOUD: f32 = 0.25;
/// Seconds over which the gain follows the swell, so its step on each tick
/// is not heard.
const SWELL_GLIDE: f32 = 0.1;

/// How loud hearth plays, on top of the system volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Volume(u8);

impl Default for Volume {
    fn default() -> Self {
        Self::FULL
    }
}

impl Volume {
    pub const FULL: Volume = Volume(100);
    const STEP: u8 = 10;

    pub fn from_percent(percent: i64) -> Result<Volume, String> {
        u8::try_from(percent)
            .ok()
            .filter(|percent| *percent <= Self::FULL.0 && percent % Self::STEP == 0)
            .map(Volume)
            .ok_or_else(Self::range_message)
    }

    pub fn louder(self) -> Volume {
        Volume((self.0 + Self::STEP).min(Self::FULL.0))
    }

    pub fn quieter(self) -> Volume {
        Volume(self.0.saturating_sub(Self::STEP))
    }

    pub fn percent(self) -> u8 {
        self.0
    }

    pub fn range_message() -> String {
        format!(
            "a volume is a percent from 0 to {} in steps of {}",
            Self::FULL.0,
            Self::STEP
        )
    }

    fn gain(self) -> f32 {
        f32::from(self.0) / f32::from(Self::FULL.0)
    }
}

#[derive(Debug)]
struct Recording {
    frames: Vec<[f32; 2]>,
    rate: u32,
}

impl Recording {
    /// The decoder trims the encoder's delay and padding, so the recording
    /// loops with no gap.
    fn decode(mp3: &[u8]) -> Self {
        let decoded = nanomp3::decode_all::<f32>(mp3);
        let channels = decoded
            .channels
            .map_or(1, |channels| usize::from(channels.num()));
        let frames = decoded
            .samples
            .chunks_exact(channels)
            .map(|frame| [frame[0], frame[channels - 1]])
            .collect();
        Self {
            frames,
            rate: decoded.sample_rate,
        }
    }
}

fn typing() -> &'static Recording {
    static TYPING: OnceLock<Recording> = OnceLock::new();
    TYPING.get_or_init(|| Recording::decode(include_bytes!("../sounds/typing.mp3")))
}

fn fire() -> &'static Recording {
    static FIRE: OnceLock<Recording> = OnceLock::new();
    FIRE.get_or_init(|| Recording::decode(include_bytes!("../sounds/fire.mp3")))
}

fn river() -> &'static Recording {
    static RIVER: OnceLock<Recording> = OnceLock::new();
    RIVER.get_or_init(|| Recording::decode(include_bytes!("../sounds/river.mp3")))
}

fn tide() -> &'static Recording {
    static TIDE: OnceLock<Recording> = OnceLock::new();
    TIDE.get_or_init(|| Recording::decode(include_bytes!("../sounds/tide.mp3")))
}

/// A recording that loops while its cue keeps coming, and fades out when
/// the cue stops. It keeps its place while silent, so it goes on where it
/// left off.
#[derive(Debug)]
struct Bed {
    recording: &'static Recording,
    /// The place in the recording, in its frames.
    place: f64,
    /// The recording's frames for each output frame.
    step: f64,
    level: f32,
    rise: f32,
    fall: f32,
    /// Output frames left before the bed fades out.
    held: u32,
    hold: u32,
}

impl Bed {
    fn new(recording: &'static Recording, rate: f32) -> Self {
        Self {
            recording,
            place: 0.0,
            step: f64::from(recording.rate) / f64::from(rate),
            level: 0.0,
            rise: 1.0 / (FADE_IN * rate),
            fall: 1.0 / (FADE_OUT * rate),
            held: 0,
            hold: (HOLD * rate) as u32,
        }
    }

    fn cue(&mut self) {
        self.held = self.hold;
    }

    fn silence(&mut self) {
        self.held = 0;
        self.level = 0.0;
    }

    fn frame(&mut self) -> [f32; 2] {
        if self.held > 0 {
            self.held -= 1;
            self.level = (self.level + self.rise).min(1.0);
        } else {
            self.level = (self.level - self.fall).max(0.0);
        }
        let frames = &self.recording.frames;
        if self.level == 0.0 || frames.is_empty() {
            return [0.0; 2];
        }
        let index = self.place as usize;
        let share = (self.place - index as f64) as f32;
        let (now, next) = (frames[index], frames[(index + 1) % frames.len()]);
        self.place = (self.place + self.step) % frames.len() as f64;
        [0, 1].map(|c| (now[c] + (next[c] - now[c]) * share) * self.level)
    }
}

/// The gain of a recording that follows a value from a scene, and glides to
/// it because the value changes only on a tick.
#[derive(Debug)]
struct Swell {
    now: f32,
    target: f32,
    glide: f32,
}

impl Swell {
    fn new(rate: f32) -> Self {
        Self {
            now: 0.0,
            target: 0.0,
            glide: 1.0 / (SWELL_GLIDE * rate),
        }
    }

    fn gain(&mut self) -> f32 {
        self.now += (self.target - self.now) * self.glide;
        SWELL_QUIET + (SWELL_LOUD - SWELL_QUIET) * self.now
    }
}

/// Turns cues into stereo frames at one rate. It holds no device, so a
/// test renders it offline.
#[derive(Debug)]
pub struct Mixer {
    typing: Bed,
    fire: Bed,
    river: Bed,
    tide: Bed,
    swell: Swell,
    on: bool,
    gain: f32,
}

impl Mixer {
    /// Decodes each recording the first time a mixer is built, which takes
    /// a moment, so the speaker builds its mixer before the stream starts.
    pub fn new(rate: u32) -> Self {
        Self {
            typing: Bed::new(typing(), rate as f32),
            fire: Bed::new(fire(), rate as f32),
            river: Bed::new(river(), rate as f32),
            tide: Bed::new(tide(), rate as f32),
            swell: Swell::new(rate as f32),
            on: true,
            gain: Volume::FULL.gain(),
        }
    }

    /// A cue that arrives while the mixer is off is dropped, so it does
    /// not play late when the sound comes back.
    pub fn play(&mut self, cue: Cue) {
        if !self.on {
            return;
        }
        match cue {
            Cue::Typing => self.typing.cue(),
            Cue::Fire => self.fire.cue(),
            Cue::River => self.river.cue(),
            Cue::Tide { swell } => {
                self.tide.cue();
                self.swell.target = swell;
            }
        }
    }

    pub fn set_on(&mut self, on: bool) {
        self.on = on;
        if !on {
            self.typing.silence();
            self.fire.silence();
            self.river.silence();
            self.tide.silence();
        }
    }

    pub fn set_volume(&mut self, volume: Volume) {
        self.gain = volume.gain();
    }

    /// Fills `out` with interleaved left and right samples.
    pub fn render(&mut self, out: &mut [f32]) {
        for frame in out.as_chunks_mut::<2>().0 {
            *frame = self.frame();
        }
    }

    pub fn frame(&mut self) -> [f32; 2] {
        let swell = self.swell.gain();
        let beds = [
            self.typing.frame(),
            self.fire.frame(),
            self.river.frame(),
            self.tide.frame().map(|sample| sample * swell),
        ];
        [0, 1].map(|channel| beds.iter().map(|bed| bed[channel]).sum::<f32>() * self.gain)
    }
}

pub use speaker::Speaker;
#[cfg(target_os = "macos")]
pub use speaker::stops_the_sound;

/// The mixer on the audio thread of the default output. Cues reach it
/// through a channel whose receiver takes no lock, so the audio thread
/// never waits for the draw loop.
#[cfg(target_os = "macos")]
mod speaker {
    use std::sync::mpsc::{self, Receiver, Sender};

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    use super::{Cue, Mixer, Volume};

    enum Message {
        Cue(Cue),
        On(bool),
        Volume(Volume),
    }

    /// A glitch passes, and macOS moves the stream to a new default device
    /// by itself. Every other error leaves the stream silent.
    pub fn stops_the_sound(kind: cpal::ErrorKind) -> bool {
        !matches!(kind, cpal::ErrorKind::Xrun | cpal::ErrorKind::DeviceChanged)
    }

    pub struct Speaker {
        send: Sender<Message>,
        errors: Receiver<String>,
        on: bool,
        volume: Volume,
        stream: cpal::Stream,
    }

    impl std::fmt::Debug for Speaker {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("Speaker")
                .field("on", &self.on)
                .finish_non_exhaustive()
        }
    }

    impl Speaker {
        pub fn open(on: bool, volume: Volume) -> Result<Self, String> {
            let device = cpal::default_host()
                .default_output_device()
                .ok_or("no output device")?;
            let supported = device.default_output_config().map_err(|e| e.to_string())?;
            if supported.sample_format() != cpal::SampleFormat::F32 {
                return Err(format!(
                    "the output takes {} samples, not f32",
                    supported.sample_format()
                ));
            }
            let config = supported.config();
            let channels = usize::from(config.channels);
            let mut mixer = Mixer::new(config.sample_rate);
            mixer.set_on(on);
            mixer.set_volume(volume);
            let (send, receive) = mpsc::channel();
            let (failed, errors) = mpsc::channel();
            let stream = device
                .build_output_stream(
                    config,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        while let Ok(message) = receive.try_recv() {
                            match message {
                                Message::Cue(cue) => mixer.play(cue),
                                Message::On(on) => mixer.set_on(on),
                                Message::Volume(volume) => mixer.set_volume(volume),
                            }
                        }
                        for frame in data.chunks_mut(channels) {
                            let [left, right] = mixer.frame();
                            match frame {
                                [mono] => *mono = (left + right) / 2.0,
                                [l, r, rest @ ..] => {
                                    (*l, *r) = (left, right);
                                    rest.fill(0.0);
                                }
                                [] => {}
                            }
                        }
                    },
                    move |e: cpal::Error| {
                        if stops_the_sound(e.kind()) {
                            let _ = failed.send(e.to_string());
                        }
                    },
                    None,
                )
                .map_err(|e| e.to_string())?;
            let speaker = Self {
                send,
                errors,
                on,
                volume,
                stream,
            };
            speaker.run()?;
            Ok(speaker)
        }

        /// The stream reads no cue while the sound is off, so none is sent
        /// then and the channel does not grow. A send fails only when the
        /// audio thread has ended, and the error that ended it arrives
        /// through `take_error`.
        pub fn play(&self, cues: impl IntoIterator<Item = Cue>) {
            if !self.on {
                return;
            }
            for cue in cues {
                let _ = self.send.send(Message::Cue(cue));
            }
        }

        pub fn set_on(&mut self, on: bool) -> Result<(), String> {
            if on == self.on {
                return Ok(());
            }
            self.on = on;
            let _ = self.send.send(Message::On(on));
            self.run()
        }

        pub fn set_volume(&mut self, volume: Volume) {
            if volume != self.volume {
                self.volume = volume;
                let _ = self.send.send(Message::Volume(volume));
            }
        }

        /// The stream stops while the sound is off, so a muted hearth
        /// keeps the audio device idle.
        fn run(&self) -> Result<(), String> {
            if self.on {
                self.stream.play().map_err(|e| e.to_string())
            } else {
                self.stream.pause().map_err(|e| e.to_string())
            }
        }

        /// Why the sound stopped while it played, such as a device that
        /// went away.
        pub fn take_error(&self) -> Option<String> {
            self.errors.try_recv().ok()
        }
    }
}

/// Off macOS there is no speaker: `cpal` on Linux links the ALSA library,
/// and the release links its Linux builds with zig, which has no copy of it.
#[cfg(not(target_os = "macos"))]
mod speaker {
    use super::{Cue, Volume};

    #[derive(Debug)]
    pub struct Speaker;

    impl Speaker {
        pub fn open(_on: bool, _volume: Volume) -> Result<Self, String> {
            Err("sound plays on macOS only".to_string())
        }

        pub fn play(&self, _cues: impl IntoIterator<Item = Cue>) {}

        pub fn set_on(&mut self, _on: bool) -> Result<(), String> {
            Ok(())
        }

        pub fn set_volume(&mut self, _volume: Volume) {}

        pub fn take_error(&self) -> Option<String> {
            None
        }
    }
}
