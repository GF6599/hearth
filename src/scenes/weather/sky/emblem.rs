//! The emblem view: one large icon for the weather, and the temperature in
//! large digits beside it, centred as one group on the plain base.

use std::f32::consts::{PI, TAU};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use super::{Dice, Sky, glow, rect};
use crate::octant::{CELL_HEIGHT, Canvas, mix};
use crate::splitmix;
use crate::theme::Palette;
use crate::weather::Type;

/// The size of the icon as a share of the height between the status line
/// and the credit.
const ICON_HEIGHT: f32 = 0.62;
/// The gap between the icon and the digits, as a share of the icon.
const GAP: f32 = 0.14;
/// The side of a block of a digit, as a share of the icon. A digit is
/// three blocks wide and five high.
const BLOCK: f32 = 1.0 / 7.0;
/// The share of the width the group may take before it shrinks.
const ROOM: f32 = 0.94;
/// The reach of the soft disc behind the icon, as a share of the icon.
const DISC: f32 = 0.62;
/// How strongly a strike lights the disc and the cloud at its brightest.
const STRIKE_LIGHT: f32 = 0.9;

const DROPS: u64 = 11;
const FLAKES: u64 = 13;
const FOG_BANDS: usize = 5;
const STARS: [(f32, f32, f32); 6] = [
    (0.72, -0.62, 0.0),
    (0.9, 0.42, 1.7),
    (-0.88, -0.66, 3.1),
    (0.25, 0.86, 4.4),
    (-0.62, 0.8, 2.2),
    (0.98, -0.12, 5.3),
];

/// The digits from 0 to 9 and the minus, each as five rows of three
/// blocks, from the top.
const GLYPHS: [[u8; 5]; 11] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b001, 0b001, 0b001],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
    [0b000, 0b000, 0b111, 0b000, 0b000],
];
const MINUS: usize = 10;

/// The colours of a cloud: the shade below, the body and the light on top.
type Cloud = [Color; 3];

impl Sky {
    pub fn emblem(
        &self,
        weather: Type,
        day: bool,
        temperature: f64,
        palette: &Palette,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let mut canvas = Canvas::new(area.width, area.height, palette);
        let (w, h) = (canvas.width() as f32, canvas.height() as f32);
        if w == 0.0 || h == 0.0 {
            return;
        }
        let (top, bottom) = (2.0 * CELL_HEIGHT as f32, h - CELL_HEIGHT as f32);
        let glyphs = glyphs(temperature);
        // A digit with the gap after it is four blocks, and the degree sign
        // is one.
        let blocks = 4.0 * glyphs.len() as f32 + 1.0;
        let mut size = ICON_HEIGHT * (bottom - top);
        let group = |size: f32| size * (1.0 + GAP + blocks * BLOCK);
        if group(size) > ROOM * w {
            size *= ROOM * w / group(size);
        }
        let middle = (top + bottom) / 2.0;
        let left = (w - group(size)) / 2.0;
        let centre = (left + size / 2.0, middle);
        let t = self.seconds();
        let struck = self
            .flash(t)
            .filter(|_| weather == Type::Storm)
            .map_or(0.0, |(_, bright)| STRIKE_LIGHT * bright);
        let disc = mix(
            palette.overlay,
            mix(palette.gold, palette.text, 0.5),
            struck,
        );
        glow(
            &mut canvas,
            centre,
            0.75 * DISC * size * struck,
            DISC * size,
            disc,
            0.6,
        );
        let icon = Icon {
            centre,
            r: size / 2.0,
            t,
            palette,
        };
        match weather {
            Type::Clear if day => icon.sun(&mut canvas, centre, 1.0),
            Type::Clear => {
                icon.stars(&mut canvas);
                icon.moon(&mut canvas, (centre.0 - 0.08 * icon.r, centre.1), 0.72);
            }
            Type::Cloud => {
                let behind = (centre.0 + 0.38 * icon.r, centre.1 - 0.36 * icon.r);
                if day {
                    icon.sun(&mut canvas, behind, 0.55);
                } else {
                    icon.moon(&mut canvas, behind, 0.4);
                }
                icon.cloud(&mut canvas, 0.1, icon.clouds(day));
            }
            Type::Rain => {
                icon.drops(&mut canvas, palette.foam, 1.0);
                icon.cloud(&mut canvas, -0.28, icon.rain_cloud());
            }
            Type::Snow => {
                icon.flakes(&mut canvas);
                icon.cloud(&mut canvas, -0.28, icon.clouds(true));
            }
            Type::Storm => {
                icon.drops(&mut canvas, palette.foam, 0.55);
                icon.bolt(&mut canvas, struck);
                let lit = icon
                    .storm_cloud()
                    .map(|colour| mix(colour, palette.text, 0.6 * struck));
                icon.cloud(&mut canvas, -0.28, lit);
            }
            Type::Fog => icon.fog(&mut canvas),
            Type::Wind => icon.wind(&mut canvas, self.gust(t)),
        }
        let block = size * BLOCK;
        let digits_left = left + size * (1.0 + GAP);
        digits(
            &mut canvas,
            &glyphs,
            (digits_left, middle - 2.5 * block),
            block,
            palette.text,
        );
        canvas.render(area, buf);
    }
}

