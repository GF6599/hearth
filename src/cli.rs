use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;

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
}
