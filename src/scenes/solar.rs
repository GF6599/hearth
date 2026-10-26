use std::f32::consts::TAU;

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::octant::{self, CELL_HEIGHT, CELL_WIDTH, COVERAGE_STEPS, Canvas, stepped};
use crate::scene::{Rng, Scene};
use crate::theme::Palette;

/// Each orbit's half width as a share of the outermost, Mercury to
/// Neptune. The real distances run from 0.4 to 30 times the Earth's, which
/// would put the inner four inside the sun at this scale, so the orbits are
/// spaced evenly instead.
const ORBITS: [f32; 8] = [0.2, 0.31, 0.43, 0.54, 0.66, 0.77, 0.89, 1.0];
/// Ticks for Mercury to go round, about eight seconds. The others take
/// longer by Kepler's third law on their shares, so Neptune takes about a
/// minute and a half.
const MERCURY_TICKS: f32 = 133.0;
/// The height of each orbit as a share of its width, for a view from above
/// at a slant. Low enough that Mercury passes in front of the sun and
/// behind it.
const TILT: f32 = 0.4;
/// Pixels between the outermost orbit and the edge of the pane.
const MARGIN: f32 = 2.0;
/// The sun's radius as a share of the outermost orbit's half width.
const SUN_SHARE: f32 = 0.13;
/// Each planet's radius in pixels, Mercury to Neptune: the rocky four small,
/// the gas giants large. A radius of 2.6 or more always lights five rows of
/// pixels, so every planet spans two rows of cells.
const RADII: [f32; 8] = [2.6, 2.8, 2.8, 2.6, 4.4, 3.8, 3.2, 3.2];
const SATURN: usize = 5;
const EARTH: usize = 2;
/// Saturn's ring, as a half width in pixels, a height as a share of it,
/// and the width of its line in pixels.
const RING_ACROSS: f32 = 8.0;
const RING_TILT: f32 = 0.3;
const RING_WIDTH: f32 = 1.5;
/// The Moon's orbit round Earth, as a half width in pixels, and its period.
const MOON_ACROSS: f32 = 7.0;
const MOON_TICKS: f32 = 40.0;
const MOON_RADIUS: f32 = 1.5;
/// How far a trail reaches behind its planet, as a share of a turn.
const TRAIL_TURN: f32 = 1.0 / 8.0;
/// How far the sun's glow reaches past its disc, as a share of its radius,
/// and how much of the sun it mixes into the sky at the disc's edge.
const GLOW_REACH: f32 = 0.9;
const GLOW_EDGE: f32 = 0.6;
/// How far an orbit's colour lies from the overlay toward muted. An orbit is
/// a guide, so it stays well below every body.
const ORBIT_SHARE: f32 = 0.5;
/// The least share of its own colour that the far side of a body keeps, so
/// a body in shadow still reads against the sky.
const NIGHT_SHARE: f32 = 0.4;
/// How a body's surface turns toward the light: across the screen toward
/// the sun, and out of the screen toward the viewer, so the sunward edge is
/// lit in full and the middle is lit in part.
const LIGHT_ACROSS: f32 = 0.8;
const LIGHT_OUT: f32 = 0.6;
/// A gain on the light, so the sunward cap of a body shows its full colour.
const LIGHT_GAIN: f32 = 2.0;

#[derive(Debug)]
pub struct Solar {
    palette: &'static Palette,
    width: u16,
    height: u16,
    time: u32,
    /// The share of a tick since the last, so a planet glides along its
    /// orbit between two ticks.
    progress: f64,
    /// Where each planet starts on its orbit, as an angle.
    phases: [f32; ORBITS.len()],
    /// The sky, the sun's glow and the orbits, which change only with the
    /// size of the pane, drawn once for the size of the last tick. A frame
    /// copies it, so each mixes only what moves.
    backdrop: Option<Canvas>,
}

impl Solar {
    pub fn new(mut rng: Rng, palette: &'static Palette) -> Self {
        Self {
            palette,
            width: 0,
            height: 0,
            time: 0,
            progress: 0.0,
            phases: std::array::from_fn(|_| rng.random_range(0.0..TAU)),
            backdrop: None,
        }
    }

    /// Where the Moon is, in pixels from the top left of the pane, at the
    /// share of a tick the scene was last given.
    pub fn moon(&self) -> (f32, f32) {
        let angle = TAU * turned(self.ticks(), MOON_TICKS);
        place(self.planets()[EARTH], MOON_ACROSS, angle)
    }

    /// Each planet's middle, Mercury first, in pixels from the top left of
    /// the pane, at the share of a tick the scene was last given.
    pub fn planets(&self) -> Vec<(f32, f32)> {
        let (centre, across) = self.frame();
        ORBITS
            .iter()
            .zip(self.angles())
            .map(|(share, angle)| place(centre, across * share, angle))
            .collect()
    }

