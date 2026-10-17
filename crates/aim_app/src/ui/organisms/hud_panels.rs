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

pub fn countdown(ctx: &egui::Context, secs: f32) {
    egui::Area::new(egui::Id::new("countdown"))
        .anchor(Align2::CENTER_CENTER, [0.0, -80.0])
        .show(ctx, |ui| {
            ui.label(big(format!("{}", secs.ceil() as i32), 56.0));
        });
}

pub fn pause_window(ctx: &egui::Context, description: &str) -> Option<UiAction> {
    let mut action = None;
    egui::Window::new("Paused")
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label(description);
            ui.separator();
            if ui.button("Resume (Esc)").clicked() {
                action = Some(UiAction::Resume);
            }
            if ui.button("Quit to menu").clicked() {
                action = Some(UiAction::BackToMenu);
            }
        });
    action
}

/// Arena status: time, health, kills/deaths, the site objective (when the arena has one),
/// and a red vignette for `damage_flash` seconds.
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
