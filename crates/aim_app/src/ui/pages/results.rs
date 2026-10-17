//! Results screen shown after a run finishes. When a new result appears, the page saves
//! its replay and analyzes it once; both are cached until the next result.

use crate::game::{AppState, LastResult};
use crate::overlay::Overlay;
use crate::persistence::library::Library;
use crate::persistence::replays;
use crate::persistence::settings::Paths;
use crate::ui::navigation::Navigator;
use crate::ui::organisms::analysis_panel::RunReport;
use crate::ui::organisms::results_card::{ResultsContext, results_card};
use aim_core::analytics::analyze;
use aim_core::scenario::Recording;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};

pub struct ResultsPage;

impl Plugin for ResultsPage {
    fn build(&self, app: &mut App) {
        app.add_systems(
            EguiPrimaryContextPass,
            results.run_if(in_state(AppState::Results)),
        );
    }
}

/// Identifies one result: a new run has a new seed or input log.
type ResultKey = (String, u64, usize);

fn result_key(rec: &Recording) -> ResultKey {
    (rec.def.id.clone(), rec.seed, rec.inputs.len())
}

/// The analysis of the result currently shown.
#[derive(Default)]
struct ReportCache(Option<(ResultKey, RunReport)>);

/// Save the replay and analyze the run the first time a result is seen.
fn refresh(cache: &mut ReportCache, last: &LastResult, paths: &Paths, lib: &Library) {
    let key = result_key(&last.recording);
    if cache.0.as_ref().is_some_and(|(k, _)| *k == key) {
        return;
    }
    if !last.recording.inputs.is_empty()
        && let Err(e) = replays::save(paths, &last.recording)
    {
        bevy::log::error!("could not save replay: {e}");
    }
    let report = RunReport::new(&analyze(&last.recording), |id| {
        lib.scenario(id).map(|d| d.name.clone())
    });
    cache.0 = Some((key, report));
}

fn results(
    mut contexts: EguiContexts,
    last: Option<Res<LastResult>>,
    overlay: Res<Overlay>,
    keys: Res<ButtonInput<KeyCode>>,
    paths: Res<Paths>,
    mut cache: Local<ReportCache>,
    mut nav: Navigator,
) -> Result {
    // Before the visibility check, so a run finished while the overlay is hidden is still
    // saved as soon as its result exists.
    if let Some(last) = &last {
        refresh(&mut cache, last, &paths, nav.library());
    }
    if !overlay.visible {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let Some(last) = last else {
        nav.apply(crate::ui::UiAction::BackToMenu);
        return Ok(());
    };
    let report = cache.0.as_ref().map(|(_, r)| r);
    let next_run = nav
        .routine()
        .and_then(|p| p.plan.get(p.index + 1))
        .and_then(|p| nav.library().scenario(&p.scenario_id))
        .map(|d| d.name.clone());
    let rc = ResultsContext {
        routine: nav.routine(),
        next_run: next_run.as_deref(),
        space: keys.just_pressed(KeyCode::Space),
        report,
    };
    if let Some(action) = results_card(ctx, &last, &rc) {
        nav.apply(action);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::assert_is_system;
    use bevy::prelude::Result;

    #[test]
    fn systems_have_no_conflicting_access() {
        assert_is_system::<(), Result, _>(super::results);
    }
}
