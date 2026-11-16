use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::dialog::Dialog;
use crate::scenes::weather::NO_LOCATION;
use crate::theme::Palette;
use crate::weather::Location;

const NAMES: [&str; 2] = ["latitude", "longitude"];
const TITLE: &str = "weather";
const HINT: &str = "tab next  enter save  esc cancel";
/// Each fetch sends the place to Open-Meteo, so the box says so before the
/// first one.
const NOTE: [&str; 2] = [
    "sent to Open-Meteo to read the",
    "weather, saved in your config",
];
/// The longest number a row takes: a sign, three whole degrees, a point and
/// six places, which is about ten centimetres on the ground.
const DIGITS: usize = 11;
/// The box hides the notice of the scene under it, which is wider than the
/// longest reason, "longitude must be from -180 to 180".
const ROW: usize = NO_LOCATION.len();

/// The place of the weather while the user types it, before a save.
#[derive(Debug, Clone)]
pub struct LocationEditor {
    fields: [String; 2],
    /// The field that takes the typing.
    row: usize,
    /// Why the last save kept the box open.
    refused: Option<String>,
}

impl LocationEditor {
    pub fn new(location: Option<Location>) -> Self {
        let fields = location.map_or_else(Default::default, |location| {
            [
                location.latitude().to_string(),
                location.longitude().to_string(),
            ]
        });
        Self {
            fields,
            row: 0,
            refused: None,
        }
    }

    pub fn next(&mut self) {
        self.row = (self.row + 1) % NAMES.len();
    }

    pub fn previous(&mut self) {
        self.row = (self.row + NAMES.len() - 1) % NAMES.len();
    }

    pub fn type_in(&mut self, letter: char) {
        let field = &mut self.fields[self.row];
        if (letter.is_ascii_digit() || letter == '.' || letter == '-') && field.len() < DIGITS {
            field.push(letter);
        }
    }

    pub fn erase(&mut self) {
        self.fields[self.row].pop();
    }

    /// The place the two fields give. `None` keeps the reason on show.
    pub fn location(&mut self) -> Option<Location> {
        let read = self.read();
        self.refused = read.as_ref().err().cloned();
        read.ok()
    }

    fn read(&self) -> Result<Location, String> {
        let [latitude, longitude] = [0, 1].map(|row| {
            self.fields[row]
                .parse::<f64>()
                .map_err(|_| format!("{} must be a number", NAMES[row]))
        });
        Location::new(latitude?, longitude?)
    }

    pub fn render(&self, area: Rect, palette: &Palette, buf: &mut Buffer) {
        let words = Style::default().fg(palette.subtle).bg(palette.base);
        let picked = Style::default().fg(palette.text).bg(palette.base);
        let refused = Style::default().fg(palette.love).bg(palette.base);
        let mut rows: Vec<Vec<(String, Style)>> = NAMES
            .iter()
            .zip(&self.fields)
            .enumerate()
            .map(|(row, (name, field))| {
                let (mark, cursor, style) = if row == self.row {
                    ("›", "█", picked)
                } else {
                    (" ", "", words)
                };
                let text = format!("{mark} {name:<9}   {field}{cursor}");
                vec![(format!("{text:<ROW$}"), style)]
            })
            .collect();
        let reason = self.refused.as_deref().unwrap_or_default();
        rows.push(vec![(format!("{reason:<ROW$}"), refused)]);
        rows.extend(NOTE.map(|line| vec![(format!("{line:<ROW$}"), words)]));
        Dialog {
            title: TITLE,
            rows,
            inner: ROW,
            hint: HINT,
        }
        .render(palette, area, buf);
    }
}
