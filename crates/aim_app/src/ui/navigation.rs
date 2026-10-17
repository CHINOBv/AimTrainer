//! Applies [`UiAction`]s: the single place where UI intent turns into state changes.

use super::UiAction;
use crate::game::{ActiveRoutine, AppState, RoutineProgress, Run, start_run};
use crate::persistence::library::Library;
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
                self.start(&id);
            }
            UiAction::StartRoutine(id) => {
                let Some(r) = self.library.routines.iter().find(|r| r.id == id) else {
                    return;
                };
                let plan = r.plan(&self.library.scenarios, routine_seed());
                let Some(first) = plan.first().map(|p| p.scenario_id.clone()) else {
                    return;
                };
                self.routine.0 = Some(RoutineProgress {
                    name: r.name.clone(),
                    plan,
                    index: 0,
                });
                self.start(&first);
            }
            UiAction::NextRoutineRun => {
                let Some(p) = self.routine.0.as_mut() else {
                    return;
                };
                let Some(id) = p.plan.get(p.index + 1).map(|n| n.scenario_id.clone()) else {
                    return;
                };
                p.index += 1;
                self.start(&id);
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

    fn start(&mut self, scenario_id: &str) {
        if let Some(def) = self.library.scenario(scenario_id) {
            start_run(&mut self.commands, &mut self.next, def.clone());
        }
    }
}

/// Seed for shuffling a routine's interleaved blocks.
fn routine_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}
