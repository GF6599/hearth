#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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

fn repository(name: &str, file: &str, text: &str) -> PathBuf {
    let repo = std::env::temp_dir().join(format!("hearth-cli-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&repo);
    fs::create_dir_all(&repo).unwrap();
    fs::write(repo.join(file), text).unwrap();
    for args in [
        &["init", "-q"][..],
        &["add", file],
        &["commit", "-q", "-m", "init"],
    ] {
        let status = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    }
    repo
}

fn run_typing(repo: &Path) -> (i32, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_hearth"))
        .arg("typing")
        .arg("--repo")
        .arg(repo)
        .output()
        .unwrap();
    let _ = fs::remove_dir_all(repo);
    (
        output.status.code().unwrap(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Given: a repository whose only file is Markdown, and one with a Rust
/// function.
/// When: hearth starts the typing scene on each, with no terminal.
/// Then: the Markdown one exits 1 with a message that says there is no
/// function, and the Rust one gets past that check.
///
/// Why it matters: a scene with nothing to type shows an empty screen
/// forever, with no word on why.
#[test]
fn a_repository_with_no_function_is_refused_at_startup() {
    // Given, when:
    let (code, stderr) = run_typing(&repository("prose", "README.md", "# Title\n\nSome text.\n"));

    // Then:
    assert_eq!(code, 1);
    assert!(stderr.contains("no function"), "{stderr}");
    let (_, stderr) = run_typing(&repository("code", "main.rs", "fn main() {}\n"));
    assert!(!stderr.contains("no function"), "{stderr}");
}
