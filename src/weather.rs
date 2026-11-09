//! The current weather at a place, read from Open-Meteo.

use std::fmt;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use serde_json::Value;
use ureq::Agent;
use ureq::tls::{Certificate, RootCerts, TlsConfig, TlsProvider};

/// Open-Meteo's forecast service. It needs no key, and its free use is for
/// non-commercial purposes only.
pub const OPEN_METEO: &str = "https://api.open-meteo.com";

const CURRENT: &str = "weather_code,temperature_2m,wind_speed_10m,is_day";

/// The wind speed in km/h from which a clear or cloudy sky reads as wind:
/// force 6 on the Beaufort scale, a strong breeze. No weather code means
/// wind.
const WIND: f64 = 39.0;

/// Open-Meteo's current data holds for 15 minutes.
const REFRESH: Duration = Duration::from_secs(15 * 60);
const RETRY: Duration = Duration::from_secs(60);
const TIMEOUT: Duration = Duration::from_secs(15);
/// A current reply is under 1 KB, so a larger one is not the weather.
const LIMIT: u64 = 64 * 1024;

const UNREACHABLE: &str = "cannot reach Open-Meteo";
const UNREADABLE: &str = "Open-Meteo sent a reply hearth cannot read";

pub const LATITUDE: f64 = 90.0;
pub const LONGITUDE: f64 = 180.0;

/// `value` if it lies from `-limit` to `limit`. A NaN lies nowhere.
pub fn degrees(value: f64, limit: f64) -> Result<f64, String> {
    if (-limit..=limit).contains(&value) {
        Ok(value)
    } else {
        Err(format!("must be from -{limit} to {limit}"))
    }
}

/// A place on the WGS84 globe, in degrees north and east.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Location {
    latitude: f64,
    longitude: f64,
}

impl Location {
    pub fn new(latitude: f64, longitude: f64) -> Result<Self, String> {
        Ok(Self {
            latitude: degrees(latitude, LATITUDE).map_err(|e| format!("latitude {e}"))?,
            longitude: degrees(longitude, LONGITUDE).map_err(|e| format!("longitude {e}"))?,
        })
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let north = if self.latitude < 0.0 { 'S' } else { 'N' };
        let east = if self.longitude < 0.0 { 'W' } else { 'E' };
        write!(
            f,
            "{:.2} {north} {:.2} {east}",
            self.latitude.abs(),
            self.longitude.abs()
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Clear,
    Cloud,
    Rain,
    Snow,
    Storm,
    Fog,
    Wind,
}

/// The weather at one moment, in Open-Meteo's default units: °C and km/h.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    words: &'static str,
    coded: Type,
    temperature: f64,
    wind: f64,
    day: bool,
}

impl Report {
    pub fn status(&self, at: Location) -> String {
        let weather = if self.weather() == Type::Wind {
            format!("wind {} km/h", self.wind.round() as i64)
        } else {
            self.words.to_string()
        };
        format!("{weather} · {} °C · {at}", self.temperature.round() as i64)
    }

    pub fn weather(&self) -> Type {
        let calm = matches!(self.coded, Type::Clear | Type::Cloud);
        if calm && self.wind >= WIND {
            Type::Wind
        } else {
            self.coded
        }
    }

