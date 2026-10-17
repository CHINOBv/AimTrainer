//! Run history stored as CSV so it is easy to inspect or import into a spreadsheet.

use crate::settings::Paths;
use aim_core::scenario::RunResult;
use bevy::prelude::Resource;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

const HEADER: &str = "timestamp,scenario,score,accuracy,kills,kills_per_min,avg_time_to_kill,headshot_rate,moving_shots";

#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub timestamp: u64,
    pub scenario: String,
    pub score: f32,
    pub accuracy: f32,
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
                })
            })
            .collect();
        Self { records }
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
            "{timestamp},{},{:.1},{:.4},{},{:.2},{},{:.4},{}",
            r.scenario_id,
            r.score,
            r.accuracy,
            r.kills,
            r.kills_per_min,
            r.avg_time_to_kill
                .map_or(String::new(), |t| format!("{t:.4}")),
            r.headshot_rate,
            r.moving_shots,
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
        });
    }
}
