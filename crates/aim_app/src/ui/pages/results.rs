//! Results screen shown after a run finishes.

use crate::game::{AppState, LastResult};
use crate::overlay::Overlay;
use crate::ui::navigation::Navigator;
use crate::ui::organisms::results_card::{ResultsContext, results_card};
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

fn results(
    mut contexts: EguiContexts,
    last: Option<Res<LastResult>>,
    overlay: Res<Overlay>,
    keys: Res<ButtonInput<KeyCode>>,
    mut nav: Navigator,
) -> Result {
    if !overlay.visible {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let Some(last) = last else {
        nav.apply(crate::ui::UiAction::BackToMenu);
        return Ok(());
    };
    let next_run = nav
        .routine()
        .and_then(|p| p.plan.get(p.index + 1))
        .and_then(|p| nav.library().scenario(&p.scenario_id))
        .map(|d| d.name.clone());
    let rc = ResultsContext {
        routine: nav.routine(),
        next_run: next_run.as_deref(),
        space: keys.just_pressed(KeyCode::Space),
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
