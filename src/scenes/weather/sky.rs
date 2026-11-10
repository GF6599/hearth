//! The landscape the weather scene draws, and the sky over it.

use std::f32::consts::{PI, TAU};

use rand::RngExt;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::octant::{self, CELL_HEIGHT, COVERAGE_STEPS, Canvas};
use crate::scene::{Rng, TICK_RATE};
use crate::splitmix;
use crate::theme::Palette;
use crate::weather::Type;

/// The share of the height down to the middle of each line of land, from
/// the furthest ridge to the near hill, and the share of the height each
/// of its two waves rises and falls.
const RIDGE: (f32, [f32; 2]) = (0.57, [0.03, 0.015]);
const FAR: (f32, [f32; 2]) = (0.66, [0.05, 0.022]);
const NEAR: (f32, [f32; 2]) = (0.8, [0.035, 0.012]);
/// How many times each wave of a line rises across the pane.
const RIDGE_WAVES: [f32; 2] = [0.6, 2.1];
const FAR_WAVES: [f32; 2] = [1.3, 3.4];
const NEAR_WAVES: [f32; 2] = [0.8, 4.6];
/// How strongly the slope of a hill toward the light lights its crest, and
/// the pixels down from the crest the light reaches on the far and the
/// near hill.
const FACE_LIGHT: f32 = 4.0;
const FAR_RIM: f32 = 4.0;
const NEAR_RIM: f32 = 7.0;
/// One column in this many holds a tuft of grass, and one pixel in this
/// many of the soil below the grass a fleck.
const TUFT_ODDS: u64 = 3;
const FLECK_ODDS: u64 = 53;
/// The pixels down from the near hill's line where the soil starts.
const SOIL: f32 = 3.0;

/// The share of the width across to the cottage.
const COTTAGE: f32 = 0.7;
/// The posts of the fence beside the cottage, and the pixels between two.
const POSTS: usize = 5;
const POST_GAP: f32 = 3.0;
/// The pixels across for each pine on the far hill, and the fewest and
/// most pines.
const PINE_ROOM: usize = 44;
const PINES: (usize, usize) = (5, 24);
/// The places of the large pines in front, as shares of the width. None
/// stands near the cottage.
const NEAR_PINES: [f32; 2] = [0.1, 0.17];
/// The tiers of a pine, and the shares of its height from its tip that the
/// top of each tier lies at.
const TIERS: [f32; 3] = [0.0, 0.25, 0.5];

/// The places of the sun and the moon, as shares of the width and the
/// height. The sun of a clear sky and of a cloudy sky is in one place, so
/// a change from one to the other moves only the clouds.
const SUN: (f32, f32) = (0.24, 0.2);
const MOON: (f32, f32) = (0.76, 0.22);
/// The radius of the sun and the moon as a share of the height, and the
/// least in pixels, so a short pane still shows a disc.
const DISC: f32 = 0.065;
const LEAST_DISC: f32 = 4.5;
/// The radius of the sun's bright core, as a share of its radius.
const CORE: f32 = 0.6;
/// How far the glow of the sun and of the moon reaches, in radii from the
/// middle, and how strong it is at the disc.
const SUN_GLOW: (f32, f32) = (2.6, 0.45);
const MOON_GLOW: (f32, f32) = (2.2, 0.25);
/// The rays round the sun, the radii from the middle each runs between,
/// and the radians they turn in a second.
const RAYS: usize = 12;
const RAY_SPAN: (f32, f32) = (1.35, 2.3);
const RAY_TURN: f32 = 0.25;

/// The pixels of sky for each star, the most stars, and the highest share
/// of the height a star lies at, so none sits in the hills.
const STAR_ROOM: f32 = 410.0;
const STARS: usize = 160;
const STAR_FLOOR: f32 = 0.55;
/// The seconds from one shooting star to the next, and the seconds one
/// takes to cross the sky.
const SHOOTING_GAP: f32 = 11.0;
const SHOOTING: f32 = 0.5;

/// The places of the birds as shares of the width and the height, and the
/// pixels a second they glide.
const BIRDS: [(f32, f32); 3] = [(0.55, 0.18), (0.6, 0.23), (0.66, 0.16)];
const GLIDE: f32 = 6.0;

/// The pixels across and down of the pane the clouds are set for. A wider
/// pane moves its clouds faster, so each crosses in the same time, and a
/// cloud keeps the shape it has on this pane.
const CLOUD_PANE: (f32, f32) = (240.0, 136.0);
/// One layer of clouds: how many clouds, their width and height as shares
/// of the pane's, their pixels a second, and the shares of the height their
/// middles lie between.
#[derive(Debug, Clone, Copy)]
struct Spread {
    count: usize,
    width: f32,
    height: f32,
    speed: f32,
    rows: (f32, f32),
}

