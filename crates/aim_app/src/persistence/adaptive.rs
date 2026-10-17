//! Adaptive difficulty levels per scenario, persisted between sessions.

use crate::persistence::settings::Paths;
use aim_core::training::adaptive::Staircase;
use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Resource, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdaptiveLevels {
    /// Staircase per base scenario id.
    pub scenarios: BTreeMap<String, Staircase>,
}

fn file(paths: &Paths) -> Option<PathBuf> {
    paths.history_file.parent().map(|d| d.join("adaptive.toml"))
}

impl AdaptiveLevels {
    pub fn load(paths: &Paths) -> Self {
        file(paths)
            .and_then(|f| fs::read_to_string(f).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, paths: &Paths) {
        let Some(f) = file(paths) else { return };
        let result = toml::to_string_pretty(self)
            .map_err(|e| e.to_string())
            .and_then(|s| fs::write(&f, s).map_err(|e| e.to_string()));
        if let Err(e) = result {
            bevy::log::error!("could not save adaptive levels: {e}");
        }
    }

    /// Level to play `scenario` at next (0 before the first adaptive run).
    pub fn level(&self, scenario: &str) -> f32 {
        self.scenarios.get(scenario).map_or(0.0, |s| s.level)
    }

    /// Record a finished adaptive run and return the next level.
    pub fn record(&mut self, scenario: &str, success: f32) -> f32 {
        self.scenarios
            .entry(scenario.to_owned())
            .or_default()
            .update(success)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_per_scenario_and_round_trips() {
        let mut a = AdaptiveLevels::default();
        assert_eq!(a.level("x"), 0.0);
        assert!(a.record("x", 1.0) > 0.0);
        assert_eq!(a.level("y"), 0.0, "scenarios are independent");
        let s = toml::to_string_pretty(&a).unwrap();
        assert_eq!(toml::from_str::<AdaptiveLevels>(&s).unwrap(), a);
    }
}
