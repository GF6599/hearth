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
/// `w` wall, `f` window frame, `s` night sky, `b` town, `y` and `r` lit
/// town windows, `T` moon, pages and the lamp's bulb, `l` sill, `c` cat,
/// `d` desk top, `n` desk front, `m` metal and the monitor, `k` screen,
/// and `L`, `I`, `G`, `P`, `R`, `F` the colours of books, lamp and mug.
const ART: [&str; 40] = [
    "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwffffffffffffffffffffffffffffwwwwwwwwwwwwwwwLwGPwFwwwwwwwwwwwwwwwwwwww",
    "wwwfssssssssssssffssssssssssssfwwwwwwwwwwwwwwwLIGPRFIwwwwwRwwwwwwwwwwwww",
    "wwwfsssTTsssssssffssssssssssssfwwwwwwwwwwwwwwwLIGPRFIwwwwRRwwwwwwwwwwwww",
    "wwwfssTTTTssssssffssssssssssssfwwwwwwwwwwwwdddddddddddddddddddddwwwwwwww",
    "wwwfssTTTTssssssffssssssssssssfwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwfsssTTsssssssffssssssssssssfwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwfssssssssssssffssssssssssssfwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    "wwwfssssssssssssffssssssssssssfwwwwwwwwwwmmmmmmmmmmmmmmmmmmmmmmwwwwwwwww",
    "wwwfssssssssssssffssssssssssssfwwwwwwwwwwmkkkkkkkkkkkkkkkkkkkkmwwwwwwwww",
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
/// The part of the picture that a pane under `LEAST` shows, from its top
/// left: the window, the cat and the sill.
pub const WINDOW: (usize, usize) = (32, 26);
/// The fewest cells across and down at which the whole room shows. A pane
/// cut further shows a strip of desk that reads as nothing.
pub const LEAST: (u16, u16) = (48, 16);
/// The screen's inner pixels: left, top, width and height.
const SCREEN: (usize, usize, usize, usize) = (42, 10, 20, 16);
const SYNTAX: [fn(&Palette) -> Color; 4] = [|p| p.iris, |p| p.foam, |p| p.gold, |p| p.subtle];
/// The cat's tail at rest, hanging over the front of the sill, in pixels.
const TAIL: [(usize, usize); 7] = [
    (28, 22),
    (28, 23),
    (28, 24),
    (28, 25),
    (28, 26),
    (29, 27),
    (30, 26),
];

/// One line of code on the screen: its indent, then each word's length and
/// colour.
#[derive(Debug, Clone)]
struct Code {
    indent: usize,
    words: Vec<(usize, Color)>,
}

#[derive(Debug)]
pub struct Study {
    palette: &'static Palette,
    rng: Rng,
    screen: Vec<Code>,
}

impl Study {
    pub fn new(rng: Rng, palette: &'static Palette) -> Self {
        let mut study = Self {
            palette,
            rng,
            screen: Vec::new(),
        };
        let lines = SCREEN.3 / 2;
        study.screen = (0..lines).map(|_| study.code()).collect();
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

    fn colour(&self, letter: u8) -> Color {
        let palette = self.palette;
        match letter {
            b'f' | b'm' => palette.muted,
            b's' | b'P' => palette.pine,
            b'b' | b'k' => palette.base,
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
        for &(x, y) in &TAIL {
            rows[y][x] = self.palette.text;
        }
        rows
    }

    fn draw_screen(&self, rows: &mut [Vec<Color>]) {
        let (left, top, _, _) = SCREEN;
        for (line, code) in self.screen.iter().enumerate() {
            let y = top + 1 + line * 2;
            let mut x = left + 2 + code.indent * 2;
            for &(length, colour) in &code.words {
                for pixel in &mut rows[y][x..x + length] {
                    *pixel = colour;
                }
                x += length + 1;
            }
        }
    }
}

impl Scene for Study {
    fn tick(&mut self, _area: Rect) {}

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let pixels = self.pixels();
        let (wide, tall) = if area.width >= LEAST.0 && area.height >= LEAST.1 {
            PICTURE
        } else {
            WINDOW
        };
        // Half the room left over, less than none where the view is cut. A
        // pixel past the view's edge takes the edge's colour, so the wall,
        // the sill and the desk run on to the pane's edges.
        let left = (area.width as isize - wide as isize) / 2;
        let top = (area.height as isize * 2 - tall as isize) / 2;
        let at = |x: isize, y: isize| {
            let x = (x - left).clamp(0, wide as isize - 1) as usize;
            let y = (y - top).clamp(0, tall as isize - 1) as usize;
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
