#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pixels;

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use hearth::octant::CELL_HEIGHT;
use hearth::scene::{Rng, Scene};
use hearth::scenes::weather::{CREDIT, NO_LOCATION, READING, Weather};
use hearth::sound::Cue;
use hearth::theme::{self, Palette};
use hearth::weather::{Location, Source, Type, parse};
use rand::SeedableRng;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

const AREA: Rect = Rect::new(0, 0, 100, 30);

/// A local server that answers every request with `status` and `body`
/// once `gate` lets it, and hands back the request line it read.
struct Server {
    base: String,
    requests: Receiver<String>,
}

fn serve_after(status: &str, body: &str, gate: Option<Receiver<()>>) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let reply = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let (sender, requests) = mpsc::channel();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut header = String::new();
            while reader.read_line(&mut header).unwrap() > 2 {
                header.clear();
            }
            if let Some(gate) = &gate {
                gate.recv().unwrap();
            }
            let _ = sender.send(line.trim_end().to_string());
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    Server { base, requests }
}

fn serve(status: &str, body: &str) -> Server {
    serve_after(status, body, None)
}

/// A base address where nothing listens.
fn closed() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    format!("http://{}", listener.local_addr().unwrap())
}

/// The reply Open-Meteo gives for the current weather by day.
fn current(code: i64, temperature: f64, wind: f64) -> String {
    current_at(code, temperature, wind, 1)
}

/// The reply Open-Meteo gives for the current weather, by day when
/// `is_day` is 1 and at night when it is 0.
fn current_at(code: i64, temperature: f64, wind: f64, is_day: i64) -> String {
    format!(
        r#"{{"latitude":59.906296,"longitude":10.744095,"current_units":{{"time":"iso8601","interval":"seconds","weather_code":"wmo code","temperature_2m":"°C","wind_speed_10m":"km/h","is_day":""}},"current":{{"time":"2026-11-09T13:15","interval":900,"weather_code":{code},"temperature_2m":{temperature},"wind_speed_10m":{wind},"is_day":{is_day}}}}}"#
    )
}

fn seeded() -> Rng {
    Rng::seed_from_u64(7)
}

fn oslo() -> Location {
    Location::new(59.91, 10.75).unwrap()
}

fn render(scene: &Weather) -> Buffer {
    let mut buffer = Buffer::empty(AREA);
    scene.render(AREA, &mut buffer);
    buffer
}

fn shows(buffer: &Buffer, text: &str) -> bool {
    (0..AREA.height).any(|y| {
        let row: String = (0..AREA.width).map(|x| buffer[(x, y)].symbol()).collect();
        row.contains(text)
    })
}

