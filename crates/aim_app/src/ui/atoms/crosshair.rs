//! Crosshair painter, snapped to physical pixels so lines stay crisp and centered.

use super::color32;
use crate::persistence::settings::Crosshair;
use bevy_egui::egui::{self, Color32, Pos2, Stroke, Vec2};

pub fn draw(ctx: &egui::Context, style: &Crosshair, hit: bool) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("crosshair"),
    ));
    let ppp = ctx.pixels_per_point();
    let c = ctx.content_rect().center();
    let snap = |v: f32| (v * ppp).round() / ppp + 0.5 / ppp;
    let center = Pos2::new(snap(c.x), snap(c.y));
    let (len, gap, th) = (style.length / ppp, style.gap / ppp, style.thickness / ppp);
    let color = if hit {
        Color32::WHITE
    } else {
        color32(style.color)
    };
    let arms = [
        Vec2::new(1.0, 0.0),
        Vec2::new(-1.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(0.0, -1.0),
    ];
    let segments: Vec<_> = arms
        .iter()
        .map(|d| (center + *d * gap, center + *d * (gap + len)))
        .collect();
    if style.outline {
        for (a, b) in &segments {
            painter.line_segment([*a, *b], Stroke::new(th + 2.0 / ppp, Color32::BLACK));
        }
    }
    for (a, b) in &segments {
        painter.line_segment([*a, *b], Stroke::new(th, color));
    }
    if style.center_dot {
        painter.circle_filled(center, th, color);
    }
}
