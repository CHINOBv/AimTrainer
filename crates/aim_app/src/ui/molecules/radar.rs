//! Radar (polar) chart: one axis per direction and one polygon per series, each vertex as
//! far out as its value (0–1). Flick directions use it with four diagonal axes and a near
//! and a far series.
//!
//! Painted with egui's painter. [`radar_point`] and [`series_points`] are the pure
//! geometry; the painting itself is not covered by automated tests.

use bevy_egui::egui::{self, Align2, Color32, FontId, Pos2, Sense, Shape, Stroke, vec2};

const SIZE_PX: f32 = 130.0;
const LABEL_MARGIN_PX: f32 = 16.0;
/// Reference rings, as shares of the full radius.
const RINGS: [f32; 2] = [0.5, 1.0];

/// One axis: its direction in radians counter-clockwise from screen right (+y up, as in
/// math), and its short label.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadarAxis {
    pub angle: f32,
    pub label: &'static str,
}

/// One polygon: a value per axis (0–1, `None` when unmeasured, drawn at the center).
#[derive(Debug, Clone, PartialEq)]
pub struct RadarSeries {
    pub values: Vec<Option<f32>>,
    pub color: Color32,
}

/// Screen point `value` (clamped to 0–1) of the way out along an axis at `angle`.
pub fn radar_point(center: Pos2, radius: f32, angle: f32, value: f32) -> Pos2 {
    let r = radius * value.clamp(0.0, 1.0);
    Pos2::new(center.x + r * angle.cos(), center.y - r * angle.sin())
}

/// The polygon of one series, one vertex per axis in axis order.
pub fn series_points(
    center: Pos2,
    radius: f32,
    axes: &[RadarAxis],
    values: &[Option<f32>],
) -> Vec<Pos2> {
    axes.iter()
        .zip(values)
        .map(|(a, v)| radar_point(center, radius, a.angle, v.unwrap_or(0.0)))
        .collect()
}

/// Draw the chart. `hint` is the hover text.
pub fn radar(ui: &mut egui::Ui, axes: &[RadarAxis], series: &[RadarSeries], hint: &str) {
    let (response, painter) = ui.allocate_painter(vec2(SIZE_PX, SIZE_PX), Sense::hover());
    let center = response.rect.center();
    let radius = SIZE_PX / 2.0 - LABEL_MARGIN_PX;
    let grid = Stroke::new(1.0, Color32::from_gray(60));
    for ring in RINGS {
        painter.circle_stroke(center, radius * ring, grid);
    }
    for axis in axes {
        painter.line_segment([center, radar_point(center, radius, axis.angle, 1.0)], grid);
        let out = radius + LABEL_MARGIN_PX / 2.0;
        painter.text(
            Pos2::new(
                center.x + out * axis.angle.cos(),
                center.y - out * axis.angle.sin(),
            ),
            Align2::CENTER_CENTER,
            axis.label,
            FontId::proportional(10.0),
            Color32::from_gray(170),
        );
    }
    for s in series {
        let points = series_points(center, radius, axes, &s.values);
        painter.add(Shape::closed_line(
            points.clone(),
            Stroke::new(2.0, s.color),
        ));
        for p in points {
            painter.circle_filled(p, 2.5, s.color);
        }
    }
    response.on_hover_text(hint);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn values_run_from_the_center_out_along_their_axis() {
        let c = Pos2::new(50.0, 50.0);
        assert_eq!(radar_point(c, 40.0, 0.0, 0.0), c);
        let up = radar_point(c, 40.0, FRAC_PI_2, 0.5);
        assert!((up - Pos2::new(50.0, 30.0)).length() < 1e-4, "{up:?}");
        let right = radar_point(c, 40.0, 0.0, 2.0);
        assert!(
            (right - Pos2::new(90.0, 50.0)).length() < 1e-4,
            "clamped to 1"
        );
    }

    #[test]
    fn unmeasured_axes_sit_at_the_center() {
        let c = Pos2::new(0.0, 0.0);
        let axes = [
            RadarAxis {
                angle: 0.0,
                label: "a",
            },
            RadarAxis {
                angle: FRAC_PI_2,
                label: "b",
            },
        ];
        let p = series_points(c, 10.0, &axes, &[Some(1.0), None]);
        assert_eq!(p.len(), 2);
        assert!((p[0] - Pos2::new(10.0, 0.0)).length() < 1e-4);
        assert_eq!(p[1], c);
    }
}
