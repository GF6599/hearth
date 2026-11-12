//! The cues a scene sends, the mixer that turns them into the recordings
//! they stand for, and the speaker that plays them.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

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
    /// The weather scene shows a clear sky by day. Each sky of the weather
    /// has a cue of its own.
    ClearDay,
    ClearNight,
    Cloud,
    Rain,
    Snow,
    Storm,
    /// A bolt struck in the storm.
    Thunder,
    Fog,
    /// A horn sounded far off in the fog.
    Foghorn,
    Wind {
        /// How strongly the gust lines on screen show, from 0 to 1.
        gust: f32,
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
/// The gains of the wind in its weakest gust and in its strongest. A mean
/// gust is about one half, which plays at the level of the fire.
const GUST_QUIET: f32 = 0.5;
const GUST_LOUD: f32 = 2.3;
/// Seconds over which the gain follows the swell or the gust, so its step
/// on each tick is not heard.
const SWELL_GLIDE: f32 = 0.1;

/// The times a shot can sound at once. A storm at its own speed strikes
/// often enough for four rolls of thunder to overlap, and no more.
const VOICES: usize = 4;
/// Seconds over which a shot fades in at the start of its recording and
/// out at its end. The thunder starts on its clap. The foghorn is cut from
/// the middle of a recording of the sea, which would start and stop with a
/// step.
const THUNDER_EDGES: (f32, f32) = (0.02, 1.0);
const FOGHORN_EDGES: (f32, f32) = (1.5, 2.0);

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

/// An mp3 in the binary, and its recording once a cue has asked for it. A
/// decoded recording takes up to 17 MB, so a scene that never runs costs
/// nothing.
#[derive(Debug)]
struct Track {
    mp3: &'static [u8],
    /// What brings the recording to the loudness of the fire. The tide and
    /// the wind take their gains from their scenes.
    gain: f32,
    recording: OnceLock<Recording>,
    preparing: AtomicBool,
}

impl Track {
    const fn new(mp3: &'static [u8], gain: f32) -> Self {
        Self {
            mp3,
            gain,
            recording: OnceLock::new(),
            preparing: AtomicBool::new(false),
        }
    }

    fn recording(&self) -> &Recording {
        self.recording.get_or_init(|| Recording::decode(self.mp3))
    }

    fn decoded(&self) -> bool {
        self.recording.get().is_some()
    }
}

static TYPING: Track = Track::new(include_bytes!("../sounds/typing.mp3"), 1.0);
static FIRE: Track = Track::new(include_bytes!("../sounds/fire.mp3"), 1.0);
static RIVER: Track = Track::new(include_bytes!("../sounds/river.mp3"), 1.0);
static TIDE: Track = Track::new(include_bytes!("../sounds/tide.mp3"), 1.0);
// The meadow and the cave of the fog were recorded more than 30 dB under
// the storm and the wind of the snow.
static CLEAR_DAY: Track = Track::new(include_bytes!("../sounds/clear_day.mp3"), 8.8);
static CLEAR_NIGHT: Track = Track::new(include_bytes!("../sounds/clear_night.mp3"), 2.0);
static CLOUD: Track = Track::new(include_bytes!("../sounds/cloud.mp3"), 4.6);
static RAIN: Track = Track::new(include_bytes!("../sounds/rain.mp3"), 0.56);
static SNOW: Track = Track::new(include_bytes!("../sounds/snow.mp3"), 0.2);
static STORM: Track = Track::new(include_bytes!("../sounds/storm.mp3"), 0.2);
static THUNDER: Track = Track::new(include_bytes!("../sounds/thunder.mp3"), 0.5);
static FOG: Track = Track::new(include_bytes!("../sounds/fog.mp3"), 5.4);
// The horn is far off, so it plays under the level of the others.
static FOGHORN: Track = Track::new(include_bytes!("../sounds/foghorn.mp3"), 0.15);
static WIND: Track = Track::new(include_bytes!("../sounds/wind.mp3"), 1.0);

/// The recording of `cue`, and the recording of the shot that sounds over
/// it. The two decode together, so the first roll of thunder in a storm is
/// not lost to its decode.
fn tracks(cue: Cue) -> (&'static Track, Option<&'static Track>) {
    match cue {
        Cue::Typing => (&TYPING, None),
        Cue::Fire => (&FIRE, None),
        Cue::River => (&RIVER, None),
        Cue::Tide { .. } => (&TIDE, None),
        Cue::ClearDay => (&CLEAR_DAY, None),
        Cue::ClearNight => (&CLEAR_NIGHT, None),
        Cue::Cloud => (&CLOUD, None),
        Cue::Rain => (&RAIN, None),
        Cue::Snow => (&SNOW, None),
        Cue::Storm => (&STORM, Some(&THUNDER)),
        Cue::Thunder => (&THUNDER, None),
        Cue::Fog => (&FOG, Some(&FOGHORN)),
        Cue::Foghorn => (&FOGHORN, None),
        Cue::Wind { .. } => (&WIND, None),
    }
}

/// Says if the recordings of `cue` are decoded, so the cue plays with no
/// wait.
pub fn decoded(cue: Cue) -> bool {
    let (track, over) = tracks(cue);
    track.decoded() && over.is_none_or(Track::decoded)
}

/// Starts the decode of the recordings of `cue` on a thread of its own,
/// once, and returns at once. A thread that does not start leaves the next
/// call to try again.
pub fn prepare(cue: Cue) {
    let (track, over) = tracks(cue);
    if decoded(cue) || track.preparing.swap(true, Ordering::AcqRel) {
        return;
    }
    let started = thread::Builder::new().name("sound".into()).spawn(move || {
        track.recording();
        if let Some(over) = over {
            over.recording();
        }
    });
    if started.is_err() {
        track.preparing.store(false, Ordering::Release);
    }
}

/// A recording that loops while its cue keeps coming, and fades out when
/// the cue stops. It keeps its place while silent, so it goes on where it
/// left off. It is silent until its recording is decoded, and it never
/// decodes in `frame`, which runs on the audio thread.
#[derive(Debug)]
struct Bed {
    track: &'static Track,
    /// The place in the recording, in its frames.
    place: f64,
    /// Output frames a second.
    rate: f64,
    level: f32,
    rise: f32,
    fall: f32,
    /// Output frames left before the bed fades out.
    held: u32,
    hold: u32,
}

impl Bed {
    fn new(track: &'static Track, rate: f32) -> Self {
        Self {
            track,
            place: 0.0,
            rate: f64::from(rate),
            level: 0.0,
            rise: 1.0 / (FADE_IN * rate),
            fall: 1.0 / (FADE_OUT * rate),
            held: 0,
            hold: (HOLD * rate) as u32,
        }
    }

    fn cue(&mut self) {
        self.track.recording();
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
        let Some(recording) = self.track.recording.get() else {
            return [0.0; 2];
        };
        let frames = &recording.frames;
        if self.level == 0.0 || frames.is_empty() {
            return [0.0; 2];
        }
        let step = f64::from(recording.rate) / self.rate;
        let index = self.place as usize;
        let share = (self.place - index as f64) as f32;
        let (now, next) = (frames[index], frames[(index + 1) % frames.len()]);
        self.place = (self.place + step) % frames.len() as f64;
        let gain = self.level * self.track.gain;
        [0, 1].map(|c| (now[c] + (next[c] - now[c]) * share) * gain)
    }
}

/// A recording that plays through once on each cue, such as the thunder of
/// a bolt. Cues close together overlap, each in a voice of its own. A cue
/// that finds every voice in use is dropped, so no roll that sounds is cut.
/// Like a bed, it is silent until its recording is decoded.
#[derive(Debug)]
struct Shot {
    track: &'static Track,
    /// The place of each voice that sounds, in the recording's frames.
    voices: [Option<f64>; VOICES],
    /// Output frames a second.
    rate: f64,
    /// Seconds of the fade in and of the fade out.
    edges: (f64, f64),
}

impl Shot {
    fn new(track: &'static Track, rate: f32, edges: (f32, f32)) -> Self {
        Self {
            track,
            voices: [None; VOICES],
            rate: f64::from(rate),
            edges: (f64::from(edges.0), f64::from(edges.1)),
        }
    }

    fn cue(&mut self) {
        self.track.recording();
        if let Some(free) = self.voices.iter_mut().find(|voice| voice.is_none()) {
            *free = Some(0.0);
        }
    }

    fn silence(&mut self) {
        self.voices = [None; VOICES];
    }

    fn frame(&mut self) -> [f32; 2] {
        let mut sum = [0.0; 2];
        let Some(recording) = self.track.recording.get() else {
            return sum;
        };
        let frames = &recording.frames;
        let rate = f64::from(recording.rate);
        for voice in &mut self.voices {
            let Some(place) = *voice else {
                continue;
            };
            let index = place as usize;
            if index + 1 >= frames.len() {
                *voice = None;
                continue;
            }
            let left = (frames.len() - 1) as f64 - place;
            let edge = (place / (self.edges.0 * rate))
                .min(left / (self.edges.1 * rate))
                .min(1.0);
            let share = (place - index as f64) as f32;
            let (now, next) = (frames[index], frames[index + 1]);
            let gain = edge as f32 * self.track.gain;
            sum = [0, 1].map(|c| sum[c] + (now[c] + (next[c] - now[c]) * share) * gain);
            *voice = Some(place + rate / self.rate);
        }
        sum
    }
}

/// The gain of a recording that follows a value from a scene, and glides to
/// it because the value changes only on a tick.
#[derive(Debug)]
struct Swell {
    now: f32,
    target: f32,
    glide: f32,
    /// The gains at a value of 0 and at a value of 1.
    gains: (f32, f32),
}

impl Swell {
    fn new(rate: f32, gains: (f32, f32)) -> Self {
        Self {
            now: 0.0,
            target: 0.0,
            glide: 1.0 / (SWELL_GLIDE * rate),
            gains,
        }
    }

    fn gain(&mut self) -> f32 {
        self.now += (self.target - self.now) * self.glide;
        self.gains.0 + (self.gains.1 - self.gains.0) * self.now
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
    clear_day: Bed,
    clear_night: Bed,
    cloud: Bed,
    rain: Bed,
    snow: Bed,
    storm: Bed,
    thunder: Shot,
    fog: Bed,
    foghorn: Shot,
    wind: Bed,
    gust: Swell,
    on: bool,
    gain: f32,
}

impl Mixer {
    pub fn new(rate: u32) -> Self {
        let rate = rate as f32;
        Self {
            typing: Bed::new(&TYPING, rate),
            fire: Bed::new(&FIRE, rate),
            river: Bed::new(&RIVER, rate),
            tide: Bed::new(&TIDE, rate),
            swell: Swell::new(rate, (SWELL_QUIET, SWELL_LOUD)),
            clear_day: Bed::new(&CLEAR_DAY, rate),
            clear_night: Bed::new(&CLEAR_NIGHT, rate),
            cloud: Bed::new(&CLOUD, rate),
            rain: Bed::new(&RAIN, rate),
            snow: Bed::new(&SNOW, rate),
            storm: Bed::new(&STORM, rate),
            thunder: Shot::new(&THUNDER, rate, THUNDER_EDGES),
            fog: Bed::new(&FOG, rate),
            foghorn: Shot::new(&FOGHORN, rate, FOGHORN_EDGES),
            wind: Bed::new(&WIND, rate),
            gust: Swell::new(rate, (GUST_QUIET, GUST_LOUD)),
            on: true,
            gain: Volume::FULL.gain(),
        }
    }

    /// A cue that arrives while the mixer is off is dropped, so it does
    /// not play late when the sound comes back. The first cue of a
    /// recording decodes it and waits for it, unless `prepare` did so before.
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
            Cue::ClearDay => self.clear_day.cue(),
            Cue::ClearNight => self.clear_night.cue(),
            Cue::Cloud => self.cloud.cue(),
            Cue::Rain => self.rain.cue(),
            Cue::Snow => self.snow.cue(),
            Cue::Storm => self.storm.cue(),
            Cue::Thunder => self.thunder.cue(),
            Cue::Fog => self.fog.cue(),
            Cue::Foghorn => self.foghorn.cue(),
            Cue::Wind { gust } => {
                self.wind.cue();
                self.gust.target = gust;
            }
        }
    }

    pub fn set_on(&mut self, on: bool) {
        self.on = on;
        if !on {
            for bed in [
                &mut self.typing,
                &mut self.fire,
                &mut self.river,
                &mut self.tide,
                &mut self.clear_day,
                &mut self.clear_night,
                &mut self.cloud,
                &mut self.rain,
                &mut self.snow,
                &mut self.storm,
                &mut self.fog,
                &mut self.wind,
            ] {
                bed.silence();
            }
            self.thunder.silence();
            self.foghorn.silence();
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

    /// A shot plays only as loud as the bed of its weather, so the thunder
    /// fades out with its storm and the foghorn with its fog.
    pub fn frame(&mut self) -> [f32; 2] {
        let (swell, gust) = (self.swell.gain(), self.gust.gain());
        let (storm, fog) = (self.storm.frame(), self.fog.frame());
        let (stormy, foggy) = (self.storm.level, self.fog.level);
        let parts = [
            self.typing.frame(),
            self.fire.frame(),
            self.river.frame(),
            self.tide.frame().map(|sample| sample * swell),
            self.clear_day.frame(),
            self.clear_night.frame(),
            self.cloud.frame(),
            self.rain.frame(),
            self.snow.frame(),
            storm,
            self.thunder.frame().map(|sample| sample * stormy),
            fog,
            self.foghorn.frame().map(|sample| sample * foggy),
            self.wind.frame().map(|sample| sample * gust),
        ];
        [0, 1].map(|channel| parts.iter().map(|part| part[channel]).sum::<f32>() * self.gain)
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

    use super::{Cue, Mixer, Volume, decoded, prepare};

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
        /// through `take_error`. A cue whose recording is not decoded yet
        /// starts the decode and is dropped, so the audio thread never
        /// decodes.
        pub fn play(&self, cues: impl IntoIterator<Item = Cue>) {
            if !self.on {
                return;
            }
            for cue in cues {
                if decoded(cue) {
                    let _ = self.send.send(Message::Cue(cue));
                } else {
                    prepare(cue);
                }
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
