//! A study desk by a window on a rainy night: a town behind the glass, a
//! cat on the sill, books, a lamp, a computer and a mug. One picture drawn
//! by hand, a letter to a pixel, in half blocks, so a pixel is as wide as
//! it is tall.

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Color;

use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// The room, a letter to a pixel. Each letter is a role of the theme:
/// `w` wall, `f` window frame, `s` night sky, `v` stars, `b` town, `y` and
/// `r` lit town windows, `T` moon, pages and the lamp's bulb, `l` sill, `c`
/// cat, `d` desk top, `n` desk front, `m` metal and the monitor, `k`
/// screen, and `L`, `I`, `G`, `P`, `R`, `F` the colours of books, lamp and
/// mug.
const ART: [&str; 40] = [
    "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwffffffffffffffffffffffffffffwwwwwwwwwwwwwwwLwGPwFwwwwwwwwwwwwwwwwwwww",
    "wwwfssssssssssssffssssssssssssfwwwwwwwwwwwwwwwLIGPRFIwwwwwRwwwwwwwwwwwww",
    "wwwfsssTTsssvsssffsssssssssvssfwwwwwwwwwwwwwwwLIGPRFIwwwwRRwwwwwwwwwwwww",
    "wwwfssTTTTssssssffssvsssssssssfwwwwwwwwwwwwdddddddddddddddddddddwwwwwwww",
    "wwwfssTTTTssssssffssssssssssssfwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwfsssTTsssssssffsssssssssssvfwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwfssssssssssssffssssssvsssssfwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwfssssssssssvsffssssssssssssfwwwwwwwwwwmmmmmmmmmmmmmmmmmmmmmmwwwwwwwww",
    "wwwfssssssvsssssffssssvsssssssfwwwwwwwwwwmkkkkkkkkkkkkkkkkkkkkmwwwwwwwww",
    "wwwfssssssssssssffbbbbssssssssfwwwwwwwwwwmkkkkkkkkkkkkkkkkkkkkmwwwwwwwww",
    "wwwffffffffffffffffffffffffffffwwwwwwwwwwmkkkkkkkkkkkkkkkkkkkkmwwwwwwwww",
    "wwwfsssbbbbsssssffbbbcsssbcbssfwwwwwwwwwwmkkkkkkkkkkkkkkkkkkkkmwwwwwwwww",
    "wwwfsssbbbbsssbbffbbyccssccbssfwwwwwwwwwwmkkkkkkkkkkkkkkkkkkkkmwwwwwwwww",
    "wwwfbbbbbbbsssybffbbbccccccbbbfwwwGGGwwwwmkkkkkkkkkkkkkkkkkkkkmwwwwwwwww",
    "wwwfrbbbbbbsssbbffbbccccccccbbfwwGGGGGwwwmkkkkkkkkkkkkkkkkkkkkmwwwwwwwww",
    "wwwfbbbbbbbbbbbbffbbbccccccbbbfwGGGGGGGwwmkkkkkkkkkkkkkkkkkkkkmwwwwwwwww",
    "wwwfrbbrbbbrbbbbffbbybccccbbbbfwwwTTTwwwwmkkkkkkkkkkkkkkkkkkkkmwwwwwwwww",
    "wwwfbbbbbbbbbbrbffbbbccccccbbbfwwwwmwwwwwmkkkkkkkkkkkkkkkkkkkkmwwwwwwwww",
    "wwwfbbbbbrbybrbbffrbccccccccbbfwwwwmwwwwwmkkkkkkkkkkkkkkkkkkkkmwwwwwwwww",
    "wwwfbbbbbbbbbbbbffbbccccccccbbfwwwwmwwwwwmkkkkkkkkkkkkkkkkkkkkmwwwwwwwww",
    "wwwfffffffffffffffffccccccccfffwwwwmwwwwwmkkkkkkkkkkkkkkkkkkkkmwwwwwwwww",
    "wllllllllllllllllllllllllllllllllwwmwwwwwmkkkkkkkkkkkkkkkkkkkkmwwwwwwwww",
    "wllllllllllllllllllllllllllllllllwwmwwwwwmkkkkkkkkkkkkkkkkkkkkmwwwwwwwww",
    "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwmwwwwwmkkkkkkkkkkkkkkkkkkkkmwwwwwwwww",
    "wwwIIIIIIIIIIITwwwwwwwwwwwwwwwwwwwwmwwwwwmmmmmmmmmmmmmmmmmmmmmmwRRRRwwww",
    "wwwIIIIIIIIIIITwwwwwwwwwwwwwwwwwwwwmwwwwwwwwwwwwwwmmmmwwwwwwwwwwRRRRRRww",
    "wwLLLLLLLLLLLLLLTwwwwwwwwwwwwwwwwwwmwwwwwwwwwwwwwwmmmmwwwwwwwwwwRRRRwRww",
    "wwLLLLLLLLLLLLLLTTTTTTTTTwwwwwwwwmmmmmwwwwwwwwwmmmmmmmmmmwwwwwwwRRRRRRww",
    "dddddddddddddddddddddddddddddddddddddddddddmmmmmmmmmmmmmmmmmdddddddddddd",
    "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnmmmmmmmmmmmmmmmmmmmmmnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnmnnnnnnnnmmmnnnnnnnnmnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnmnnnnnnnnnnnnnnnnnnnmnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnmmmmmmmmmmmmmmmmmmmmmnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn",
    "nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn",
];

