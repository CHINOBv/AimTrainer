//! Live coaching overlay for arenas: crosshair placement against head height at the
//! nearest exposed corner, and movement accuracy. It only draws the pure values of
//! `aim_world::arena::Coaching`; no automated test covers the drawing.

use crate::ui::atoms::{BAD, GOOD};
use aim_world::arena::{COACH_CORNER_TOLERANCE_DEG, COACH_HEAD_LEVEL_TOLERANCE_DEG, Coaching};
use bevy_egui::egui::{self, Align2, Color32, FontId, RichText};

const TITLE: &str = "COACH";
const NO_CORNER: &str = "No corner in view";
const ACCURATE: &str = "ACCURATE";
const MOVING: &str = "MOVING";
/// Height of the head-level gauge, pixels, and the error it spans each way, degrees.
const GAUGE_PX: f32 = 60.0;
const GAUGE_SPAN_DEG: f32 = 5.0;

fn state(good: bool) -> Color32 {
    if good { GOOD } else { BAD }
}

/// "+2.1° high", "-0.4° low" or "on head level".
fn head_level_text(error_deg: f32) -> String {
    if error_deg.abs() <= COACH_HEAD_LEVEL_TOLERANCE_DEG {
        format!("Head level: on ({error_deg:+.1}°)")
    } else if error_deg > 0.0 {
        format!("Head level: {error_deg:+.1}° high")
    } else {
        format!("Head level: {error_deg:+.1}° low")
    }
}

pub fn coaching_overlay(ctx: &egui::Context, c: &Coaching) {
    egui::Area::new(egui::Id::new("arena-coaching"))
        .anchor(Align2::LEFT_CENTER, [24.0, 0.0])
        .interactable(false)
        .show(ctx, |ui| {
            ui.label(RichText::new(TITLE).small().strong());
            match (c.head_level_error_deg, c.corner_offset_deg) {
                (Some(error), Some(offset)) => {
                    ui.horizontal(|ui| {
                        gauge(ui, error);
                        ui.vertical(|ui| {
                            ui.colored_label(
                                state(c.at_head_level() == Some(true)),
                                head_level_text(error),
                            );
                            ui.colored_label(
                                state(c.on_corner() == Some(true)),
                                format!(
                                    "Nearest corner: {offset:.0}° away (hold within {COACH_CORNER_TOLERANCE_DEG:.0}°)"
                                ),
                            );
                        });
                    });
                }
                _ => {
                    ui.label(NO_CORNER);
                }
            }
            let movement = if c.accurate {
                ACCURATE.to_string()
            } else {
                format!("{MOVING} {:.1} m/s", c.speed)
            };
            ui.label(
                RichText::new(movement)
                    .color(state(c.accurate))
                    .strong()
                    .font(FontId::proportional(18.0)),
            );
        });
}

/// A vertical bar: the center line is head height at the corner, the dot is the crosshair.
fn gauge(ui: &mut egui::Ui, error_deg: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, GAUGE_PX), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let stroke = egui::Stroke::new(1.0, Color32::from_gray(160));
    painter.line_segment([rect.center_top(), rect.center_bottom()], stroke);
    let band = COACH_HEAD_LEVEL_TOLERANCE_DEG / GAUGE_SPAN_DEG * GAUGE_PX / 2.0;
    painter.rect_filled(
        egui::Rect::from_center_size(center, egui::vec2(14.0, band * 2.0)),
        0.0,
        GOOD.gamma_multiply(0.35),
    );
    // Screen y grows downward; a crosshair above head level draws above the center.
    let offset = (error_deg / GAUGE_SPAN_DEG).clamp(-1.0, 1.0) * GAUGE_PX / 2.0;
    let good = error_deg.abs() <= COACH_HEAD_LEVEL_TOLERANCE_DEG;
    painter.circle_filled(center - egui::vec2(0.0, offset), 4.0, state(good));
}
