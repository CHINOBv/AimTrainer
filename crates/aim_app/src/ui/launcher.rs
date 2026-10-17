//! Starts sessions requested from outside the window (IPC `warmup`/`micro` or hotkeys):
//! a queue warm-up, or a buy-phase micro drill that hides the overlay when it ends.

use super::UiAction;
use super::navigation::Navigator;
use crate::overlay::QuickLaunch;
use crate::platform::ipc::Command;
use aim_core::training::warmup;
use bevy::prelude::*;

pub fn launch_requested(mut quick: ResMut<QuickLaunch>, mut nav: Navigator) {
    let Some(cmd) = quick.pending.take() else {
        return;
    };
    let action = match cmd {
        Command::Warmup(minutes) => {
            UiAction::StartRoutineDef(Box::new(warmup::queue_warmup(minutes as f32)))
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
    }
}
