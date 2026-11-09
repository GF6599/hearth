use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::scene::{self, Rng, Scene};
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

type Read = Result<Report, String>;

#[derive(Debug)]
pub struct Weather {
    palette: &'static Palette,
    location: Option<Location>,
    /// The reads on their way from a thread that fetches, so the app never
    /// waits on the network.
    reading: Option<Watch>,
    shown: Option<Read>,
    sky: Sky,
}

impl Weather {
    pub fn new(rng: Rng, palette: &'static Palette, source: Option<Source>) -> Self {
        let location = source.as_ref().map(Source::location);
        let reading = source.and_then(Source::watch);
        let shown = (location.is_some() && reading.is_none()).then(|| Err(NO_THREAD.to_string()));
        Self {
            palette,
            location,
            reading,
            shown,
            sky: Sky::new(rng, palette),
        }
    }

    fn shows_sky(&self) -> bool {
        matches!(self.shown, Some(Ok(_)))
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
        if let Some(read) = self.reading.as_ref().and_then(Watch::latest) {
            self.shown = Some(read);
        }
        if self.shows_sky() {
            self.sky.tick();
        }
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
        match &self.shown {
            None => scene::draw_notice(READING, area, self.palette, buf),
            Some(Err(reason)) => scene::draw_notice(reason, area, self.palette, buf),
            Some(Ok(report)) => {
                self.sky
                    .render(report.weather(), report.day(), self.palette, area, buf);
                let top = area.y + area.height.min(2) / 2;
                let bottom = area.y + area.height - 1;
                self.line(&report.status(location), top, self.palette.text, area, buf);
                self.line(CREDIT, bottom, self.palette.subtle, area, buf);
            }
        }
    }

    fn repaint(&mut self, palette: &'static Palette) -> bool {
        self.palette = palette;
        self.sky.repaint(palette);
        true
    }
}
