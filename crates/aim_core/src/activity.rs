//! Activities: anything the player does for a while that produces a result. A scenario
//! [`Session`] is one; `aim_world`'s arena session is another. Routines sequence
//! activities, so a day can mix drills and deathmatch (see `training::routine`).

use crate::math::ViewAngles;
use crate::scenario::{FrameInput, RunResult, Session, SessionEvent};
use serde::{Deserialize, Serialize};

/// Which kind of activity a routine block or planned run refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityKind {
    /// A single scenario drill run through a `Session`.
    #[default]
    Scenario,
    /// An arena session (aim_world).
    Arena,
}

/// What happened in a frame, in terms every activity shares.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActivityEvent {
    /// A shot was fired.
    Shot {
        /// Whether the shot hit a target.
        hit: bool,
    },
    /// A target was killed.
    Kill {
        /// Seconds from the target becoming visible to its death.
        time_to_kill_secs: f32,
    },
    /// The player took damage (arenas only).
    Damaged,
    /// The activity reached its end.
    Finished,
}

/// Anything the player can do for a while that produces a result.
pub trait Activity {
    /// Advance by one frame and return what happened.
    fn update(&mut self, input: FrameInput) -> Vec<ActivityEvent>;
    /// Whether the activity has ended.
    fn is_finished(&self) -> bool;
    /// Final numbers of the run so far.
    fn result(&self) -> RunResult;
    /// Seconds played.
    fn time(&self) -> f32;
    /// Where the player looks after the last update (activities may move the view, e.g.
    /// respawning faces the spawn direction).
    fn view(&self) -> Option<ViewAngles> {
        None
    }
}

impl Activity for Session {
    fn update(&mut self, input: FrameInput) -> Vec<ActivityEvent> {
        Session::update(self, input)
            .into_iter()
            .filter_map(|e| match e {
                SessionEvent::Shot { hit, .. } => Some(ActivityEvent::Shot { hit: hit.is_some() }),
                SessionEvent::Killed { time_to_kill, .. } => Some(ActivityEvent::Kill {
                    time_to_kill_secs: time_to_kill,
                }),
                SessionEvent::Finished => Some(ActivityEvent::Finished),
                SessionEvent::Spawned { .. }
                | SessionEvent::Expired { .. }
                | SessionEvent::DecoyAvoided { .. }
                | SessionEvent::Reload { .. }
                | SessionEvent::PhaseStarted { .. }
                | SessionEvent::RunFailed { .. }
                | SessionEvent::Cue { .. } => None,
            })
            .collect()
    }

    fn is_finished(&self) -> bool {
        Session::is_finished(self)
    }

    fn result(&self) -> RunResult {
        Session::result(self)
    }

    fn time(&self) -> f32 {
        Session::time(self)
    }
}

/// Play `activity` with `frame` until it finishes (or `max_frames` pass) and return its
/// result. Used by tests and simulations that treat every activity alike.
pub fn play_to_end<A: Activity + ?Sized>(
    activity: &mut A,
    max_frames: usize,
    mut frame: impl FnMut(&A) -> FrameInput,
) -> RunResult {
    for _ in 0..max_frames {
        if activity.is_finished() {
            break;
        }
        let input = frame(activity);
        activity.update(input);
    }
    activity.result()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::builtin;

    fn scenario(id: &str) -> crate::scenario::ScenarioDef {
        builtin::all().into_iter().find(|d| d.id == id).unwrap()
    }

    fn idle(dt: f32) -> FrameInput {
        FrameInput {
            dt,
            view: ViewAngles::default(),
            ..FrameInput::default()
        }
    }

    #[test]
    fn a_scenario_session_is_an_activity() {
        let def = scenario("micro-adjust").with_duration(5.0);
        let mut session = Session::new(def, 7);
        let activity: &mut dyn Activity = &mut session;
        let result = play_to_end(activity, 10_000, |_| idle(1.0 / 60.0));
        assert!(activity.is_finished());
        assert!((activity.time() - 5.0).abs() < 1e-3, "{}", activity.time());
        assert_eq!(result.scenario_id, "micro-adjust~5s");
    }

    #[test]
    fn session_events_map_to_activity_events() {
        let def = scenario("micro-adjust").with_duration(1.0);
        let mut session = Session::new(def, 7);
        let mut events = Vec::new();
        while !Activity::is_finished(&session) {
            let mut input = idle(0.1);
            input.fire_pressed = true;
            events.extend(Activity::update(&mut session, input));
        }
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ActivityEvent::Shot { .. }))
        );
        assert_eq!(events.last(), Some(&ActivityEvent::Finished));
    }
}