/// The layers of clouds from the furthest to the nearest.
const LAYERS: [Spread; 3] = [
    Spread {
        count: 4,
        width: 0.16,
        height: 0.045,
        speed: 1.8,
        rows: (0.14, 0.34),
    },
    Spread {
        count: 3,
        width: 0.24,
        height: 0.07,
        speed: 4.5,
        rows: (0.26, 0.44),
    },
    Spread {
        count: 2,
        width: 0.34,
        height: 0.1,
        speed: 10.0,
        rows: (0.12, 0.26),
    },
];

/// The bands of fog as the share of the height to their middle, their half
/// thickness, their thickest share, their pixels a second, and where each
/// lies: 0 behind the far hill, 1 between the hills, 2 in front.
const FOG: [(f32, f32, f32, f32, u8); 5] = [
    (0.62, 0.06, 0.6, 3.0, 0),
    (0.7, 0.05, 0.55, -2.0, 0),
    (0.77, 0.05, 0.55, 4.0, 1),
    (0.87, 0.06, 0.5, 5.5, 2),
    (0.96, 0.05, 0.45, -3.5, 2),
];

/// The steps a share of a glow or a fade rounds to.
const SHADE_STEPS: f32 = 64.0;

/// What the sky shows. Rain, snow, a storm and the wind draw a cloudy sky
/// until each has a sky of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Look {
    Clear,
    Cloud,
    Fog,
}

impl Look {
    fn of(weather: Type) -> Self {
        match weather {
            Type::Clear => Look::Clear,
            Type::Fog => Look::Fog,
            Type::Cloud | Type::Rain | Type::Snow | Type::Storm | Type::Wind => Look::Cloud,
        }
    }

