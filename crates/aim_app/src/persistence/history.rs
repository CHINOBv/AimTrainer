//! Run history stored as CSV so it is easy to inspect or import into a spreadsheet.

use crate::persistence::settings::Paths;
use aim_core::scenario::RunResult;
use aim_core::training::progress::RunRecord;
use aim_core::training::store::{RunStore, record_of};
use bevy::prelude::Resource;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const HEADER: &str = "timestamp,scenario,score,accuracy,kills,kills_per_min,avg_time_to_kill,headshot_rate,moving_shots,duration_secs";
/// Rows written before `duration_secs` existed were full-length runs.
const LEGACY_DURATION_SECS: f32 = 60.0;

/// Run history kept in `history.csv`: the app's adapter for the core's [`RunStore`] port.
/// The file keeps every column of the result; the store keeps what the core queries.
/// Built only by [`History::load`], so every run reaches the file (no `Default`).
#[derive(Resource, Debug)]
pub struct History {
    records: Vec<RunRecord>,
    file: Option<PathBuf>,
}

impl History {
    pub fn load(paths: &Paths) -> Self {
        let file = Some(paths.history_file.clone());
        let Ok(src) = fs::read_to_string(&paths.history_file) else {
            return Self {
                records: Vec::new(),
                file,
            };
        };
        let records = src
            .lines()
            .skip(1)
            .filter_map(|line| {
                let f: Vec<&str> = line.split(',').collect();
                Some(RunRecord {
                    timestamp: f.first()?.parse().ok()?,
                    scenario: f.get(1)?.to_string(),
                    score: f.get(2)?.parse().ok()?,
                    duration_secs: f
                        .get(9)
                        .and_then(|d| d.parse().ok())
                        .unwrap_or(LEGACY_DURATION_SECS),
                })
            })
            .collect();
        Self { records, file }
    }

    /// Keep a run that finished now.
    pub fn record(&mut self, r: &RunResult) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        self.append(r, timestamp);
    }
}

impl RunStore for History {
    fn runs(&self) -> &[RunRecord] {
        &self.records
    }

    fn append(&mut self, r: &RunResult, timestamp: u64) {
        if let Some(path) = &self.file {
            let new_file = !path.exists();
            let line = format!(
                "{timestamp},{},{:.1},{:.4},{},{:.2},{},{:.4},{},{:.1}",
                r.scenario_id,
                r.score,
                r.accuracy,
                r.kills,
                r.kills_per_min,
                r.avg_time_to_kill
                    .map_or(String::new(), |t| format!("{t:.4}")),
                r.headshot_rate,
                r.moving_shots,
                r.duration_secs,
            );
            let written = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| {
                    if new_file {
                        writeln!(f, "{HEADER}")?;
                    }
                    writeln!(f, "{line}")
                });
            if let Err(e) = written {
                bevy::log::error!("could not write history: {e}");
            }
        }
        self.records.push(record_of(r, timestamp));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths_in(dir: &std::path::Path) -> Paths {
        Paths {
            settings_file: dir.join("settings.toml"),
            scenarios_dir: dir.join("scenarios"),
            routines_dir: dir.join("routines"),
            benchmarks_dir: dir.join("benchmarks"),
            history_file: dir.join("history.csv"),
        }
    }

    #[test]
    fn legacy_rows_load_with_a_default_duration_and_new_rows_keep_theirs() {
        let dir = std::env::temp_dir().join(format!("aimtrainer-history-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let paths = paths_in(&dir);
        fs::write(
            &paths.history_file,
            "timestamp,scenario,score,accuracy,kills,kills_per_min,avg_time_to_kill,headshot_rate,moving_shots\n1,micro-adjust,100.0,0.5,1,1.0,,0.0,0\n",
        )
        .unwrap();
        let mut h = History::load(&paths);
        assert_eq!(h.runs()[0].duration_secs, LEGACY_DURATION_SECS);
        let result = RunResult {
            scenario_id: "micro-adjust~20s".into(),
            score: 50.0,
            accuracy: 0.5,
            kills: 1,
            kills_per_min: 3.0,
            avg_time_to_kill: None,
            headshot_rate: 0.0,
            moving_shots: 0,
            expired: 0,
            duration_secs: 20.0,
        };
        h.record(&result);
        let reloaded = History::load(&paths);
        assert_eq!(reloaded.runs().len(), 2);
        assert_eq!(reloaded.runs()[1].duration_secs, 20.0);
        assert_eq!(reloaded.best("micro-adjust~20s"), Some(50.0));
        fs::remove_dir_all(&dir).unwrap();
    }
}