/// The picture's width and height, in pixels.
pub const PICTURE: (usize, usize) = (72, 40);
/// The part of the picture that a pane under `LEAST` shows: the window,
/// the cat and the sill, with a pixel of wall on each side of the frame,
/// so the window is in its middle.
pub const WINDOW: Area = (2, 1, 30, 24);
/// The fewest cells across and down at which the whole room shows. A pane
/// cut further shows a strip of desk that reads as nothing.
pub const LEAST: (u16, u16) = (48, 16);
/// A part of the picture: left, top, width and height, in pixels.
pub type Area = (usize, usize, usize, usize);

const SCREEN: Area = (42, 10, 20, 16);
/// The glass the rain runs on, the steam over the mug, the lamp with the
/// light it throws on the wall and the desk, and the room the cat's tail
/// swings in, the same way.
const RAIN: Area = (4, 3, 26, 19);
const STEAM: Area = (63, 17, 8, 9);
const LAMP: Area = (32, 15, 7, 16);
const SWING: Area = (25, 22, 7, 7);
/// The column of the lamp's bulb, the rows of wall under the shade its
/// light falls on, and the row of the desk it lights.
const BULB: usize = 35;
const LIT_WALL: (usize, usize) = (19, 22);
const LIT_DESK: usize = 30;
/// The light on the wall under the shade, a row of it for each row down:
/// the pixels from the bulb that take gold, those that take rose, and how
/// many in turn are lit, so the light thins out as it falls. The pole and
/// the base stay in front of it.
const HALO: [(usize, usize, usize); 3] = [(1, 3, 2), (0, 2, 2), (0, 3, 4)];
/// Each layer that moves, by name, and the area it draws in. No layer
/// draws outside its own area, so the rest of the room stays still.
pub const LAYERS: [(&str, Area); 5] = [
    ("rain", RAIN),
    ("steam", STEAM),
    ("lamp", LAMP),
    ("tail", SWING),
    ("screen", SCREEN),
];
const SYNTAX: [fn(&Palette) -> Color; 4] = [|p| p.iris, |p| p.foam, |p| p.gold, |p| p.subtle];
/// The letters of the picture outside the glass. The rain runs over these
/// only, because the frame and the cat are inside the room.
const OUTSIDE: &[u8] = b"sbyrTv";
const DROPS: usize = 14;
/// The ticks from one pixel typed on the screen to the next, half a
/// second. The cursor blinks at twice this.
const TYPING: u64 = 8;
/// The pixels a finished line waits, counted in typed pixels, before the
/// screen scrolls up.
const LINE_PAUSE: usize = 3;
/// Where the steam leaves the mug, how fast it rises in pixels a tick, and
/// how far it sways to each side.
const SPOUT: (f64, f64) = (65.5, 25.0);
const RISE: f64 = 0.1;
const SWAY: f64 = 1.5;
/// The cat's tail at rest, swung left and swung right, hanging over the
/// front of the sill.
const TAILS: [[(usize, usize); 7]; 3] = [
    [
        (28, 22),
        (28, 23),
        (28, 24),
        (28, 25),
        (28, 26),
        (29, 27),
        (30, 26),
    ],
    [
        (28, 22),
        (28, 23),
        (27, 24),
        (27, 25),
        (26, 26),
        (26, 27),
        (25, 26),
    ],
    [
        (28, 22),
        (28, 23),
        (29, 24),
        (29, 25),
        (30, 26),
        (30, 27),
        (31, 26),
    ],
];
/// One flick of the tail, a pose for each three ticks: left, rest, right,
/// rest.
const FLICK: [usize; 4] = [1, 0, 2, 0];
const FLICK_TICKS: usize = 3;

