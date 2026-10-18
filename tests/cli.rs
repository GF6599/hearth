#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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

/// A folder for `XDG_CONFIG_HOME` that holds `config` as hearth's file, or
/// no file at all, so a run never reads the config of the machine it runs on.
fn config_home(name: &str, config: Option<&str>) -> PathBuf {
    let home =
        std::env::temp_dir().join(format!("hearth-cli-{}-{name}-config", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(home.join("hearth")).unwrap();
    if let Some(config) = config {
        fs::write(home.join("hearth").join("config.toml"), config).unwrap();
    }
    home
}

fn run(args: &[&str], home: &Path) -> (i32, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_hearth"))
        .args(args)
        .env("XDG_CONFIG_HOME", home)
        .output()
        .unwrap();
    let _ = fs::remove_dir_all(home);
    (
        output.status.code().unwrap(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn run_typing(repo: &Path) -> (i32, String) {
    let name = repo.file_name().unwrap().to_string_lossy().into_owned();
    let result = run(
        &["typing", "--repo", &repo.to_string_lossy()],
        &config_home(&name, None),
    );
    let _ = fs::remove_dir_all(repo);
    result
}

/// Given: a config file whose fire speed is a word, and one whose scene is
/// no scene.
/// When: hearth starts with each, with no terminal.
/// Then: each exits 1 with a message that names the file and the key.
///
/// Why it matters: a bad value read as the default leaves the user
/// wondering why the file does nothing.
#[test]
fn a_bad_config_file_stops_startup_and_names_the_key() {
    for (name, config, key) in [
        ("word", "[speed]\nfire = \"fast\"\n", "speed.fire"),
        ("scene", "scene = \"nope\"\n", "scene"),
    ] {
        // Given:
        let home = config_home(name, Some(config));
        let file = home.join("hearth").join("config.toml");

        // When:
        let (code, stderr) = run(&["fire"], &home);

        // Then:
        assert_eq!(code, 1, "{name}");
        assert!(stderr.contains(&file.display().to_string()), "{stderr}");
        assert!(stderr.contains(key), "{stderr}");
    }
}

/// Given: a config file whose repo is a repository with no function.
/// When: hearth starts the typing scene with no `--repo`.
/// Then: it refuses that repository, so the file chose it.
#[test]
fn the_config_file_chooses_the_repository() {
    let repo = repository("configured", "README.md", "# Title\n");
    let config = format!("repo = {:?}\n", repo.display().to_string());
    let (code, stderr) = run(&["typing"], &config_home("configured", Some(&config)));
    let _ = fs::remove_dir_all(&repo);
    assert_eq!(code, 1);
    assert!(stderr.contains("no function"), "{stderr}");
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

/// Given: no `HOME` and no `XDG_CONFIG_HOME`, and a bad config file in the
/// temp folder, where any user on the machine can put one.
/// When: hearth starts fire.
/// Then: it does not read that file.
///
/// Why it matters: a config read from a shared folder lets another user
/// choose the repository hearth runs git in, and a symlink planted there
/// sends a save over a file of their choosing.
#[test]
fn no_home_folder_means_no_config_file() {
    // Given:
    let temp = std::env::temp_dir().join(format!("hearth-cli-{}-tmpdir", std::process::id()));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(temp.join("hearth")).unwrap();
    fs::write(
        temp.join("hearth").join("config.toml"),
        "scene = \"nope\"\n",
    )
    .unwrap();

    // When:
    let output = Command::new(env!("CARGO_BIN_EXE_hearth"))
        .arg("fire")
        .env_remove("HOME")
        .env_remove("XDG_CONFIG_HOME")
        .env("TMPDIR", &temp)
        .output()
        .unwrap();
    let _ = fs::remove_dir_all(&temp);

    // Then:
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("scene must be"), "{stderr}");
}

/// Runs `args` in a new pseudo terminal through `script`, so hearth opens
/// its screen when the test has no terminal. The util-linux `script` takes
/// the command as one shell line, and the BSD one on macOS as arguments.
fn in_terminal(args: &[&str]) -> Command {
    let mut script = Command::new("script");
    script.arg("-q");
    if cfg!(target_os = "linux") {
        let line: Vec<String> = args.iter().map(|arg| format!("'{arg}'")).collect();
        script.args(["-e", "-c", &line.join(" "), "/dev/null"]);
    } else {
        script.arg("/dev/null").args(args);
    }
    script
}

/// Given: a config file that sets a theme, and a `git` first on the path
/// that counts its runs and finds no history.
/// When: hearth starts the river in a terminal, and `q` quits it after
/// git has run.
/// Then: git ran once.
///
/// Why it matters: each river that is built reads the history with its
/// own `git log`, so a scene built twice at startup reads a large
/// repository twice.
#[test]
fn the_river_reads_its_history_once_at_startup() {
    // Given:
    let home = config_home("once", Some("theme = \"rose-pine-moon\"\n"));
    let calls = home.join("calls");
    let bin = home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let git = bin.join("git");
    let counter = format!("#!/bin/sh\necho run >> '{}'\nexit 1\n", calls.display());
    fs::write(&git, counter).unwrap();
    fs::set_permissions(&git, fs::Permissions::from_mode(0o755)).unwrap();
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let search =
        std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&inherited)))
            .unwrap();
    let runs = || fs::read_to_string(&calls).map_or(0, |text| text.lines().count());

    // When:
    let mut hearth = in_terminal(&[env!("CARGO_BIN_EXE_hearth"), "river"])
        .current_dir(&home)
        .env("PATH", search)
        .env("XDG_CONFIG_HOME", &home)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while runs() == 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    hearth.stdin.take().unwrap().write_all(b"q").unwrap();
    while hearth.try_wait().unwrap().is_none() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    let quit = hearth.try_wait().unwrap().is_some();
    if !quit {
        hearth.kill().unwrap();
    }
    let count = runs();
    let _ = fs::remove_dir_all(&home);

    // Then:
    assert!(quit, "hearth did not quit on q");
    assert_eq!(count, 1);
}
