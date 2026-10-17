//! Where the player is in the weekly method: chosen tier and today's (possibly split) day.

use crate::game::{ActiveRoutine, AppState};
use crate::persistence::settings::Paths;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Loads the method progress and advances it as method-day runs finish.
pub struct MethodProgressPlugin;

impl Plugin for MethodProgressPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Results), advance_day);
    }
}

/// A finished run of today's method session moves its saved position forward.
fn advance_day(
    routine: Res<ActiveRoutine>,
    mut progress: ResMut<MethodProgress>,
    paths: Res<Paths>,
) {
    let Some(p) = routine.0.as_ref() else { return };
    let Some(day) = progress.today.as_mut() else {
        return;
    };
    if day.routine == p.routine_id && day.seed == p.seed {
        day.next_index = day.next_index.max(p.index + 1);
        progress.save(&paths);
    }
}

#[derive(Resource, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MethodProgress {
    /// Tier the player picked; `None` follows the benchmark suggestion.
    pub tier: Option<String>,
    pub today: Option<DayProgress>,
}

/// A method day in progress. Matching `date` and `routine` means "resume", anything else
/// means a new day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DayProgress {
    /// Local date, `YYYY-MM-DD`.
    pub date: String,
    pub routine: String,
    pub level: f32,
    pub seed: u64,
    /// Index of the next run to play.
    pub next_index: usize,
    pub total_runs: usize,
}

impl DayProgress {
    pub fn is_done(&self) -> bool {
        self.next_index >= self.total_runs
    }
}

fn file(paths: &Paths) -> Option<PathBuf> {
    paths
        .history_file
        .parent()
        .map(|d| d.join("method-progress.toml"))
}

impl MethodProgress {
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
            bevy::log::error!("could not save method progress: {e}");
        }
    }

    /// Today's day progress if it is still for `date` and `routine`.
    pub fn resumable(&self, date: &str, routine: &str) -> Option<&DayProgress> {
        self.today
            .as_ref()
            .filter(|d| d.date == date && d.routine == routine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(date: &str) -> DayProgress {
        DayProgress {
            date: date.into(),
            routine: "r".into(),
            level: 0.0,
            seed: 1,
            next_index: 3,
            total_runs: 10,
        }
    }

    #[test]
    fn systems_have_no_conflicting_access() {
        bevy::ecs::system::assert_is_system(advance_day);
    }

    #[test]
    fn resumes_only_the_same_day_and_routine() {
        let p = MethodProgress {
            tier: None,
            today: Some(day("2026-10-17")),
        };
        assert!(p.resumable("2026-10-17", "r").is_some());
        assert!(p.resumable("2026-10-18", "r").is_none());
        assert!(p.resumable("2026-10-17", "other").is_none());
    }

    #[test]
    fn round_trips_through_toml() {
        let p = MethodProgress {
            tier: Some("advanced".into()),
            today: Some(day("2026-10-17")),
        };
        let s = toml::to_string_pretty(&p).unwrap();
        assert_eq!(toml::from_str::<MethodProgress>(&s).unwrap(), p);
        assert!(!p.today.as_ref().unwrap().is_done());
    }
}
