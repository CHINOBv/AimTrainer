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

/// Extra gap of the crosshair bloom at full inaccuracy, physical pixels.
const MAX_BLOOM_PX: u8 = 12;

/// Extra gap, in physical pixels, that the crosshair bloom adds at `inaccuracy` (0–1).
/// Display only: it never changes where a shot goes.
pub fn bloom_px(inaccuracy: f32) -> u8 {
    (inaccuracy.clamp(0.0, 1.0) * f32::from(MAX_BLOOM_PX)).round() as u8
}

/// The four arms of a set of lines, their offset plus `bloom` pixels away from the center
/// pixel.
fn arms(lines: &Lines, bloom: u8) -> Vec<PxRect> {
    let t = i32::from(lines.thickness_px);
    let (across0, across1) = centered(t);
    let gap = i32::from(lines.offset_px) + i32::from(bloom);
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

/// Every filled rectangle of the crosshair with its opacity, outlines first. `bloom` pushes
/// the arms out; the center dot stays.
fn layers(style: &Crosshair, bloom: u8) -> Vec<(PxRect, f64, bool)> {
    let mut shapes: Vec<(PxRect, f64)> = Vec::new();
    for lines in [&style.outer, &style.inner] {
        if lines.is_visible() {
            shapes.extend(arms(lines, bloom).into_iter().map(|r| (r, lines.opacity)));
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

/// Draw the crosshair at the screen center; `flash` replaces its color during a hit flash
/// and the arms sit `bloom` extra pixels out (see [`bloom_px`]).
pub fn draw(ctx: &egui::Context, style: &Crosshair, flash: Option<Rgba>, bloom: u8) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("crosshair"),
    ));
    let ppp = ctx.pixels_per_point();
    let c = ctx.content_rect().center();
    // Index of the physical pixel under the screen center.
    let (cx, cy) = ((c.x * ppp).floor() as i32, (c.y * ppp).floor() as i32);
    let to_points = |x: i32, y: i32| Pos2::new((cx + x) as f32 / ppp, (cy + y) as f32 / ppp);
    let color = flash.unwrap_or_else(|| style.color());
    for (r, opacity, outline) in layers(style, bloom) {
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
        let a = arms(&lines, 0);
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
        assert!(layers(&ch, 0).is_empty());
        ch.center_dot = true;
        // The dot plus its outline.
        assert_eq!(layers(&ch, 0).len(), 2);
    }

    #[test]
    fn bloom_gap_grows_with_inaccuracy_up_to_its_maximum() {
        assert_eq!(bloom_px(0.0), 0);
        assert_eq!(bloom_px(-1.0), 0);
        assert_eq!(bloom_px(1.0), MAX_BLOOM_PX);
        assert_eq!(bloom_px(5.0), MAX_BLOOM_PX, "clamped");
        let steps: Vec<u8> = (0..=10).map(|i| bloom_px(i as f32 / 10.0)).collect();
        assert!(steps.windows(2).all(|w| w[1] >= w[0]), "{steps:?}");
    }

    #[test]
    fn bloom_pushes_the_arms_out_but_not_the_dot() {
        let ch = Crosshair::default();
        let still = layers(&ch, 0);
        let open = layers(&ch, 4);
        assert_eq!(still.len(), open.len());
        let spread = |l: &[(PxRect, f64, bool)]| l.iter().map(|(r, ..)| r.x1).max();
        assert_eq!(spread(&open), spread(&still).map(|x| x + 4));
        // The center dot (and its outline) stays where it was.
        let mut dot_only = ch;
        dot_only.inner.show = false;
        dot_only.outer.show = false;
        dot_only.center_dot = true;
        assert_eq!(layers(&dot_only, 0), layers(&dot_only, 4));
    }

    #[test]
    fn element_opacity_scales_the_color_alpha() {
        let c = with_opacity(Rgba::WHITE, 0.5);
        assert_eq!(c.a(), 128);
    }
}