/// One line of code on the screen: its indent, then each word's length and
/// colour.
#[derive(Debug, Clone)]
struct Code {
    indent: usize,
    words: Vec<(usize, Color)>,
}

impl Code {
    fn width(&self) -> usize {
        self.words
            .iter()
            .map(|(length, _)| length + 1)
            .sum::<usize>()
            - 1
    }
}

/// A streak of rain on the glass: its column, the row of its lowest pixel,
/// its speed in pixels a tick and its length.
#[derive(Debug, Clone, Copy)]
struct Streak {
    x: usize,
    y: f64,
    speed: f64,
    length: usize,
}

#[derive(Debug)]
pub struct Study {
    palette: &'static Palette,
    rng: Rng,
    ticks: u64,
    screen: Vec<Code>,
    /// The pixels typed on the last line of the screen.
    typed: usize,
    rain: Vec<Streak>,
    /// The age of each wisp of steam in ticks, and where its sway starts.
    steam: Vec<(f64, f64)>,
    next_wisp: u32,
    /// The ticks left in the lamp's flicker, and the ticks until the next.
    flicker: u32,
    next_flicker: u32,
    /// The tick of the tail's flick, or none while it rests.
    flick: Option<usize>,
    next_flick: u32,
}

impl Study {
    pub fn new(rng: Rng, palette: &'static Palette) -> Self {
        let mut study = Self {
            palette,
            rng,
            ticks: 0,
            screen: Vec::new(),
            typed: 0,
            rain: Vec::new(),
            steam: Vec::new(),
            next_wisp: 0,
            flicker: 0,
            next_flicker: 0,
            flick: None,
            next_flick: 0,
        };
        let lines = SCREEN.3 / 2;
        study.screen = (0..lines).map(|_| study.code()).collect();
        study.rain = (0..DROPS).map(|_| study.streak(false)).collect();
        study.next_flicker = study.rng.random_range(300..900);
        study.next_flick = study.rng.random_range(80..250);
        study
    }

    fn code(&mut self) -> Code {
        let indent = self.rng.random_range(0..3);
        let mut room = SCREEN.2 - 4 - indent * 2;
        let mut words = Vec::new();
        while room >= 3 {
            let length = self.rng.random_range(2..=5).min(room - 1);
            let colour = SYNTAX[self.rng.random_range(0..SYNTAX.len())](self.palette);
            words.push((length, colour));
            room -= length + 1;
            if self.rng.random_bool(0.3) {
                break;
            }
        }
        Code { indent, words }
    }

    /// A new streak in a column of glass, anywhere on it at the start, and
    /// above it after, so the streaks do not start in step.
    fn streak(&mut self, above: bool) -> Streak {
        let (left, top, wide, tall) = RAIN;
        let x = loop {
            let x = self.rng.random_range(left..left + wide);
            if ART[top].as_bytes()[x] != b'f' {
                break x;
            }
        };
        let y = if above {
            top as f64 - self.rng.random_range(0.0..12.0)
        } else {
            self.rng.random_range(top as f64..(top + tall) as f64)
        };
        Streak {
            x,
            y,
            speed: self.rng.random_range(0.25..0.7),
            length: self.rng.random_range(2..=4),
        }
    }

