//! Radial wedge chart: one pie wedge per direction, clockwise from up, whose area grows
//! with its count. The miss compass uses it with eight directions.
//!
//! Painted with egui's painter. [`wedge_radius`] and [`wedge_points`] are the pure
//! geometry; the painting itself is not covered by automated tests.

use crate::ui::atoms::{NEUTRAL, WARN};
use bevy_egui::egui::{self, Align2, Color32, FontId, Pos2, Sense, Shape, Stroke, vec2};

const SIZE_PX: f32 = 130.0;
/// Room for the count labels outside the wedges.
const LABEL_MARGIN_PX: f32 = 14.0;
/// Empty angle between neighbouring wedges, as a share of one wedge.
const GAP_SHARE: f32 = 0.12;
/// Points along each wedge's outer edge.
const EDGE_SEGMENTS: usize = 8;

/// Outer radius of a wedge holding `count` of the busiest wedge's `max`: its area is
/// proportional to the count, so a wedge with twice the misses looks twice as big.
pub fn wedge_radius(count: u32, max: u32, outer: f32) -> f32 {
    if max == 0 {
        return 0.0;
    }
    outer * (count.min(max) as f32 / max as f32).sqrt()
}

/// Screen point at `angle` radians clockwise from up, `radius` from `center`.
fn polar(center: Pos2, radius: f32, angle: f32) -> Pos2 {
    Pos2::new(
        center.x + radius * angle.sin(),
        center.y - radius * angle.cos(),
    )
}

/// The convex polygon of wedge `index` of `of_n` (clockwise from up, each centered on its
/// direction): the center, then its outer edge. A small gap separates neighbours.
pub fn wedge_points(center: Pos2, index: usize, of_n: usize, radius: f32) -> Vec<Pos2> {
    let step = std::f32::consts::TAU / of_n.max(1) as f32;
    let half = step * (1.0 - GAP_SHARE) / 2.0;
    let mid = step * index as f32;
    std::iter::once(center)
        .chain((0..=EDGE_SEGMENTS).map(|i| {
            let a = mid - half + 2.0 * half * i as f32 / EDGE_SEGMENTS as f32;
            polar(center, radius, a)
        }))
        .collect()
}

/// Draw the chart for `counts` (clockwise from up); wedges in `highlight` use the warning
/// colour. `hint` is the hover text.
pub fn radial_wedges(ui: &mut egui::Ui, counts: &[u32], highlight: &[usize], hint: &str) {
    let (response, painter) = ui.allocate_painter(vec2(SIZE_PX, SIZE_PX), Sense::hover());
    let center = response.rect.center();
    let outer = SIZE_PX / 2.0 - LABEL_MARGIN_PX;
    let n = counts.len();
    let max = counts.iter().copied().max().unwrap_or(0);
    painter.circle_stroke(center, outer, Stroke::new(1.0, Color32::from_gray(60)));
    for (i, &count) in counts.iter().enumerate() {
        let color = if highlight.contains(&i) {
            WARN
        } else {
            NEUTRAL
        };
        let r = wedge_radius(count, max, outer);
        if r > 0.0 {
            painter.add(Shape::convex_polygon(
                wedge_points(center, i, n, r),
                color.gamma_multiply(0.85),
                Stroke::NONE,
            ));
        }
        let angle = std::f32::consts::TAU * i as f32 / n.max(1) as f32;
        painter.text(
            polar(center, outer + LABEL_MARGIN_PX / 2.0, angle),
            Align2::CENTER_CENTER,
            count.to_string(),
            FontId::proportional(11.0),
            Color32::from_gray(190),
        );
    }
    painter.circle_filled(center, 2.5, Color32::WHITE);
    response.on_hover_text(hint);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wedge_area_grows_with_the_count() {
        assert_eq!(wedge_radius(4, 4, 40.0), 40.0);
        assert_eq!(wedge_radius(1, 4, 40.0), 20.0, "a quarter of the area");
        assert_eq!(wedge_radius(0, 4, 40.0), 0.0);
        assert_eq!(wedge_radius(3, 0, 40.0), 0.0, "no misses, no wedges");
    }

    #[test]
    fn the_first_wedge_points_up_and_the_third_points_right() {
        let c = Pos2::new(0.0, 0.0);
        let up = wedge_points(c, 0, 8, 10.0);
        assert_eq!(up[0], c);
        let tip = up[1 + EDGE_SEGMENTS / 2];
        assert!((tip - Pos2::new(0.0, -10.0)).length() < 1e-4, "{tip:?}");
        let right = wedge_points(c, 2, 8, 10.0)[1 + EDGE_SEGMENTS / 2];
        assert!((right - Pos2::new(10.0, 0.0)).length() < 1e-4, "{right:?}");
    }

    #[test]
    fn neighbouring_wedges_leave_a_gap() {
        let c = Pos2::new(0.0, 0.0);
        let first_end = *wedge_points(c, 0, 8, 10.0).last().unwrap();
        let second_start = wedge_points(c, 1, 8, 10.0)[1];
        assert!((first_end - second_start).length() > 0.5);
    }
}
