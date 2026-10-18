//! In-run HUD: crosshair, status, off-screen target arrows, movement meter, countdown and
//! pause menu.

use crate::game::AppState;
use crate::game::MainCamera;
use crate::game::feedback::HitMarkerStyle;
use crate::overlay::Overlay;
use crate::persistence::settings::Settings;
use crate::platform::game_key;
use crate::ui::UiAction;
use crate::ui::atoms::{crosshair, hit_marker};
use crate::ui::navigation::Navigator;
use crate::ui::organisms::hud_panels;
use aim_core::scenario::RunKey;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};

pub struct HudPage;

/// Floor height picked in the pause window, for the run it was picked in.
#[derive(Default)]
struct FloorChoice {
    run_id: String,
    chosen: f32,
}

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
    keys: Res<ButtonInput<KeyCode>>,
    camera: Query<&Projection, With<MainCamera>>,
    mut floor: Local<FloorChoice>,
    mut nav: Navigator,
) -> Result {
    if !overlay.visible {
        return Ok(());
    }
    // The restart key works during the countdown, the run and the pause window alike.
    if game_key::bindable(&settings.restart_key).is_some_and(|k| keys.just_pressed(k)) {
        nav.apply(UiAction::Restart);
        return Ok(());
    }
    let restart = game_key::label(&settings.restart_key);
    let ctx = contexts.ctx_mut()?;
    if let Some(arena) = nav.arena() {
        hit_feedback(ctx, &settings, arena.hit_flash);
        hud_panels::arena_status(ctx, &arena.session, arena.damage_flash);
        if arena.countdown > 0.0 {
            hud_panels::countdown(ctx, arena.countdown, restart);
        }
        let action = arena
            .paused
            .then(|| hud_panels::pause_window(ctx, &arena.session.def().description, restart, None))
            .flatten();
        if let Some(action) = action {
            nav.apply(action);
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
    hit_feedback(ctx, &settings, run.hit_flash);
    hud_panels::run_status(ctx, &run.session);
    if run.session.def().player.movement {
        hud_panels::movement_meter(ctx, &run.session);
    }
    if run.session.recoil_profile().is_some() {
        hud_panels::spray_meter(ctx, &run.session);
    }
    if run.countdown > 0.0 {
        hud_panels::countdown(ctx, run.countdown, restart);
    }
    let def = run.session.def();
    let current = def.player.elevation;
    // A new run (or a restart) resets the picker to the height being played.
    if floor.run_id != def.id {
        floor.run_id = def.id.clone();
        floor.chosen = current;
    }
    // Only library scenarios can be rebuilt at another height (not e.g. daily challenges).
    let restartable = nav
        .library()
        .scenario(RunKey::parse(&def.id).base)
        .is_some();
    let pick = restartable.then(|| hud_panels::FloorPick {
        current,
        chosen: &mut floor.chosen,
    });
    let action = run
        .paused
        .then(|| hud_panels::pause_window(ctx, &def.description, restart, pick))
        .flatten();
    if let Some(action) = action {
        nav.apply(action);
    }
    Ok(())
}

/// The crosshair and the hit marker, `marker_secs_left` seconds before the marker ends.
fn hit_feedback(ctx: &bevy_egui::egui::Context, settings: &Settings, marker_secs_left: f32) {
    let fb = &settings.hit_feedback;
    let showing = marker_secs_left > 0.0;
    let flash = showing && fb.marker == HitMarkerStyle::Flash;
    crosshair::draw(ctx, &settings.crosshair, flash);
    if showing {
        let t = 1.0 - marker_secs_left / fb.marker_secs();
        let color = settings.crosshair.color();
        hit_marker::draw(ctx, fb.marker, fb.marker_color, color, t);
    }
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
