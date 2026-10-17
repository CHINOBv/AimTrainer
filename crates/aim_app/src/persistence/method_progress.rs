//! Where the player is in the weekly method (chosen tier and today's possibly split day)
//! and in every other routine played today. The resume rules live in
//! `aim_core::training::resume`; this file stores them.

use crate::game::{ActiveRoutine, AppState};
use crate::persistence::settings::Paths;
use aim_core::training::resume::{PositionStore, RoutinePosition, record_played};
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

/// A finished routine run moves its saved position forward: today's method day, or the
/// routine's own position.
fn advance_day(
    routine: Res<ActiveRoutine>,
    mut progress: ResMut<MethodProgress>,
    paths: Res<Paths>,
) {
    let Some(p) = routine.0.as_ref() else { return };
    match progress.today.as_mut() {
        Some(day) if day.routine == p.routine_id && day.seed == p.seed => {
            day.next_index = day.next_index.max(p.index + 1);
        }
        _ => record_played(
            &mut *progress,
            &local_date(),
            &p.routine_id,
            p.base_level,
            p.seed,
            p.index,
            p.plan.len(),
        ),
    }
    progress.save(&paths);
}

/// Today's local date, `YYYY-MM-DD`: sessions follow the player's clock.
pub fn local_date() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

#[derive(Resource, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MethodProgress {
    /// Tier the player picked; `None` follows the benchmark suggestion.
    pub tier: Option<String>,
    pub today: Option<DayProgress>,
    /// Other routines played today, so a split session resumes where it stopped.
    pub routines: Vec<RoutinePosition>,
}

/// A method day in progress (same shape and rules as any routine position).
pub type DayProgress = RoutinePosition;

impl PositionStore for MethodProgress {
    fn positions(&self) -> &[RoutinePosition] {
        &self.routines
    }

    fn save_positions(&mut self, positions: Vec<RoutinePosition>) {
        self.routines = positions;
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

    /// Today's method day if it is still for `date` and `routine`.
    pub fn method_day(&self, date: &str, routine: &str) -> Option<&DayProgress> {
        self.today.as_ref().filter(|d| d.resumes(date, routine))
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
            routines: Vec::new(),
        };
        assert!(p.method_day("2026-10-17", "r").is_some());
        assert!(p.method_day("2026-10-18", "r").is_none());
        assert!(p.method_day("2026-10-17", "other").is_none());
    }

    #[test]
    fn files_saved_before_routine_positions_still_load() {
        let old = "tier = \"advanced\"\n[today]\ndate = \"2026-10-17\"\nroutine = \"r\"\nlevel = 0.0\nseed = 1\nnext_index = 3\ntotal_runs = 10\n";
        let p: MethodProgress = toml::from_str(old).unwrap();
        assert_eq!(p.today, Some(day("2026-10-17")));
        assert!(p.routines.is_empty());
    }

    #[test]
    fn round_trips_through_toml() {
        let p = MethodProgress {
            tier: Some("advanced".into()),
            today: Some(day("2026-10-17")),
            routines: vec![day("2026-10-17")],
        };
        let s = toml::to_string_pretty(&p).unwrap();
        assert_eq!(toml::from_str::<MethodProgress>(&s).unwrap(), p);
        assert!(!p.today.as_ref().unwrap().is_done());
    }
}
