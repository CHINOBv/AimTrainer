//! In-run overlays: run status, movement accuracy meter, spray meter, countdown, pause
//! window.

use crate::ui::UiAction;
use crate::ui::atoms::{BAD, GOOD, big};
use aim_core::scenario::{Session, parse_run_key};
use bevy_egui::egui::{self, Align2, FontId, RichText};

pub fn run_status(ctx: &egui::Context, s: &Session) {
    let stats = s.stats();
    let accuracy = s.result().accuracy * 100.0;
    let hold = s.def().weapon.fire == aim_core::scenario::FireMode::Hold;
    egui::Area::new(egui::Id::new("hud"))
        .anchor(Align2::CENTER_TOP, [0.0, 16.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(big(format!("{:.1}", s.remaining()), 28.0));
                let (_, level) = parse_run_key(&s.def().id);
                if level != 0.0 {
                    ui.small(format!("difficulty {level:+.2}"));
                }
                ui.label(format!(
                    "{}  ·  score {:.0}  ·  {accuracy:.0}% acc  ·  {} kills",
                    s.def().name,
                    stats.score.max(0.0),
                    stats.kills
                ));
                if hold {
                    ui.label(format!(
                        "on target {:.1}s / firing {:.1}s",
                        stats.on_target_secs, stats.hold_secs
                    ));
                }
            });
        });
}

pub fn movement_meter(ctx: &egui::Context, s: &Session) {
    let limit = s.movement_profile().accuracy_speed_limit();
    let (text, color) = if s.is_accurate() {
        ("ACCURATE", GOOD)
    } else {
        ("MOVING", BAD)
    };
    egui::Area::new(egui::Id::new("speed"))
        .anchor(Align2::CENTER_BOTTOM, [0.0, -40.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(text)
                        .color(color)
                        .strong()
                        .font(FontId::proportional(20.0)),
                );
                ui.label(format!(
                    "{:.2} m/s  (accurate ≤ {limit:.2})  ·  moving shots {}",
                    s.player_speed(),
                    s.stats().moving_shots
                ));
            });
        });
}

/// Recoil scenarios only: the bullet index of the burst in progress, a green READY when the
/// next shot is a fully recovered first bullet, and the recovery countdown otherwise.
pub fn spray_meter(ctx: &egui::Context, s: &Session) {
    let (Some(profile), Some(remaining)) = (s.recoil_profile(), s.recoil_recovery_remaining())
    else {
        return;
    };
    let bullet = s.burst_index();
    let protected = profile.protected_bullets;
    egui::Area::new(egui::Id::new("spray"))
        .anchor(Align2::RIGHT_BOTTOM, [-24.0, -24.0])
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                if bullet == 0 {
                    ui.label(
                        RichText::new("READY")
                            .color(GOOD)
                            .strong()
                            .font(FontId::proportional(20.0)),
                    );
                    ui.label("next shot: first bullet");
                } else {
                    let color = if bullet < protected { GOOD } else { BAD };
                    ui.label(
                        RichText::new(format!("BULLET {bullet}"))
                            .color(color)
                            .strong()
                            .font(FontId::proportional(20.0)),
                    );
                    ui.label(format!(
                        "reset in {remaining:.2}s  ·  protected {protected}"
                    ));
                }
                let st = s.stats();
                ui.small(format!(
                    "bursts {}  ·  avg {:.1}  ·  first bullets {:.0}%",
                    st.bursts,
                    st.avg_burst_len(),
                    st.first_bullet_share() * 100.0
                ));
            });
        });
}

/// Countdown before a run, with a reminder of the restart key (`restart`, e.g. "R").
pub fn countdown(ctx: &egui::Context, secs: f32, restart: &str) {
    egui::Area::new(egui::Id::new("countdown"))
        .anchor(Align2::CENTER_CENTER, [0.0, -80.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(big(format!("{}", secs.ceil() as i32), 56.0));
                ui.small(format!("{restart}: restart"));
            });
        });
}

/// Floor-height picker of a scenario run's pause window.
pub struct FloorPick<'a> {
    /// Height the run is played at, meters.
    pub current: f32,
    /// Height being chosen, meters; edited by the picker.
    pub chosen: &'a mut f32,
}

/// Step of the floor-height picker, meters.
const FLOOR_STEP: f32 = 0.5;

