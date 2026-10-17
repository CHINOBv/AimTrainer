//! Run history stored as CSV so it is easy to inspect or import into a spreadsheet.

use crate::persistence::settings::Paths;
use aim_core::scenario::RunResult;
use bevy::prelude::Resource;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

const HEADER: &str = "timestamp,scenario,score,accuracy,kills,kills_per_min,avg_time_to_kill,headshot_rate,moving_shots,duration_secs";
/// Rows written before `duration_secs` existed were full-length runs.
const LEGACY_DURATION_SECS: f32 = 60.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub timestamp: u64,
    pub scenario: String,
    pub score: f32,
    pub accuracy: f32,
    pub duration_secs: f32,
}

#[derive(Resource, Debug, Default)]
pub struct History {
    pub records: Vec<Record>,
}

impl History {
    pub fn load(paths: &Paths) -> Self {
        let Ok(src) = fs::read_to_string(&paths.history_file) else {
            return Self::default();
        };
        let records = src
            .lines()
            .skip(1)
            .filter_map(|line| {
                let f: Vec<&str> = line.split(',').collect();
                Some(Record {
                    timestamp: f.first()?.parse().ok()?,
                    scenario: f.get(1)?.to_string(),
                    score: f.get(2)?.parse().ok()?,
                    accuracy: f.get(3)?.parse().ok()?,
                    duration_secs: f
                        .get(9)
                        .and_then(|d| d.parse().ok())
                        .unwrap_or(LEGACY_DURATION_SECS),
                })
            })
            .collect();
        Self { records }
    }

    /// History in the core's format, for progress signals (readiness, fatigue, goals).
    pub fn run_records(&self) -> Vec<aim_core::training::progress::RunRecord> {
        self.records
            .iter()
            .map(|r| aim_core::training::progress::RunRecord {
                timestamp: r.timestamp,
                scenario: r.scenario.clone(),
                score: r.score,
                duration_secs: r.duration_secs,
            })
            .collect()
    }

    pub fn best(&self, scenario: &str) -> Option<f32> {
        self.records
            .iter()
            .filter(|r| r.scenario == scenario)
            .map(|r| r.score)
            .reduce(f32::max)
    }

    pub fn bests(&self) -> HashMap<String, f32> {
        let mut out = HashMap::new();
        for r in &self.records {
            let e = out.entry(r.scenario.clone()).or_insert(r.score);
            *e = e.max(r.score);
        }
        out
    }

    pub fn recent(&self, scenario: &str, n: usize) -> Vec<f32> {
        let mut v: Vec<f32> = self
            .records
            .iter()
            .filter(|r| r.scenario == scenario)
            .map(|r| r.score)
            .collect();
        let skip = v.len().saturating_sub(n);
        v.drain(..skip);
        v
    }

    pub fn append(&mut self, paths: &Paths, r: &RunResult) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let new_file = !paths.history_file.exists();
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
            .open(&paths.history_file)
            .and_then(|mut f| {
                if new_file {
                    writeln!(f, "{HEADER}")?;
                }
                writeln!(f, "{line}")
            });
        if let Err(e) = written {
            bevy::log::error!("could not write history: {e}");
        }
        self.records.push(Record {
            timestamp,
            scenario: r.scenario_id.clone(),
            score: r.score,
            accuracy: r.accuracy,
            duration_secs: r.duration_secs,
        });
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
        assert_eq!(h.records[0].duration_secs, LEGACY_DURATION_SECS);
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
        h.append(&paths, &result);
        let reloaded = History::load(&paths);
        assert_eq!(reloaded.records.len(), 2);
        assert_eq!(reloaded.records[1].duration_secs, 20.0);
        assert_eq!(reloaded.run_records()[1].duration_secs, 20.0);
        fs::remove_dir_all(&dir).unwrap();
    }
}