/// The frame once the scene has stopped reading, ticking as a pane would.
fn settled(scene: &mut Weather) -> Buffer {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        scene.tick(AREA);
        let shown = render(scene);
        if !shows(&shown, READING) || Instant::now() > deadline {
            return shown;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

/// The weather scene shows the weather where the user is, with the credit.
/// Given: a server that answers with moderate rain at 7.4 °C.
/// When: the scene for Oslo reads it.
/// Then: the frame names the rain, the rounded temperature and the place,
/// shows the Open-Meteo credit, and the request asked for that place.
///
/// Why it matters: the licence of the data asks for the credit beside it,
/// and a request for the wrong place shows someone else's weather.
#[test]
fn the_scene_names_the_weather_the_temperature_and_the_place() {
    // Given:
    let server = serve("200 OK", &current(63, 7.4, 12.0));

    // When:
    let mut scene = Weather::new(
        seeded(),
        &theme::ROSE_PINE,
        Some(Source::new(&server.base, oslo())),
    );
    let shown = settled(&mut scene);

    // Then:
    assert!(shows(&shown, "moderate rain · 7 °C · 59.91 N 10.75 E"));
    assert!(shows(&shown, CREDIT));
    let request = server
        .requests
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert!(request.starts_with("GET /v1/forecast?"), "{request}");
    for part in [
        "latitude=59.91",
        "longitude=10.75",
        "current=weather_code,temperature_2m,wind_speed_10m,is_day",
    ] {
        assert!(request.contains(part), "{part} in {request}");
    }
}

/// Each weather code reads as its words, and a strong wind as the wind.
/// Given: replies across the codes, the wind threshold and both hemispheres.
/// When: each is read for its place.
/// Then: the status line names the weather, the rounded temperature and the
/// place, with south and west for negative coordinates.
///
/// Why it matters: no weather code means wind, so a gale over a clear sky
/// would read as calm weather.
#[test]
fn each_code_reads_as_its_weather_and_a_strong_wind_as_the_wind() {
    let santiago = Location::new(-33.87, -70.65).unwrap();
    for (code, temperature, wind, place, expected) in [
        (0, 4.2, 5.0, oslo(), "clear sky · 4 °C · 59.91 N 10.75 E"),
        (3, 9.0, 10.0, oslo(), "overcast · 9 °C · 59.91 N 10.75 E"),
        (45, 3.0, 2.0, oslo(), "fog · 3 °C · 59.91 N 10.75 E"),
        (
            73,
            -2.4,
            8.0,
            oslo(),
            "moderate snowfall · -2 °C · 59.91 N 10.75 E",
        ),
        (
            95,
            14.0,
            20.0,
            oslo(),
            "thunderstorm · 14 °C · 59.91 N 10.75 E",
        ),
        (
            1,
            8.0,
            38.9,
            oslo(),
            "mainly clear · 8 °C · 59.91 N 10.75 E",
        ),
        (
            1,
            8.0,
            39.0,
            oslo(),
            "wind 39 km/h · 8 °C · 59.91 N 10.75 E",
        ),
        (
            63,
            7.0,
            60.0,
            oslo(),
            "moderate rain · 7 °C · 59.91 N 10.75 E",
        ),
        (
            2,
            -0.3,
            0.0,
            santiago,
            "partly cloudy · 0 °C · 33.87 S 70.65 W",
        ),
    ] {
        // When:
        let report = parse(&current(code, temperature, wind)).unwrap();

        // Then:
        assert_eq!(report.status(place), expected, "code {code}, wind {wind}");
    }
}

/// Each weather code falls in one of the seven weather types, and a strong
/// wind over a clear or cloudy sky is wind.
/// Given: every code Open-Meteo sends, with the wind below and at the
/// threshold.
/// When: each reply is read.
/// Then: each code gives its type, and the wind turns only a clear or
/// cloudy sky to wind.
///
/// Why it matters: the type picks what the sky draws, so a code in the
/// wrong type draws snow in the rain.
#[test]
fn each_code_falls_in_its_weather_type() {
    let rain = [51, 53, 55, 56, 57, 61, 63, 65, 66, 67, 80, 81, 82];
    for (codes, wind, expected) in [
        (&[0, 1][..], 38.9, Type::Clear),
        (&[2, 3], 38.9, Type::Cloud),
        (&[45, 48], 39.0, Type::Fog),
        (&rain, 39.0, Type::Rain),
        (&[71, 73, 75, 77, 85, 86], 39.0, Type::Snow),
        (&[95, 96, 97, 99], 39.0, Type::Storm),
        (&[0, 1, 2, 3], 39.0, Type::Wind),
    ] {
        for &code in codes {
            // When:
            let report = parse(&current(code, 10.0, wind)).unwrap();

            // Then:
            assert_eq!(report.weather(), expected, "code {code}, wind {wind}");
        }
    }
}

/// Open-Meteo says whether it is day or night at the place.
/// Given: a reply with `is_day` 1, one with 0, and one with none.
/// When: each is read.
/// Then: the first is day, the second is night, and the third names what
/// it lacks.
///
/// Why it matters: a clear sky shows the sun by day and the moon at night,
/// so a guess would show the sun at midnight.
#[test]
fn is_day_tells_day_from_night() {
    // When:
    let day = parse(&current_at(0, 10.0, 5.0, 1)).unwrap();
    let night = parse(&current_at(0, 10.0, 5.0, 0)).unwrap();
    let neither =
        parse(r#"{"current":{"weather_code":0,"temperature_2m":10.0,"wind_speed_10m":5.0}}"#);

    // Then:
    assert!(day.day());
    assert!(!night.day());
    assert_eq!(neither.unwrap_err(), "Open-Meteo sent no is_day");
}

/// A reply hearth cannot use shows what went wrong, in plain text.
/// Given: a refusal whose reason carries an escape sequence, a server
/// error, a page that is not the weather, a reply with no temperature, an
/// unknown weather code, and an address where nothing listens.
/// When: the scene reads each.
/// Then: the frame says what went wrong, and no cell holds a control
/// character.
///
/// Why it matters: the reason is text from the network, and an escape
/// sequence drawn as it is would drive the user's terminal.
#[test]
fn a_reply_hearth_cannot_use_shows_what_went_wrong() {
    let refusal = r#"{"error":true,"reason":"Latitude must be in range of -90 to 90°.\u001b[2J"}"#;
    let no_temperature = r#"{"current":{"weather_code":3,"wind_speed_10m":2.0,"is_day":1}}"#;
    for (base, expected) in [
        (
            serve("400 Bad Request", refusal).base,
            "Open-Meteo: Latitude must be in range of -90 to 90°.",
        ),
        (
            serve("500 Internal Server Error", "<html>").base,
            "Open-Meteo answered 500",
        ),
        (
            serve("200 OK", "<html>").base,
            "Open-Meteo sent a reply hearth cannot read",
        ),
        (
            serve("200 OK", no_temperature).base,
            "Open-Meteo sent no temperature_2m",
        ),
        (
            serve("200 OK", &current(42, 5.0, 3.0)).base,
            "Open-Meteo sent weather code 42, which hearth does not know",
        ),
        (closed(), "cannot reach Open-Meteo"),
    ] {
        // When:
        let mut scene = Weather::new(
            seeded(),
            &theme::ROSE_PINE,
            Some(Source::new(&base, oslo())),
        );
        let shown = settled(&mut scene);

        // Then:
        assert!(shows(&shown, expected), "{expected}");
        for position in AREA.positions() {
            let symbol = shown[position].symbol();
            assert!(
                !symbol.chars().any(char::is_control),
                "{symbol:?} at {position:?}"
            );
        }
    }
}

/// The scene says it is reading until the weather lands.
/// Given: a server that holds its answer.
/// When: the scene ticks before the answer, and again after it.
/// Then: the frame says it is reading the weather, then shows the weather.
///
/// Why it matters: a slow network must not freeze the app or leave an
/// empty pane that reads as a fault.
#[test]
fn the_scene_says_it_is_reading_until_the_weather_lands() {
    // Given:
    let (open, gate) = mpsc::channel();
    let server = serve_after("200 OK", &current(3, 9.0, 10.0), Some(gate));
    let mut scene = Weather::new(
        seeded(),
        &theme::ROSE_PINE,
        Some(Source::new(&server.base, oslo())),
    );

    // When:
    scene.tick(AREA);

    // Then:
    assert!(shows(&render(&scene), READING));

    // When:
    open.send(()).unwrap();

    // Then:
    assert!(shows(&settled(&mut scene), "overcast · 9 °C"));
}

/// The scene asks again after a failure, and stops asking once it closes.
/// Given: a server that fails every request, and a scene that retries after
/// 200 ms.
/// When: the scene asks twice, then closes while it waits to ask again.
/// Then: no request reaches the server after the close.
///
/// Why it matters: each request sends the user's place to Open-Meteo, so a
/// scene that is gone must not ask once more.
#[test]
fn the_scene_asks_again_after_a_failure_and_stops_once_closed() {
    // Given:
    let server = serve("500 Internal Server Error", "<html>");
    let wait = Duration::from_millis(200);
    let source = Source::new(&server.base, oslo()).every(wait, wait);

    // When:
    let scene = Weather::new(seeded(), &theme::ROSE_PINE, Some(source));
    for _ in 0..2 {
        server
            .requests
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
    }
    thread::sleep(wait / 2);
    drop(scene);

    // Then:
    let late = server.requests.recv_timeout(wait * 2);
    assert!(late.is_err(), "asked after the close: {late:?}");
}

/// Without a location, the scene says how to give one.
/// Given: no location in the file or on the command line.
/// When: the scene ticks.
/// Then: the frame says how to set the location, and nothing is fetched.
///
/// Why it matters: an empty pane gives no hint that the scene needs a
/// place.
#[test]
fn without_a_location_the_scene_says_how_to_give_one() {
    // When:
    let mut scene = Weather::new(seeded(), &theme::ROSE_PINE, None);
    scene.tick(AREA);

    // Then:
    assert!(shows(&render(&scene), NO_LOCATION));
}

/// A location takes latitudes from -90 to 90 and longitudes from -180 to
/// 180, and nothing else.
/// Given: coordinates on each edge, just past it, and one that is no number.
/// When: each makes a location.
/// Then: the edges are taken, and each other value is refused with the
/// range it broke.
///
/// Why it matters: a value out of range reaches Open-Meteo as a refusal on
/// every fetch, long after the typo was made.
#[test]
fn a_location_holds_only_coordinates_in_range() {
    for (latitude, longitude) in [(90.0, 180.0), (-90.0, -180.0), (0.0, 0.0)] {
        assert!(
            Location::new(latitude, longitude).is_ok(),
            "{latitude}, {longitude}"
        );
    }
    for (latitude, longitude, range) in [
        (90.01, 0.0, "latitude must be from -90 to 90"),
        (0.0, -180.01, "longitude must be from -180 to 180"),
        (f64::NAN, 0.0, "latitude must be from -90 to 90"),
    ] {
        let refused = Location::new(latitude, longitude).unwrap_err();
        assert!(refused.contains(range), "{refused}");
    }
}

/// The ticks between the two frames a test of motion compares: about four
/// seconds.
const LATER: u32 = 66;

fn landed_in(palette: &'static Palette, body: &str) -> Weather {
    let server = serve("200 OK", body);
    let mut scene = Weather::new(seeded(), palette, Some(Source::new(&server.base, oslo())));
    settled(&mut scene);
    scene
}

fn landed(body: &str) -> Weather {
    landed_in(&theme::ROSE_PINE, body)
}

fn later(scene: &mut Weather) {
    for _ in 0..LATER {
        scene.tick(AREA);
    }
}

fn channels(colour: Color) -> (u8, u8, u8) {
    match colour {
        Color::Rgb(r, g, b) => (r, g, b),
        other => panic!("{other:?} is not a colour of the theme"),
    }
}

/// The sun is gold toward the text: bright, and far redder than blue.
fn is_sunlit(colour: Color) -> bool {
    let (r, _, b) = channels(colour);
    r >= 200 && i32::from(r) - i32::from(b) >= 50
}

/// The moon, the stars and the clouds lean to the text and the subtle
/// colour, which in Rose Pine are light and hold at least as much blue as
/// red. The sky holds less light, and the sun and its rays less blue.
fn is_pale(colour: Color) -> bool {
    let (r, g, b) = channels(colour);
    r.min(g).min(b) >= 100 && b >= r
}

/// Each pixel of the frame in the rows from `top` to `bottom`, as shares of
/// the frame's height, that `keep` accepts, with its place.
fn pixels_in(
    scene: &Weather,
    (top, bottom): (f32, f32),
    keep: impl Fn(Color) -> bool,
) -> Vec<(usize, usize)> {
    let grid = pixels::colours(&render(scene), AREA);
    let height = grid.len() as f32;
    let rows = (top * height) as usize..(bottom * height) as usize;
    rows.flat_map(|y| {
        let keep = &keep;
        grid[y]
            .iter()
            .enumerate()
            .filter(move |(_, colour)| keep(**colour))
            .map(move |(x, _)| (x, y))
    })
    .collect()
}

/// The pixels with no other within `reach` pixels, as a star stands alone
/// and the moon does not.
fn alone(places: &[(usize, usize)], reach: usize) -> usize {
    places
        .iter()
        .filter(|(x, y)| {
            !places.iter().any(|(ox, oy)| {
                (ox, oy) != (x, y) && x.abs_diff(*ox) <= reach && y.abs_diff(*oy) <= reach
            })
        })
        .count()
}

fn top_light(scene: &Weather) -> u32 {
    pixels::colours(&render(scene), AREA)[0]
        .iter()
        .map(|colour| {
            let (r, g, b) = channels(*colour);
            u32::from(r) + u32::from(g) + u32::from(b)
        })
        .sum()
}

/// A clear sky shows the sun by day, and the moon and the stars at night.
/// Given: a clear sky by day and a clear sky at night.
/// When: each scene draws.
/// Then: the day frame has a sun in the upper half. The night frame has no
/// sun, a moon, stars that stand alone, and a darker sky.
///
/// Why it matters: the sky is how the scene tells day from night, so the
/// sun at midnight shows the wrong time of day.
#[test]
fn a_clear_sky_shows_the_sun_by_day_and_the_moon_and_stars_at_night() {
    // When:
    let day = landed(&current_at(0, 18.0, 5.0, 1));
    let night = landed(&current_at(0, 4.0, 5.0, 0));

    // Then:
    let upper = (0.0, 0.5);
    let sun = pixels_in(&day, upper, is_sunlit).len();
    assert!(sun >= 40, "{sun} pixels of sun by day");
    assert!(
        pixels_in(&night, upper, is_sunlit).is_empty(),
        "a sun at night"
    );
    let pale = pixels_in(&night, upper, is_pale);
    assert!(pale.len() >= 40, "{} pale pixels at night", pale.len());
    let stars = alone(&pale, 2);
    assert!(stars >= 5, "{stars} stars at night");
    assert!(top_light(&day) > top_light(&night));
}

/// Clouds drift across the sky.
/// Given: a partly cloudy sky by day.
/// When: the scene draws, then draws again four seconds later.
/// Then: both frames hold clouds, and the clouds lie in other places.
///
/// Why it matters: the scene is a screensaver, and a sky that stands still
/// reads as a frozen app.
#[test]
fn clouds_drift_across_the_sky() {
    // Given:
    let mut scene = landed(&current(2, 9.0, 10.0));
    let sky = (0.0, 0.5);

    // When:
    let before = pixels_in(&scene, sky, is_pale);
    later(&mut scene);
    let after = pixels_in(&scene, sky, is_pale);

    // Then:
    assert!(before.len() >= 200, "{} pixels of cloud", before.len());
    assert!(after.len() >= 200, "{} pixels of cloud", after.len());
    let moved = before.iter().filter(|place| !after.contains(place)).count();
    assert!(moved >= 50, "{moved} pixels of cloud moved");
}

/// Fog drifts over the land in front of the hills.
/// Given: a fog and a clear sky, both by day.
/// When: each scene draws, then draws again four seconds later.
/// Then: the ground at the foot of the frame changes under the fog, and
/// stays the same under the clear sky.
///
/// Why it matters: fog that drifts only behind the hills leaves the land
/// sharp, and the scene reads as a clear day.
#[test]
fn fog_drifts_over_the_land_in_front_of_the_hills() {
    let ground = (0.86, 0.94);
    for (body, drifts) in [(current(45, 3.0, 2.0), true), (current(0, 3.0, 2.0), false)] {
        // Given:
        let mut scene = landed(&body);
        let grid = |scene: &Weather| pixels::colours(&render(scene), AREA);

        // When:
        let before = grid(&scene);
        later(&mut scene);
        let after = grid(&scene);

        // Then:
        let height = before.len() as f32;
        let rows = (ground.0 * height) as usize..(ground.1 * height) as usize;
        let changed = rows
            .flat_map(|y| (0..before[y].len()).map(move |x| (x, y)))
            .filter(|&(x, y)| before[y][x] != after[y][x])
            .count();
        assert_eq!(changed >= 20, drifts, "{changed} pixels of ground changed");
    }
}

/// Rain and a storm fall onto the land.
/// Given: a rain, a storm and a cloudy sky, all by day.
/// When: each scene draws, then draws again four seconds later.
/// Then: the ground at the foot of the frame changes under the rain and
/// the storm, and stays the same under the clouds.
///
/// Why it matters: rain that falls only in the sky reads as a far shower,
/// and the land of the scene stays dry.
#[test]
fn rain_and_storms_fall_onto_the_land() {
    for (code, falls) in [(63, true), (95, true), (3, false)] {
        // Given:
        let mut scene = landed(&current(code, 3.0, 2.0));

        // When:
        let before = pixels::colours(&render(&scene), AREA);
        later(&mut scene);
        let after = pixels::colours(&render(&scene), AREA);

        // Then:
        let changed = ground_rows(&before)
            .flat_map(|y| (0..before[y].len()).map(move |x| (x, y)))
            .filter(|&(x, y)| before[y][x] != after[y][x])
            .count();
        assert_eq!(
            changed >= 20,
            falls,
            "code {code}: {changed} pixels of ground changed"
        );
    }
}

/// The pixel rows of the ground at the foot of a frame, above the credit.
fn ground_rows(grid: &[Vec<Color>]) -> std::ops::Range<usize> {
    let height = grid.len() as f32;
    (0.86 * height) as usize..(0.94 * height) as usize
}

/// The colour most pixels of the ground at the foot of the frame show, and
/// the pixels of the whole frame that show it.
fn ground_cover(scene: &Weather) -> (Color, usize) {
    let grid = pixels::colours(&render(scene), AREA);
    let mut counts = std::collections::HashMap::new();
    for colour in ground_rows(&grid).flat_map(|y| grid[y].iter()) {
        *counts.entry(*colour).or_insert(0_usize) += 1;
    }
    let (ground, _) = counts.into_iter().max_by_key(|(_, count)| *count).unwrap();
    let cover = grid.iter().flatten().filter(|c| **c == ground).count();
    (ground, cover)
}

/// How much lighter than the sky behind it a flake is at the least, where
/// the bands of the sky differ by far less.
const FLAKE_LIGHT: u32 = 20_000;

/// The ticks in which the snow grows to its full depth, with room to spare.
const SNOWED_IN: u32 = 1000;

/// Snow falls through the air, lies on the land and grows deeper.
/// Given: a snow and a rain, both by day.
/// When: each scene draws, the snow draws again four seconds later, and
/// again a minute later.
/// Then: flakes lighter than the sky move in the air under the clouds. The
/// ground is pale under the snow and not under the rain, and after the
/// minute the snow in front covers more of the frame.
///
/// Why it matters: flakes that fall on green hills read as blossom, and
/// snow that never collects reads as a loop that started a second ago.
#[test]
fn snow_lies_on_the_land_and_grows_deeper() {
    // Given:
    let mut snow = landed(&current(73, -2.0, 4.0));
    let rain = landed(&current(63, 9.0, 10.0));

    // When:
    let air = (0.3, 0.5);
    let grid = pixels::colours(&render(&snow), AREA);
    let sky = grid[(air.1 * grid.len() as f32) as usize - 1]
        .iter()
        .map(|colour| theme::luma(*colour))
        .min()
        .unwrap();
    let is_flake = |colour| theme::luma(colour) > sky + FLAKE_LIGHT;
    let flakes = pixels_in(&snow, air, is_flake);
    let (white, first) = ground_cover(&snow);
    let (wet, _) = ground_cover(&rain);
    later(&mut snow);
    let moved = pixels_in(&snow, air, is_flake);
    for _ in 0..SNOWED_IN {
        snow.tick(AREA);
    }
    let (still_white, deep) = ground_cover(&snow);

    // Then:
    assert!(flakes.len() >= 20, "{} pixels of flake", flakes.len());
    let fell = flakes.iter().filter(|place| !moved.contains(place)).count();
    assert!(fell >= 20, "{fell} pixels of flake moved");
    assert!(is_pale(white), "snow lies in {white:?}");
    assert!(!is_pale(wet), "rain leaves the land in {wet:?}");
    assert_eq!(white, still_white);
    assert!(
        deep >= first + 200,
        "the snow covers {deep} pixels after {first}"
    );
}

/// The ticks in which a storm strikes at least once: the longest wait
/// between two strikes is under fourteen seconds.
const STRIKE_TICKS: u32 = 240;

fn light(colour: Color) -> u32 {
    let (r, g, b) = channels(colour);
    u32::from(r) + u32::from(g) + u32::from(b)
}

/// The light of the upper half of each frame over `STRIKE_TICKS`, with the
/// brightest frame.
fn sky_light(scene: &mut Weather) -> (Vec<u32>, Vec<Vec<Color>>) {
    let mut lights = Vec::new();
    let mut brightest = Vec::new();
    for _ in 0..STRIKE_TICKS {
        scene.tick(AREA);
        let grid = pixels::colours(&render(scene), AREA);
        let sum = grid[..grid.len() / 2]
            .iter()
            .flatten()
            .map(|c| light(*c))
            .sum();
        if lights.iter().all(|other| sum > *other) {
            brightest = grid;
        }
        lights.push(sum);
    }
    (lights, brightest)
}

/// A storm lights the sky with each strike, and a bolt runs from the
/// clouds to the hills.
/// Given: a storm and a rain, both at night.
/// When: each scene draws for fourteen seconds.
/// Then: the brightest sky of the storm holds far more light than its
/// darkest, and the sky of the rain holds steady. In the brightest frame
/// of the storm, each row between the clouds and the hills holds a pixel
/// of the bolt, and a few rows hold two that stand apart, as a branch does.
///
/// Why it matters: a storm without its strike is a heavy rain, and a
/// bolt that stops in the air reads as a fault in the drawing.
#[test]
fn a_storm_lights_the_sky_with_each_strike() {
    // Given:
    let mut storm = landed(&current_at(95, 16.0, 12.0, 0));
    let mut rain = landed(&current_at(63, 9.0, 10.0, 0));

    // When:
    let (struck, brightest) = sky_light(&mut storm);
    let (steady, _) = sky_light(&mut rain);

    // Then:
    let span = |lights: &[u32]| {
        let (least, most) = (lights.iter().min().unwrap(), lights.iter().max().unwrap());
        f64::from(*most) / f64::from(*least)
    };
    assert!(
        span(&struck) >= 1.5,
        "a strike lights the sky {}",
        span(&struck)
    );
    assert!(span(&steady) <= 1.15, "the rain flickers {}", span(&steady));

    let height = brightest.len() as f32;
    let air = (0.2 * height) as usize..(0.58 * height) as usize;
    let bolt = |y: usize| -> Vec<usize> {
        (0..brightest[y].len())
            .filter(|&x| light(brightest[y][x]) >= BOLT_LIGHT)
            .collect()
    };
    for y in air.clone() {
        assert!(!bolt(y).is_empty(), "no bolt in pixel row {y}");
    }
    let branches = air
        .filter(|&y| {
            let row = bolt(y);
            row.last().unwrap() - row.first().unwrap() >= 5
        })
        .count();
    assert!(branches >= 3, "{branches} rows hold a branch");
}

/// The light of a pixel of a bolt at the least: no drop, cloud or sky of a
/// storm at night is as bright.
const BOLT_LIGHT: u32 = 450;

/// A strong wind and a light one under the same overcast sky, from one
/// seed, so both scenes hold the same land.
fn wind_and_calm() -> (Weather, Weather) {
    (
        landed(&current(3, 12.0, 55.0)),
        landed(&current(3, 12.0, 5.0)),
    )
}

/// A part of the frame, as shares of its height and of its width.
type Part = ((f32, f32), (f32, f32));

/// The far hill and its pines, above the near hill.
const FAR_HILL: Part = ((0.56, 0.71), (0.0, 1.0));
/// The line of the near hill between its pines and the cottage.
const MEADOW: Part = ((0.74, 0.87), (0.25, 0.65));
/// The land and the air over it, below the clouds and above the credit.
const OVER_LAND: Part = ((0.45, 0.93), (0.0, 1.0));
/// The air below the clouds and above the grass of the near hill.
const AIR: Part = ((0.45, 0.72), (0.0, 1.0));

fn part(scene: &Weather, (rows, columns): Part) -> Vec<((usize, usize), Color)> {
    let grid = pixels::colours(&render(scene), AREA);
    let (height, width) = (grid.len() as f32, grid[0].len() as f32);
    let rows = (rows.0 * height) as usize..(rows.1 * height) as usize;
    let columns = (columns.0 * width) as usize..(columns.1 * width) as usize;
    rows.flat_map(|y| columns.clone().map(move |x| (x, y)))
        .map(|(x, y)| ((x, y), grid[y][x]))
        .collect()
}

/// What the wind changes in a part of the frame: the pixels that show
/// another colour than in the calm frame. A cell shows two colours, so a
/// pixel beside one that moved can take a colour of neither group.
struct Blown {
    /// The places with a colour the calm part holds elsewhere: mostly land
    /// that moved.
    land: Vec<(usize, usize)>,
    /// The pixels with a colour the calm part does not hold: mostly what
    /// the wind carries through the air.
    carried: Vec<((usize, usize), Color)>,
}

fn blown(wind: &Weather, calm: &Weather, place: Part) -> Blown {
    let calm = part(calm, place);
    let own: std::collections::HashSet<Color> = calm.iter().map(|(_, colour)| *colour).collect();
    let mut blown = Blown {
        land: Vec::new(),
        carried: Vec::new(),
    };
    for ((at, colour), (_, still)) in part(wind, place).into_iter().zip(calm) {
        if !own.contains(&colour) {
            blown.carried.push((at, colour));
        } else if colour != still {
            blown.land.push(at);
        }
    }
    blown
}

/// A leaf is gold, rose or love: far redder than blue, where a gust line
/// and the land of Rose Pine hold more blue than red.
fn is_leaf(colour: Color) -> bool {
    let (r, _, b) = channels(colour);
    i32::from(r) - i32::from(b) >= 40
}

/// The pines of the far hill are darker than the hills behind them and
/// than a leaf or a gust line that crosses them.
fn is_pine(colour: Color) -> bool {
    light(colour) < 290
}

/// The ticks of one sway of a pine and of one gust line, with room: three
/// seconds.
const SWAY_TICKS: u32 = 50;

/// A strong wind drives the clouds across the sky faster.
/// Given: a strong wind and a light wind under the same overcast sky.
/// When: each scene draws tick by tick for a second.
/// Then: the upper half of the frame changes half as much again or more
/// under the strong wind.
///
/// Why it matters: clouds that drift at the pace of a calm day make the
/// wind read as a cloudy sky with a few leaves.
#[test]
fn a_strong_wind_drives_the_clouds_faster() {
    // Given:
    let (mut wind, mut calm) = wind_and_calm();
    let sky = ((0.0, 0.5), (0.0, 1.0));
    let changes = |scene: &mut Weather| {
        let mut changed = 0;
        for _ in 0..16 {
            let before = part(scene, sky);
            scene.tick(AREA);
            let after = part(scene, sky);
            changed += before.iter().zip(&after).filter(|(b, a)| b != a).count();
        }
        changed
    };

    // When:
    let (driven, drifting) = (changes(&mut wind), changes(&mut calm));

    // Then:
    assert!(drifting >= 1000, "{drifting} pixels of calm sky changed");
    assert!(
        driven * 2 >= drifting * 3,
        "{driven} pixels changed in the wind, {drifting} in the calm"
    );
}

/// The grass bends in waves that travel downwind.
/// Given: a strong wind and a light wind over the same land.
/// When: each scene draws tick by tick.
/// Then: in the wind, the grass along the near hill differs from the calm
/// grass in some columns and not in others. A tick later those columns lie
/// a pixel further right, the way the wind blows, more than further left.
///
/// Why it matters: grass that bends all at once, or in waves that run
/// against the leaves, reads as a flicker and not as wind.
#[test]
fn the_grass_bends_in_waves_that_travel_downwind() {
    // Given:
    let (mut wind, mut calm) = wind_and_calm();
    let bends = |wind: &Weather, calm: &Weather| -> std::collections::BTreeSet<usize> {
        blown(wind, calm, MEADOW)
            .land
            .into_iter()
            .map(|(x, _)| x)
            .collect()
    };
    let meadow: std::collections::BTreeSet<usize> =
        part(&calm, MEADOW).iter().map(|((x, _), _)| *x).collect();
    let (mut downwind, mut upwind) = (0, 0);

    // When:
    let mut bent = bends(&wind, &calm);
    for _ in 0..10 {
        // Then:
        assert!(bent.len() >= 20, "{} columns of grass bend", bent.len());
        assert!(
            bent.len() <= meadow.len() * 3 / 4,
            "{} of {} columns of grass bend",
            bent.len(),
            meadow.len()
        );
        wind.tick(AREA);
        calm.tick(AREA);
        let next = bends(&wind, &calm);
        downwind += bent.iter().filter(|x| next.contains(&(**x + 1))).count();
        upwind += bent
            .iter()
            .filter(|x| **x > 0 && next.contains(&(**x - 1)))
            .count();
        bent = next;
    }
    assert!(
        downwind >= upwind + 20,
        "{downwind} columns moved downwind and {upwind} upwind"
    );
}

/// The pines on the far hill sway in the wind.
/// Given: a strong wind and a light wind over the same land.
/// When: each scene draws tick by tick for one sway.
/// Then: the dark pixels of the pines on the far hill stand away from the
/// calm pines, at one time by twice the pixels of another time.
///
/// Why it matters: a pine that stands straight, or leans and holds still,
/// makes the wind stop at the far hill.
#[test]
fn the_pines_on_the_far_hill_sway_in_the_wind() {
    // Given:
    let (mut wind, mut calm) = wind_and_calm();
    let pines = |scene: &Weather| -> std::collections::HashSet<(usize, usize)> {
        part(scene, FAR_HILL)
            .into_iter()
            .filter(|(_, colour)| is_pine(*colour))
            .map(|(at, _)| at)
            .collect()
    };

    // When:
    let leans: Vec<usize> = (0..SWAY_TICKS)
        .map(|_| {
            wind.tick(AREA);
            calm.tick(AREA);
            pines(&wind).symmetric_difference(&pines(&calm)).count()
        })
        .collect();

    // Then:
    let (least, most) = (*leans.iter().min().unwrap(), *leans.iter().max().unwrap());
    assert!(most >= 20, "{most} pixels of pine lean at the most");
    assert!(
        least * 2 <= most,
        "from {least} to {most} pixels of pine lean"
    );
}

/// The wind blows leaves across the land.
/// Given: a strong wind and a light wind over the same land, by day.
/// When: each scene draws, then draws again a tick later.
/// Then: leaves show over the land where the calm frame has none, and most
/// of them are in another place a tick later.
///
/// Why it matters: the leaves are the part of the wind the eye follows, and
/// a leaf that hangs in the air reads as a fault of the terminal.
#[test]
fn the_wind_blows_leaves_across_the_land() {
    // Given:
    let (mut wind, mut calm) = wind_and_calm();
    let leaves = |wind: &Weather, calm: &Weather| -> Vec<(usize, usize)> {
        blown(wind, calm, OVER_LAND)
            .carried
            .into_iter()
            .filter(|(_, colour)| is_leaf(*colour))
            .map(|(at, _)| at)
            .collect()
    };

    // When:
    let before = leaves(&wind, &calm);
    wind.tick(AREA);
    calm.tick(AREA);
    let after = leaves(&wind, &calm);

    // Then:
    assert!(before.len() >= 10, "{} pixels of leaf", before.len());
    assert!(after.len() >= 10, "{} pixels of leaf", after.len());
    let moved = before.iter().filter(|at| !after.contains(at)).count();
    assert!(
        moved * 2 >= before.len(),
        "{moved} of {} pixels of leaf moved",
        before.len()
    );
}

/// Gust lines cross the air over the land.
/// Given: a strong wind and a light wind over the same land, by day.
/// When: each scene draws tick by tick for the life of a gust line.
/// Then: at some tick, pixels that are no leaf and no colour of the calm
/// frame fill a run of columns as long as a line, where a pine that leans
/// changes a few columns.
///
/// Why it matters: the gust lines show the wind in the air, and without
/// them only the land moves.
#[test]
fn gust_lines_cross_the_air_over_the_land() {
    // Given:
    let (mut wind, mut calm) = wind_and_calm();

    // When:
    let mut longest = 0;
    for _ in 0..SWAY_TICKS {
        let columns: std::collections::BTreeSet<usize> = blown(&wind, &calm, AIR)
            .carried
            .into_iter()
            .filter(|(_, colour)| !is_leaf(*colour))
            .map(|((x, _), _)| x)
            .collect();
        let (mut run, mut last) = (0, None);
        for x in columns {
            run = if last.is_some_and(|last| last + 1 == x) {
                run + 1
            } else {
                1
            };
            longest = longest.max(run);
            last = Some(x);
        }
        wind.tick(AREA);
        calm.tick(AREA);
    }

    // Then:
    assert!(longest >= 30, "the longest line is {longest} pixels long");
}

/// The cues of each of the next `ticks` ticks.
fn cues_of(scene: &mut Weather, ticks: u32) -> Vec<Vec<Cue>> {
    scene.drain_cues(&mut Vec::new());
    (0..ticks)
        .map(|_| {
            scene.tick(AREA);
            let mut cues = Vec::new();
            scene.drain_cues(&mut cues);
            cues
        })
        .collect()
}

/// The ticks of a minute.
const MINUTE: u32 = 1000;

/// Each weather sends the cue of its own sound on every tick.
/// Given: a scene for each weather: a clear sky by day and at night, an
/// overcast sky by day and at night, rain, snow, a storm, fog and a strong
/// wind.
/// When: each scene ticks for a minute.
/// Then: every tick sends the cue of that weather, once. The storm also
/// sends a thunder cue and the fog a foghorn cue on at least one tick, and
/// no other cue comes.
///
/// Why it matters: the mixer keeps a sound up only while its cue comes, so
/// a tick without it fades the sound, and the cue of another weather plays
/// rain under a clear sky.
#[test]
fn each_weather_sends_the_cue_of_its_own_sound() {
    let weathers = [
        (current_at(0, 20.0, 5.0, 1), Cue::ClearDay, None),
        (current_at(0, 12.0, 5.0, 0), Cue::ClearNight, None),
        (current_at(3, 12.0, 5.0, 1), Cue::Cloud, None),
        (current_at(3, 9.0, 5.0, 0), Cue::Cloud, None),
        (current_at(63, 9.0, 10.0, 1), Cue::Rain, None),
        (current_at(73, -3.0, 5.0, 1), Cue::Snow, None),
        (
            current_at(95, 16.0, 12.0, 0),
            Cue::Storm,
            Some(Cue::Thunder),
        ),
        (current_at(45, 6.0, 3.0, 1), Cue::Fog, Some(Cue::Foghorn)),
        (current_at(3, 12.0, 55.0, 1), Cue::Wind { gust: 0.0 }, None),
    ];
    for (body, sound, at_times) in weathers {
        // Given:
        let mut scene = landed(&body);

        // When:
        let ticks = cues_of(&mut scene, MINUTE);

        // Then:
        let mut times = 0;
        for (tick, cues) in ticks.iter().enumerate() {
            let cues: Vec<Cue> = cues
                .iter()
                .map(|cue| match cue {
                    Cue::Wind { .. } => Cue::Wind { gust: 0.0 },
                    other => *other,
                })
                .collect();
            match cues[..] {
                [only] => assert_eq!(only, sound, "tick {tick}"),
                [first, second] => {
                    assert_eq!(first, sound, "tick {tick}");
                    assert_eq!(Some(second), at_times, "tick {tick} of {sound:?}");
                    times += 1;
                }
                _ => panic!("tick {tick} of {sound:?} sent {cues:?}"),
            }
        }
        assert_eq!(
            times >= 1,
            at_times.is_some(),
            "{sound:?} sent {at_times:?} {times} times"
        );
    }
}

/// A scene that shows no sky sends no cue.
/// Given: a scene with no location, and a scene whose read fails.
/// When: each ticks for a second.
/// Then: neither sends a cue.
///
/// Why it matters: a notice with rain under it sounds of a weather the
/// scene does not know.
#[test]
fn a_scene_that_shows_no_sky_sends_no_cue() {
    // Given:
    let mut nowhere = Weather::new(seeded(), &theme::ROSE_PINE, None);
    let mut failed = Weather::new(
        seeded(),
        &theme::ROSE_PINE,
        Some(Source::new(&closed(), oslo())),
    );
    settled(&mut failed);

    // When:
    let cues = [cues_of(&mut nowhere, 17), cues_of(&mut failed, 17)];

    // Then:
    for ticks in cues {
        assert!(ticks.iter().all(Vec::is_empty), "{ticks:?}");
    }
}

/// The ticks a strike lights the sky at the most.
const FLASH_TICKS: usize = 6;

/// A thunder cue comes with each strike.
/// Given: a storm at night.
/// When: the scene draws tick by tick for twenty-eight seconds.
/// Then: the sky lights up at least twice, and the thunder cue comes on the
/// first lit tick of each strike and on no other tick.
///
/// Why it matters: thunder with no bolt, or a bolt with no thunder, reads
/// as a sound that plays on its own clock.
#[test]
fn a_thunder_cue_comes_with_each_strike() {
    // Given:
    let mut storm = landed(&current_at(95, 16.0, 12.0, 0));
    storm.drain_cues(&mut Vec::new());

    // When:
    let (mut lights, mut thunder) = (Vec::new(), Vec::new());
    for tick in 0..2 * STRIKE_TICKS as usize {
        storm.tick(AREA);
        let mut cues = Vec::new();
        storm.drain_cues(&mut cues);
        if cues.contains(&Cue::Thunder) {
            thunder.push(tick);
        }
        let grid = pixels::colours(&render(&storm), AREA);
        let upper = grid[..grid.len() / 2].iter().flatten();
        lights.push(upper.map(|colour| light(*colour)).sum::<u32>());
    }

    // Then:
    let darkest = f64::from(*lights.iter().min().unwrap());
    let lit = |tick: usize| f64::from(lights[tick]) >= 1.3 * darkest;
    // A strike under way at the first tick began before the cues were read.
    let strikes: Vec<usize> = (FLASH_TICKS..lights.len())
        .filter(|&tick| lit(tick) && !(tick - FLASH_TICKS..tick).any(lit))
        .collect();
    thunder.retain(|tick| *tick >= FLASH_TICKS);
    assert!(strikes.len() >= 2, "the sky lit up at ticks {strikes:?}");
    assert_eq!(thunder, strikes);
}

/// The gust of the wind cue rises and falls with no jump.
/// Given: a strong wind.
/// When: the scene ticks for a minute.
/// Then: every tick sends one wind cue with a gust from 0 to 1. The gusts
/// span at least 0.4 and cross their mean at least ten times, and no gust
/// differs from the one before it by more than 0.05.
///
/// Why it matters: the level of the wind's sound follows this number as
/// the gust lines come and go. A number that held still would play a flat
/// hiss, and one that jumped would lurch.
#[test]
fn the_gust_of_the_wind_cue_rises_and_falls_with_no_jump() {
    // Given:
    let mut wind = landed(&current(3, 12.0, 55.0));

    // When:
    let gusts: Vec<f32> = cues_of(&mut wind, MINUTE)
        .iter()
        .map(|cues| match cues[..] {
            [Cue::Wind { gust }] => gust,
            _ => panic!("the wind sent {cues:?}"),
        })
        .collect();

    // Then:
    let least = gusts.iter().copied().fold(f32::INFINITY, f32::min);
    let most = gusts.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    assert!(least >= 0.0 && most <= 1.0, "gusts from {least} to {most}");
    assert!(most - least >= 0.4, "gusts from {least} to {most}");
    let mean = gusts.iter().sum::<f32>() / gusts.len() as f32;
    let crossings = gusts
        .windows(2)
        .filter(|pair| (pair[0] < mean) != (pair[1] < mean))
        .count();
    assert!(
        crossings >= 10,
        "the gust crosses its mean {crossings} times"
    );
    let jump = gusts
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0, f32::max);
    assert!(jump <= 0.05, "the gust jumps by {jump}");
}

/// The ticks of half an hour.
const HALF_HOUR: u32 = 30_000;
/// The ticks of twenty seconds, longer than a call of the foghorn lasts.
const HORN_REST: usize = 333;

/// A far foghorn sounds now and then in the fog.
/// Given: a foggy scene.
/// When: it ticks for half an hour.
/// Then: the foghorn cue comes from twenty to sixty times, and no two of
/// them within twenty seconds.
///
/// Why it matters: a horn on every tick starts its recording again and
/// again, and two calls close together play over each other. Calls at
/// times of their own come close together only now and then, so a short
/// run does not show it.
#[test]
fn a_far_foghorn_sounds_now_and_then_in_the_fog() {
    // Given:
    let mut fog = landed(&current_at(45, 6.0, 3.0, 1));

    // When:
    let calls: Vec<usize> = cues_of(&mut fog, HALF_HOUR)
        .iter()
        .enumerate()
        .filter(|(_, cues)| cues.contains(&Cue::Foghorn))
        .map(|(tick, _)| tick)
        .collect();

    // Then:
    assert!((20..=60).contains(&calls.len()), "calls at ticks {calls:?}");
    let closest = calls.windows(2).map(|pair| pair[1] - pair[0]).min();
    assert!(
        closest >= Some(HORN_REST),
        "two calls are {closest:?} ticks apart"
    );
}

/// A new theme repaints the sky and keeps the weather.
/// Given: a cloudy sky in Rose Pine.
/// When: the theme changes to Rose Pine Dawn.
/// Then: the sky takes other colours, and the weather still shows with no
/// new read.
///
/// Why it matters: a repaint that read the weather again would send the
/// place to Open-Meteo on each theme key.
#[test]
fn a_new_theme_repaints_the_sky_and_keeps_the_weather() {
    // Given:
    let mut scene = landed(&current(3, 9.0, 10.0));
    let before = pixels::colours(&render(&scene), AREA);

    // When:
    assert!(scene.repaint(&theme::ROSE_PINE_DAWN));
    let shown = render(&scene);

    // Then:
    let after = pixels::colours(&shown, AREA);
    assert_ne!(before[0][0], after[0][0]);
    assert!(shows(&shown, "overcast · 9 °C"));
}

/// Rose Pine over the terminal's background, and the same theme with
/// another base. A colour mixed with the base moves when the base moves.
const SEE_THROUGH: &Palette = &Palette {
    see_through: true,
    ..theme::ROSE_PINE
};
const OTHER_BASE: &Palette = &Palette {
    base: Color::Rgb(250, 0, 250),
    see_through: true,
    ..theme::ROSE_PINE
};

/// The cells of a frame that may differ between two bases, one in a
/// hundred. A cell of three colours or more with a pixel of the base keeps
/// the pair of colours nearest its pixels, the base among them, so a few
/// such cells keep another pair. A colour mixed with the base changes a
/// band of sky, thousands of cells.
const PAIRS_THAT_MOVE: usize = AREA.area() as usize / 100;

/// Over the terminal's background, no colour of the sky mixes with it.
/// Given: Rose Pine over the terminal's background, and the same theme with
/// another base.
/// When: a clear day, a clear night, a cloudy sky, a fog, a rain, a snow,
/// a storm and a strong wind draw in each.
/// Then: the cells that show no base show the same glyph and colours in
/// both, but for a few cells that keep another pair.
///
/// Why it matters: the terminal's background is not known, so a mix with
/// it shows as a colour that belongs to no theme.
#[test]
fn over_the_terminal_background_nothing_mixes_with_the_base() {
    for body in [
        current_at(0, 18.0, 5.0, 1),
        current_at(0, 4.0, 5.0, 0),
        current(2, 9.0, 10.0),
        current(45, 3.0, 2.0),
        current(63, 9.0, 10.0),
        current(73, -2.0, 4.0),
        current(95, 16.0, 12.0),
        current(3, 12.0, 55.0),
    ] {
        // When:
        let one = render(&landed_in(SEE_THROUGH, &body));
        let other = render(&landed_in(OTHER_BASE, &body));

        // Then:
        let mut compared = 0;
        let mut moved = Vec::new();
        for position in AREA.positions() {
            let (cell, twin) = (&one[position], &other[position]);
            if [cell.fg, cell.bg].contains(&SEE_THROUGH.base)
                || [twin.fg, twin.bg].contains(&OTHER_BASE.base)
            {
                continue;
            }
            compared += 1;
            if (cell.symbol(), cell.fg, cell.bg) != (twin.symbol(), twin.fg, twin.bg) {
                moved.push(position);
            }
        }
        assert!(compared > 0, "no cell without the base in {body}");
        assert!(
            moved.len() <= PAIRS_THAT_MOVE,
            "{} cells differ, {moved:?} in {body}",
            moved.len()
        );
    }
}

/// In every theme, the night is dark under a bright moon, and the status
/// reads over the sky.
/// Given: each theme, light and dark.
/// When: a clear night and a clear day draw.
/// Then: at night the brightest pixel of the upper sky outshines the sky at
/// its top by far. By day and at night, each letter of the status is
/// lighter than the cell behind it.
///
/// Why it matters: a light theme swaps its base and its text, so a sky
/// mixed from them shows a pale night under a dark moon, with the status
/// lost in it.
#[test]
fn every_theme_draws_a_dark_night_and_a_status_that_reads() {
    for (name, palette) in theme::THEMES {
        for (body, night) in [
            (current_at(0, 4.0, 5.0, 0), true),
            (current_at(0, 18.0, 5.0, 1), false),
        ] {
            // When:
            let shown = render(&landed_in(palette, &body));

            // Then:
            if night {
                let grid = pixels::colours(&shown, AREA);
                let sky = theme::luma(grid[CELL_HEIGHT * 2][0]);
                let brightest = grid[..grid.len() / 2]
                    .iter()
                    .flatten()
                    .map(|colour| theme::luma(*colour))
                    .max()
                    .unwrap();
                assert!(
                    brightest > sky + 100_000,
                    "{name}: the moon at {brightest} over a sky at {sky}"
                );
            }
            let status = (0..AREA.width)
                .map(|x| &shown[(x, 1)])
                .filter(|cell| cell.symbol().chars().all(char::is_alphanumeric));
            for cell in status {
                assert!(
                    theme::luma(cell.fg) > theme::luma(cell.bg),
                    "{name}, night {night}: {:?} on {:?}",
                    cell.fg,
                    cell.bg
                );
            }
        }
    }
}

/// A smaller area draws only inside it.
/// Given: a clear sky, a rain, a snow, a storm and a strong wind that have
/// landed.
/// When: each renders into areas smaller than the buffer, down to nothing.
/// Then: no cell outside the area changes.
///
/// Why it matters: a pane that draws past its edge paints over the pane
/// beside it.
#[test]
fn a_smaller_area_draws_only_inside_it() {
    // Given:
    let canvas = Rect::new(0, 0, 40, 12);
    for (code, wind) in [(0, 5.0), (63, 5.0), (73, 5.0), (95, 5.0), (3, 55.0)] {
        let mut scene = landed(&current(code, 18.0, wind));

        // When, then:
        for area in [
            Rect::new(3, 2, 20, 6),
            Rect::new(3, 2, 1, 1),
            Rect::new(3, 2, 5, 0),
            Rect::new(3, 2, 0, 0),
        ] {
            // A storm strikes within each `STRIKE_TICKS`, so the bolt
            // draws in each area too.
            for _ in 0..STRIKE_TICKS {
                scene.tick(area);
                let mut buffer = Buffer::empty(canvas);
                scene.render(area, &mut buffer);
                pixels::assert_blank_outside(&buffer, area);
            }
        }
    }
}