    /// How much of the sun or the moon reaches the land.
    fn light(self) -> f32 {
        match self {
            Look::Clear => 1.0,
            Look::Cloud => 0.6,
            Look::Fog => 0.3,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Land {
    ridge: Color,
    far: Color,
    far_lit: Color,
    near: Color,
    near_lit: Color,
    /// The tufts of grass on the near hill, and the flecks in its soil.
    grass: Color,
    fleck: Color,
    pine: Color,
    pine_lit: Color,
    trunk: Color,
    wall: Color,
    door: Color,
    roof: Color,
    eave: Color,
    chimney: Color,
    fence: Color,
    smoke: Color,
}

/// The colours of the sky by day or at night, mixed once from the palette.
#[derive(Debug, Clone, Copy)]
struct Hours {
    land: Land,
    /// Each sky from its top to the haze over the hills.
    clear: [Color; 3],
    cloudy: [Color; 3],
    foggy: [Color; 3],
    /// The clouds from the furthest layer to the nearest: the shade below,
    /// the body and the light on top.
    clouds: [[Color; 3]; 3],
    fog: Color,
    /// The sun or the moon through the fog.
    veiled: Color,
}

#[derive(Debug, Clone, Copy)]
struct Shades {
    /// The status line, lighter than any sky behind it.
    bright: Color,
    day: Hours,
    night: Hours,
    sun: Color,
    sun_core: Color,
    sun_glow: Color,
    moon: Color,
    crater: Color,
    moon_glow: Color,
    star: Color,
    bird: Color,
    glass: Color,
    lamp: Color,
    /// The lit window from its dimmest flicker to its brightest.
    window: [Color; 4],
}

impl Shades {
    fn new(palette: &Palette) -> Self {
        let mix = |from, to, share| octant::blend(palette, from, to, share);
        // A light theme swaps its base and its text, so the sky takes its
        // dark end and its bright end by their lightness, not their role.
        let (dark, bright) = if palette.is_light() {
            (palette.text, palette.surface)
        } else {
            (palette.base, palette.text)
        };
        // The overlay and the surface of a dark theme lie a little above its
        // dark end, and a light theme's lie far above, so a light theme
        // lifts its dark end a little instead.
        let (overlay, surface) = if palette.is_light() {
            (
                octant::mix(dark, bright, 0.15),
                octant::mix(dark, bright, 0.06),
            )
        } else {
            (palette.overlay, palette.surface)
        };
        let day_haze = mix(dark, palette.foam, 0.8);
        let day_far = mix(mix(day_haze, palette.pine, 0.6), dark, 0.2);
        let day_near = mix(dark, palette.pine, 0.42);
        let day_land = Land {
            ridge: mix(day_haze, palette.pine, 0.3),
            far: day_far,
            far_lit: mix(day_far, palette.foam, 0.35),
            near: day_near,
            near_lit: mix(day_near, palette.foam, 0.3),
            grass: mix(day_near, palette.foam, 0.3),
            fleck: mix(day_near, palette.foam, 0.15),
            pine: mix(palette.pine, dark, 0.62),
            pine_lit: mix(palette.pine, dark, 0.38),
            trunk: mix(palette.love, dark, 0.75),
            wall: mix(dark, palette.rose, 0.38),
            door: mix(mix(dark, palette.rose, 0.38), dark, 0.55),
            roof: mix(dark, palette.love, 0.55),
            eave: mix(dark, palette.love, 0.3),
            chimney: mix(dark, overlay, 0.9),
            fence: mix(day_near, palette.rose, 0.4),
            smoke: palette.subtle,
        };
        let night_haze = mix(dark, palette.iris, 0.24);
        let night_far = mix(mix(dark, palette.iris, 0.16), overlay, 0.55);
        let night_near = mix(dark, surface, 0.6);
        let night_land = Land {
            ridge: mix(night_haze, overlay, 0.4),
            far: night_far,
            far_lit: mix(night_far, palette.subtle, 0.15),
            near: night_near,
            near_lit: mix(night_near, palette.subtle, 0.18),
            grass: mix(night_near, palette.muted, 0.3),
            fleck: mix(night_near, palette.muted, 0.15),
            pine: mix(night_far, dark, 0.5),
            pine_lit: mix(night_far, palette.subtle, 0.12),
            trunk: mix(dark, overlay, 0.5),
            wall: mix(dark, overlay, 0.75),
            door: mix(dark, overlay, 0.3),
            roof: mix(dark, palette.love, 0.3),
            eave: mix(dark, palette.love, 0.15),
            chimney: mix(dark, overlay, 0.9),
            fence: mix(night_near, palette.muted, 0.4),
            smoke: palette.muted,
        };
        let clouds = |bodies: [Color; 3], shade: Color, light: Color, lean: f32| {
            bodies.map(|body| [mix(body, shade, 0.5), body, mix(body, light, lean)])
        };
        let day_cloudy = [
            mix(dark, palette.pine, 0.6),
            mix(dark, palette.foam, 0.45),
            mix(mix(dark, palette.foam, 0.6), palette.subtle, 0.4),
        ];
        Self {
            bright,
            day: Hours {
                land: day_land,
                clear: [
                    mix(dark, palette.pine, 0.8),
                    mix(dark, palette.foam, 0.55),
                    day_haze,
                ],
                cloudy: day_cloudy,
                foggy: [
                    mix(dark, palette.subtle, 0.4),
                    mix(dark, palette.subtle, 0.55),
                    mix(dark, palette.subtle, 0.68),
                ],
                clouds: clouds(
                    [
                        mix(day_cloudy[0], palette.subtle, 0.45),
                        mix(palette.subtle, bright, 0.1),
                        mix(palette.subtle, bright, 0.4),
                    ],
                    palette.muted,
                    bright,
                    0.35,
                ),
                fog: mix(palette.subtle, bright, 0.3),
                veiled: mix(mix(dark, palette.subtle, 0.4), bright, 0.45),
            },
            night: Hours {
                land: night_land,
                clear: [dark, mix(dark, palette.iris, 0.1), night_haze],
                cloudy: [
                    dark,
                    mix(dark, palette.muted, 0.15),
                    mix(dark, palette.muted, 0.28),
                ],
                foggy: [
                    mix(dark, palette.muted, 0.12),
                    mix(dark, palette.muted, 0.22),
                    mix(dark, palette.muted, 0.32),
                ],
                clouds: clouds(
                    [
                        mix(dark, overlay, 0.6),
                        mix(overlay, palette.muted, 0.35),
                        mix(overlay, palette.muted, 0.6),
                    ],
                    dark,
                    palette.subtle,
                    0.25,
                ),
                fog: mix(palette.muted, palette.subtle, 0.4),
                veiled: mix(mix(dark, palette.muted, 0.12), palette.subtle, 0.4),
            },
            sun: mix(palette.gold, bright, 0.35),
            sun_core: mix(mix(palette.gold, bright, 0.35), bright, 0.3),
            sun_glow: palette.gold,
            moon: mix(bright, palette.rose, 0.12),
            crater: mix(mix(bright, palette.rose, 0.12), palette.subtle, 0.35),
            moon_glow: palette.subtle,
            star: bright,
            bird: mix(dark, overlay, 0.8),
            glass: mix(dark, palette.foam, 0.4),
            lamp: palette.gold,
            window: [0.7, 0.8, 0.9, 1.0].map(|share| mix(dark, palette.gold, share)),
        }
    }
}

/// A line of land across the pane: the share of the height down to its
/// middle, and two waves, each the share of the height it rises and falls,
/// how many times it rises across the pane, and its phase.
#[derive(Debug, Clone, Copy)]
struct Line {
    middle: f32,
    waves: [(f32, f32, f32); 2],
}

impl Line {
    fn new(rng: &mut Rng, (middle, swell): (f32, [f32; 2]), waves: [f32; 2]) -> Self {
        let mut wave = |index: usize| (swell[index], waves[index], rng.random_range(0.0..TAU));
        Self {
            middle,
            waves: [wave(0), wave(1)],
        }
    }

    /// The pixel row of the line at pixel column `x` of a pane `w` by `h`.
    fn at(&self, x: f32, w: f32, h: f32) -> f32 {
        let along = x / w * TAU;
        let swell: f32 = self
            .waves
            .iter()
            .map(|(height, times, phase)| height * (along * times + phase).sin())
            .sum();
        h * (self.middle + swell)
    }
}

#[derive(Debug, Clone, Copy)]
struct Star {
    place: (f32, f32),
    brightness: f32,
    rate: f32,
    phase: f32,
}

/// A puff of a cloud: its place from the cloud's middle and its radius, in
/// heights of the cloud, so one cloud draws at any size, and whether it
/// rises above the cloud's flat base and so catches the light.
#[derive(Debug, Clone, Copy)]
struct Puff {
    across: f32,
    down: f32,
    radius: f32,
    tall: bool,
}

#[derive(Debug, Clone)]
struct Cloud {
    across: f32,
    down: f32,
    puffs: Vec<Puff>,
    /// The heights of the cloud from its middle to its furthest edge across.
    reach: f32,
}

/// One layer of clouds: the clouds, their height as a share of the pane's,
/// and the pixels a second they drift.
#[derive(Debug, Clone)]
struct Layer {
    clouds: Vec<Cloud>,
    height: f32,
    speed: f32,
}

/// A cloud `aspect` heights across: tall puffs close enough to merge on a
/// flat base of small ones.
fn cloud(rng: &mut Rng, aspect: f32) -> Vec<Puff> {
    let count = ((aspect / 0.9) as usize).max(4);
    let mut puffs = Vec::with_capacity(count * 3);
    for index in 0..count {
        let along = (index as f32 + 0.5) / count as f32 * 2.0 - 1.0;
        let radius = (0.45 + 0.55 * (1.0 - along.abs().powf(1.6))) * rng.random_range(0.85..1.1);
        puffs.push(Puff {
            across: along * aspect / 2.0 * 0.85,
            down: -radius * 0.45 + rng.random_range(-0.06..0.06),
            radius,
            tall: true,
        });
    }
    for index in 0..2 * count {
        let along = (index as f32 + 0.5) / (2 * count) as f32 * 2.0 - 1.0;
        puffs.push(Puff {
            across: along * aspect / 2.0,
            down: -0.05,
            radius: 0.42,
            tall: false,
        });
    }
    puffs
}

#[derive(Debug)]
pub struct Sky {
    shades: Shades,
    lines: [Line; 3],
    pines: Vec<(f32, f32)>,
    near_pines: [f32; 2],
    stars: Vec<Star>,
    layers: Vec<Layer>,
    fog_phases: [f32; FOG.len()],
    seed: u64,
    ticks: u64,
    progress: f32,
}

impl Sky {
    pub fn new(mut rng: Rng, palette: &Palette) -> Self {
        let lines = [
            Line::new(&mut rng, RIDGE, RIDGE_WAVES),
            Line::new(&mut rng, FAR, FAR_WAVES),
            Line::new(&mut rng, NEAR, NEAR_WAVES),
        ];
        let pines = (0..PINES.1)
            .map(|_| (rng.random_range(0.04..0.96), rng.random_range(5.0..10.0)))
            .collect();
        let near_pines = [rng.random_range(13.0..17.0), rng.random_range(10.0..14.0)];
        let stars = (0..STARS)
            .map(|_| Star {
                place: (
                    rng.random_range(0.0..1.0),
                    rng.random_range(0.0..STAR_FLOOR),
                ),
                brightness: rng.random_range(0.35..1.0),
                rate: rng.random_range(1.5..4.5),
                phase: rng.random_range(0.0..TAU),
            })
            .collect();
        let layers = LAYERS
            .iter()
            .map(
                |&Spread {
                     count,
                     width,
                     height,
                     speed,
                     rows: (high, low),
                 }| Layer {
                    clouds: (0..count)
                        .map(|_| {
                            let puffs =
                                cloud(&mut rng, width / height * CLOUD_PANE.0 / CLOUD_PANE.1);
                            let reach = puffs
                                .iter()
                                .map(|puff| puff.across.abs() + puff.radius)
                                .fold(0.0, f32::max);
                            Cloud {
                                across: rng.random_range(0.0..1.0),
                                down: rng.random_range(high..low),
                                puffs,
                                reach,
                            }
                        })
                        .collect(),
                    height,
                    speed,
                },
            )
            .collect();
        Self {
            shades: Shades::new(palette),
            lines,
            pines,
            near_pines,
            stars,
            layers,
            fog_phases: std::array::from_fn(|_| rng.random_range(0.0..TAU)),
            seed: rng.random(),
            ticks: 0,
            progress: 0.0,
        }
    }

    pub fn status(&self) -> Color {
        self.shades.bright
    }

    pub fn repaint(&mut self, palette: &Palette) {
        self.shades = Shades::new(palette);
    }

    pub fn tick(&mut self) {
        self.ticks += 1;
        self.progress = 0.0;
    }

    /// Returns whether the progress moves the sky, which it does at every
    /// new progress, as the stars twinkle and the clouds drift.
    pub fn set_progress(&mut self, progress: f64) -> bool {
        let progress = progress as f32;
        std::mem::replace(&mut self.progress, progress) != progress
    }

    fn seconds(&self) -> f32 {
        (self.ticks as f32 + self.progress) * TICK_RATE.as_secs_f32()
    }

    pub fn render(
        &self,
        weather: Type,
        day: bool,
        palette: &Palette,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let mut canvas = Canvas::new(area.width, area.height, palette);
        let (w, h) = (canvas.width() as f32, canvas.height() as f32);
        if w == 0.0 || h == 0.0 {
            return;
        }
        let hours = if day {
            self.shades.day
        } else {
            self.shades.night
        };
        let look = Look::of(weather);
        let t = self.seconds();
        let sky = match look {
            Look::Clear => hours.clear,
            Look::Cloud => hours.cloudy,
            Look::Fog => hours.foggy,
        };
        canvas.bands((h * FAR.0) as usize, &sky);
        let disc = (h * DISC).max(LEAST_DISC);
        let sun = (SUN.0 * w, SUN.1 * h);
        let moon = (MOON.0 * w, MOON.1 * h);
        match (look, day) {
            (Look::Fog, true) => canvas.disc(sun, disc, hours.veiled),
            (Look::Fog, false) => canvas.disc(moon, disc, hours.veiled),
            (_, true) => self.sun(&mut canvas, sun, disc, t),
            (Look::Clear, false) => {
                self.stars(&mut canvas, t);
                self.moon(&mut canvas, moon, disc);
                self.shooting_star(&mut canvas, t);
            }
            (Look::Cloud, false) => self.moon(&mut canvas, moon, disc),
        }
        if look == Look::Cloud {
            for (layer, colours) in self.layers.iter().zip(hours.clouds) {
                draw_layer(&mut canvas, layer, colours, t);
            }
        }
        if look == Look::Clear && day {
            self.birds(&mut canvas, t);
        }
        // Light from the left by day, where the sun is, and from the right
        // at night, where the moon is.
        let light = if day { look.light() } else { -look.light() };
        let land = hours.land;
        let [ridge, far, near] = self.lines;
        // Fog hides the furthest ridge.
        if look != Look::Fog {
            fill_below(&mut canvas, |x| ridge.at(x, w, h), land.ridge);
        }
        self.fog(&mut canvas, look, hours.fog, 0, t);
        let far_line = |x: f32| far.at(x, w, h);
        fill_below(&mut canvas, far_line, land.far);
        light_crest(&mut canvas, far_line, land.far_lit, light, FAR_RIM);
        self.far_pines(&mut canvas, land, light);
        self.fog(&mut canvas, look, hours.fog, 1, t);
        let near_line = |x: f32| near.at(x, w, h);
        fill_below(&mut canvas, near_line, land.near);
        light_crest(&mut canvas, near_line, land.near_lit, light, NEAR_RIM);
        self.grass(&mut canvas, land);
        self.near_pines(&mut canvas, land, light);
        self.cottage(&mut canvas, land, day, t);
        self.fog(&mut canvas, look, hours.fog, 2, t);
        canvas.render(area, buf);
    }

    fn noise(&self, x: usize, y: usize) -> u64 {
        splitmix::mix(self.seed ^ ((x as u64) << 32) ^ y as u64)
    }

    fn far_pines(&self, canvas: &mut Canvas, land: Land, light: f32) {
        let (w, h) = (canvas.width() as f32, canvas.height() as f32);
        let count = (canvas.width() / PINE_ROOM).clamp(PINES.0, PINES.1);
        let scale = (h / 136.0).max(0.7);
        for &(across, tall) in self.pines.iter().take(count) {
            let x = (across * w).round();
            let ground = self.lines[1].at(x, w, h);
            pine(canvas, (x, ground), tall * scale, land, light);
        }
    }

    fn near_pines(&self, canvas: &mut Canvas, land: Land, light: f32) {
        let (w, h) = (canvas.width() as f32, canvas.height() as f32);
        let scale = (h / 136.0).max(0.7);
        for (across, tall) in NEAR_PINES.iter().zip(self.near_pines) {
            let x = (across * w).round();
            let ground = self.lines[2].at(x, w, h) + 1.0;
            pine(canvas, (x, ground), tall * scale, land, light);
        }
    }

    /// Tufts of grass along the near hill's line, and flecks in the soil
    /// below it, each in the same place every frame.
    fn grass(&self, canvas: &mut Canvas, land: Land) {
        let (w, h) = (canvas.width() as f32, canvas.height() as f32);
        let near = self.lines[2];
        // The bottom row of cells holds the credit, and a fleck under a
        // letter would light the cell behind it.
        let floor = canvas.height().saturating_sub(CELL_HEIGHT);
        for x in 0..canvas.width() {
            let line = near.at(x as f32, w, h).floor();
            let noise = self.noise(x, 0);
            if noise.is_multiple_of(TUFT_ODDS) {
                let tall = 1 + (noise >> 8) % 2;
                for rise in 1..=tall {
                    set_at(canvas, (x as f32, line - rise as f32), land.grass);
                }
            }
            let soil = (line + SOIL).max(0.0) as usize;
            for y in soil..floor {
                if self.noise(x, y).is_multiple_of(FLECK_ODDS) {
                    canvas.set(x, y, land.fleck);
                }
            }
        }
    }

    fn cottage(&self, canvas: &mut Canvas, land: Land, day: bool, t: f32) {
        let (w, h) = (canvas.width() as f32, canvas.height() as f32);
        let near = self.lines[2];
        let left = (COTTAGE * w).round();
        let ground = near.at(left, w, h).round();
        let wide = (w * 0.045).max(8.0).round();
        let tall = (h * 0.045).max(5.0).round();
        let top = ground - tall;
        let right = left + wide;
        rect(canvas, (left, top), (right, ground + 2.0), land.wall);
        rect(
            canvas,
            (right - 3.0, ground - 3.0),
            (right - 1.0, ground + 2.0),
            land.door,
        );
        let chimney = right - 3.0;
        rect(
            canvas,
            (chimney, top - 5.0),
            (chimney + 2.0, top),
            land.chimney,
        );
        rect(
            canvas,
            (chimney - 0.5, top - 6.0),
            (chimney + 2.5, top - 5.0),
            land.chimney,
        );
        let peak = (tall / 2.0).floor() + 2.0;
        canvas.polygon(
            &[
                (left - 1.5, top - 0.5),
                (left - 1.5 + peak, top - 0.5 - peak),
                (right + 1.5 - peak, top - 0.5 - peak),
                (right + 1.5, top - 0.5),
            ],
            land.roof,
        );
        canvas.line((left - 1.5, top), (right + 1.5, top), 1.0, land.eave);
        let window = (left + (wide / 3.0).floor() + 1.0, top + 3.0);
        if day {
            canvas.disc(window, 1.4, self.shades.glass);
        } else {
            let flicker = 0.5 + 0.5 * (t * 9.0).sin() * (t * 3.3).sin();
            let step = ((flicker * 3.0).round() as usize).min(3);
            glow(canvas, window, 1.0, 7.0, self.shades.lamp, 0.25);
            canvas.disc(window, 1.4, self.shades.window[step]);
        }
        for post in 0..POSTS {
            let x = right + 3.0 + post as f32 * POST_GAP;
            let foot = near.at(x, w, h).round();
            rect(canvas, (x, foot - 3.0), (x + 1.0, foot + 1.0), land.fence);
        }
        let last = right + 3.0 + (POSTS - 1) as f32 * POST_GAP;
        let mut x = right + 3.0;
        while x <= last {
            set_at(canvas, (x, near.at(x, w, h).round() - 2.0), land.fence);
            x += 1.0;
        }
        for puff in 0..10 {
            let rise = puff as f32;
            let x = chimney + 1.0 + rise * 0.6 + (t * 1.5 - rise * 0.6).sin() * 1.2;
            let y = top - 7.0 - rise * 1.4;
            let share = octant::stepped(0.45 * (1.0 - rise / 10.0), SHADE_STEPS);
            blend_at(canvas, (x, y), land.smoke, share);
        }
    }

    fn sun(&self, canvas: &mut Canvas, centre: (f32, f32), radius: f32, t: f32) {
        let (reach, edge) = SUN_GLOW;
        glow(
            canvas,
            centre,
            radius,
            radius * reach,
            self.shades.sun_glow,
            edge,
        );
        let (from, to) = (radius * RAY_SPAN.0, radius * RAY_SPAN.1);
        let dots = (to - from).ceil() as usize;
        for ray in 0..RAYS {
            let angle = ray as f32 / RAYS as f32 * TAU + t * RAY_TURN;
            for dot in 0..dots {
                let along = dot as f32 / dots as f32;
                let out = from + (to - from) * along;
                let place = (centre.0 + angle.cos() * out, centre.1 + angle.sin() * out);
                let share = octant::stepped(0.6 * (1.0 - along), SHADE_STEPS);
                blend_at(canvas, place, self.shades.sun_glow, share);
            }
        }
        canvas.disc(centre, radius, self.shades.sun);
        canvas.disc(
            (centre.0 - radius * 0.15, centre.1 - radius * 0.15),
            radius * CORE,
            self.shades.sun_core,
        );
    }

    fn moon(&self, canvas: &mut Canvas, centre: (f32, f32), radius: f32) {
        let (reach, edge) = MOON_GLOW;
        glow(
            canvas,
            centre,
            radius,
            radius * reach,
            self.shades.moon_glow,
            edge,
        );
        canvas.disc(centre, radius, self.shades.moon);
        for (across, down, size) in [(-0.3, -0.2, 0.28), (0.25, 0.3, 0.2), (0.35, -0.35, 0.14)] {
            canvas.disc(
                (centre.0 + across * radius, centre.1 + down * radius),
                size * radius,
                self.shades.crater,
            );
        }
    }

    fn stars(&self, canvas: &mut Canvas, t: f32) {
        let (w, h) = (canvas.width() as f32, canvas.height() as f32);
        let count = ((w * h * STAR_FLOOR / STAR_ROOM) as usize).clamp(4, STARS);
        for star in self.stars.iter().take(count) {
            let place = (star.place.0 * w, star.place.1 * h);
            let twinkle = star.brightness * (0.6 + 0.4 * (t * star.rate + star.phase).sin());
            blend_at(
                canvas,
                place,
                self.shades.star,
                octant::stepped(twinkle, SHADE_STEPS),
            );
            if star.brightness > 0.9 {
                let share = octant::stepped(0.4 * twinkle, SHADE_STEPS);
                for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                    blend_at(
                        canvas,
                        (place.0 + dx, place.1 + dy),
                        self.shades.star,
                        share,
                    );
                }
            }
        }
    }

    fn shooting_star(&self, canvas: &mut Canvas, t: f32) {
        let round = (t / SHOOTING_GAP).floor();
        let along = (t - round * SHOOTING_GAP) / SHOOTING;
        if along > 1.0 {
            return;
        }
        let (w, h) = (canvas.width() as f32, canvas.height() as f32);
        let mixed = splitmix::mix(self.seed ^ round as u64);
        let start = (
            (mixed % 1000) as f32 / 1000.0 * 0.4 + 0.05,
            (mixed / 1000 % 1000) as f32 / 1000.0 * 0.1 + 0.05,
        );
        let from = (start.0 * w, start.1 * h);
        let to = ((start.0 + 0.3) * w, (start.1 + 0.18) * h);
        let length = (to.0 - from.0).hypot(to.1 - from.1);
        let (dx, dy) = ((to.0 - from.0) / length, (to.1 - from.1) / length);
        let head = (
            from.0 + (to.0 - from.0) * along,
            from.1 + (to.1 - from.1) * along,
        );
        let fade = (along * PI).sin();
        for tail in 0..16 {
            let back = tail as f32;
            let share = octant::stepped((1.0 - back / 16.0) * fade, SHADE_STEPS);
            blend_at(
                canvas,
                (head.0 - dx * back, head.1 - dy * back),
                self.shades.star,
                share,
            );
        }
    }

    fn birds(&self, canvas: &mut Canvas, t: f32) {
        let (w, h) = (canvas.width() as f32, canvas.height() as f32);
        for (index, &(across, down)) in BIRDS.iter().enumerate() {
            let phase = index as f32 * 1.3;
            let x = wrap(across * w + t * GLIDE, w, 4.0);
            let y = (down * h + (t * 0.8 + phase).sin() * 1.5).round();
            let flap = (t * 6.0 + phase).sin().round();
            for (dx, lift) in [(-2.0, 0.5), (-1.0, 0.3), (0.0, 0.0), (1.0, 0.3), (2.0, 0.5)] {
                set_at(canvas, (x + dx, y - flap * lift), self.shades.bird);
            }
        }
    }

    fn fog(&self, canvas: &mut Canvas, look: Look, colour: Color, depth: u8, t: f32) {
        if look != Look::Fog {
            return;
        }
        let (w, h) = (canvas.width(), canvas.height());
        let (wf, hf) = (w as f32, h as f32);
        for (&(middle, half, thickest, speed, lies), phase) in FOG.iter().zip(self.fog_phases) {
            if lies != depth {
                continue;
            }
            let (middle, half) = (middle * hf, half * hf);
            let top = (middle - half).floor().max(0.0) as usize;
            let bottom = ((middle + half).ceil() as usize).min(h);
            for y in top..bottom {
                let off = (y as f32 - middle) / half;
                let thick = thickest * (1.0 - off * off).max(0.0);
                for x in 0..w {
                    let along = (x as f32 - speed * t * wf / CLOUD_PANE.0) / wf * TAU;
                    let wave = 0.55
                        + 0.3 * (along * 2.0 + phase).sin()
                        + 0.15 * (along * 5.0 - phase).sin();
                    let share = octant::stepped(thick * wave, SHADE_STEPS);
                    canvas.blend(x, y, colour, share);
                }
            }
        }
    }
}

/// A pine of three tiers on a trunk, its foot at `foot`, with the side
/// toward the light lit. A positive `light` comes from the left.
fn pine(canvas: &mut Canvas, foot: (f32, f32), tall: f32, land: Land, light: f32) {
    let (x, ground) = foot;
    rect(
        canvas,
        (x - 0.5, ground - 2.0),
        (x + 0.5, ground + 1.0),
        land.trunk,
    );
    let tip = ground - tall;
    let tier = tall * 0.5;
    for (index, from) in TIERS.iter().enumerate() {
        let top = tip + from * tall;
        let bottom = (top + tier).min(ground - 1.0);
        let half = tall * (0.2 + 0.08 * index as f32);
        canvas.polygon(
            &[(x, top), (x + half, bottom), (x - half, bottom)],
            land.pine,
        );
        let side = if light >= 0.0 { -half } else { half };
        let lit = land.pine_lit;
        if light.abs() > 0.5 {
            canvas.polygon(&[(x, top), (x, bottom), (x + side, bottom)], lit);
        }
    }
}

/// Lights the crest of the line `top` gives where it slopes toward the
/// light, from `rim` pixels down to the line, brightest at the line.
fn light_crest(canvas: &mut Canvas, top: impl Fn(f32) -> f32, lit: Color, light: f32, rim: f32) {
    let height = canvas.height();
    for x in 0..canvas.width() {
        let at = x as f32;
        let slope = (top(at + 1.0) - top(at - 1.0)) / 2.0;
        let face = (-slope * light * FACE_LIGHT).clamp(0.0, 1.0);
        if face == 0.0 {
            continue;
        }
        let line = top(at);
        let first = line.ceil().max(0.0) as usize;
        let last = ((line + rim).ceil().max(0.0) as usize).min(height);
        for y in first..last {
            let down = (y as f32 - line) / rim;
            let share = octant::stepped(face * (1.0 - down), SHADE_STEPS);
            canvas.blend(x, y, lit, share);
        }
    }
}

fn draw_layer(canvas: &mut Canvas, layer: &Layer, colours: [Color; 3], t: f32) {
    let (w, h) = (canvas.width() as f32, canvas.height() as f32);
    let tall = layer.height * h;
    let [shade, body, light] = colours;
    for cloud in &layer.clouds {
        let x = wrap(
            cloud.across * w + layer.speed * t * w / CLOUD_PANE.0,
            w,
            cloud.reach * tall,
        );
        let y = cloud.down * h;
        let at = |puff: &Puff, down: f32| (x + puff.across * tall, y + (puff.down + down) * tall);
        for puff in &cloud.puffs {
            canvas.disc(at(puff, puff.radius * 0.25), puff.radius * tall, shade);
        }
        for puff in &cloud.puffs {
            canvas.disc(at(puff, 0.0), puff.radius * 0.95 * tall, body);
        }
        for puff in cloud.puffs.iter().filter(|puff| puff.tall) {
            let (px, py) = at(puff, -puff.radius * 0.3);
            canvas.disc(
                (px - puff.radius * 0.2 * tall, py),
                puff.radius * 0.55 * tall,
                light,
            );
        }
    }
}

/// `x` moved into the span from `-margin` to `span + margin`, so a shape
/// leaves one edge whole before it comes in at the other.
fn wrap(x: f32, span: f32, margin: f32) -> f32 {
    (x + margin).rem_euclid(span + 2.0 * margin) - margin
}

/// Fills each column below the line `top` gives at its middle, with the
/// edge pixel covered by its share below the line.
fn fill_below(canvas: &mut Canvas, top: impl Fn(f32) -> f32, colour: Color) {
    let height = canvas.height();
    for x in 0..canvas.width() {
        let line = top(x as f32);
        let first = (line - 0.5).floor().max(0.0) as usize;
        for y in first..height {
            let share = (y as f32 + 0.5 - line).clamp(0.0, 1.0);
            canvas.blend(x, y, colour, octant::stepped(share, COVERAGE_STEPS));
        }
    }
}

/// Blends a glow round `centre` that is `edge` at `inner` pixels out and
/// fades to none at `reach`.
fn glow(canvas: &mut Canvas, centre: (f32, f32), inner: f32, reach: f32, colour: Color, edge: f32) {
    let left = (centre.0 - reach).floor().max(0.0) as usize;
    let top = (centre.1 - reach).floor().max(0.0) as usize;
    let right = ((centre.0 + reach).ceil().max(0.0) as usize).min(canvas.width());
    let bottom = ((centre.1 + reach).ceil().max(0.0) as usize).min(canvas.height());
    for y in top..bottom {
        for x in left..right {
            let distance = (x as f32 - centre.0).hypot(y as f32 - centre.1);
            if distance >= reach {
                continue;
            }
            let out = ((distance - inner) / (reach - inner)).max(0.0);
            canvas.blend(x, y, colour, octant::falloff(out, edge, SHADE_STEPS));
        }
    }
}

/// Fills the pixels from `from` to `to`, each a pixel's top left corner.
fn rect(canvas: &mut Canvas, from: (f32, f32), to: (f32, f32), colour: Color) {
    let (left, top) = (from.0.max(0.0) as usize, from.1.max(0.0) as usize);
    let (right, bottom) = (to.0.max(0.0) as usize, to.1.max(0.0) as usize);
    for y in top..bottom {
        for x in left..right {
            canvas.set(x, y, colour);
        }
    }
}

fn pixel(place: (f32, f32)) -> Option<(usize, usize)> {
    let (x, y) = (place.0.round(), place.1.round());
    (x >= 0.0 && y >= 0.0).then_some((x as usize, y as usize))
}

fn blend_at(canvas: &mut Canvas, place: (f32, f32), colour: Color, share: f32) {
    if let Some((x, y)) = pixel(place) {
        canvas.blend(x, y, colour, share);
    }
}

fn set_at(canvas: &mut Canvas, place: (f32, f32), colour: Color) {
    if let Some((x, y)) = pixel(place) {
        canvas.set(x, y, colour);
    }
}
