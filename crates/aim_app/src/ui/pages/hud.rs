//! In-run HUD: crosshair, status, off-screen target arrows, movement meter, countdown and
//! pause menu.

use crate::game::AppState;
use crate::game::MainCamera;
use crate::game::arena::ArenaRun;
use crate::overlay::Overlay;
use crate::persistence::settings::Settings;
use crate::ui::atoms::crosshair;
use crate::ui::navigation::Navigator;
use crate::ui::organisms::hud_panels;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};

pub struct HudPage;

impl Plugin for HudPage {
    fn build(&self, app: &mut App) {
        app.add_systems(
            EguiPrimaryContextPass,
            hud.run_if(in_state(AppState::Playing)),
        );
    }
}

fn hud(
    mut contexts: EguiContexts,
    settings: Res<Settings>,
    overlay: Res<Overlay>,
    arena: Option<ResMut<ArenaRun>>,
    camera: Query<&Projection, With<MainCamera>>,
    mut nav: Navigator,
) -> Result {
    if !overlay.visible {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    if let Some(mut arena) = arena {
        crosshair::draw(ctx, &settings.crosshair, arena.hit_flash > 0.0);
        hud_panels::arena_status(ctx, &arena.session, arena.damage_flash);
        if arena.countdown > 0.0 {
            hud_panels::countdown(ctx, arena.countdown);
        }
        let action = arena
            .paused
            .then(|| hud_panels::pause_window(ctx, &arena.session.def().description))
            .flatten();
        match action {
            Some(crate::ui::UiAction::Resume) => arena.paused = false,
            Some(other) => nav.apply(other),
            None => {}
        }
        return Ok(());
    }
    let Some(run) = nav.run() else { return Ok(()) };
    if settings.offscreen_arrows
        && !run.paused
        && let Ok(Projection::Perspective(p)) = camera.single()
    {
        let cam = hud_panels::ArrowCamera {
            eye: run.session.eye_position(),
            view: run.view,
            vfov_deg: p.fov.to_degrees(),
            aspect: p.aspect_ratio,
        };
        hud_panels::offscreen_arrows(ctx, &run.session, &cam, settings.target_color);
    }
    crosshair::draw(ctx, &settings.crosshair, run.hit_flash > 0.0);
    hud_panels::run_status(ctx, &run.session);
    if run.session.def().player.movement {
        hud_panels::movement_meter(ctx, &run.session);
    }
    if run.session.recoil_profile().is_some() {
        hud_panels::spray_meter(ctx, &run.session);
    }
    if run.countdown > 0.0 {
        hud_panels::countdown(ctx, run.countdown);
    }
    let action = run
        .paused
        .then(|| hud_panels::pause_window(ctx, &run.session.def().description))
        .flatten();
    if let Some(action) = action {
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
        assert_is_system::<(), Result, _>(super::hud);
    }
}
