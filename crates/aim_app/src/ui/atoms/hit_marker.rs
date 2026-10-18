//! Hit-marker painter: a small, semi-transparent mark around the crosshair that fades out
//! after a hit. Sizes are in points, kept clear of the crosshair's center.

use crate::game::feedback::{HitMarkerStyle, MarkerColor, MarkerLook, marker_alpha};
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

/// The marks of `style` at `scale` times their base size.
fn marks(style: HitMarkerStyle, scale: f32) -> Vec<Mark> {
    base_marks(style)
        .into_iter()
        .map(|m| match m {
            Mark::Segment(a, b) => Mark::Segment(a * scale, b * scale),
            Mark::Circle { radius, filled } => Mark::Circle {
                radius: radius * scale,
                filled,
            },
        })
        .collect()
}

fn base_marks(style: HitMarkerStyle) -> Vec<Mark> {
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

fn marker_color(rgb: Option<[u8; 3]>, crosshair: Rgba, alpha: f32) -> Color32 {
    let [r, g, b] = rgb.unwrap_or([crosshair.r, crosshair.g, crosshair.b]);
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
    let look = MarkerLook {
        style,
        rgb: color.rgb(),
        scale: 1.0,
        stroke_scale: 1.0,
        flash_rgb: [255, 255, 255],
    };
    paint_look(painter, center, &look, crosshair, t);
}

/// Paint a marker `look` around `center`, `t` (0–1) of the way through its fade.
pub fn paint_look(
    painter: &egui::Painter,
    center: Pos2,
    look: &MarkerLook,
    crosshair: Rgba,
    t: f32,
) {
    let fill = marker_color(look.rgb, crosshair, marker_alpha(t));
    if fill.a() == 0 {
        return;
    }
    let stroke = Stroke::new(STROKE * look.stroke_scale, fill);
    for mark in marks(look.style, look.scale) {
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

/// Width of the kill flash's glow along the screen edges, points.
const KILL_FLASH_EDGE: f32 = 40.0;

/// The kill flash: a soft white glow along the screen edges at `alpha` (0–1), behind the
/// HUD so it never covers the crosshair or a target in the middle of the view.
pub fn draw_kill_flash(ctx: &egui::Context, alpha: f32) {
    let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
    if a == 0 {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("kill_flash"),
    ));
    painter.rect_stroke(
        ctx.content_rect().shrink(KILL_FLASH_EDGE / 2.0),
        0.0,
        Stroke::new(
            KILL_FLASH_EDGE,
            Color32::from_rgba_unmultiplied(255, 255, 255, a),
        ),
        egui::StrokeKind::Inside,
    );
}

/// Draw the hit marker at the screen center, above everything else.
pub fn draw(ctx: &egui::Context, look: &MarkerLook, crosshair: Rgba, t: f32) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("hit_marker"),
    ));
    paint_look(&painter, ctx.content_rect().center(), look, crosshair, t);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_and_flash_draw_no_marker() {
        assert!(marks(HitMarkerStyle::Off, 1.0).is_empty());
        assert!(marks(HitMarkerStyle::Flash, 1.0).is_empty());
    }

    #[test]
    fn cross_arms_keep_clear_of_the_crosshair_center() {
        let cross = marks(HitMarkerStyle::Cross, 1.0);
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
        let brackets = marks(HitMarkerStyle::Brackets, 1.0);
        assert_eq!(brackets.len(), 8);
        let sum = brackets.iter().fold(Vec2::ZERO, |s, m| match m {
            Mark::Segment(a, b) => s + *a + *b,
            Mark::Circle { .. } => s,
        });
        assert!(sum.length() < 1e-4, "{sum:?}");
    }

    #[test]
    fn scaled_marks_grow_from_the_center_and_keep_their_shape() {
        for style in HitMarkerStyle::ALL {
            let base = marks(style, 1.0);
            let big = marks(style, 1.6);
            assert_eq!(base.len(), big.len(), "{style:?}");
            for (b, s) in base.iter().zip(&big) {
                match (b, s) {
                    (Mark::Segment(a0, b0), Mark::Segment(a1, b1)) => {
                        assert!((*a0 * 1.6 - *a1).length() < 1e-4);
                        assert!((*b0 * 1.6 - *b1).length() < 1e-4);
                    }
                    (Mark::Circle { radius: r0, .. }, Mark::Circle { radius: r1, .. }) => {
                        assert!((r0 * 1.6 - r1).abs() < 1e-4);
                    }
                    _ => panic!("{style:?}: shape changed"),
                }
            }
        }
    }

    #[test]
    fn marker_color_uses_the_crosshair_color_when_asked() {
        let crosshair = Rgba::opaque(10, 20, 30);
        let c = marker_color(MarkerColor::Crosshair.rgb(), crosshair, 1.0);
        assert_eq!((c.r(), c.g(), c.b(), c.a()), (10, 20, 30, 255));
        let white = marker_color(MarkerColor::White.rgb(), crosshair, 0.0);
        assert_eq!(white.a(), 0);
    }
}