/// The glyphs of the temperature rounded to a whole degree.
fn glyphs(temperature: f64) -> Vec<usize> {
    let whole = temperature.round() as i64;
    let mut glyphs: Vec<usize> = whole
        .unsigned_abs()
        .to_string()
        .bytes()
        .map(|digit| usize::from(digit - b'0'))
        .collect();
    if whole < 0 {
        glyphs.insert(0, MINUS);
    }
    glyphs
}

/// Draws the glyphs from the top left corner `at`, each block `block`
/// pixels wide, and the degree sign after them.
fn digits(canvas: &mut Canvas, glyphs: &[usize], at: (f32, f32), block: f32, colour: Color) {
    let mut x = at.0;
    for glyph in glyphs {
        for (row, bits) in GLYPHS[*glyph].iter().enumerate() {
            for column in 0..3 {
                if bits & (0b100 >> column) != 0 {
                    let corner = (x + column as f32 * block, at.1 + row as f32 * block);
                    rect(canvas, corner, (corner.0 + block, corner.1 + block), colour);
                }
            }
        }
        x += 4.0 * block;
    }
    let thin = (block / 3.0).max(1.0);
    let (left, top) = (x, at.1);
    let (right, low) = (x + block, at.1 + block);
    rect(canvas, (left, top), (right, top + thin), colour);
    rect(canvas, (left, low - thin), (right, low), colour);
    rect(canvas, (left, top), (left + thin, low), colour);
    rect(canvas, (right - thin, top), (right, low), colour);
}

/// One icon: its centre, its radius in pixels, the time in seconds, and the
/// palette its colours come from.
struct Icon<'a> {
    centre: (f32, f32),
    r: f32,
    t: f32,
    palette: &'a Palette,
}

