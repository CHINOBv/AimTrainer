//! Today's compound missions and the recent runs they are built from. The rules live in
//! `aim_core::training::missions`; this file stores them in `missions.toml` next to the
//! run history.

use crate::persistence::settings::Paths;
use aim_core::scenario::RunResult;
use aim_core::training::missions::{Commitment, MissionBook};
use bevy::prelude::*;
use std::fs;
use std::path::PathBuf;

/// Loads the mission book at startup (the paths are inserted after the plugins).
pub struct MissionsPlugin;

impl Plugin for MissionsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Missions>()
            .add_systems(Startup, load_missions);
    }
}

fn load_missions(mut missions: ResMut<Missions>, paths: Res<Paths>) {
    *missions = Missions::load(&paths);
    if missions.0.seed == 0 {
        // A per-install seed, so players get different missions on the same day.
        missions.0.seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos() as u64 | 1);
        missions.save(&paths);
    }
}

#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct Missions(pub MissionBook);

fn file(paths: &Paths) -> Option<PathBuf> {
    paths.history_file.parent().map(|d| d.join("missions.toml"))
}

impl Missions {
    pub fn load(paths: &Paths) -> Self {
        Self(
            file(paths)
                .and_then(|f| fs::read_to_string(f).ok())
                .and_then(|s| toml::from_str(&s).ok())
                .unwrap_or_default(),
        )
    }

    pub fn save(&self, paths: &Paths) {
        let Some(f) = file(paths) else { return };
        let result = toml::to_string_pretty(&self.0)
            .map_err(|e| e.to_string())
            .and_then(|s| fs::write(&f, s).map_err(|e| e.to_string()));
        if let Err(e) = result {
            bevy::log::error!("could not save missions: {e}");
        }
    }

    /// Generate today's missions if needed, saving when they change.
    pub fn refresh(&mut self, date: &str, day: i64, commitment: Commitment, paths: &Paths) {
        if self.0.refresh(date, day, commitment) {
            self.save(paths);
        }
    }

    /// Count a finished run (`shots_hit` from the session stats) and save.
    pub fn record(&mut self, result: &RunResult, shots_hit: u32, paths: &Paths) {
        self.0.record(result, shots_hit);
        self.save(paths);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missions_survive_a_restart() {
        let dir = std::env::temp_dir().join(format!("aimtrainer-missions-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let paths = Paths {
            settings_file: dir.join("settings.toml"),
            scenarios_dir: dir.join("scenarios"),
            routines_dir: dir.join("routines"),
            benchmarks_dir: dir.join("benchmarks"),
            history_file: dir.join("history.csv"),
            coach_memory_file: dir.join("coach.db"),
        };
        let result = RunResult {
            scenario_id: "micro-adjust".into(),
            score: 900.0,
            accuracy: 0.8,
            kills: 50,
            kills_per_min: 50.0,
            avg_time_to_kill: Some(0.7),
            headshot_rate: 0.0,
            moving_shots: 0,
            expired: 0,
            duration_secs: 60.0,
        };
        let mut m = Missions::default();
        m.record(&result, 55, &paths);
        m.refresh("2026-10-18", 1, Commitment::Medium, &paths);
        m.record(&result, 55, &paths);
        let loaded = Missions::load(&paths);
        assert_eq!(loaded, m);
        assert!(loaded.0.today.unwrap().missions.iter().any(|x| x.done));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn systems_have_no_conflicting_access() {
        bevy::ecs::system::assert_is_system(load_missions);
    }
}