    /// Where each planet is on its orbit, Mercury first, as an angle.
    fn angles(&self) -> [f32; ORBITS.len()] {
        std::array::from_fn(|index| {
            let period = MERCURY_TICKS * (ORBITS[index] / ORBITS[0]).powf(1.5);
            (self.phases[index] + TAU * turned(self.ticks(), period)).rem_euclid(TAU)
        })
    }

    /// The ticks run, with the share of the next one.
    fn ticks(&self) -> f64 {
        f64::from(self.time) + self.progress
    }

    /// The middle of the pane and the half width of the outermost orbit, in
    /// pixels. The orbit fits the width or the height, whichever is tighter.
    fn frame(&self) -> ((f32, f32), f32) {
        let width = f32::from(self.width) * CELL_WIDTH as f32;
        let height = f32::from(self.height) * CELL_HEIGHT as f32;
        let centre = ((width - 1.0) / 2.0, (height - 1.0) / 2.0);
        let across = (width / 2.0 - MARGIN).min((height / 2.0 - MARGIN) / TILT);
        (centre, across.max(0.0))
    }
}

/// The share of a turn made after `ticks`, at `period` ticks a turn.
/// Whole turns come off in f64 first, because an angle that grows without
/// end in f32 loses the precision of one tick within a day.
fn turned(ticks: f64, period: f32) -> f32 {
    (ticks / f64::from(period)).fract() as f32
}

/// The place at `angle` on the orbit `across` pixels wide each side of the
/// centre. The angle grows the way the planets go, against the clock seen
/// from above, so a planet crosses the near side, the bottom of the pane,
/// from left to right.
fn place(centre: (f32, f32), across: f32, angle: f32) -> (f32, f32) {
    (
        centre.0 + across * angle.cos(),
        centre.1 - across * TILT * angle.sin(),
    )
}

/// The share of a body's own colour at the point `normal` of its disc,
/// from its middle in radii, lit from `toward`, the way to the sun across
/// the screen in a length of 1 or none.
fn lit(normal: (f32, f32), toward: (f32, f32)) -> f32 {
    let out = (1.0 - normal.0 * normal.0 - normal.1 * normal.1)
        .max(0.0)
        .sqrt();
    let light = LIGHT_ACROSS * (normal.0 * toward.0 + normal.1 * toward.1) + LIGHT_OUT * out;
    (NIGHT_SHARE + (1.0 - NIGHT_SHARE) * LIGHT_GAIN * light).clamp(NIGHT_SHARE, 1.0)
}

/// The steps a share of a glow or a fade rounds to.
const SHADE_STEPS: f32 = 64.0;

/// The share of a pixel `distance` pixels from the middle of a line `width`
/// pixels wide, of one pixel or more, that the line covers.
fn hairline(distance: f32, width: f32) -> f32 {
    stepped(
        (width / 2.0 + 0.5 - distance).clamp(0.0, 1.0),
        COVERAGE_STEPS,
    )
}

/// Visits each pixel near the ellipse `across` by `down` pixels each side of
/// `centre` once, with the share a line `width` pixels wide along the
/// ellipse covers. The columns visit where the ellipse runs flatter than a
/// diagonal and the rows where it runs steeper, so each pixel lies within a
/// pixel of the curve along the side that the curve crosses. Only the
/// pixels inside `bounds`, left, top, right and bottom, are visited.
fn ellipse(
    centre: (f32, f32),
    across: f32,
    down: f32,
    width: f32,
    bounds: [f32; 4],
    mut visit: impl FnMut(usize, usize, f32),
) {
    if across <= 0.0 || down <= 0.0 {
        return;
    }
    let mut each = |x: i32, y: i32, by_column: bool| {
        let (Ok(column), Ok(row)) = (usize::try_from(x), usize::try_from(y)) else {
            return;
        };
        let (dx, dy) = (x as f32 - centre.0, y as f32 - centre.1);
        let (slope_x, slope_y) = (dx / (across * across), dy / (down * down));
        if by_column != (slope_y.abs() >= slope_x.abs()) {
            return;
        }
        let level = (dx / across).powi(2) + (dy / down).powi(2) - 1.0;
        let gradient = 2.0 * slope_x.hypot(slope_y);
        if gradient == 0.0 {
            return;
        }
        let coverage = hairline(level.abs() / gradient, width);
        if coverage > 0.0 {
            visit(column, row, coverage);
        }
    };
    let [left, top, right, bottom] = bounds;
    let columns = (centre.0 - across - 1.0).max(left).floor() as i32
        ..=(centre.0 + across + 1.0).min(right).ceil() as i32;
    let rows = (centre.1 - down - 1.0).max(top).floor() as i32
        ..=(centre.1 + down + 1.0).min(bottom).ceil() as i32;
    let (column_span, row_span) = (columns.clone(), rows.clone());
    for x in columns {
        let height = down
            * (1.0 - ((x as f32 - centre.0) / across).powi(2))
                .max(0.0)
                .sqrt();
        let top = (centre.1 - height).floor() as i32 - 1..=(centre.1 - height).ceil() as i32 + 1;
        let bottom = (centre.1 + height).floor() as i32 - 1..=(centre.1 + height).ceil() as i32 + 1;
        let start = *bottom.start().max(&(top.end() + 1));
        for y in top.clone().chain(start..=*bottom.end()) {
            if row_span.contains(&y) {
                each(x, y, true);
            }
        }
    }
    for y in rows {
        let half = across
            * (1.0 - ((y as f32 - centre.1) / down).powi(2))
                .max(0.0)
                .sqrt();
        let left = (centre.0 - half).floor() as i32 - 1..=(centre.0 - half).ceil() as i32 + 1;
        let right = (centre.0 + half).floor() as i32 - 1..=(centre.0 + half).ceil() as i32 + 1;
        let start = *right.start().max(&(left.end() + 1));
        for x in left.clone().chain(start..=*right.end()) {
            if column_span.contains(&x) {
                each(x, y, false);
            }
        }
    }
}

