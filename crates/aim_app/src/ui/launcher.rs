//! Starts sessions requested from outside the window (IPC `warmup`/`micro` or hotkeys):
//! a queue warm-up, or a buy-phase micro drill that hides the overlay when it ends.
//!
//! The queue warm-up is sized by the player's personal dose ([`PersonalWarmup`]) when the
//! history has enough sessions, and by the requested length otherwise.

use super::UiAction;
use super::navigation::Navigator;
use crate::overlay::QuickLaunch;
use crate::persistence::history::History;
use crate::persistence::settings::Settings;
use crate::platform::ipc::Command;
use aim_core::training::progress::{RunRecord, SESSION_GAP_SECS, bests_until};
use aim_core::training::store::RunStore;
use aim_core::training::warmup::{self, WarmupDose};
use bevy::prelude::*;

/// The player's personal warm-up dose, recomputed only when the history changes.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct PersonalWarmup(pub Option<WarmupDose>);

/// Recompute [`PersonalWarmup`]; runs only when the history changed (a run was recorded).
pub fn refresh_personal_warmup(history: Res<History>, mut dose: ResMut<PersonalWarmup>) {
    dose.set_if_neq(PersonalWarmup(personal_dose(history.runs())));
}

/// Runs split into sessions: a gap longer than [`SESSION_GAP_SECS`] between two runs
/// starts a new one (the same rule as `progress::current_session`).
fn sessions(records: &[RunRecord]) -> Vec<&[RunRecord]> {
    let mut out = Vec::new();
    let mut start = 0;
    for i in 1..records.len() {
        if records[i]
            .timestamp
            .saturating_sub(records[i - 1].timestamp)
            > SESSION_GAP_SECS
        {
            out.push(&records[start..i]);
            start = i;
        }
    }
    if start < records.len() {
        out.push(&records[start..]);
    }
    out
}

/// The warm-up dose from every past session, scored against the all-time personal bests.
pub fn personal_dose(records: &[RunRecord]) -> Option<WarmupDose> {
    let bests = bests_until(records, u64::MAX);
    let samples: Vec<_> = sessions(records)
        .into_iter()
        .map(|s| warmup::session_samples(s, &bests))
        .collect();
    warmup::warmup_dose(&samples)
}

pub fn launch_requested(
    mut quick: ResMut<QuickLaunch>,
    dose: Res<PersonalWarmup>,
    settings: Res<Settings>,
    mut nav: Navigator,
) {
    let Some(cmd) = quick.pending.take() else {
        return;
    };
    let action = match cmd {
        Command::Warmup(minutes) => {
            let dose = dose.0.as_ref().filter(|_| settings.personal_warmup);
            let minutes = warmup::queue_warmup_minutes(dose, minutes as f32);
            UiAction::StartRoutineDef(Box::new(warmup::queue_warmup(minutes)))
        }
        Command::Micro => {
            let (scenario, secs) = warmup::micro_drill(quick.micro_count);
            quick.micro_count += 1;
            UiAction::StartTimed {
                scenario: scenario.to_owned(),
                secs,
                auto_hide: true,
            }
        }
        Command::Toggle | Command::Show | Command::Hide => return,
    };
    nav.apply(action);
}