    fn colour(&self, letter: u8) -> Color {
        let palette = self.palette;
        match letter {
            b'f' | b'm' => palette.muted,
            b'P' => palette.pine,
            b's' | b'k' => palette.base,
            b'b' => palette.overlay,
            b'v' => palette.subtle,
            b'y' | b'G' => palette.gold,
            b'r' | b'R' => palette.rose,
            b'T' | b'c' => palette.text,
            b'l' | b'd' => palette.subtle,
            b'n' => palette.surface,
            b'L' => palette.love,
            b'I' => palette.iris,
            b'F' => palette.foam,
            _ => palette.overlay,
        }
    }

    fn pixels(&self) -> Vec<Vec<Color>> {
        let mut rows: Vec<Vec<Color>> = ART
            .iter()
            .map(|line| line.bytes().map(|letter| self.colour(letter)).collect())
            .collect();
        self.draw_screen(&mut rows);
        self.draw_rain(&mut rows);
        self.draw_steam(&mut rows);
        self.draw_lamp(&mut rows);
        let pose = self.flick.map_or(0, |tick| FLICK[tick / FLICK_TICKS]);
        for &(x, y) in &TAILS[pose] {
            rows[y][x] = self.palette.text;
        }
        rows
    }

    fn draw_screen(&self, rows: &mut [Vec<Color>]) {
        let (left, top, _, _) = SCREEN;
        let last = self.screen.len() - 1;
        for (line, code) in self.screen.iter().enumerate() {
            let y = top + 1 + line * 2;
            let start = left + 2 + code.indent * 2;
            let shown = if line == last { self.typed } else { usize::MAX };
            let mut x = start;
            for &(length, colour) in &code.words {
                for (offset, pixel) in rows[y][x..x + length].iter_mut().enumerate() {
                    if x - start + offset < shown {
                        *pixel = colour;
                    }
                }
                x += length + 1;
            }
            if line == last && (self.ticks / (TYPING * 2)).is_multiple_of(2) {
                rows[y][start + self.typed.min(code.width())] = self.palette.text;
            }
        }
    }

    fn draw_rain(&self, rows: &mut [Vec<Color>]) {
        let (_, top, _, tall) = RAIN;
        for streak in &self.rain {
            let lowest = streak.y.floor() as isize;
            for row in lowest + 1 - streak.length as isize..=lowest {
                let Ok(row) = usize::try_from(row) else {
                    continue;
                };
                if (top..top + tall).contains(&row)
                    && OUTSIDE.contains(&ART[row].as_bytes()[streak.x])
                {
                    rows[row][streak.x] = self.palette.foam;
                }
            }
        }
    }

    fn draw_steam(&self, rows: &mut [Vec<Color>]) {
        let (left, top, wide, tall) = STEAM;
        for &(age, sway) in &self.steam {
            for behind in [0.0, 8.0, 16.0] {
                let age = age - behind;
                if age < 0.0 {
                    continue;
                }
                let x = (SPOUT.0 + (age * 0.12 + sway).sin() * SWAY).round() as usize;
                let y = (SPOUT.1 - age * RISE).round() as usize;
                if (left..left + wide).contains(&x) && (top..top + tall).contains(&y) {
                    rows[y][x] = self.palette.subtle;
                }
            }
        }
    }

