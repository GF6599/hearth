//! The commits the river scene draws: the newest few hundred of a
//! repository, read once with `git log`.

use std::path::Path;
use std::process::Command;

use crate::corpus;

/// The most commits read. A repository with years of history would make
/// the first frame wait on the whole of it.
pub const DEPTH: usize = 500;

/// Splits the fields of one commit in the log. Neither a hash nor a list
/// of parents holds it, and the subject comes last, so a subject that holds
/// it stays whole.
const FIELD: u8 = 0x1f;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub hash: String,
    pub parents: Vec<String>,
    /// Safe to print: a tab becomes spaces and every other glyph that is
    /// not one cell wide is dropped, so an escape sequence in a subject
    /// never reaches the terminal.
    pub subject: String,
}

/// The newest commit first, in the order `git log --graph` draws them.
#[derive(Debug, Clone, Default)]
pub struct History {
    commits: Vec<Commit>,
}

impl History {
    /// Empty when `root` is not a repository, holds no commit, or git does
    /// not run, so the scene shows its notice and the app still starts.
    pub fn read(root: &Path) -> Self {
        let Ok(output) = Command::new("git")
            .arg("-C")
            .arg(root)
            .args([
                "log",
                "--no-show-signature",
                "--topo-order",
                "-z",
                "--format=%H%x1f%P%x1f%s",
            ])
            .arg(format!("--max-count={DEPTH}"))
            .output()
        else {
            return Self::default();
        };
        if !output.status.success() {
            return Self::default();
        }
        let commits = output
            .stdout
            .split(|&byte| byte == 0)
            .filter_map(|record| {
                let mut fields = record.splitn(3, |&byte| byte == FIELD);
                let hash = std::str::from_utf8(fields.next()?).ok()?;
                let parents = std::str::from_utf8(fields.next()?).ok()?;
                let subject = String::from_utf8_lossy(fields.next()?);
                Some(Commit {
                    hash: hash.to_string(),
                    parents: parents.split_whitespace().map(str::to_string).collect(),
                    subject: corpus::normalised(&subject),
                })
            })
            .collect();
        Self { commits }
    }

    pub fn commits(&self) -> &[Commit] {
        &self.commits
    }
}
