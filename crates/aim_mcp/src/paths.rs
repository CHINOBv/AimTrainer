//! Where the trainer keeps the player's files.
//!
//! Mirrors `Paths::resolve` in `crates/aim_app/src/persistence/settings.rs`: `aim_app` is a
//! binary, so the resolution cannot be shared. Keep the qualifier, organization and file
//! names in sync with it. Unlike the app, this never creates folders: the server only reads,
//! except for an opt-in `create_scenario` with `dry_run = false` and the coach memory
//! (`remember` and `recall` open `coach.db` in the data folder, creating it if needed).

use std::path::{Path, PathBuf};

use directories::ProjectDirs;

/// File name of the coach memory in the data folder (the app's `COACH_MEMORY_FILE`).
pub const MEMORY_FILE: &str = "coach.db";

/// The player's content and history locations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// Custom scenario TOML files; `create_scenario` saves here.
    pub scenarios_dir: PathBuf,
    /// Custom routine TOML files.
    pub routines_dir: PathBuf,
    /// Custom benchmark TOML files.
    pub benchmarks_dir: PathBuf,
    /// The run history (`history.csv`).
    pub history_file: PathBuf,
    /// The coach memory (SQLite), shared with the trainer's Coach page.
    pub memory_file: PathBuf,
}

impl Paths {
    /// The same folders the trainer uses on this OS.
    pub fn resolve() -> Self {
        match ProjectDirs::from("dev", "CustomAimTrainer", "aimtrainer") {
            Some(d) => Self::in_dirs(d.config_dir(), d.data_dir()),
            None => Self::in_dirs(Path::new("aimtrainer-config"), Path::new("aimtrainer-data")),
        }
    }

    /// The layout under a config folder and a data folder.
    pub fn in_dirs(config: &Path, data: &Path) -> Self {
        Self {
            scenarios_dir: config.join("scenarios"),
            routines_dir: config.join("routines"),
            benchmarks_dir: config.join("benchmarks"),
            history_file: data.join("history.csv"),
            memory_file: data.join(MEMORY_FILE),
        }
    }
}
