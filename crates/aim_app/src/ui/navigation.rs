//! Applies [`UiAction`]s: the single place where UI intent turns into state changes.

use super::UiAction;
use crate::game::{ActiveRoutine, AppState, RoutineProgress, Run, start_run};
use crate::persistence::library::Library;
use aim_core::scenario::parse_run_key;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

#[derive(SystemParam)]
pub struct Navigator<'w, 's> {
    commands: Commands<'w, 's>,
    next: ResMut<'w, NextState<AppState>>,
    routine: ResMut<'w, ActiveRoutine>,
    library: Res<'w, Library>,
    run: Option<ResMut<'w, Run>>,
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
                let (base, level) = parse_run_key(&id);
                let base = base.to_owned();
                self.start(&base, level);
            }
            UiAction::StartRoutine(id) => self.start_routine(&id, 0.0, 0, routine_seed()),
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
                let Some(def) = self.library.scenario(&next.scenario_id) else {
                    return;
                };
                let def = def.at_difficulty(level);
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

    fn start(&mut self, scenario_id: &str, level: f32) {
        if let Some(def) = self.library.scenario(scenario_id) {
            start_run(&mut self.commands, &mut self.next, def.at_difficulty(level));
        }
    }

    /// Start `routine` at `level`, from run `index` of the plan shuffled with `seed`.
    fn start_routine(&mut self, routine: &str, level: f32, index: usize, seed: u64) {
        let Some(r) = self.library.routines.iter().find(|r| r.id == routine) else {
            return;
        };
        let plan = r.plan(&self.library.scenarios, seed);
        let Some(run) = plan.get(index) else { return };
        let (scenario, run_level) = (run.scenario_id.clone(), run.level(level));
        self.routine.0 = Some(RoutineProgress {
            routine_id: r.id.clone(),
            name: r.name.clone(),
            plan,
            index,
            base_level: level,
            seed,
        });
        self.start(&scenario, run_level);
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
}
