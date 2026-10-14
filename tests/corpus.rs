#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use hearth::corpus::Corpus;

/// A fresh directory under the system temp dir, emptied first.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hearth-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed in {repo:?}");
}

/// A repository with one tracked text file, one tracked binary file and
/// one untracked file.
fn repository(name: &str) -> PathBuf {
    let repo = scratch(name);
    git(&repo, &["init", "-q"]);
    fs::write(repo.join("main.rs"), "fn main() {}\n").unwrap();
    fs::write(repo.join("blob.bin"), b"PK\x03\x04\x00\x00\x00\x08").unwrap();
    git(&repo, &["add", "main.rs", "blob.bin"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    fs::write(repo.join("notes.txt"), "not tracked\n").unwrap();
    repo
}

/// Given: a checkout with a tracked text file, a tracked binary file and
/// an untracked file.
/// When: the corpus is read from it.
/// Then: only the text file is in the corpus.
///
/// Why it matters: a binary file types as garbage, and an untracked file
/// is where a secret lives.
#[test]
fn a_checkout_yields_its_tracked_text_files_only() {
    // Given:
    let repo = repository("checkout");

    // When:
    let corpus = Corpus::from_checkout(&repo).unwrap();

    // Then:
    let paths: Vec<&str> = corpus.files().iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["main.rs"]);
    assert_eq!(corpus.files()[0].text, "fn main() {}\n");
    let _ = fs::remove_dir_all(&repo);
}

/// Given: a repository standing in for a remote, and an empty cache.
/// When: the corpus is cloned once, the remote is deleted, and the corpus
/// is cloned again from the same address.
/// Then: the second read succeeds from the cache and holds the same file.
///
/// Why it matters: a screensaver with no network has to start on what it
/// fetched last time.
#[test]
fn a_second_clone_is_served_from_the_cache() {
    // Given:
    let remote = repository("remote");
    let cache = scratch("cache");
    let address = remote.to_str().unwrap().to_string();

    // When:
    let first = Corpus::cloned(&address, &cache).unwrap();
    fs::remove_dir_all(&remote).unwrap();
    let second = Corpus::cloned(&address, &cache).unwrap();

    // Then:
    assert_eq!(first.files().len(), 1);
    assert_eq!(second.files()[0].path, "main.rs");
    assert_eq!(second.files()[0].text, first.files()[0].text);
    let _ = fs::remove_dir_all(&cache);
}

/// Given: a path that is not a repository.
/// When: the corpus is read from it.
/// Then: the error names the path.
#[test]
fn a_directory_without_a_repository_is_refused() {
    let dir = scratch("plain");
    let error = Corpus::from_checkout(&dir).unwrap_err();
    assert!(error.to_string().contains("plain"), "{error}");
    let _ = fs::remove_dir_all(&dir);
}