impl Icon<'_> {
    fn at(&self, across: f32, down: f32) -> (f32, f32) {
        (
            self.centre.0 + across * self.r,
            self.centre.1 + down * self.r,
        )
    }

    fn clouds(&self, day: bool) -> Cloud {
        let p = self.palette;
        if day {
            [
                mix(p.muted, p.subtle, 0.5),
                p.subtle,
                mix(p.subtle, p.text, 0.45),
            ]
        } else {
            [
                p.muted,
                mix(p.muted, p.subtle, 0.6),
                mix(p.subtle, p.text, 0.2),
            ]
        }
    }

    fn rain_cloud(&self) -> Cloud {
        let p = self.palette;
        [p.muted, mix(p.muted, p.subtle, 0.5), p.subtle]
    }

    fn storm_cloud(&self) -> Cloud {
        let p = self.palette;
        [
            mix(p.muted, p.overlay, 0.3),
            p.muted,
            mix(p.muted, p.subtle, 0.6),
        ]
    }

    /// The sun at `centre`, `scale` times the full icon, with rays that
    /// turn slowly and breathe.
    fn sun(&self, canvas: &mut Canvas, centre: (f32, f32), scale: f32) {
        let p = self.palette;
        let r = self.r * scale;
        glow(
            canvas,
            centre,
            0.4 * r,
            0.9 * r,
            mix(p.overlay, p.gold, 0.35),
            0.5,
        );
        let turn = self.t * 0.12;
        for ray in 0..12 {
            let angle = turn + ray as f32 * TAU / 12.0;
            let reach = 0.82 + 0.1 * (self.t * 1.3 + ray as f32 * 1.9).sin();
            let (dx, dy) = (angle.cos(), angle.sin());
            let from = (centre.0 + dx * 0.6 * r, centre.1 + dy * 0.6 * r);
            let to = (centre.0 + dx * reach * r, centre.1 + dy * reach * r);
            canvas.line(from, to, (0.07 * r).max(1.0), p.gold);
        }
        canvas.disc(centre, 0.46 * r, p.gold);
        canvas.disc(
            (centre.0 - 0.08 * r, centre.1 - 0.08 * r),
            0.3 * r,
            mix(p.gold, p.text, 0.35),
        );
    }

    /// A crescent moon at `centre`, `scale` times the full icon.
    fn moon(&self, canvas: &mut Canvas, centre: (f32, f32), scale: f32) {
        let p = self.palette;
        let r = self.r * scale;
        glow(
            canvas,
            centre,
            0.6 * r,
            1.25 * r,
            mix(p.overlay, p.rose, 0.25),
            0.5,
        );
        let colour = mix(p.rose, p.text, 0.6);
        let cut = (centre.0 + 0.5 * r, centre.1 - 0.18 * r);
        let reach = r.ceil() as i64 + 1;
        for dy in -reach..=reach {
            for dx in -reach..=reach {
                let (x, y) = (centre.0 + dx as f32, centre.1 + dy as f32);
                let lit = r - (x - centre.0).hypot(y - centre.1);
                let shaded = (x - cut.0).hypot(y - cut.1) - 0.8 * r;
                let cover = (lit.min(shaded) + 0.5).clamp(0.0, 1.0);
                if cover > 0.0 && x >= 0.0 && y >= 0.0 {
                    canvas.blend(x as usize, y as usize, colour, cover);
                }
            }
        }
    }

    /// Stars round the moon that twinkle, each in its own time.
    fn stars(&self, canvas: &mut Canvas) {
        for (across, down, phase) in STARS {
            let place = self.at(across, down);
            let bright = 0.75 + 0.25 * (self.t * 1.7 + phase).sin();
            let colour = mix(self.palette.overlay, self.palette.text, bright);
            let arm = (0.07 * self.r * bright).max(0.6);
            canvas.line(
                (place.0 - arm, place.1),
                (place.0 + arm, place.1),
                1.0,
                colour,
            );
            canvas.line(
                (place.0, place.1 - arm),
                (place.0, place.1 + arm),
                1.0,
                colour,
            );
        }
    }

    /// A cloud as wide as the icon, `down` of the radius below the centre,
    /// that rises and falls a little.
    fn cloud(&self, canvas: &mut Canvas, down: f32, [shade, body, light]: Cloud) {
        let bob = 0.03 * (self.t * 0.8).sin();
        let puffs = [(-0.55, 0.12, 0.4), (-0.08, -0.2, 0.56), (0.47, 0.03, 0.44)];
        let floor = down + bob + 0.48;
        for (colour, lift) in [(shade, -0.06), (body, 0.0)] {
            for (index, (across, up, radius)) in puffs.iter().enumerate() {
                let swell = 1.0 + 0.025 * (self.t * 0.9 + index as f32 * 2.1).sin();
                let place = self.at(*across, up + down + bob - lift);
                canvas.disc(place, radius * swell * self.r, colour);
            }
            let left = self.at(-0.55, down + bob + 0.12 - lift);
            let right = self.at(0.47, floor - lift);
            rect(canvas, left, right, colour);
        }
        let shine = self.at(-0.2, down + bob - 0.36);
        canvas.disc(shine, 0.3 * self.r, light);
    }

    /// Drops that fall from under the cloud to the foot of the icon.
    fn drops(&self, canvas: &mut Canvas, colour: Color, strength: f32) {
        let colour = mix(self.palette.overlay, colour, strength);
        for drop in 0..DROPS {
            let mut dice = Dice(splitmix::mix(drop.wrapping_mul(0x9e37_79b9)));
            let across = -0.72 + 1.44 * (drop as f32 + 0.5) / DROPS as f32;
            let fall = (self.t * (0.9 + 0.3 * dice.share()) + dice.share()).fract();
            let head = self.at(across - 0.1 * fall, 0.25 + 0.7 * fall);
            let tail = (head.0 + 0.04 * self.r, head.1 - 0.12 * self.r);
            canvas.line(tail, head, (0.04 * self.r).max(1.0), colour);
        }
    }

    /// Flakes that drift down and sway from side to side.
    fn flakes(&self, canvas: &mut Canvas) {
        for flake in 0..FLAKES {
            let mut dice = Dice(splitmix::mix(flake.wrapping_mul(0x517c_c1b7)));
            let across = -0.75 + 1.5 * (flake as f32 + 0.5) / FLAKES as f32;
            let fall = (self.t * (0.22 + 0.1 * dice.share()) + dice.share()).fract();
            let sway = 0.06 * (self.t * 1.4 + flake as f32).sin();
            let place = self.at(across + sway, 0.25 + 0.72 * fall);
            let arm = (0.05 * self.r).max(0.6);
            let colour = self.palette.text;
            canvas.line(
                (place.0 - arm, place.1),
                (place.0 + arm, place.1),
                1.0,
                colour,
            );
            canvas.line(
                (place.0, place.1 - arm),
                (place.0, place.1 + arm),
                1.0,
                colour,
            );
        }
    }

    /// The bolt under the storm cloud, dim between strikes and bright in
    /// one.
    fn bolt(&self, canvas: &mut Canvas, bright: f32) {
        let p = self.palette;
        let colour = mix(
            mix(p.overlay, p.gold, 0.55),
            mix(p.gold, p.text, 0.4),
            bright,
        );
        let corners = [
            (0.02, 0.12),
            (0.2, 0.12),
            (0.08, 0.48),
            (0.24, 0.48),
            (-0.12, 0.98),
            (-0.02, 0.6),
            (-0.16, 0.6),
        ];
        let corners: Vec<(f32, f32)> = corners
            .iter()
            .map(|(across, down)| self.at(*across, *down))
            .collect();
        canvas.polygon(&corners, colour);
    }

    /// Bands of fog that drift to and fro, each with a gap that moves.
    fn fog(&self, canvas: &mut Canvas) {
        let p = self.palette;
        let shades = [p.muted, mix(p.muted, p.subtle, 0.5), p.subtle];
        for band in 0..FOG_BANDS {
            let down = -0.7 + 1.4 * band as f32 / (FOG_BANDS - 1) as f32;
            let drift = 0.12 * (self.t * 0.35 + band as f32 * 1.3).sin();
            let gap = (self.t * 0.05 + band as f32 * 0.37).fract() * 1.6 - 0.8;
            let colour = shades[band % shades.len()];
            let width = (0.07 * self.r).max(1.0);
            let wave = |across: f32| down + 0.04 * (across * 5.0 + self.t + band as f32).sin();
            let ends = [(-1.0, gap - 0.12), (gap + 0.12, 1.0)];
            for (from, to) in ends {
                let (from, to) = ((from + drift).max(-1.0), (to + drift).min(1.0));
                if to - from < 0.05 {
                    continue;
                }
                let steps = 8;
                for step in 0..steps {
                    let a = from + (to - from) * step as f32 / steps as f32;
                    let b = from + (to - from) * (step + 1) as f32 / steps as f32;
                    canvas.line(self.at(a, wave(a)), self.at(b, wave(b)), width, colour);
                }
            }
        }
    }

    /// Three lines of wind that end in a curl, with a bright gust that runs
    /// along each. They show stronger in a stronger gust.
    fn wind(&self, canvas: &mut Canvas, gust: f32) {
        let p = self.palette;
        let strength = 0.55 + 0.45 * gust;
        let line = mix(p.overlay, p.subtle, strength);
        let width = (0.06 * self.r).max(1.0);
        for (index, (down, end, curl)) in
            [(-0.42, 0.55, 0.2), (0.02, 0.75, 0.24), (0.42, 0.4, 0.17)]
                .into_iter()
                .enumerate()
        {
            let start = self.at(-1.0 + 0.12 * index as f32, down);
            let turn = self.at(end, down);
            canvas.line(start, turn, width, line);
            let centre = (turn.0, turn.1 - curl * self.r);
            let steps = 14;
            for step in 0..steps {
                let a = PI / 2.0 - 1.6 * PI * step as f32 / steps as f32;
                let b = PI / 2.0 - 1.6 * PI * (step + 1) as f32 / steps as f32;
                let radius =
                    |angle: f32| curl * self.r * (0.7 + 0.3 * (angle - PI / 2.0).abs() / PI);
                let from = (
                    centre.0 + a.cos() * radius(a),
                    centre.1 + a.sin() * radius(a),
                );
                let to = (
                    centre.0 + b.cos() * radius(b),
                    centre.1 + b.sin() * radius(b),
                );
                canvas.line(from, to, width, line);
            }
            let run = (self.t * 0.6 + index as f32 * 0.33).fract();
            let head = start.0 + (turn.0 - start.0) * run;
            let tail = (head - 0.3 * self.r).max(start.0);
            canvas.line(
                (tail, turn.1),
                (head, turn.1),
                width,
                mix(line, p.text, strength),
            );
        }
    }
}
