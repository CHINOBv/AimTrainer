//! In-run HUD: crosshair, status, off-screen target arrows, movement meter, countdown, pause
//! menu, and the arena's game-feel effects (damage direction, kill feed, death screen, round
//! transitions).

use crate::game::AppState;
use crate::game::MainCamera;
use crate::game::SensDrill;
use crate::game::arena::{ArenaFeedback, ArenaRun};
use crate::game::feedback::{
    HitFrame, HitMarkerStyle, health_fraction, kill_flash_alpha, marker_look,
};
use crate::overlay::Overlay;
use crate::persistence::settings::{Settings, Toggle};
use crate::platform::game_key;
use crate::ui::UiAction;
use crate::ui::atoms::{crosshair, hit_marker};
use crate::ui::navigation::Navigator;
use crate::ui::organisms::damage_indicator::{self, RecentHits, screen_bearing};
use crate::ui::organisms::death_screen::{self, DeathView, ShotSide, shot_side};
use crate::ui::organisms::health_bar::{HealthBar, HealthGhost, low_hp_pulse};
use crate::ui::organisms::kill_feed::{self, KillFeed};
use crate::ui::organisms::radar_overlay::{self, RadarPanel};
use crate::ui::organisms::target_overlays::{self, BarView};
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
        .init_resource::<ArenaFx>()
        .add_systems(OnEnter(AppState::Playing), reset_arena_fx)
        .add_systems(
            EguiPrimaryContextPass,
            (
                track_arena_fx.before(hud),
                arena_fx_hud.after(track_arena_fx),
            )
                .run_if(in_state(AppState::Playing)),
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
        if settings.feel.arena_arrows
            && !arena.paused
            && let Ok(Projection::Perspective(p)) = camera.single()
        {
            let cam = hud_panels::ArrowCamera {
                eye: arena.session.eye_position(),
                view: arena.view,
                vfov_deg: p.fov.to_degrees(),
                aspect: p.aspect_ratio,
            };
            let points = hud_panels::arena_arrow_points(arena.session.bots());
            hud_panels::offscreen_arrows(ctx, &points, &cam, settings.target_color);
        }
        hit_feedback(ctx, &settings, arena.hit_frame());
        if arena.countdown > 0.0 {
            hud_panels::countdown(
                ctx,
                arena.countdown,
                restart,
                settings.feel.round_transitions,
            );
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
        let points = hud_panels::scenario_arrow_points(&run.session);
        hud_panels::offscreen_arrows(ctx, &points, &cam, settings.target_color);
    }
    hit_feedback(ctx, &settings, run.hit_frame());
    let shown = run.session.def().feedback;
    if (shown.damage_numbers || shown.health_bars)
        && let Ok(Projection::Perspective(p)) = camera.single()
    {
        let cam = hud_panels::ArrowCamera {
            eye: run.session.eye_position(),
            view: run.view,
            vfov_deg: p.fov.to_degrees(),
            aspect: p.aspect_ratio,
        };
        let bars = if shown.health_bars {
            health_bars(&run.session)
        } else {
            Vec::new()
        };
        target_overlays::target_overlays(ctx, &cam, &run.damage_pops, &bars);
    }
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
        hud_panels::countdown(ctx, run.countdown, restart, settings.feel.round_transitions);
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

/// A health bar for every visible target whose starting health is above 1.
fn health_bars(s: &aim_core::scenario::Session) -> Vec<BarView> {
    let tops = crate::game::target_tops(s);
    s.targets()
        .filter_map(|t| {
            let fraction = health_fraction(t.health, t.max_health)?;
            let &(_, top) = tops.iter().find(|(id, _)| *id == t.id)?;
            Some(BarView { top, fraction })
        })
        .collect()
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

/// The crosshair and the hit marker of `frame` (head, body and kill markers differ with
/// `feel.distinct_markers`), the crosshair bloom and the arena kill flash.
fn hit_feedback(ctx: &bevy_egui::egui::Context, settings: &Settings, frame: HitFrame) {
    let fb = &settings.hit_feedback;
    let showing = frame.marker_secs_left > 0.0;
    let look = marker_look(
        frame.marker,
        fb.marker,
        fb.marker_color,
        settings.feel.distinct_markers,
    );
    let flash = (showing && fb.marker == HitMarkerStyle::Flash).then(|| {
        let [r, g, b] = look.flash_rgb;
        aim_core::crosshair::Rgba::opaque(r, g, b)
    });
    let bloom = if settings.feel.crosshair_bloom {
        crosshair::bloom_px(frame.inaccuracy)
    } else {
        0
    };
    crosshair::draw(ctx, &settings.crosshair, flash, bloom);
    if showing {
        let t = 1.0 - frame.marker_secs_left / fb.marker_secs();
        hit_marker::draw(ctx, &look, settings.crosshair.color(), t);
    }
    if frame.kill_flash_secs > 0.0 {
        hit_marker::draw_kill_flash(ctx, kill_flash_alpha(frame.kill_flash_secs));
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

/// Where the player stood and looked in a frame.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pose {
    eye: Vec3,
    yaw_deg: f32,
}

/// The death screen's state: where the killing shot came from and how long ago.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Death {
    side: Option<ShotSide>,
    age: f32,
}

/// What the running arena reports for one frame, for [`ArenaFx::update`].
#[derive(Debug, Clone, Copy, PartialEq)]
struct ArenaFrame {
    /// Seconds since the last frame; 0 while paused.
    dt: f32,
    pose: Pose,
    health: f32,
    /// Seconds of spawn protection left.
    protection: f32,
}

/// The arena HUD's timed effects, fed by [`ArenaFeedback`]. Display state only: it decides
/// no gameplay. Reset whenever `Playing` is entered (a new run or a restart).
#[derive(Resource, Debug, Default)]
pub struct ArenaFx {
    /// Seconds the arena has been live, for pulses.
    clock: f32,
    hits: RecentHits,
    kills: KillFeed,
    ghost: HealthGhost,
    death: Option<Death>,
    go_age: Option<f32>,
    end_age: Option<f32>,
    /// The previous frame's pose. A death respawns the player within the frame, so the
    /// killing shot's side is judged from where they stood before it.
    pose: Option<Pose>,
}

impl ArenaFx {
    fn update<'a>(
        &mut self,
        frame: ArenaFrame,
        feedback: impl IntoIterator<Item = &'a ArenaFeedback>,
    ) {
        let dt = frame.dt;
        self.clock += dt;
        self.hits.advance(dt);
        self.kills.advance(dt);
        let age = |a: Option<f32>, secs: f32| a.map(|a| a + dt).filter(|a| *a < secs);
        self.go_age = age(self.go_age, hud_panels::GO_SECS);
        self.end_age = age(self.end_age, hud_panels::ROUND_END_SECS);
        if let Some(d) = &mut self.death {
            d.age += dt;
        }
        // The death screen lasts as long as the respawn's spawn protection.
        self.death
            .take_if(|d| d.age > 0.0 && frame.protection <= 0.0);
        let before = self.pose.unwrap_or(frame.pose);
        for f in feedback {
            match *f {
                ArenaFeedback::PlayerHit { from, damage, .. } => self.hits.push(from, damage),
                ArenaFeedback::PlayerDied { from } => {
                    self.hits.clear();
                    self.kills.on_death();
                    self.death = Some(Death {
                        side: from.map(|from| {
                            shot_side(screen_bearing(before.eye, before.yaw_deg, from))
                        }),
                        age: 0.0,
                    });
                }
                ArenaFeedback::BotKilled {
                    bot_id,
                    headshot,
                    streak,
                } => self.kills.on_kill(bot_id, headshot, streak),
                ArenaFeedback::RoundStarted => self.go_age = Some(0.0),
                ArenaFeedback::RoundEnded => self.end_age = Some(0.0),
                ArenaFeedback::CountdownTick { .. } | ArenaFeedback::BotFired { .. } => {}
            }
        }
        self.ghost.update(frame.health, dt);
        self.pose = Some(frame.pose);
    }

    fn health_bar(&self, health: f32, pulse: bool) -> HealthBar {
        HealthBar {
            health,
            ghost: self.ghost.value().max(health),
            max: aim_world::arena::PLAYER_HEALTH,
            pulse: if pulse {
                low_hp_pulse(health, self.clock)
            } else {
                0.0
            },
        }
    }
}

/// An effect's intensity as a factor (1 = 100%).
fn strength(t: Toggle) -> f32 {
    f32::from(t.intensity_percent) / 100.0
}

fn reset_arena_fx(mut fx: ResMut<ArenaFx>) {
    *fx = ArenaFx::default();
}

/// Feed this frame's arena feedback into [`ArenaFx`]. Effects stand still while paused.
fn track_arena_fx(
    time: Res<Time>,
    arena: Option<Res<ArenaRun>>,
    mut feedback: MessageReader<ArenaFeedback>,
    mut fx: ResMut<ArenaFx>,
) {
    let Some(arena) = arena else {
        feedback.clear();
        return;
    };
    let s = &arena.session;
    let eye = s.eye_position();
    let frame = ArenaFrame {
        dt: if arena.paused { 0.0 } else { time.delta_secs() },
        pose: Pose {
            eye: Vec3::new(eye.x, eye.y, eye.z),
            yaw_deg: arena.view.yaw_deg,
        },
        health: s.player_health(),
        protection: s.spawn_protection_secs(),
    };
    fx.update(frame, feedback.read());
}

/// The arena status (time, health, kills) and its game-feel overlays, each behind its
/// `[feel]` setting.
fn arena_fx_hud(
    mut contexts: EguiContexts,
    overlay: Res<Overlay>,
    settings: Res<Settings>,
    arena: Option<Res<ArenaRun>>,
    fx: Res<ArenaFx>,
) -> Result {
    let Some(arena) = arena.filter(|_| overlay.visible) else {
        return Ok(());
    };
    let ctx = contexts.ctx_mut()?;
    let feel = &settings.feel;
    let s = &arena.session;
    let eye = s.eye_position();
    let eye = Vec3::new(eye.x, eye.y, eye.z);
    let yaw = arena.view.yaw_deg;
    let extras = hud_panels::ArenaStatusExtras {
        health_bar: feel
            .health_bar
            .then(|| fx.health_bar(s.player_health(), feel.low_hp_pulse)),
        death_screen: feel.death_screen && fx.death.is_some(),
    };
    hud_panels::arena_status(ctx, s, &extras);
    if feel.damage_vignette.enabled {
        let alpha = damage_indicator::strongest_vignette(&fx.hits, strength(feel.damage_vignette));
        damage_indicator::draw_vignette(ctx, alpha);
    }
    if feel.screen_punch.enabled {
        damage_indicator::draw_punch(ctx, &fx.hits, eye, yaw, strength(feel.screen_punch));
    }
    if feel.damage_indicator.enabled {
        let k = strength(feel.damage_indicator);
        damage_indicator::draw_indicators(ctx, &fx.hits, eye, yaw, k);
    }
    if feel.kill_feed {
        kill_feed::draw_feed(ctx, &fx.kills);
    }
    if feel.streak_banner {
        kill_feed::draw_banner(ctx, &fx.kills);
    }
    if let Some(death) = fx.death.filter(|_| feel.death_screen) {
        let view = DeathView {
            side: death.side,
            age: death.age,
            respawn_secs: s.spawn_protection_secs(),
        };
        death_screen::draw(ctx, &view);
    }
    if feel.round_transitions {
        if let Some(age) = fx.go_age {
            hud_panels::go_pop(ctx, age);
        }
        if let Some(age) = fx.end_age {
            let stats = s.stats();
            hud_panels::round_end_banner(ctx, age, stats.kills, stats.deaths);
        }
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
        assert_is_system::<(), Result, _>(super::sens_drill_hud);
        assert_is_system::<(), Result, _>(super::coaching_hud);
        assert_is_system::<(), Result, _>(super::arena_fx_hud);
        assert_is_system::<(), (), _>(super::track_arena_fx);
        assert_is_system::<(), (), _>(super::reset_arena_fx);
    }

    use super::{ArenaFeedback, ArenaFrame, ArenaFx, Pose, ShotSide};
    use bevy::prelude::Vec3;

    fn frame(yaw_deg: f32, health: f32, protection: f32) -> ArenaFrame {
        ArenaFrame {
            dt: 0.1,
            pose: Pose {
                eye: Vec3::new(0.0, 1.6, 0.0),
                yaw_deg,
            },
            health,
            protection,
        }
    }

    #[test]
    fn the_killing_shot_side_uses_the_pose_before_the_respawn() {
        let mut fx = ArenaFx::default();
        fx.update(frame(0.0, 100.0, 0.0), []);
        let hit = ArenaFeedback::PlayerHit {
            from: Vec3::new(5.0, 1.6, 0.0),
            damage: 40.0,
            zone: None,
        };
        fx.update(frame(0.0, 60.0, 0.0), [&hit]);
        assert_eq!(fx.hits.iter().count(), 1);
        // The respawn turned the player around within the frame of the death.
        let died = ArenaFeedback::PlayerDied {
            from: Some(Vec3::new(5.0, 1.6, 0.0)),
        };
        fx.update(frame(180.0, 100.0, 1.5), [&hit, &died]);
        let death = fx.death.expect("death screen");
        assert_eq!(death.side, Some(ShotSide::Right));
        assert_eq!(fx.hits.iter().count(), 0, "a death clears the arcs");
        assert_eq!(fx.ghost.value(), 100.0, "the respawn refills the ghost");
        // It stays while the spawn protection lasts, then goes.
        fx.update(frame(180.0, 100.0, 0.5), []);
        assert!(fx.death.is_some());
        fx.update(frame(180.0, 100.0, 0.0), []);
        assert!(fx.death.is_none());
    }

    #[test]
    fn kills_and_round_cues_start_their_effects() {
        let mut fx = ArenaFx::default();
        fx.update(frame(0.0, 100.0, 0.0), [&ArenaFeedback::RoundStarted]);
        assert_eq!(fx.go_age, Some(0.0));
        let kill = |bot_id, streak| ArenaFeedback::BotKilled {
            bot_id,
            headshot: true,
            streak,
        };
        fx.update(frame(0.0, 100.0, 0.0), [&kill(1, 1), &kill(2, 2)]);
        assert_eq!(fx.kills.lines().count(), 2);
        assert_eq!(fx.kills.banner().map(|(t, _)| t), Some("Double"));
        assert!(fx.go_age.is_some_and(|a| a > 0.0));
        fx.update(frame(0.0, 100.0, 0.0), [&ArenaFeedback::RoundEnded]);
        assert_eq!(fx.end_age, Some(0.0));
        // A paused frame ages nothing.
        let mut paused = frame(0.0, 100.0, 0.0);
        paused.dt = 0.0;
        fx.update(paused, []);
        assert_eq!(fx.end_age, Some(0.0));
    }
}
