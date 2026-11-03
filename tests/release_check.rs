#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::Path;
use std::process::{Command, Output};

/// Runs the release check on `binary` for `target`.
fn check(binary: &str, target: &str) -> Output {
    Command::new("sh")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/macos-binary-starts.sh"))
        .arg(binary)
        .arg(target)
        .output()
        .unwrap()
}

/// A binary the check cannot read fails the release.
/// Given: a path with no file at it.
/// When: the check runs for the Intel target, the one no later step
/// starts on the release runner.
/// Then: it exits non-zero.
///
/// Why it matters: the check guards a crash that only a launch shows, and
/// a check that passes a binary it never read ships that crash.
#[test]
fn a_binary_the_check_cannot_read_fails_it() {
    // Given, when:
    let output = check("/no/such/hearth", "x86_64-apple-darwin");

    // Then:
    assert!(
        !output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The binary this build made passes the check.
/// Given: the hearth binary of this build and the target of this machine.
/// When: the check runs.
/// Then: it exits zero.
///
/// Why it matters: a check that fails a sound binary blocks every release.
#[cfg(target_os = "macos")]
#[test]
fn the_built_binary_passes_the_check() {
    // Given, when:
    let target = format!("{}-apple-darwin", std::env::consts::ARCH);
    let output = check(env!("CARGO_BIN_EXE_hearth"), &target);

    // Then:
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
