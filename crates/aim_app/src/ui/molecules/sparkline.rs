//! Sparkline: a small line of values with no axes, the last one marked. Pace and skill
//! trends use it; the score history chart shares its point layout.
//!
//! Painted with egui's painter. [`spark_points`] is the pure geometry; the painting itself
//! is not covered by automated tests.

use bevy_egui::egui::{self, Color32, Pos2, Rect, Sense, Stroke, vec2};

const HEIGHT_PX: f32 = 26.0;
const PADDING_PX: f32 = 3.0;

/// Screen positions of `values` inside `rect`, spread evenly in x and scaled between the
/// lowest and highest value in y (a flat series sits in the middle).
pub fn spark_points(values: &[f32], rect: Rect) -> Vec<Pos2> {
    let (lo, hi) = values
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &s| {
            (lo.min(s), hi.max(s))
        });
    let span = hi - lo;
    let last = values.len().saturating_sub(1).max(1) as f32;
    values
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let x = rect.left() + rect.width() * i as f32 / last;
            let t = if span > 0.0 { (s - lo) / span } else { 0.5 };
            Pos2::new(x, rect.bottom() - rect.height() * t)
        })
        .collect()
}

/// Draw a sparkline `width` pixels wide. Draws nothing for fewer than two values.
pub fn sparkline(
    ui: &mut egui::Ui,
    values: &[f32],
    width: f32,
    color: Color32,
) -> Option<egui::Response> {
    if values.len() < 2 {
        return None;
    }
    let (response, painter) = ui.allocate_painter(vec2(width, HEIGHT_PX), Sense::hover());
    let points = spark_points(values, response.rect.shrink(PADDING_PX));
    painter.line(points.clone(), Stroke::new(1.5, color));
    if let Some(last) = points.last() {
        painter.circle_filled(*last, 2.5, color);
    }
    Some(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_span_the_rect_from_lowest_to_highest_value() {
        let rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(100.0, 50.0));
        let p = spark_points(&[10.0, 30.0, 20.0], rect);
        assert_eq!(
            p,
            vec![
                Pos2::new(0.0, 50.0),
                Pos2::new(50.0, 0.0),
                Pos2::new(100.0, 25.0)
            ]
        );
    }

    #[test]
    fn a_flat_series_sits_in_the_middle() {
        let rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(10.0, 10.0));
        let p = spark_points(&[5.0, 5.0], rect);
        assert!(p.iter().all(|p| p.y == 5.0));
    }
}
