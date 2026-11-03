#![allow(
    dead_code,
    reason = "each test binary builds this module and uses only some of it"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hearth-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs git in `repo` with a fixed author and no hooks or signing from the
/// machine's own config, and answers what it printed.
pub fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?} failed in {repo:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

pub fn repository(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let repo = scratch(name);
    git(&repo, &["init", "-q"]);
    for (file, text) in files {
        fs::write(repo.join(file), text).unwrap();
        git(&repo, &["add", file]);
    }
    git(&repo, &["commit", "-q", "-m", "init"]);
    repo
}
