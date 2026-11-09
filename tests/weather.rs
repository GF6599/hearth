#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use hearth::scene::Scene;
use hearth::scenes::weather::{CREDIT, NO_LOCATION, READING, Weather};
use hearth::theme;
use hearth::weather::{Location, Source, parse};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

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

/// The reply Open-Meteo gives for the current weather.
fn current(code: i64, temperature: f64, wind: f64) -> String {
    format!(
        r#"{{"latitude":59.906296,"longitude":10.744095,"current_units":{{"time":"iso8601","interval":"seconds","weather_code":"wmo code","temperature_2m":"°C","wind_speed_10m":"km/h","is_day":""}},"current":{{"time":"2026-11-09T13:15","interval":900,"weather_code":{code},"temperature_2m":{temperature},"wind_speed_10m":{wind},"is_day":1}}}}"#
    )
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
    let mut scene = Weather::new(&theme::ROSE_PINE, Some(Source::new(&server.base, oslo())));
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
        "current=weather_code,temperature_2m,wind_speed_10m",
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
        let mut scene = Weather::new(&theme::ROSE_PINE, Some(Source::new(&base, oslo())));
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
    let mut scene = Weather::new(&theme::ROSE_PINE, Some(Source::new(&server.base, oslo())));

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
    let scene = Weather::new(&theme::ROSE_PINE, Some(source));
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
    let mut scene = Weather::new(&theme::ROSE_PINE, None);
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
