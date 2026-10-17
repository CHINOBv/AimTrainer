//! Port for run history. The core asks for runs; an adapter (the app's CSV file, a test's
//! memory) decides where they live. Every history query is written once, here.

use super::progress::RunRecord;
use crate::scenario::RunResult;
use std::collections::HashMap;

/// Where finished runs are kept.
pub trait RunStore {
    /// Every stored run, oldest first.
    fn runs(&self) -> &[RunRecord];

    /// Keep a finished run, finished at Unix second `timestamp`.
    fn append(&mut self, result: &RunResult, timestamp: u64);

    /// Best score stored under the run key `key`.
    fn best(&self, key: &str) -> Option<f32> {
        self.runs()
            .iter()
            .filter(|r| r.scenario == key)
            .map(|r| r.score)
            .reduce(f32::max)
    }

    /// Best score per run key.
    fn bests(&self) -> HashMap<String, f32> {
        let mut out = HashMap::new();
        for r in self.runs() {
            let e = out.entry(r.scenario.clone()).or_insert(r.score);
            *e = e.max(r.score);
        }
        out
    }

    /// The last `n` scores under `key`, oldest first.
    fn recent(&self, key: &str, n: usize) -> Vec<f32> {
        let mut v: Vec<f32> = self
            .runs()
            .iter()
            .filter(|r| r.scenario == key)
            .map(|r| r.score)
            .collect();
        let skip = v.len().saturating_sub(n);
        v.drain(..skip);
        v
    }
}

/// The stored form of a result.
pub fn record_of(result: &RunResult, timestamp: u64) -> RunRecord {
    RunRecord {
        timestamp,
        scenario: result.scenario_id.clone(),
        score: result.score,
        duration_secs: result.duration_secs,
    }
}

/// A store that keeps runs in memory (tests, simulations).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MemoryStore(pub Vec<RunRecord>);

impl RunStore for MemoryStore {
    fn runs(&self) -> &[RunRecord] {
        &self.0
    }

    fn append(&mut self, result: &RunResult, timestamp: u64) {
        self.0.push(record_of(result, timestamp));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(key: &str, score: f32) -> RunResult {
        RunResult {
            scenario_id: key.into(),
            score,
            accuracy: 0.5,
            kills: 1,
            kills_per_min: 1.0,
            avg_time_to_kill: None,
            headshot_rate: 0.0,
            moving_shots: 0,
            expired: 0,
            duration_secs: 60.0,
        }
    }

    #[test]
    fn queries_answer_from_the_stored_runs() {
        let mut s = MemoryStore::default();
        assert_eq!(s.best("a"), None);
        for (i, (k, v)) in [("a", 10.0), ("b", 5.0), ("a", 30.0), ("a", 20.0)]
            .into_iter()
            .enumerate()
        {
            s.append(&result(k, v), i as u64);
        }
        assert_eq!(s.runs().len(), 4);
        assert_eq!(s.runs()[2].timestamp, 2);
        assert_eq!(s.best("a"), Some(30.0));
        assert_eq!(s.bests()["b"], 5.0);
        assert_eq!(s.recent("a", 2), [30.0, 20.0]);
        assert_eq!(s.recent("a", 9), [10.0, 30.0, 20.0]);
        assert_eq!(s.best("a@+0.50"), None, "run keys never mix difficulties");
    }
}
