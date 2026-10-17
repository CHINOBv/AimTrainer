//! Applies [`UiAction`]s: the single place where UI intent turns into state changes.

use super::UiAction;
use crate::game::arena::{ArenaRun, start_arena};
use crate::game::{ActiveRoutine, AppState, RoutineProgress, Run, start_run, start_run_with};
use crate::persistence::adaptive::AdaptiveLevels;
use crate::persistence::library::Library;
use aim_core::scenario::{ScenarioDef, parse_run_key_full};
use aim_core::training::routine::RoutineDef;
use aim_world::arena::ArenaSession;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

#[derive(SystemParam)]
pub struct Navigator<'w, 's> {
    commands: Commands<'w, 's>,
    next: ResMut<'w, NextState<AppState>>,
    routine: ResMut<'w, ActiveRoutine>,
    library: Res<'w, Library>,
    run: Option<ResMut<'w, Run>>,
    adaptive: Res<'w, AdaptiveLevels>,
}

impl Navigator<'_, '_> {
    pub fn library(&self) -> &Library {
        &self.library
    }

    /// The active run, if one exists. Pages read it through here so that a page never
    /// requests `Run` a second time next to the navigator (Bevy B0002 conflict).
    pub fn run(&self) -> Option<&Run> {
        self.run.as_deref()
    }

    pub fn routine(&self) -> Option<&RoutineProgress> {
        self.routine.0.as_ref()
    }

    pub fn apply(&mut self, action: UiAction) {
        match action {
            UiAction::StartScenario(id) | UiAction::Retry(id) => {
                self.routine.0 = None;
                // Results carry run keys (`id@+0.50`); retry at the same difficulty.
                let (base, level, secs) = parse_run_key_full(&id);
                let base = base.to_owned();
                self.start_with(&base, level, secs);
            }
            UiAction::StartRoutine(id) => self.start_routine(&id, 0.0, 0, routine_seed()),
            UiAction::StartAdaptive(id) => {
                self.routine.0 = None;
                let level = self.adaptive.level(&id);
                if let Some(def) = self.library.scenario(&id) {
                    let mut run = Run::new(def.at_difficulty(level));
                    run.adaptive = true;
                    start_run_with(&mut self.commands, &mut self.next, run);
                }
            }
            UiAction::StartRoutineDef(def) => self.start_routine_def(&def, 0.0, 0, routine_seed()),
            UiAction::StartArena(id) => {
                self.routine.0 = None;
                match aim_world::arena::builtin::load(&id) {
                    Some(Ok((def, map))) => match ArenaSession::new(def, map, routine_seed()) {
                        Ok(session) => {
                            start_arena(&mut self.commands, &mut self.next, ArenaRun::new(session))
                        }
                        Err(e) => bevy::log::error!("arena {id}: {e}"),
                    },
                    Some(Err(e)) => bevy::log::error!("arena {id}: {e}"),
                    None => {}
                }
            }
            UiAction::StartTimed {
                scenario,
                secs,
                auto_hide,
            } => {
                self.routine.0 = None;
                if let Some(def) = self.library.scenario(&scenario) {
                    let mut run = Run::new(def.with_duration(secs));
                    run.auto_hide = auto_hide;
                    start_run_with(&mut self.commands, &mut self.next, run);
                }
            }
            UiAction::StartRoutineAt {
                routine,
                level,
                index,
                seed,
            } => self.start_routine(&routine, level, index, seed),
            UiAction::NextRoutineRun => {
                let Some(p) = self.routine.0.as_mut() else {
                    return;
                };
                let Some(next) = p.plan.get(p.index + 1) else {
                    return;
                };
                let level = next.level(p.base_level);
                let secs = next.duration_secs;
                let Some(def) = self.library.scenario(&next.scenario_id) else {
                    return;
                };
                let def = shaped(def, level, secs);
                p.index += 1;
                start_run(&mut self.commands, &mut self.next, def);
            }
            UiAction::Resume => {
                if let Some(run) = self.run.as_mut() {
                    run.paused = false;
                }
            }
            UiAction::BackToMenu => {
                self.routine.0 = None;
                self.next.set(AppState::Menu);
            }
            UiAction::Quit => {
                self.commands.write_message(AppExit::Success);
            }
        }
    }

    fn start_with(&mut self, scenario_id: &str, level: f32, secs: Option<f32>) {
        if let Some(def) = self.library.scenario(scenario_id) {
            start_run(&mut self.commands, &mut self.next, shaped(def, level, secs));
        }
    }

    /// Start `routine` at `level`, from run `index` of the plan shuffled with `seed`.
    fn start_routine(&mut self, routine: &str, level: f32, index: usize, seed: u64) {
        if let Some(r) = self
            .library
            .routines
            .iter()
            .find(|r| r.id == routine)
            .cloned()
        {
            self.start_routine_def(&r, level, index, seed);
        }
    }

