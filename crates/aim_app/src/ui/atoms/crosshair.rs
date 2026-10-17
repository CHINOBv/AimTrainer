//! Crosshair painter. Every element is a rectangle on whole physical pixels, so lines
//! stay crisp and symmetric around the center pixel at any UI scale.

use aim_core::crosshair::{Crosshair, Lines, Rgba};
use bevy_egui::egui::{self, Color32, Pos2, Rect};

/// A rectangle in physical pixels relative to the center pixel: `x0..x1`, `y0..y1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PxRect {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

impl PxRect {
    fn grow(self, by: i32) -> Self {
        Self {
            x0: self.x0 - by,
            y0: self.y0 - by,
            x1: self.x1 + by,
            y1: self.y1 + by,
        }
    }
}

/// Pixel span of `size` pixels centered on pixel 0 (`-(size-1)/2 ..`).
fn centered(size: i32) -> (i32, i32) {
    let start = -(size - 1).div_euclid(2);
    (start, start + size)
}

/// The four arms of a set of lines, `offset` pixels away from the center pixel.
fn arms(lines: &Lines) -> Vec<PxRect> {
    let t = i32::from(lines.thickness_px);
    let (across0, across1) = centered(t);
    let gap = i32::from(lines.offset_px);
    let mut out = Vec::with_capacity(4);
    let h = i32::from(lines.length_px);
    if h > 0 {
        out.push(PxRect {
            x0: gap + 1,
            x1: gap + 1 + h,
            y0: across0,
            y1: across1,
        });
        out.push(PxRect {
            x0: -gap - h,
            x1: -gap,
            y0: across0,
            y1: across1,
        });
    }
    let v = i32::from(lines.effective_vertical_length_px());
    if v > 0 {
        out.push(PxRect {
            y0: gap + 1,
            y1: gap + 1 + v,
            x0: across0,
            x1: across1,
        });
        out.push(PxRect {
            y0: -gap - v,
            y1: -gap,
            x0: across0,
            x1: across1,
        });
    }
    out
}

/// Every filled rectangle of the crosshair with its opacity, outlines first.
fn layers(style: &Crosshair) -> Vec<(PxRect, f64, bool)> {
    let mut shapes: Vec<(PxRect, f64)> = Vec::new();
    for lines in [&style.outer, &style.inner] {
        if lines.is_visible() {
            shapes.extend(arms(lines).into_iter().map(|r| (r, lines.opacity)));
        }
    }
    if style.center_dot && style.dot_thickness_px > 0 {
        let (a, b) = centered(i32::from(style.dot_thickness_px));
        shapes.push((
            PxRect {
                x0: a,
                x1: b,
                y0: a,
                y1: b,
            },
            style.dot_opacity,
        ));
    }
    let mut out = Vec::with_capacity(shapes.len() * 2);
    if style.outlines && style.outline_thickness_px > 0 {
        let by = i32::from(style.outline_thickness_px);
        out.extend(
            shapes
                .iter()
                .map(|&(r, _)| (r.grow(by), style.outline_opacity, true)),
        );
    }
    out.extend(shapes.into_iter().map(|(r, a)| (r, a, false)));
    out
}

fn with_opacity(c: Rgba, opacity: f64) -> Color32 {
    let a = (f64::from(c.a) * opacity.clamp(0.0, 1.0)).round() as u8;
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, a)
}

pub fn draw(ctx: &egui::Context, style: &Crosshair, hit: bool) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("crosshair"),
    ));
    let ppp = ctx.pixels_per_point();
    let c = ctx.content_rect().center();
    // Index of the physical pixel under the screen center.
    let (cx, cy) = ((c.x * ppp).floor() as i32, (c.y * ppp).floor() as i32);
    let to_points = |x: i32, y: i32| Pos2::new((cx + x) as f32 / ppp, (cy + y) as f32 / ppp);
    let color = if hit { Rgba::WHITE } else { style.color() };
    for (r, opacity, outline) in layers(style) {
        let fill = if outline {
            Color32::from_black_alpha((opacity.clamp(0.0, 1.0) * 255.0).round() as u8)
        } else {
            with_opacity(color, opacity)
        };
        let rect = Rect::from_min_max(to_points(r.x0, r.y0), to_points(r.x1, r.y1));
        painter.rect_filled(rect, 0.0, fill);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odd_and_even_spans_stay_on_whole_pixels() {
        assert_eq!(centered(1), (0, 1));
        assert_eq!(centered(2), (0, 2));
        assert_eq!(centered(3), (-1, 2));
    }

    #[test]
    fn arms_are_symmetric_around_the_center_pixel() {
        let lines = Lines {
            thickness_px: 1,
            length_px: 4,
            offset_px: 2,
            ..Lines::INNER
        };
        let a = arms(&lines);
        assert_eq!(a.len(), 4);
        // Right arm starts 2 px after the center pixel; left arm ends 2 px before it.
        assert_eq!((a[0].x0, a[0].x1), (3, 7));
        assert_eq!((a[1].x0, a[1].x1), (-6, -2));
    }

    #[test]
    fn hidden_elements_draw_nothing() {
        let mut ch = Crosshair::default();
        ch.inner.show = false;
        ch.outer.show = false;
        ch.center_dot = false;
        assert!(layers(&ch).is_empty());
        ch.center_dot = true;
        // The dot plus its outline.
        assert_eq!(layers(&ch).len(), 2);
    }

    #[test]
    fn element_opacity_scales_the_color_alpha() {
        let c = with_opacity(Rgba::WHITE, 0.5);
        assert_eq!(c.a(), 128);
    }
}