/// Bounds that hold every pixel.
const EVERYWHERE: [f32; 4] = [f32::MIN, f32::MIN, f32::MAX, f32::MAX];

impl Solar {
    /// The colour of each planet, Mercury to Neptune.
    fn colours(&self) -> [Color; ORBITS.len()] {
        let palette = self.palette;
        [
            palette.subtle,
            palette.text,
            palette.foam,
            palette.love,
            palette.rose,
            palette.gold,
            palette.pine,
            palette.iris,
        ]
    }

    fn orbit(&self) -> Color {
        octant::mix(self.palette.overlay, self.palette.muted, ORBIT_SHARE)
    }

    /// The sky of a pane `columns` by `rows` cells, with the sun's glow and
    /// every orbit. An orbit on the far half lies under the glow and one on
    /// the near half over it.
    fn backdrop(&self, columns: u16, rows: u16) -> Canvas {
        let (centre, across) = self.frame();
        let mut canvas = Canvas::new(columns, rows, self.palette);
        let orbit = self.orbit();
        let orbits = |canvas: &mut Canvas, front: bool| {
            for share in ORBITS {
                let reach = across * share;
                ellipse(
                    centre,
                    reach,
                    reach * TILT,
                    1.0,
                    EVERYWHERE,
                    |x, y, coverage| {
                        if (y as f32 > centre.1) == front {
                            canvas.blend(x, y, orbit, coverage);
                        }
                    },
                );
            }
        };
        orbits(&mut canvas, false);
        let radius = across * SUN_SHARE;
        let glow = radius * (1.0 + GLOW_REACH);
        for y in (centre.1 - glow).floor() as i32..=(centre.1 + glow).ceil() as i32 {
            for x in (centre.0 - glow).floor() as i32..=(centre.0 + glow).ceil() as i32 {
                let (Ok(column), Ok(row)) = (usize::try_from(x), usize::try_from(y)) else {
                    continue;
                };
                let out =
                    ((x as f32 - centre.0).hypot(y as f32 - centre.1) - radius) / (glow - radius);
                if (0.0..1.0).contains(&out) {
                    let share = octant::falloff(out, GLOW_EDGE, SHADE_STEPS);
                    canvas.blend(column, row, self.palette.gold, share);
                }
            }
        }
        orbits(&mut canvas, true);
        canvas
    }