/// Pause window, with a restart button labelled with the restart key (`restart`). With
/// `floor`, it also offers a floor-height picker and "Apply and restart".
pub fn pause_window(
    ctx: &egui::Context,
    description: &str,
    restart: &str,
    floor: Option<FloorPick>,
) -> Option<UiAction> {
    let mut action = None;
    egui::Window::new("Paused")
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(description);
            ui.separator();
            if let Some(FloorPick { current, chosen }) = floor {
                let range = aim_core::scenario::def::ELEVATION_RANGE;
                ui.horizontal(|ui| {
                    ui.label("Floor height");
                    if ui.small_button("−").clicked() {
                        *chosen = (*chosen - FLOOR_STEP).max(range.min);
                    }
                    ui.monospace(format!("{:+.1} m", *chosen));
                    if ui.small_button("+").clicked() {
                        *chosen = (*chosen + FLOOR_STEP).min(range.max);
                    }
                });
                ui.small("Your floor relative to the targets' floor.");
                let changed = (*chosen - current).abs() > 1e-3;
                if ui
                    .add_enabled(changed, egui::Button::new("Apply and restart"))
                    .clicked()
                {
                    action = Some(UiAction::RestartAtElevation(*chosen));
                }
                ui.separator();
            }
            if ui.button("Resume (Esc)").clicked() {
                action = Some(UiAction::Resume);
            }
            if ui
                .button(format!("Restart ({restart})"))
                .on_hover_text("Start this run again as it is; nothing is recorded")
                .clicked()
            {
                action = Some(UiAction::Restart);
            }
            if ui.button("Quit to menu").clicked() {
                action = Some(UiAction::BackToMenu);
            }
        });
    action
}

/// While the spawn grace lasts, say why the player stands where they do: a respawn after a
/// death (deaths otherwise look like a teleport) or the protected entry spawn.
fn spawn_note(ctx: &egui::Context, s: &aim_world::arena::ArenaSession) {
    let left = s.spawn_protection_secs();
    if left <= 0.0 {
        return;
    }
    egui::Area::new(egui::Id::new("arena-spawn-note"))
        .anchor(Align2::CENTER_CENTER, [0.0, 90.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                if s.stats().deaths > 0 {
                    ui.label(big("You died · respawned", 26.0).color(BAD));
                }
                ui.label(format!("Spawn protection {left:.1} s"));
            });
        });
}

/// Arena status: time, health, kills/deaths, the site objective (when the arena has one),
/// a respawn note during the spawn grace, and a red vignette for `damage_flash` seconds.
pub fn arena_status(ctx: &egui::Context, s: &aim_world::arena::ArenaSession, damage_flash: f32) {
    let stats = s.stats();
    egui::Area::new(egui::Id::new("arena-hud"))
        .anchor(Align2::CENTER_TOP, [0.0, 16.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(big(format!("{:.1}", s.remaining()), 28.0));
                ui.label(format!(
                    "{}  ·  {} kills  ·  {} deaths",
                    s.def().name,
                    stats.kills,
                    stats.deaths
                ));
                if s.is_practice() {
                    ui.colored_label(GOOD, "PRACTICE · bots don't shoot");
                }
                if let Some(o) = s.objective() {
                    let site = if o.capture_progress > 0.0 {
                        RichText::new(format!("SITE CONTESTED {:.0}%", o.capture_progress * 100.0))
                            .color(BAD)
                            .strong()
                    } else {
                        RichText::new("Site held").color(GOOD)
                    };
                    ui.label(site);
                    ui.label(format!(
                        "{} of {} attackers left",
                        o.attackers_left, o.attackers
                    ));
                }
            });
        });
    let health = s.player_health();
    let color = if health > 50.0 { GOOD } else { BAD };
    egui::Area::new(egui::Id::new("arena-health"))
        .anchor(Align2::LEFT_BOTTOM, [24.0, -24.0])
        .show(ctx, |ui| {
            ui.label(
                RichText::new(format!("{health:.0} HP"))
                    .color(color)
                    .strong()
                    .font(FontId::proportional(26.0)),
            );
            if !s.is_accurate() {
                ui.colored_label(BAD, "MOVING");
            }
        });
    spawn_note(ctx, s);
    if damage_flash > 0.0 {
        let alpha = (damage_flash / 0.25).clamp(0.0, 1.0) * 0.35;
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("damage"),
        ));
        let rect = ctx.content_rect();
        painter.rect_stroke(
            rect.shrink(20.0),
            0.0,
            egui::Stroke::new(
                40.0,
                egui::Color32::from_rgba_unmultiplied(220, 30, 30, (alpha * 255.0) as u8),
            ),
            egui::StrokeKind::Inside,
        );
    }
}

