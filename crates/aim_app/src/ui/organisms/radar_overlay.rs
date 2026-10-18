//! Radar of a radar-recall drill: a round minimap in the top-left corner that turns with
//! the view (forward is up) and shows where the round's enemies stand while it lasts.
//!
//! [`RadarPanel`] is plain data built from the core's [`RadarView`]; the view only draws it.

use aim_core::scenario::cue::RadarView;
use bevy_egui::egui::{self, Align2, Color32, Pos2, Sense, Stroke, Vec2};

const RADIUS_PX: f32 = 90.0;
const DOT_PX: f32 = 6.0;
const BACKGROUND: Color32 = Color32::from_rgba_premultiplied(10, 14, 20, 200);
const RING: Color32 = Color32::from_rgb(120, 130, 140);
const PLAYER: Color32 = Color32::from_rgb(240, 240, 240);
const ENEMY: Color32 = Color32::from_rgb(235, 70, 70);

#[derive(Debug, Clone, PartialEq)]
pub struct RadarPanel {
    /// Each enemy as `(right, forward)` in radar radii: (0, 0) is the player, 1 the rim.
    pub dots: Vec<(f32, f32)>,
    /// "Radar 1.2 s".
    pub label: String,
}

impl RadarPanel {
    pub fn new(radar: &RadarView, view_yaw_deg: f32) -> Self {
        let range = radar.range_m();
        let dots = radar
            .offsets(view_yaw_deg)
            .into_iter()
            .map(|(right, forward)| {
                let (x, y) = (right / range, forward / range);
                // Keep a dot on the rim rather than outside the radar.
                let k = 1.0 / x.hypot(y).max(1.0);
                (x * k, y * k)
            })
            .collect();
        Self {
            dots,
            label: format!("Radar {:.1} s", radar.remaining_secs),
        }
    }
}

pub fn radar_overlay(ctx: &egui::Context, panel: &RadarPanel) {
    egui::Area::new(egui::Id::new("radar"))
        .anchor(Align2::LEFT_TOP, [24.0, 96.0])
        .interactable(false)
        .show(ctx, |ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(2.0 * RADIUS_PX), Sense::hover());
            let painter = ui.painter_at(rect);
            let c = rect.center();
            painter.circle(c, RADIUS_PX, BACKGROUND, Stroke::new(1.5, RING));
            painter.circle_stroke(c, RADIUS_PX / 2.0, Stroke::new(1.0, RING));
            // The player, pointing forward (up).
            let tip = c + Vec2::new(0.0, -9.0);
            let left = c + Vec2::new(-6.0, 6.0);
            let right = c + Vec2::new(6.0, 6.0);
            painter.add(egui::Shape::convex_polygon(
                vec![tip, right, left],
                PLAYER,
                Stroke::NONE,
            ));
            for &(x, y) in &panel.dots {
                let p = Pos2::new(c.x + x * RADIUS_PX, c.y - y * RADIUS_PX);
                painter.circle_filled(p, DOT_PX, ENEMY);
            }
            ui.label(panel.label.as_str());
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::math::Vec3;

    #[test]
    fn dots_are_scaled_to_the_radar_and_turn_with_the_view() {
        let radar = RadarView {
            dots: vec![Vec3::new(0.0, 0.0, -8.0), Vec3::new(4.0, 0.0, 0.0)],
            player: Vec3::ZERO,
            remaining_secs: 1.25,
        };
        // range = 8 m × 1.25 = 10 m.
        let p = RadarPanel::new(&radar, 0.0);
        assert!((p.dots[0].0).abs() < 1e-5 && (p.dots[0].1 - 0.8).abs() < 1e-5);
        assert!((p.dots[1].0 - 0.4).abs() < 1e-5 && p.dots[1].1.abs() < 1e-5);
        assert_eq!(p.label, "Radar 1.2 s");
        // Turned right by 90°, the enemy ahead is now on the left.
        let turned = RadarPanel::new(&radar, 90.0);
        assert!((turned.dots[0].0 + 0.8).abs() < 1e-5, "{turned:?}");
    }
}
