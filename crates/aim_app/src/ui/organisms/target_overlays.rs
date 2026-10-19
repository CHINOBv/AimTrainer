//! Per-scenario target overlays (`[feedback]`): floating damage numbers and small health
//! bars, drawn with egui at the targets' projected screen positions. Drawing only: the
//! numbers and health come from the run.

use crate::game::feedback::DamagePop;
use crate::ui::organisms::hud_panels::ArrowCamera;
use aim_core::math::Vec3;
use bevy_egui::egui;

/// A health bar to draw: where the target's top is and the share of health left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarView {
    pub top: Vec3,
    pub fraction: f32,
}

/// Size of a health bar, points.
const BAR_SIZE: egui::Vec2 = egui::vec2(36.0, 5.0);
/// Gap between a target's top and its bar, points.
const BAR_GAP_PX: f32 = 8.0;
/// Text size of a damage number, points (head hits are larger).
const NUMBER_PX: f32 = 18.0;
const HEAD_NUMBER_PX: f32 = 22.0;
const NUMBER_RGB: [u8; 3] = [255, 255, 255];
const HEAD_NUMBER_RGB: [u8; 3] = [255, 214, 64];
/// Points closer than this in front of the eye are not drawn, meters.
const NEAR_M: f32 = 0.05;

/// Where `point` appears on screen inside `viewport` (the 3D view's rectangle), or `None`
/// when it is behind the eye or outside the view.
pub fn project(cam: &ArrowCamera, viewport: egui::Rect, point: Vec3) -> Option<egui::Pos2> {
    let forward = cam.view.direction();
    let (sy, cy) = cam.view.yaw_deg.to_radians().sin_cos();
    let right = Vec3::new(cy, 0.0, sy);
    let up = right.cross(forward);
    let d = point - cam.eye;
    let (x, y, z) = (d.dot(right), d.dot(up), d.dot(forward));
    if z < NEAR_M {
        return None;
    }
    let half_v = (cam.vfov_deg / 2.0).to_radians().tan();
    let half_h = (aim_core::fov::horizontal_from_vertical(cam.vfov_deg as f64, cam.aspect as f64)
        as f32
        / 2.0)
        .to_radians()
        .tan();
    let (nx, ny) = (x / (z * half_h), y / (z * half_v));
    if nx.abs() > 1.0 || ny.abs() > 1.0 {
        return None;
    }
    let c = viewport.center();
    Some(egui::pos2(
        c.x + nx * viewport.width() / 2.0,
        c.y - ny * viewport.height() / 2.0,
    ))
}

/// The 3D view's rectangle: the window, pillarboxed to 16:9 like the camera.
fn viewport(ctx: &egui::Context) -> egui::Rect {
    let screen = ctx.content_rect();
    let (w, h) = aim_core::fov::valorant_viewport(screen.width() as f64, screen.height() as f64);
    egui::Rect::from_center_size(screen.center(), egui::vec2(w as f32, h as f32))
}

/// Draw `bars` over their targets and `pops` rising from where they were hit.
pub fn target_overlays(
    ctx: &egui::Context,
    cam: &ArrowCamera,
    pops: &[DamagePop],
    bars: &[BarView],
) {
    let view = viewport(ctx);
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("target_overlays"),
    ));
    for bar in bars {
        let Some(p) = project(cam, view, bar.top) else {
            continue;
        };
        let rect = egui::Rect::from_center_size(
            p - egui::vec2(0.0, BAR_GAP_PX + BAR_SIZE.y / 2.0),
            BAR_SIZE,
        );
        painter.rect_filled(rect, 1.0, egui::Color32::from_black_alpha(160));
        let mut fill = rect.shrink(1.0);
        fill.set_width(fill.width() * bar.fraction.clamp(0.0, 1.0));
        painter.rect_filled(fill, 1.0, crate::ui::atoms::GOOD);
    }
    for pop in pops {
        let top = Vec3::new(
            pop.position.x,
            pop.position.y + pop.rise_m(),
            pop.position.z,
        );
        let Some(p) = project(cam, view, top) else {
            continue;
        };
        let ([r, g, b], size) = if pop.head {
            (HEAD_NUMBER_RGB, HEAD_NUMBER_PX)
        } else {
            (NUMBER_RGB, NUMBER_PX)
        };
        let alpha = (pop.alpha() * 255.0).round() as u8;
        painter.text(
            p - egui::vec2(0.0, BAR_GAP_PX * 2.0),
            egui::Align2::CENTER_BOTTOM,
            format!("{:.0}", pop.amount),
            egui::FontId::proportional(size),
            egui::Color32::from_rgba_unmultiplied(r, g, b, alpha),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::math::ViewAngles;

    fn cam() -> ArrowCamera {
        ArrowCamera {
            eye: Vec3::new(0.0, 1.6, 0.0),
            view: ViewAngles::default(),
            vfov_deg: 70.0,
            aspect: 16.0 / 9.0,
        }
    }

    #[test]
    fn points_project_inside_the_view_and_behind_or_outside_are_skipped() {
        let view = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1600.0, 900.0));
        let ahead = project(&cam(), view, Vec3::new(0.0, 1.6, -10.0)).unwrap();
        assert!((ahead - view.center()).length() < 1e-3, "{ahead:?}");
        let right_up = project(&cam(), view, Vec3::new(1.0, 2.6, -10.0)).unwrap();
        assert!(right_up.x > view.center().x, "+x is right on screen");
        assert!(right_up.y < view.center().y, "+y is up on screen");
        assert_eq!(project(&cam(), view, Vec3::new(0.0, 1.6, 10.0)), None);
        assert_eq!(project(&cam(), view, Vec3::new(100.0, 1.6, -1.0)), None);
    }
}
