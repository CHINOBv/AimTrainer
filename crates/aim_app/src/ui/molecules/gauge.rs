//! Ring gauge: a value as a coloured arc around its number, with its label below.
//!
//! Painted with egui's painter (no plotting dependency). [`arc_points`] is the pure
//! geometry; the painting itself is not covered by automated tests.

use crate::ui::atoms::Tone;
use bevy_egui::egui::{self, Align2, Color32, FontId, Pos2, Sense, Shape, Stroke, vec2};

const SIZE_PX: f32 = 74.0;
const RING_PX: f32 = 7.0;
const LABEL_PX: f32 = 16.0;
/// Points on a full ring; partial arcs use a share of them.
const FULL_RING_SEGMENTS: f32 = 64.0;

/// One gauge, already formatted.
#[derive(Debug, Clone, PartialEq)]
pub struct GaugeModel {
    pub label: &'static str,
    /// The number in the middle, with its unit ("212 ms").
    pub value: String,
    /// Share of the ring filled, 0–1.
    pub fill: f32,
    pub tone: Tone,
    /// What the value means and which thresholds colour it (hover text).
    pub hint: String,
}

/// Points of an arc around `center` that starts at the top and runs clockwise over
/// `fill` of a full turn (clamped to 0–1). Empty for an empty arc.
pub fn arc_points(center: Pos2, radius: f32, fill: f32) -> Vec<Pos2> {
    let fill = fill.clamp(0.0, 1.0);
    if fill <= 0.0 {
        return Vec::new();
    }
    let segments = (FULL_RING_SEGMENTS * fill).ceil().max(1.0) as usize;
    (0..=segments)
        .map(|i| {
            let a = std::f32::consts::TAU * fill * i as f32 / segments as f32;
            Pos2::new(center.x + radius * a.sin(), center.y - radius * a.cos())
        })
        .collect()
}

/// Draw one gauge.
pub fn gauge(ui: &mut egui::Ui, g: &GaugeModel) -> egui::Response {
    let (response, painter) =
        ui.allocate_painter(vec2(SIZE_PX, SIZE_PX + LABEL_PX), Sense::hover());
    let rect = response.rect;
    let center = Pos2::new(rect.center().x, rect.top() + SIZE_PX / 2.0);
    let radius = SIZE_PX / 2.0 - RING_PX;
    painter.circle_stroke(center, radius, Stroke::new(RING_PX, Color32::from_gray(55)));
    let arc = arc_points(center, radius, g.fill);
    if arc.len() >= 2 {
        painter.add(Shape::line(arc, Stroke::new(RING_PX, g.tone.color())));
    }
    painter.text(
        center,
        Align2::CENTER_CENTER,
        &g.value,
        FontId::proportional(15.0),
        ui.visuals().strong_text_color(),
    );
    painter.text(
        Pos2::new(center.x, rect.bottom()),
        Align2::CENTER_BOTTOM,
        g.label,
        FontId::proportional(12.0),
        ui.visuals().text_color(),
    );
    response.on_hover_text(&g.hint)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Pos2, b: Pos2) -> bool {
        (a - b).length() < 1e-3
    }

    #[test]
    fn an_arc_starts_at_the_top_and_runs_clockwise() {
        let c = Pos2::new(10.0, 10.0);
        let quarter = arc_points(c, 5.0, 0.25);
        assert!(close(quarter[0], Pos2::new(10.0, 5.0)), "{quarter:?}");
        assert!(close(*quarter.last().unwrap(), Pos2::new(15.0, 10.0)));
        let half = arc_points(c, 5.0, 0.5);
        assert!(close(*half.last().unwrap(), Pos2::new(10.0, 15.0)));
    }

    #[test]
    fn empty_and_overfull_arcs_are_clamped() {
        let c = Pos2::new(0.0, 0.0);
        assert!(arc_points(c, 5.0, 0.0).is_empty());
        assert!(arc_points(c, 5.0, -1.0).is_empty());
        let full = arc_points(c, 5.0, 3.0);
        assert!(close(full[0], *full.last().unwrap()), "a full ring closes");
    }
}