/// Distance of an off-screen arrow's tip from the screen edge, points.
const ARROW_INSET: f32 = 28.0;
/// Length of each arm of an off-screen arrow, points.
const ARROW_SIZE: f32 = 14.0;

/// What [`offscreen_arrows`] needs to know about the camera.
pub struct ArrowCamera {
    pub eye: aim_core::math::Vec3,
    pub view: aim_core::math::ViewAngles,
    /// Current vertical FOV (including ADS zoom), degrees.
    pub vfov_deg: f32,
    /// Width / height of the 3D viewport.
    pub aspect: f32,
}

/// A small semi-transparent chevron at the screen edge for every live, visible target
/// outside the current view, pointing towards it.
pub fn offscreen_arrows(ctx: &egui::Context, s: &Session, cam: &ArrowCamera, rgb: [f32; 3]) {
    let half_v = cam.vfov_deg / 2.0;
    let half_h = aim_core::fov::horizontal_from_vertical(cam.vfov_deg as f64, cam.aspect as f64)
        as f32
        / 2.0;
    let head = s.dims().head_center_height();
    let humanoid = matches!(
        s.def().targets.shape,
        aim_core::scenario::Shape::Humanoid { .. }
    );
    let screen = ctx.content_rect();
    let (w, h) = aim_core::fov::valorant_viewport(screen.width() as f64, screen.height() as f64);
    let viewport = egui::Rect::from_center_size(screen.center(), egui::vec2(w as f32, h as f32));
    let [r, g, b] = rgb.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    let stroke = egui::Stroke::new(3.0, egui::Color32::from_rgba_unmultiplied(r, g, b, 150));
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("offscreen_arrows"),
    ));
    for t in s.targets().filter(|t| t.health > 0.0) {
        let mut p = t.position;
        if humanoid {
            p.y += head;
        }
        let Some(bearing) = aim_core::fov::offscreen_bearing(cam.view, cam.eye, p, half_h, half_v)
        else {
            continue;
        };
        let tip = edge_point(viewport, bearing, ARROW_INSET);
        let dir = egui::vec2(bearing.cos(), -bearing.sin());
        let side = egui::vec2(-dir.y, dir.x);
        let back = tip - dir * ARROW_SIZE;
        painter.line_segment([back + side * ARROW_SIZE * 0.8, tip], stroke);
        painter.line_segment([back - side * ARROW_SIZE * 0.8, tip], stroke);
    }
}

/// Where a ray from the center of `rect` along `bearing` (radians, 0 = right,
/// counter-clockwise) meets the rectangle shrunk by `inset` on every side.
fn edge_point(rect: egui::Rect, bearing: f32, inset: f32) -> egui::Pos2 {
    let c = rect.center();
    let hx = (rect.width() / 2.0 - inset).max(0.0);
    let hy = (rect.height() / 2.0 - inset).max(0.0);
    let (dx, dy) = (bearing.cos(), -bearing.sin());
    let tx = if dx.abs() > 1e-6 {
        hx / dx.abs()
    } else {
        f32::INFINITY
    };
    let ty = if dy.abs() > 1e-6 {
        hy / dy.abs()
    } else {
        f32::INFINITY
    };
    c + egui::vec2(dx, dy) * tx.min(ty)
}

#[cfg(test)]
mod offscreen_tests {
    use super::edge_point;
    use bevy_egui::egui::{Pos2, Rect};
    use std::f32::consts::{FRAC_PI_2, PI};

    #[test]
    fn arrows_sit_on_the_inset_screen_edge_towards_the_bearing() {
        let r = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(200.0, 100.0));
        let close = |a: Pos2, b: Pos2| (a - b).length() < 1e-3;
        assert!(close(edge_point(r, 0.0, 10.0), Pos2::new(190.0, 50.0)));
        assert!(close(edge_point(r, PI, 10.0), Pos2::new(10.0, 50.0)));
        // Screen y grows downwards, so "up" (π/2) is the top edge.
        assert!(close(
            edge_point(r, FRAC_PI_2, 10.0),
            Pos2::new(100.0, 10.0)
        ));
        assert!(close(
            edge_point(r, -FRAC_PI_2, 10.0),
            Pos2::new(100.0, 90.0)
        ));
        // A diagonal hits the nearer (top) edge first on a wide screen.
        let p = edge_point(r, PI / 4.0, 10.0);
        assert!((p.y - 10.0).abs() < 1e-3 && p.x > 100.0, "{p:?}");
    }
}
