use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;

use crate::corpus::{self, Corpus};
use crate::scene::SceneKind;

#[derive(Debug, Parser)]
#[command(version, about)]
pub struct Cli {
    /// The scene to run. With none, the picker opens
    #[arg(value_enum)]
    pub scene: Option<SceneKind>,

    /// Seed for the random source, so a run can be replayed
    #[arg(long)]
    pub seed: Option<u64>,

    /// What the typing scene types: a checkout, `owner/name` on GitHub, or a
    /// clone address. Without it, hearth types its own source
    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

impl Cli {
    /// The seed given on the command line, or one from the clock, so two
    /// runs started in the same second still differ by their nanoseconds.
    pub fn seed(&self) -> u64 {
        self.seed.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos() as u64)
                .unwrap_or_default()
        })
    }

    /// The source files for the typing scene. A clone happens on the first
    /// run for an address, with one line on stderr before the terminal
    /// opens, and the cache serves every run after.
    pub fn corpus(&self) -> io::Result<Corpus> {
        let Some(repo) = &self.repo else {
            return Ok(Corpus::embedded());
        };
        if Path::new(repo).is_dir() {
            return Corpus::from_checkout(Path::new(repo));
        }
        let address = clone_address(repo);
        let cache = cache_dir();
        if !corpus::cache_path(&address, &cache).join(".git").exists() {
            eprintln!("hearth: fetching {address} into {}", cache.display());
        }
        Corpus::cloned(&address, &cache)
    }
}

/// `owner/name` means GitHub. Anything with a scheme or an `@` is used as
/// given.
fn clone_address(repo: &str) -> String {
    let bare = !repo.contains("://") && !repo.contains('@');
    let two_parts = repo.split('/').filter(|part| !part.is_empty()).count() == 2;
    if bare && two_parts {
        format!("https://github.com/{}.git", repo.trim_matches('/'))
    } else {
        repo.to_string()
    }
}

fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("hearth")
}
