//! The cues a scene sends, the synth that turns them into samples, and the
//! speaker that plays them.

use std::f32::consts::TAU;
use std::ops::Range;

use rand::{RngExt, SeedableRng};

use crate::scene::{Rng, TICK_RATE};
use crate::splitmix;

/// What happened in a scene on its last tick, for the synth to sound.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cue {
    /// A key struck `at` its share of the way through the tick, from 0 up
    /// to but not including 1.
    Key { at: f32 },
}

/// The clicks that can ring at once. The synth makes room for all of them
/// when it is built, because the audio thread must not wait on the
/// allocator. A key past the last voice is dropped.
const VOICES: usize = 64;
const KEY_LEVEL: Range<f32> = 0.2..0.35;
/// The cut-off of the noise in a click, in hertz: a lower one sounds like
/// a softer key.
const KEY_BRIGHTNESS: Range<f32> = 2500.0..5500.0;
/// The click loses its lows below this, so it snaps and does not thud.
const KEY_FLOOR: f32 = 500.0;
/// The pitch of the knock of the key case under the snap, in hertz.
const KEY_PITCH: Range<f32> = 140.0..260.0;
/// Seconds for the snap and for the knock to fall to about a third.
const KEY_SNAP: Range<f32> = 0.0012..0.0022;
const KEY_KNOCK: Range<f32> = 0.006..0.010;
const KNOCK_SHARE: f32 = 0.5;
/// A click ends when it has fallen this far, about 0.1 % of its start.
const KEY_RING: f32 = 7.0;
/// Seconds for the sound to fade in or out a third of the way, short
/// enough to feel at once and long enough not to click.
const FADE: f32 = 0.005;

#[derive(Debug)]
struct Click {
    /// Samples before the click starts.
    wait: u32,
    level: f32,
    bright: f32,
    floor: f32,
    low: f32,
    lowest: f32,
    snap: f32,
    snap_fall: f32,
    knock: f32,
    knock_fall: f32,
    phase: f32,
    step: f32,
}

impl Click {
    /// The next sample, or `None` once the click has rung out.
    fn sample(&mut self, noise: f32) -> Option<f32> {
        if self.wait > 0 {
            self.wait -= 1;
            return Some(0.0);
        }
        if self.knock < (-KEY_RING).exp() {
            return None;
        }
        self.low += self.bright * (noise - self.low);
        self.lowest += self.floor * (self.low - self.lowest);
        let snap = (self.low - self.lowest) * self.snap;
        let knock = self.phase.sin() * self.knock * KNOCK_SHARE;
        self.snap *= self.snap_fall;
        self.knock *= self.knock_fall;
        self.phase = (self.phase + self.step) % TAU;
        Some(self.level * (snap + knock))
    }
}

/// Turns cues into mono samples at one rate. It holds no device, so a test
/// renders it offline. A seed fixes every sample.
#[derive(Debug)]
pub struct Synth {
    rate: f32,
    rng: Rng,
    noise: u64,
    clicks: Vec<Click>,
    on: bool,
    gain: f32,
    fade: f32,
}

impl Synth {
    pub fn new(rate: u32, seed: u64) -> Self {
        let rate = rate as f32;
        let mut rng = Rng::seed_from_u64(seed);
        Self {
            rate,
            noise: rng.random(),
            rng,
            clicks: Vec::with_capacity(VOICES),
            on: true,
            gain: 1.0,
            fade: 1.0 - (-1.0 / (FADE * rate)).exp(),
        }
    }

    /// A cue that arrives while the synth is off is dropped, so it does not
    /// play late when the sound comes back.
    pub fn play(&mut self, cue: Cue) {
        if !self.on || self.clicks.len() >= VOICES {
            return;
        }
        match cue {
            Cue::Key { at } => {
                let tick = TICK_RATE.as_secs_f32() * self.rate;
                let click = self.click((at.clamp(0.0, 1.0) * tick) as u32);
                self.clicks.push(click);
            }
        }
    }

    fn click(&mut self, wait: u32) -> Click {
        let pole = |hertz: f32| 1.0 - (-TAU * hertz / self.rate).exp();
        let fall = |seconds: f32| (-1.0 / (seconds * self.rate)).exp();
        Click {
            wait,
            level: self.rng.random_range(KEY_LEVEL),
            bright: pole(self.rng.random_range(KEY_BRIGHTNESS)),
            floor: pole(KEY_FLOOR),
            low: 0.0,
            lowest: 0.0,
            snap: 1.0,
            snap_fall: fall(self.rng.random_range(KEY_SNAP)),
            knock: 1.0,
            knock_fall: fall(self.rng.random_range(KEY_KNOCK)),
            phase: 0.0,
            step: TAU * self.rng.random_range(KEY_PITCH) / self.rate,
        }
    }

    /// Off fades the sound out. On fades it in again, and the clicks that
    /// were ringing as it went off do not come back.
    pub fn set_on(&mut self, on: bool) {
        if on && !self.on {
            self.clicks.clear();
        }
        self.on = on;
    }

    pub fn render(&mut self, out: &mut [f32]) {
        for sample in out {
            *sample = self.sample();
        }
    }

    /// The soft limit keeps every sample under full scale, however many
    /// clicks ring at once.
    pub fn sample(&mut self) -> f32 {
        let mut mix = 0.0;
        let noise = &mut self.noise;
        self.clicks.retain_mut(|click| {
            *noise = noise.wrapping_add(1);
            let white = (splitmix::mix(*noise) >> 40) as f32 / (1u64 << 23) as f32 - 1.0;
            click.sample(white).map(|sample| mix += sample).is_some()
        });
        let target = if self.on { 1.0 } else { 0.0 };
        self.gain += (target - self.gain) * self.fade;
        (mix * self.gain).tanh()
    }
}

pub use speaker::Speaker;
#[cfg(target_os = "macos")]
pub use speaker::stops_the_sound;

/// The synth on the audio thread of the default output. Cues reach it
/// through a channel whose receiver takes no lock, so the audio thread
/// never waits for the draw loop.
#[cfg(target_os = "macos")]
mod speaker {
    use std::sync::mpsc::{self, Receiver, Sender};

    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    use super::{Cue, Synth};

    enum Message {
        Cue(Cue),
        On(bool),
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
        pub fn open(seed: u64, on: bool) -> Result<Self, String> {
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
            let mut synth = Synth::new(config.sample_rate, seed);
            synth.set_on(on);
            let (send, receive) = mpsc::channel();
            let (failed, errors) = mpsc::channel();
            let stream = device
                .build_output_stream(
                    config,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        while let Ok(message) = receive.try_recv() {
                            match message {
                                Message::Cue(cue) => synth.play(cue),
                                Message::On(on) => synth.set_on(on),
                            }
                        }
                        for frame in data.chunks_mut(channels) {
                            frame.fill(synth.sample());
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
    use super::Cue;

    #[derive(Debug)]
    pub struct Speaker;

    impl Speaker {
        pub fn open(_seed: u64, _on: bool) -> Result<Self, String> {
            Err("sound plays on macOS only".to_string())
        }

        pub fn play(&self, _cues: impl IntoIterator<Item = Cue>) {}

        pub fn set_on(&mut self, _on: bool) -> Result<(), String> {
            Ok(())
        }

        pub fn take_error(&self) -> Option<String> {
            None
        }
    }
}
