//! In-run overlays: run status, movement accuracy meter, countdown, pause window.

use crate::ui::UiAction;
use crate::ui::atoms::{BAD, GOOD, big};
use aim_core::scenario::Session;
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