    fn draw_lamp(&self, rows: &mut [Vec<Color>]) {
        let (left, top, wide, tall) = LAMP;
        let dim = !self.flicker.is_multiple_of(2);
        let palette = self.palette;
        for (y, (row, line)) in rows.iter_mut().zip(ART).enumerate().skip(top).take(tall) {
            let cells = row.iter_mut().zip(line.bytes()).enumerate();
            for (x, (pixel, letter)) in cells.skip(left).take(wide) {
                let off = x.abs_diff(BULB);
                if dim {
                    match letter {
                        b'T' => *pixel = palette.gold,
                        b'G' => *pixel = palette.rose,
                        _ => {}
                    }
                } else if (LIT_WALL.0..LIT_WALL.1).contains(&y) && letter == b'w' {
                    let (gold, rose, every) = HALO[y - LIT_WALL.0];
                    if off <= gold {
                        *pixel = palette.gold;
                    } else if off <= rose && (x + y).is_multiple_of(every) {
                        *pixel = palette.rose;
                    }
                } else if y == LIT_DESK && letter == b'd' && off <= 3 {
                    *pixel = if off <= 2 { palette.gold } else { palette.rose };
                }
            }
        }
    }

    fn type_on(&mut self) {
        let width = self.screen[self.screen.len() - 1].width();
        if self.typed < width + LINE_PAUSE {
            self.typed += 1;
        } else {
            self.screen.remove(0);
            let code = self.code();
            self.screen.push(code);
            self.typed = 0;
        }
    }
}

impl Scene for Study {
    fn tick(&mut self, _area: Rect) {
        self.ticks += 1;
        if self.ticks.is_multiple_of(TYPING) {
            self.type_on();
        }
        let bottom = (RAIN.1 + RAIN.3) as f64;
        for index in 0..self.rain.len() {
            let streak = &mut self.rain[index];
            streak.y += streak.speed;
            if streak.y - streak.length as f64 >= bottom {
                self.rain[index] = self.streak(true);
            }
        }
        for wisp in &mut self.steam {
            wisp.0 += 1.0;
        }
        // A wisp goes once the last pixel that follows it has risen out of
        // the steam's area.
        let gone = SPOUT.1 - STEAM.1 as f64 + 1.0 + 16.0 * RISE;
        self.steam.retain(|(age, _)| age * RISE < gone);
        if self.next_wisp == 0 {
            let sway = self.rng.random_range(0.0..std::f64::consts::TAU);
            self.steam.push((0.0, sway));
            self.next_wisp = self.rng.random_range(30..60);
        } else {
            self.next_wisp -= 1;
        }
        if self.flicker > 0 {
            self.flicker -= 1;
        } else if self.next_flicker == 0 {
            self.flicker = 7;
            self.next_flicker = self.rng.random_range(300..900);
        } else {
            self.next_flicker -= 1;
        }
        self.flick = match self.flick {
            Some(tick) if tick + 1 < FLICK.len() * FLICK_TICKS => Some(tick + 1),
            Some(_) => {
                self.next_flick = self.rng.random_range(80..250);
                None
            }
            None if self.next_flick == 0 => Some(0),
            None => {
                self.next_flick -= 1;
                None
            }
        };
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let pixels = self.pixels();
        let (from_x, from_y, wide, tall) = if area.width >= LEAST.0 && area.height >= LEAST.1 {
            (0, 0, PICTURE.0, PICTURE.1)
        } else {
            WINDOW
        };
        // The view is centred, so a pane too small for it cuts it evenly.
        // Past the view the room around it shows, and a pixel past the
        // picture's edge takes the edge's colour, so the wall, the sill and
        // the desk run on to the pane's edges.
        let left = from_x as isize - (area.width as isize - wide as isize) / 2;
        let top = from_y as isize - (area.height as isize * 2 - tall as isize) / 2;
        let at = |x: isize, y: isize| {
            let x = (x + left).clamp(0, PICTURE.0 as isize - 1) as usize;
            let y = (y + top).clamp(0, PICTURE.1 as isize - 1) as usize;
            pixels[y][x]
        };
        for row in 0..area.height {
            for column in 0..area.width {
                let (x, y) = (column as isize, row as isize * 2);
                buf[Position::new(area.x + column, area.y + row)]
                    .set_char('▀')
                    .set_fg(at(x, y))
                    .set_bg(at(x, y + 1));
            }
        }
    }
}
