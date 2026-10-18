//! What hearth reads from a repository: the source files the typing scene
//! types, from the crate's own tree, a checkout on disk, or a clone kept in
//! a cache, and where the river scene reads its history from.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use unicode_width::UnicodeWidthChar;

use crate::history;

include!(concat!(env!("OUT_DIR"), "/embedded.rs"));

/// Files over this size are skipped. A generated file or a vendored blob
/// is not worth typing.
pub const SIZE_CAP: usize = 64 * 1024;

const TAB: &str = "    ";

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: String,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Corpus {
    files: Vec<SourceFile>,
    /// The repository whose history the river reads. Only the river reads
    /// it, so no other scene runs git for it.
    history_root: Option<PathBuf>,
}

impl Corpus {
    pub fn embedded() -> Self {
        Self::from_files(
            EMBEDDED
                .iter()
                .map(|(path, text)| ((*path).to_string(), (*text).to_string())),
        )
    }

    /// Every line of every file is made safe to type: tabs become spaces
    /// and any glyph that is not one cell wide is dropped. A file with no
    /// text left is dropped too.
    pub fn from_files(files: impl IntoIterator<Item = (String, String)>) -> Self {
        let files = files
            .into_iter()
            .filter_map(|(path, text)| {
                let text = normalised(&text);
                (!text.trim().is_empty()).then(|| SourceFile {
                    path: normalised(&path),
                    text,
                })
            })
            .collect();
        Self {
            files,
            history_root: None,
        }
    }

    pub fn with_history_root(self, root: PathBuf) -> Self {
        Self {
            history_root: Some(root),
            ..self
        }
    }

    /// The tracked text files of a checkout. Only `git ls-files` decides
    /// what is read, so an ignored or untracked file is never opened. The
    /// metadata is read without following links, so a tracked symlink in a
    /// cloned repository cannot point the scene at a file on this machine,
    /// and a file over the cap is never read into memory.
    pub fn from_checkout(root: &Path) -> io::Result<Self> {
        let listing = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["ls-files", "-z"])
            .output()
            .map_err(|e| io::Error::new(e.kind(), format!("cannot run git: {e}")))?;
        if !listing.status.success() {
            return Err(io::Error::other(format!(
                "{} is not a git checkout",
                root.display()
            )));
        }
        let mut files = Vec::new();
        for path in listing.stdout.split(|&byte| byte == 0) {
            let Ok(path) = std::str::from_utf8(path) else {
                continue;
            };
            if path.is_empty() {
                continue;
            }
            let file = root.join(path);
            let Ok(meta) = fs::symlink_metadata(&file) else {
                continue;
            };
            if !meta.is_file() || meta.len() > SIZE_CAP as u64 {
                continue;
            }
            let Ok(bytes) = fs::read(&file) else {
                continue;
            };
            if bytes.contains(&0) {
                continue;
            }
            if let Ok(text) = String::from_utf8(bytes) {
                files.push((path.to_string(), text));
            }
        }
        Ok(Self::from_files(files).with_history_root(root.to_path_buf()))
    }

    /// Clones `address` into the cache on the first call, and reads the
    /// cached checkout on every call after, so a run with no network still
    /// starts. The clone holds as much history as the river reads.
    pub fn cloned(address: &str, cache: &Path) -> io::Result<Self> {
        let checkout = cache_path(address, cache);
        if !checkout.join(".git").exists() {
            fs::create_dir_all(cache)?;
            let status = Command::new("git")
                .args([
                    "clone",
                    "--quiet",
                    "--depth",
                    &history::DEPTH.to_string(),
                    "--",
                    address,
                ])
                .arg(&checkout)
                .status()
                .map_err(|e| io::Error::new(e.kind(), format!("cannot run git: {e}")))?;
            if !status.success() {
                let _ = fs::remove_dir_all(&checkout);
                return Err(io::Error::other(format!("cannot clone {address}")));
            }
        }
        Self::from_checkout(&checkout)
    }

    pub fn files(&self) -> &[SourceFile] {
        &self.files
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn history_root(&self) -> Option<&Path> {
        self.history_root.as_deref()
    }
}

/// Where a clone of `address` lives under the cache: the address with
/// every character outside `[A-Za-z0-9._-]` replaced, so one address maps
/// to one directory on every platform.
pub fn cache_path(address: &str, cache: &Path) -> PathBuf {
    let key: String = address
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    cache.join(key)
}

fn normalised(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for glyph in text.chars() {
        match glyph {
            '\n' => out.push('\n'),
            '\t' => out.push_str(TAB),
            glyph if glyph.width() == Some(1) => out.push(glyph),
            _ => {}
        }
    }
    out
}
