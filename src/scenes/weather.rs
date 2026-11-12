use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::scene::{self, Rng, Scene};
use crate::sound::Cue;
use crate::theme::Palette;
use crate::weather::{Location, Report, Source, Watch};

mod sky;

use sky::Sky;

pub const READING: &str = "reading the weather";
pub const NO_LOCATION: &str = "set latitude and longitude in [weather] to see the weather";
/// The licence of Open-Meteo's data, CC BY 4.0, asks for this credit next
/// to every place the data shows.
pub const CREDIT: &str = "weather data by Open-Meteo.com";
const NO_THREAD: &str = "cannot start a thread to read the weather";

/// How the scene draws the weather: as a landscape with the weather in it,
/// or as one large icon with the temperature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Sky,
    Emblem,
}

impl View {
    pub const NAMES: &str = "sky or emblem";

    pub fn name(self) -> &'static str {
        match self {
            View::Sky => "sky",
            View::Emblem => "emblem",
        }
    }

    pub fn from_name(name: &str) -> Option<View> {
        [View::Sky, View::Emblem]
            .into_iter()
            .find(|view| view.name() == name)
    }

    pub fn other(self) -> View {
        match self {
            View::Sky => View::Emblem,
            View::Emblem => View::Sky,
        }
    }
}

#[derive(Debug)]
pub struct Weather {
    palette: &'static Palette,
    location: Option<Location>,
    /// The reads on their way from a thread that fetches, so the app never
    /// waits on the network.
    reading: Option<Watch>,
    /// The last good read and when it landed. A failed read after it keeps
    /// it on screen, so a short loss of the network does not blank the sky.
    report: Option<(Report, Instant)>,
    /// Why the reads since the last good one failed.
    failure: Option<String>,
    view: View,
    sky: Sky,
    cues: Vec<Cue>,
}

impl Weather {
    pub fn new(rng: Rng, palette: &'static Palette, source: Option<Source>) -> Self {
        let location = source.as_ref().map(Source::location);
        let reading = source.and_then(Source::watch);
        let failure = (location.is_some() && reading.is_none()).then(|| NO_THREAD.to_string());
        Self {
            palette,
            location,
            reading,
            report: None,
            failure,
            view: View::default(),
            sky: Sky::new(rng, palette),
            cues: Vec::new(),
        }
    }

    fn shows_sky(&self) -> bool {
        self.report.is_some()
    }

    fn line(&self, text: &str, row: u16, colour: Color, area: Rect, buf: &mut Buffer) {
        let length = text.chars().count() as u16;
        let x = area.x + area.width.saturating_sub(length) / 2;
        buf.set_stringn(
            x,
            row,
            text,
            usize::from(area.width - (x - area.x)),
            Style::default().fg(colour),
        );
    }
}

impl Scene for Weather {
    fn tick(&mut self, _area: Rect) {
        match self.reading.as_ref().and_then(Watch::latest) {
            Some(Ok(report)) => {
                self.report = Some((report, Instant::now()));
                self.failure = None;
            }
            Some(Err(reason)) => self.failure = Some(reason),
            None => {}
        }
        if let Some((report, _)) = &self.report {
            self.sky.tick(report.weather());
            self.sky
                .cues(report.weather(), report.day(), &mut self.cues);
        }
    }

    fn drain_cues(&mut self, out: &mut Vec<Cue>) {
        out.append(&mut self.cues);
    }

    fn set_progress(&mut self, progress: f64) -> bool {
        self.shows_sky() && self.sky.set_progress(progress)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let Some(location) = self.location else {
            scene::draw_notice(NO_LOCATION, area, self.palette, buf);
            return;
        };
        let Some((report, landed)) = &self.report else {
            let notice = self.failure.as_deref().unwrap_or(READING);
            scene::draw_notice(notice, area, self.palette, buf);
            return;
        };
        match self.view {
            View::Sky => self
                .sky
                .render(report.weather(), report.day(), self.palette, area, buf),
            View::Emblem => self.sky.emblem(
                report.weather(),
                report.day(),
                report.temperature(),
                self.palette,
                area,
                buf,
            ),
        }
        let top = area.y + area.height.min(2) / 2;
        let bottom = area.y + area.height - 1;
        let mut status = report.status(location);
        if let Some(reason) = &self.failure {
            let minutes = landed.elapsed().as_secs() / 60;
            status = format!("{status} · {minutes} min ago");
            if top + 1 < bottom {
                self.line(reason, top + 1, self.sky.status(), area, buf);
            }
        }
        self.line(&status, top, self.sky.status(), area, buf);
        self.line(CREDIT, bottom, self.palette.subtle, area, buf);
    }

    fn set_view(&mut self, view: View) {
        self.view = view;
    }

    fn repaint(&mut self, palette: &'static Palette) -> bool {
        self.palette = palette;
        self.sky.repaint(palette);
        true
    }
}
