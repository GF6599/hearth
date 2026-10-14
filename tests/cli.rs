#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use clap::Parser;
use hearth::cli::Cli;

/// Given: a speed of zero, one over the ceiling, and one that is no number.
/// When: the command line is parsed.
/// Then: each is refused with a message that names the flag, and a speed
/// inside the range is taken.
///
/// Why it matters: a speed of zero types nothing, and the screen would
/// stay empty with no word on why.
#[test]
fn a_speed_outside_the_range_is_refused() {
    for bad in ["0", "1001", "fast"] {
        let error = Cli::try_parse_from(["hearth", "--speed", bad]).unwrap_err();
        assert!(error.to_string().contains("--speed"), "{error}");
    }
    let cli = Cli::try_parse_from(["hearth", "--speed", "80"]).unwrap();
    assert_eq!(cli.speed, "80".parse().unwrap());
}
