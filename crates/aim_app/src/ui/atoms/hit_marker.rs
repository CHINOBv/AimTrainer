//! Hit-marker painter: a small, semi-transparent mark around the crosshair that fades out
//! after a hit. Sizes are in points, kept clear of the crosshair's center.

use crate::game::feedback::{HitMarkerStyle, MarkerColor, marker_alpha};
use aim_core::crosshair::Rgba;
use bevy_egui::egui::{self, Color32, Pos2, Stroke, Vec2};

/// Distance from the center where the cross starts, points.
const GAP: f32 = 5.0;
/// Length of each cross arm, points.
const ARM: f32 = 5.0;
/// Half-size of the bracket box and radius of the ring, points.
const BOX: f32 = 9.0;
/// Length of each bracket leg, points.
const LEG: f32 = 4.0;
const DOT_RADIUS: f32 = 2.0;
const STROKE: f32 = 1.5;

/// One drawn element, relative to the screen center.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Mark {
    Segment(Vec2, Vec2),
    Circle { radius: f32, filled: bool },
}

fn marks(style: HitMarkerStyle) -> Vec<Mark> {
    let corners =
        [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)].map(|(x, y)| Vec2::new(x, y));
    match style {
        HitMarkerStyle::Off | HitMarkerStyle::Flash => Vec::new(),
        HitMarkerStyle::Dot => vec![Mark::Circle {
            radius: DOT_RADIUS,
            filled: true,
        }],
        HitMarkerStyle::Ring => vec![Mark::Circle {
            radius: BOX,
            filled: false,
        }],
        HitMarkerStyle::Cross => corners
            .iter()
            .map(|c| {
                let d = c.normalized();
                Mark::Segment(d * GAP, d * (GAP + ARM))
            })
            .collect(),
        HitMarkerStyle::Brackets => corners
            .iter()
            .flat_map(|&c| {
                let corner = c * BOX;
                [
                    Mark::Segment(corner, corner - Vec2::new(c.x * LEG, 0.0)),
                    Mark::Segment(corner, corner - Vec2::new(0.0, c.y * LEG)),
                ]
            })
            .collect(),
    }
}

fn marker_color(color: MarkerColor, crosshair: Rgba, alpha: f32) -> Color32 {
    let [r, g, b] = color
        .rgb()
        .unwrap_or([crosshair.r, crosshair.g, crosshair.b]);
    Color32::from_rgba_unmultiplied(r, g, b, (alpha.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// Paint `style` around `center`, `t` (0–1) of the way through its fade.
pub fn paint(
    painter: &egui::Painter,
    center: Pos2,
    style: HitMarkerStyle,
    color: MarkerColor,
    crosshair: Rgba,
    t: f32,
) {
    let fill = marker_color(color, crosshair, marker_alpha(t));
    if fill.a() == 0 {
        return;
    }
    let stroke = Stroke::new(STROKE, fill);
    for mark in marks(style) {
        match mark {
            Mark::Segment(a, b) => {
                painter.line_segment([center + a, center + b], stroke);
            }
            Mark::Circle {
                radius,
                filled: true,
            } => {
                painter.circle_filled(center, radius, fill);
            }
            Mark::Circle { radius, .. } => {
                painter.circle_stroke(center, radius, stroke);
            }
        }
    }
}

/// Draw the hit marker at the screen center, above everything else.
pub fn draw(
    ctx: &egui::Context,
    style: HitMarkerStyle,
    color: MarkerColor,
    crosshair: Rgba,
    t: f32,
) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hit_marker"),
    ));
    paint(
        &painter,
        ctx.content_rect().center(),
        style,
        color,
        crosshair,
        t,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_and_flash_draw_no_marker() {
        assert!(marks(HitMarkerStyle::Off).is_empty());
        assert!(marks(HitMarkerStyle::Flash).is_empty());
    }

    #[test]
    fn cross_arms_keep_clear_of_the_crosshair_center() {
        let cross = marks(HitMarkerStyle::Cross);
        assert_eq!(cross.len(), 4);
        for m in cross {
            let Mark::Segment(a, b) = m else {
                panic!("cross arms are segments")
            };
            assert!(a.length() >= GAP - 1e-4 && b.length() > a.length());
        }
    }

    #[test]
    fn brackets_are_four_symmetric_corners() {
        let brackets = marks(HitMarkerStyle::Brackets);
        assert_eq!(brackets.len(), 8);
        let sum = brackets.iter().fold(Vec2::ZERO, |s, m| match m {
            Mark::Segment(a, b) => s + *a + *b,
            Mark::Circle { .. } => s,
        });
        assert!(sum.length() < 1e-4, "{sum:?}");
    }

    #[test]
    fn marker_color_uses_the_crosshair_color_when_asked() {
        let crosshair = Rgba::opaque(10, 20, 30);
        let c = marker_color(MarkerColor::Crosshair, crosshair, 1.0);
        assert_eq!((c.r(), c.g(), c.b(), c.a()), (10, 20, 30, 255));
        let white = marker_color(MarkerColor::White, crosshair, 0.0);
        assert_eq!(white.a(), 0);
    }
}
