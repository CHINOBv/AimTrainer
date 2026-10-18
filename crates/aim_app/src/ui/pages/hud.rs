//! In-run HUD: crosshair, status, off-screen target arrows, movement meter, countdown and
//! pause menu.

use crate::game::AppState;
use crate::game::MainCamera;
use crate::game::SensDrill;
use crate::game::arena::ArenaRun;
use crate::game::feedback::HitMarkerStyle;
use crate::overlay::Overlay;
use crate::persistence::settings::Settings;
use crate::platform::game_key;
use crate::ui::UiAction;
use crate::ui::atoms::{crosshair, hit_marker};
use crate::ui::navigation::Navigator;
use crate::ui::organisms::radar_overlay::{self, RadarPanel};
use crate::ui::organisms::{coaching_overlay, hud_panels};
use aim_core::scenario::{RunKey, Tuning};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};

pub struct HudPage;

/// Floor height and tuning picked in the pause window, for the run they were picked in.
#[derive(Default)]
struct RestartChoice {
    run_id: String,
    elevation: f32,
    tuning: Tuning,
}

impl Plugin for HudPage {
    fn build(&self, app: &mut App) {
        app.add_systems(
            EguiPrimaryContextPass,
            (hud, sens_drill_hud).run_if(in_state(AppState::Playing)),
        )
        .add_systems(
            EguiPrimaryContextPass,
            coaching_hud.run_if(in_state(AppState::Playing)),
        );
    }
}

fn hud(
    mut contexts: EguiContexts,
    settings: Res<Settings>,
    overlay: Res<Overlay>,
    keys: Res<ButtonInput<KeyCode>>,
    camera: Query<&Projection, With<MainCamera>>,
    mut choice: Local<RestartChoice>,
    mut nav: Navigator,
) -> Result {
    if !overlay.visible {
        return Ok(());
    }
    // The restart key works during the countdown, the run and the pause window alike. A
    // reserved or unknown stored key falls back to the default, and the HUD shows that one.
    let restart_name = game_key::effective_restart(&settings.restart_key);
    if game_key::key_code(restart_name)
        .is_some_and(|k| game_key::plain_press(keys.just_pressed(k), |m| keys.pressed(m)))
    {
        nav.apply(UiAction::Restart);
        return Ok(());
    }
    let restart = game_key::label(restart_name);
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
    // The session stands still during the countdown and the pause: hide the radar then, so
    // it is never on screen longer than `show_secs`.
    let radar_live = run.countdown <= 0.0 && !run.paused;
    if let Some(radar) = run.session.radar().filter(|_| radar_live) {
        radar_overlay::radar_overlay(ctx, &RadarPanel::new(&radar, run.view.yaw_deg));
    }
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
    let key = RunKey::parse(&def.id);
    // A new run (or a restart) resets the pickers to what is being played.
    if choice.run_id != def.id {
        choice.run_id = def.id.clone();
        choice.elevation = def.player.elevation;
        choice.tuning = key.tuning;
    }
    // Only library scenarios can be rebuilt (not e.g. daily challenges).
    let restartable = nav.library().scenario(key.base).is_some();
    let choice = &mut *choice;
    let pick = restartable.then(|| hud_panels::RestartPick {
        current_elevation: def.player.elevation,
        elevation: &mut choice.elevation,
        current_tuning: key.tuning,
        tuning: &mut choice.tuning,
        size_applies: Tuning::size_applies(def),
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

/// The sensitivity randomiser's current multiplier, bottom left, while the drill runs.
fn sens_drill_hud(
    mut contexts: EguiContexts,
    overlay: Res<Overlay>,
    drill: Res<SensDrill>,
) -> Result {
    if !overlay.visible || drill.0.is_none() {
        return Ok(());
    }
    let multiplier = drill.multiplier();
    use bevy_egui::egui;
    egui::Area::new(egui::Id::new("sens-drill"))
        .anchor(egui::Align2::LEFT_BOTTOM, [24.0, -24.0])
        .interactable(false)
        .show(contexts.ctx_mut()?, |ui| {
            ui.label(crate::ui::atoms::big(
                format!("Sens ×{multiplier:.2}"),
                18.0,
            ));
        });
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

/// The opt-in live coaching overlay of a running arena (crosshair placement against head
/// height at the nearest exposed corner, and movement accuracy). Drawing only: the values
/// come from `ArenaSession::coaching`.
fn coaching_hud(
    mut contexts: EguiContexts,
    overlay: Res<Overlay>,
    settings: Res<Settings>,
    arena: Option<Res<ArenaRun>>,
) -> Result {
    let Some(arena) = arena else { return Ok(()) };
    if !(settings.arena_coaching && overlay.visible && !arena.paused && arena.countdown <= 0.0) {
        return Ok(());
    }
    coaching_overlay::coaching_overlay(contexts.ctx_mut()?, &arena.session.coaching());
    Ok(())
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::assert_is_system;
    use bevy::prelude::Result;

    #[test]
    fn systems_have_no_conflicting_access() {
        assert_is_system::<(), Result, _>(super::hud);
        assert_is_system::<(), Result, _>(super::sens_drill_hud);
        assert_is_system::<(), Result, _>(super::coaching_hud);
    }
}