    /// Each pixel of the planets' trails, with its coverage and colour,
    /// fading from the planet's colour to the orbit's.
    fn trails(&self) -> Vec<(usize, usize, f32, Color)> {
        let (centre, across) = self.frame();
        let (orbit, colours, angles) = (self.orbit(), self.colours(), self.angles());
        let mut trails = Vec::new();
        for (index, &share) in ORBITS.iter().enumerate() {
            let reach = across * share;
            let down = reach * TILT;
            let arc = (0..=8).map(|step| {
                place(
                    centre,
                    reach,
                    angles[index] - TAU * TRAIL_TURN * step as f32 / 8.0,
                )
            });
            let bounds = arc.fold(
                [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
                |[left, top, right, bottom], (x, y)| {
                    [
                        left.min(x - 2.0),
                        top.min(y - 2.0),
                        right.max(x + 2.0),
                        bottom.max(y + 2.0),
                    ]
                },
            );
            let mut shades = [None; SHADE_STEPS as usize + 1];
            ellipse(centre, reach, down, 1.0, bounds, |x, y, coverage| {
                let (dx, dy) = (x as f32 - centre.0, y as f32 - centre.1);
                let angle = (-dy / down).atan2(dx / reach);
                let lag = (angles[index] - angle).rem_euclid(TAU) / (TAU * TRAIL_TURN);
                if lag < 1.0 {
                    let step = ((1.0 - lag).powi(2) * SHADE_STEPS).round();
                    let shade = *shades[step as usize].get_or_insert_with(|| {
                        octant::mix(orbit, colours[index], step / SHADE_STEPS)
                    });
                    trails.push((x, y, coverage, shade));
                }
            });
        }
        trails
    }

    /// Draws a body of `radius` round `middle` in `colour`, lit on the side
    /// that faces `sun`, with its far side shading toward the overlay.
    fn body(
        &self,
        canvas: &mut Canvas,
        middle: (f32, f32),
        radius: f32,
        colour: Color,
        sun: (f32, f32),
    ) {
        let (dx, dy) = (sun.0 - middle.0, sun.1 - middle.1);
        let length = dx.hypot(dy);
        let toward = if length > 0.0 {
            (dx / length, dy / length)
        } else {
            (0.0, 0.0)
        };
        let reach = radius + 1.0;
        for y in (middle.1 - reach).floor() as i32..=(middle.1 + reach).ceil() as i32 {
            for x in (middle.0 - reach).floor() as i32..=(middle.0 + reach).ceil() as i32 {
                let (Ok(column), Ok(row)) = (usize::try_from(x), usize::try_from(y)) else {
                    continue;
                };
                let normal = (
                    (x as f32 - middle.0) / radius,
                    (y as f32 - middle.1) / radius,
                );
                let coverage = stepped(
                    (radius + 0.5 - normal.0.hypot(normal.1) * radius).clamp(0.0, 1.0),
                    COVERAGE_STEPS,
                );
                if coverage > 0.0 {
                    let shade = octant::mix(self.palette.overlay, colour, lit(normal, toward));
                    canvas.blend(column, row, shade, coverage);
                }
            }
        }
    }
}

impl Scene for Solar {
    fn tick(&mut self, area: Rect) {
        let resized = (self.width, self.height) != (area.width, area.height);
        self.width = area.width;
        self.height = area.height;
        self.time = self.time.wrapping_add(1);
        if resized || self.backdrop.is_none() {
            self.backdrop = Some(self.backdrop(area.width, area.height));
        }
    }

    fn set_progress(&mut self, progress: f64) -> bool {
        std::mem::replace(&mut self.progress, progress) != progress
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        let (centre, across) = self.frame();
        let palette = self.palette;
        let colours = self.colours();
        let mut canvas = match &self.backdrop {
            Some(backdrop) if (self.width, self.height) == (area.width, area.height) => {
                backdrop.clone()
            }
            _ => self.backdrop(area.width, area.height),
        };

        // Each body by its planet's index, or none for the Moon, nearest
        // last, so a nearer body draws over a further one.
        let mut bodies: Vec<(Option<usize>, (f32, f32))> = self
            .planets()
            .into_iter()
            .enumerate()
            .map(|(index, place)| (Some(index), place))
            .collect();
        bodies.push((None, self.moon()));
        bodies.sort_by(|one, other| one.1.1.total_cmp(&other.1.1));
        let draw_body = |canvas: &mut Canvas, (planet, place): (Option<usize>, (f32, f32))| {
            let draw_ring = |canvas: &mut Canvas, front: bool| {
                ellipse(
                    place,
                    RING_ACROSS,
                    RING_ACROSS * RING_TILT,
                    RING_WIDTH,
                    EVERYWHERE,
                    |x, y, coverage| {
                        if (y as f32 > place.1) == front {
                            canvas.blend(x, y, colours[SATURN], coverage);
                        }
                    },
                );
            };
            match planet {
                // The Moon is a few pixels across, too small for a cell to
                // show a shade across it.
                None => canvas.disc(place, MOON_RADIUS, palette.subtle),
                Some(SATURN) => {
                    draw_ring(canvas, false);
                    self.body(canvas, place, RADII[SATURN], colours[SATURN], centre);
                    draw_ring(canvas, true);
                }
                Some(index) => self.body(canvas, place, RADII[index], colours[index], centre),
            }
        };
        let trails = self.trails();
        let draw_trails = |canvas: &mut Canvas, front: bool| {
            for &(x, y, coverage, colour) in &trails {
                if (y as f32 > centre.1) == front {
                    canvas.blend(x, y, colour, coverage);
                }
            }
        };
        draw_trails(&mut canvas, false);
        for &body in bodies.iter().filter(|body| body.1.1 <= centre.1) {
            draw_body(&mut canvas, body);
        }
        canvas.disc(centre, across * SUN_SHARE, palette.gold);
        draw_trails(&mut canvas, true);
        for &body in bodies.iter().filter(|body| body.1.1 > centre.1) {
            draw_body(&mut canvas, body);
        }
        canvas.render(area, buf);
    }
}