/// Open the scenario or arena given on the command line, once.
pub fn launch_startup(mut startup: ResMut<super::StartupPlay>, mut nav: Navigator) {
    let Some(id) = startup.0.take() else { return };
    let action = if aim_world::arena::builtin::get(&id).is_some() {
        UiAction::StartArena(id)
    } else if nav.library().scenario(&id).is_some() {
        UiAction::StartScenario(id)
    } else {
        bevy::log::warn!("`play {id}`: no scenario or arena with that id");
        return;
    };
    nav.apply(action);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{ActiveRoutine, AppState, Run};
    use crate::persistence::adaptive::AdaptiveLevels;
    use crate::persistence::library::Library;
    use bevy::ecs::system::{RunSystemOnce, assert_is_system};

    fn world() -> World {
        let mut w = World::new();
        w.insert_resource(NextState::<AppState>::default());
        w.insert_resource(ActiveRoutine::default());
        w.insert_resource(AdaptiveLevels::default());
        w.insert_resource(QuickLaunch::default());
        w.insert_resource(PersonalWarmup::default());
        w.insert_resource(Settings::default());
        w.insert_resource(Library {
            scenarios: aim_core::scenario::builtin::all(),
            routines: Vec::new(),
            benchmarks: Vec::new(),
            methods: Vec::new(),
            errors: Vec::new(),
        });
        w
    }

    #[test]
    fn systems_have_no_conflicting_access() {
        assert_is_system(launch_requested);
        assert_is_system(launch_startup);
    }

    #[test]
    fn micro_requests_start_a_short_auto_hiding_drill_and_rotate() {
        let mut w = world();
        w.resource_mut::<QuickLaunch>().request(Command::Micro);
        w.run_system_once(launch_requested).unwrap();
        let first = w.resource::<Run>().session.def().id.clone();
        assert!(w.resource::<Run>().auto_hide);
        assert!(first.ends_with("~20s"), "{first}");
        w.resource_mut::<QuickLaunch>().request(Command::Micro);
        w.run_system_once(launch_requested).unwrap();
        assert_ne!(w.resource::<Run>().session.def().id, first, "drills rotate");
        assert!(w.resource::<QuickLaunch>().pending.is_none());
    }

    #[test]
    fn warmup_requests_start_the_queue_warmup_routine() {
        let mut w = world();
        w.resource_mut::<QuickLaunch>().request(Command::Warmup(3));
        w.run_system_once(launch_requested).unwrap();
        let routine = w.resource::<ActiveRoutine>().0.clone().unwrap();
        assert_eq!(routine.routine_id, warmup::QUEUE_WARMUP_ID);
        assert!(!w.resource::<Run>().auto_hide);
        assert_eq!(routine.plan.len(), 6, "3 min of 30 s runs without a dose");
    }

    #[test]
    fn a_personal_dose_sizes_the_queue_warmup() {
        let mut w = world();
        w.insert_resource(PersonalWarmup(Some(WarmupDose {
            minutes: 12.0,
            sessions: 4,
        })));
        w.resource_mut::<QuickLaunch>().request(Command::Warmup(5));
        w.run_system_once(launch_requested).unwrap();
        let routine = w.resource::<ActiveRoutine>().0.clone().unwrap();
        assert_eq!(routine.plan.len(), 24, "12 min of 30 s runs");
    }

    #[test]
    fn turning_the_personal_dose_off_keeps_the_set_length() {
        let mut w = world();
        w.insert_resource(PersonalWarmup(Some(WarmupDose {
            minutes: 12.0,
            sessions: 4,
        })));
        w.resource_mut::<Settings>().personal_warmup = false;
        w.resource_mut::<QuickLaunch>().request(Command::Warmup(5));
        w.run_system_once(launch_requested).unwrap();
        let routine = w.resource::<ActiveRoutine>().0.clone().unwrap();
        assert_eq!(routine.plan.len(), 10, "the player's 5 min, not the dose");
    }

    fn rec(timestamp: u64, scenario: &str, score: f32) -> RunRecord {
        RunRecord {
            timestamp,
            scenario: scenario.into(),
            score,
            duration_secs: 60.0,
        }
    }

    /// One run a minute for 10 minutes starting at `start`, rising to the best at minute 6.
    fn session(start: u64) -> Vec<RunRecord> {
        (1..=10)
            .map(|m| {
                let score = 1000.0 * (1.0 - 0.05 * (6.0 - m as f32)).min(1.0);
                rec(start + m * 60, "track-precise", score)
            })
            .collect()
    }

    #[test]
    fn sessions_split_on_gaps_longer_than_twenty_minutes() {
        let r = [
            rec(1_000, "a", 1.0),
            rec(1_000 + SESSION_GAP_SECS, "a", 1.0),
            rec(1_001 + 2 * SESSION_GAP_SECS, "a", 1.0),
        ];
        let s = sessions(&r);
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].len(), 2);
        assert_eq!(s[1].len(), 1);
        assert!(sessions(&[]).is_empty());
    }

    #[test]
    fn the_personal_dose_comes_from_past_sessions_in_the_history() {
        let day = 86_400;
        let mut records = session(day);
        assert_eq!(personal_dose(&records), None, "one session is not enough");
        records.extend(session(2 * day));
        records.extend(session(3 * day));
        let dose = personal_dose(&records).expect("three sessions");
        assert_eq!(dose.sessions, 3);
        assert_eq!(dose.minutes, 6.0);
    }

    #[test]
    fn the_dose_is_recomputed_when_the_history_changes() {
        bevy::ecs::system::assert_is_system(refresh_personal_warmup);
    }
}