    pub fn day(&self) -> bool {
        self.day
    }
}

/// The WMO weather codes that Open-Meteo sends, in its own words.
fn meaning(code: u8) -> Option<(&'static str, Type)> {
    Some(match code {
        0 => ("clear sky", Type::Clear),
        1 => ("mainly clear", Type::Clear),
        2 => ("partly cloudy", Type::Cloud),
        3 => ("overcast", Type::Cloud),
        45 => ("fog", Type::Fog),
        48 => ("depositing rime fog", Type::Fog),
        51 => ("light drizzle", Type::Rain),
        53 => ("moderate drizzle", Type::Rain),
        55 => ("dense drizzle", Type::Rain),
        56 => ("light freezing drizzle", Type::Rain),
        57 => ("dense freezing drizzle", Type::Rain),
        61 => ("slight rain", Type::Rain),
        63 => ("moderate rain", Type::Rain),
        65 => ("heavy rain", Type::Rain),
        66 => ("light freezing rain", Type::Rain),
        67 => ("heavy freezing rain", Type::Rain),
        71 => ("slight snowfall", Type::Snow),
        73 => ("moderate snowfall", Type::Snow),
        75 => ("heavy snowfall", Type::Snow),
        77 => ("snow grains", Type::Snow),
        80 => ("slight rain showers", Type::Rain),
        81 => ("moderate rain showers", Type::Rain),
        82 => ("violent rain showers", Type::Rain),
        85 => ("slight snow showers", Type::Snow),
        86 => ("heavy snow showers", Type::Snow),
        95 => ("thunderstorm", Type::Storm),
        96 => ("thunderstorm with slight hail", Type::Storm),
        97 => ("heavy thunderstorm", Type::Storm),
        99 => ("thunderstorm with heavy hail", Type::Storm),
        _ => return None,
    })
}

pub fn parse(body: &str) -> Result<Report, String> {
    let reply: Value = serde_json::from_str(body).map_err(|_| UNREADABLE.to_string())?;
    let current = reply.get("current").ok_or(UNREADABLE)?;
    let number = |name: &str| {
        current
            .get(name)
            .and_then(Value::as_f64)
            .ok_or_else(|| format!("Open-Meteo sent no {name}"))
    };
    let sent = current
        .get("weather_code")
        .ok_or("Open-Meteo sent no weather_code")?;
    let known = sent
        .as_u64()
        .and_then(|code| u8::try_from(code).ok())
        .and_then(meaning);
    let (words, coded) = known.ok_or_else(|| {
        format!("Open-Meteo sent weather code {sent}, which hearth does not know")
    })?;
    Ok(Report {
        words,
        coded,
        temperature: number("temperature_2m")?,
        wind: number("wind_speed_10m")?,
        day: number("is_day")? != 0.0,
    })
}

#[derive(Debug, Clone)]
pub struct Source {
    base: String,
    location: Location,
    refresh: Duration,
    retry: Duration,
}

impl Source {
    pub fn new(base: &str, location: Location) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            location,
            refresh: REFRESH,
            retry: RETRY,
        }
    }

    /// Reads again after `refresh`, or after `retry` when a read fails.
    pub fn every(self, refresh: Duration, retry: Duration) -> Self {
        Self {
            refresh,
            retry,
            ..self
        }
    }

    pub fn open_meteo(location: Location) -> Self {
        Self::new(OPEN_METEO, location)
    }

    pub fn location(&self) -> Location {
        self.location
    }

    fn fetch(&self, agent: &Agent) -> Result<Report, String> {
        let url = format!(
            "{}/v1/forecast?latitude={}&longitude={}&current={CURRENT}",
            self.base, self.location.latitude, self.location.longitude
        );
        let mut response = agent.get(&url).call().map_err(|_| UNREACHABLE)?;
        let status = response.status();
        let body = response
            .body_mut()
            .with_config()
            .limit(LIMIT)
            .read_to_string()
            .map_err(|_| UNREACHABLE)?;
        if status.is_success() {
            return parse(&body);
        }
        let reason = serde_json::from_str::<Value>(&body).ok().and_then(|reply| {
            reply
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
        Err(match reason {
            Some(reason) => format!("Open-Meteo: {reason}"),
            None => format!("Open-Meteo answered {}", status.as_u16()),
        })
    }

    /// Reads the weather on a thread of its own: at once, then each time it
    /// may have changed, and sooner after a failure. `None` when no thread
    /// starts.
    pub fn watch(self) -> Option<Watch> {
        let (sender, reads) = mpsc::channel();
        let (stop, stopped) = mpsc::channel::<()>();
        thread::Builder::new()
            .name("weather".into())
            .spawn(move || {
                let agent = agent();
                loop {
                    let read = self.fetch(&agent);
                    let wait = if read.is_ok() {
                        self.refresh
                    } else {
                        self.retry
                    };
                    if sender.send(read).is_err() {
                        return;
                    }
                    if stopped.recv_timeout(wait) != Err(RecvTimeoutError::Timeout) {
                        return;
                    }
                }
            })
            .ok()
            .map(|_| Watch { reads, _stop: stop })
    }
}

/// The reads of a watch on its way. Dropping the watch ends its thread at
/// once, even mid-wait, so a closed scene sends the place nowhere again.
#[derive(Debug)]
pub struct Watch {
    reads: Receiver<Result<Report, String>>,
    _stop: Sender<()>,
}

impl Watch {
    /// The newest read since the last call, if any.
    pub fn latest(&self) -> Option<Result<Report, String>> {
        self.reads.try_iter().last()
    }
}

/// An HTTP client that trusts the system's root certificates and does its
/// TLS in pure Rust. A status other than 2xx is a reply to read, because a
/// refusal carries its reason in the body.
fn agent() -> Agent {
    let roots: Vec<Certificate<'static>> = rustls_native_certs::load_native_certs()
        .certs
        .iter()
        .map(|der| Certificate::from_der(der.as_ref()).to_owned())
        .collect();
    let tls = TlsConfig::builder()
        .provider(TlsProvider::Rustls)
        .root_certs(RootCerts::new_with_certs(&roots))
        .unversioned_rustls_crypto_provider(Arc::new(rustls_rustcrypto::provider()))
        .build();
    Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build()
        .new_agent()
}