    fn start_routine_def(&mut self, r: &RoutineDef, level: f32, index: usize, seed: u64) {
        let plan = r.plan(&self.library.scenarios, seed);
        let Some(run) = plan.get(index) else { return };
        let (scenario, run_level, secs) =
            (run.scenario_id.clone(), run.level(level), run.duration_secs);
        self.routine.0 = Some(RoutineProgress {
            routine_id: r.id.clone(),
            name: r.name.clone(),
            plan,
            index,
            base_level: level,
            seed,
        });
        self.start_with(&scenario, run_level, secs);
    }
}

/// A scenario at a difficulty level and optional shortened duration.
fn shaped(def: &ScenarioDef, level: f32, secs: Option<f32>) -> ScenarioDef {
    let d = def.at_difficulty(level);
    match secs {
        Some(s) => d.with_duration(s),
        None => d,
    }
}

/// Seed for shuffling a routine's interleaved blocks.
fn routine_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    fn world() -> World {
        let mut world = World::new();
        world.insert_resource(NextState::<AppState>::default());
        world.insert_resource(ActiveRoutine::default());
        world.insert_resource(AdaptiveLevels::default());
        world.insert_resource(Library {
            scenarios: aim_core::scenario::builtin::all(),
            routines: aim_core::training::routine::builtin::all(),
            benchmarks: Vec::new(),
            methods: Vec::new(),
            errors: Vec::new(),
        });
        world
    }

    fn apply(world: &mut World, action: UiAction) {
        world
            .run_system_once(move |mut nav: Navigator| nav.apply(action.clone()))
            .unwrap();
    }

    fn routine_index(world: &World) -> Option<usize> {
        world
            .resource::<ActiveRoutine>()
            .0
            .as_ref()
            .map(|p| p.index)
    }

    fn running(world: &World) -> Option<String> {
        world
            .get_resource::<Run>()
            .map(|r| r.session.def().id.clone())
    }

    fn pending(world: &World) -> Option<AppState> {
        match world.resource::<NextState<AppState>>() {
            NextState::Pending(s) | NextState::PendingIfDifferent(s) => Some(s.clone()),
            NextState::Unchanged => None,
        }
    }

    #[test]
    fn starting_a_routine_runs_its_first_scenario() {
        let mut w = world();
        apply(&mut w, UiAction::StartRoutine("quick-warmup".into()));
        let first = w.resource::<ActiveRoutine>().0.as_ref().unwrap().plan[0]
            .scenario_id
            .clone();
        assert_eq!(routine_index(&w), Some(0));
        assert_eq!(running(&w), Some(first));
        assert_eq!(pending(&w), Some(AppState::Playing));
    }

    #[test]
    fn next_routine_run_advances_and_stops_at_the_end() {
        let mut w = world();
        apply(&mut w, UiAction::StartRoutine("quick-warmup".into()));
        let plan = w
            .resource::<ActiveRoutine>()
            .0
            .as_ref()
            .unwrap()
            .plan
            .clone();
        apply(&mut w, UiAction::NextRoutineRun);
        assert_eq!(routine_index(&w), Some(1));
        assert_eq!(running(&w), Some(plan[1].scenario_id.clone()));
        for _ in 2..plan.len() {
            apply(&mut w, UiAction::NextRoutineRun);
        }
        let last = plan.len() - 1;
        assert_eq!(routine_index(&w), Some(last));
        apply(&mut w, UiAction::NextRoutineRun);
        assert_eq!(
            routine_index(&w),
            Some(last),
            "no run past the end of the plan"
        );
        assert_eq!(running(&w), Some(plan[last].scenario_id.clone()));
    }

    #[test]
    fn next_routine_run_with_an_unknown_scenario_keeps_the_index() {
        let mut w = world();
        apply(&mut w, UiAction::StartRoutine("quick-warmup".into()));
        let current = running(&w);
        w.resource_mut::<ActiveRoutine>().0.as_mut().unwrap().plan[1].scenario_id =
            "removed-scenario".into();
        apply(&mut w, UiAction::NextRoutineRun);
        assert_eq!(
            routine_index(&w),
            Some(0),
            "the index must not skip an unplayable run"
        );
        assert_eq!(running(&w), current, "the current run stays in place");
    }

    #[test]
    fn retry_and_single_scenarios_leave_the_routine() {
        let mut w = world();
        apply(&mut w, UiAction::StartRoutine("quick-warmup".into()));
        apply(&mut w, UiAction::Retry("micro-adjust".into()));
        assert_eq!(routine_index(&w), None);
        assert_eq!(running(&w).as_deref(), Some("micro-adjust"));
        apply(&mut w, UiAction::StartScenario("flick-wide".into()));
        assert_eq!(running(&w).as_deref(), Some("flick-wide"));
    }

    #[test]
    fn unknown_ids_change_nothing() {
        let mut w = world();
        apply(&mut w, UiAction::StartScenario("does-not-exist".into()));
        apply(&mut w, UiAction::StartRoutine("does-not-exist".into()));
        assert_eq!(running(&w), None);
        assert_eq!(routine_index(&w), None);
        assert_eq!(pending(&w), None);
    }

    #[test]
    fn resume_unpauses_and_back_to_menu_clears_the_routine() {
        let mut w = world();
        apply(&mut w, UiAction::StartRoutine("quick-warmup".into()));
        w.resource_mut::<Run>().paused = true;
        apply(&mut w, UiAction::Resume);
        assert!(!w.resource::<Run>().paused);
        apply(&mut w, UiAction::BackToMenu);
        assert_eq!(routine_index(&w), None);
        assert_eq!(pending(&w), Some(AppState::Menu));
    }

    #[test]
    fn adaptive_runs_use_the_stored_level_and_are_flagged() {
        let mut w = world();
        w.resource_mut::<AdaptiveLevels>()
            .record("micro-adjust", 1.0);
        apply(&mut w, UiAction::StartAdaptive("micro-adjust".into()));
        let run = w.resource::<Run>();
        assert!(run.adaptive);
        assert_eq!(run.session.def().id, "micro-adjust@+0.25");
    }

    #[test]
    fn retry_keeps_the_difficulty_of_a_keyed_result() {
        let mut w = world();
        apply(&mut w, UiAction::Retry("flick-wide@+0.50".into()));
        assert_eq!(running(&w).as_deref(), Some("flick-wide@+0.50"));
    }

    #[test]
    fn generated_routines_start_without_being_in_the_library() {
        let mut w = world();
        let mut r = aim_core::training::routine::builtin::all().remove(0);
        r.id = "personalized".into();
        apply(&mut w, UiAction::StartRoutineDef(Box::new(r)));
        assert_eq!(
            w.resource::<ActiveRoutine>()
                .0
                .as_ref()
                .map(|p| p.routine_id.as_str()),
            Some("personalized")
        );
        assert!(running(&w).is_some());
    }

    #[test]
    fn method_days_start_at_the_tier_level_and_resume_mid_plan() {
        let mut w = world();
        let action = UiAction::StartRoutineAt {
            routine: "skill-clicking-static".into(),
            level: 0.5,
            index: 3,
            seed: 9,
        };
        apply(&mut w, action);
        let p = w.resource::<ActiveRoutine>().0.clone().unwrap();
        assert_eq!(p.index, 3);
        let expected = p.plan[3].level(0.5);
        let (_, level, _) = parse_run_key_full(&running(&w).unwrap());
        assert!((level - expected).abs() < 1e-3, "{level} vs {expected}");
    }

    #[test]
    fn timed_runs_are_shortened_and_can_auto_hide() {
        let mut w = world();
        let action = UiAction::StartTimed {
            scenario: "micro-adjust".into(),
            secs: 20.0,
            auto_hide: true,
        };
        apply(&mut w, action);
        let run = w.resource::<Run>();
        assert!(run.auto_hide);
        assert_eq!(run.session.def().duration_secs, 20.0);
        assert_eq!(run.session.def().id, "micro-adjust~20s");
    }

    #[test]
    fn queue_warmups_play_short_runs_in_order() {
        let mut w = world();
        let r = aim_core::training::warmup::queue_warmup(3.0);
        apply(&mut w, UiAction::StartRoutineDef(Box::new(r)));
        let first = running(&w).unwrap();
        assert!(first.starts_with("track-precise@-0.50~30s"), "{first}");
        apply(&mut w, UiAction::Retry(first.clone()));
        assert_eq!(running(&w), Some(first), "retry keeps level and duration");
    }

    #[test]
    fn arenas_and_scenario_runs_replace_each_other() {
        let mut w = world();
        apply(&mut w, UiAction::StartScenario("micro-adjust".into()));
        assert!(w.contains_resource::<Run>());
        apply(&mut w, UiAction::StartArena("yard-deathmatch".into()));
        assert!(w.contains_resource::<ArenaRun>());
        assert!(
            !w.contains_resource::<Run>(),
            "starting an arena drops the scenario run"
        );
        assert_eq!(pending(&w), Some(AppState::Playing));
        apply(&mut w, UiAction::StartScenario("micro-adjust".into()));
        assert!(
            !w.contains_resource::<ArenaRun>(),
            "starting a scenario drops the arena"
        );
        apply(&mut w, UiAction::StartArena("no-such-arena".into()));
        assert!(!w.contains_resource::<ArenaRun>());
    }
}
