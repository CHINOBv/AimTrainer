//! Score history chart.
//!
//! A small line chart painted with egui's painter (no plotting dependency): one point per
//! run, oldest on the left, with the personal best marked.

use crate::ui::atoms::GOOD;
use bevy_egui::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, vec2};

const HEIGHT_PX: f32 = 80.0;
const PADDING_PX: f32 = 6.0;

/// Screen positions of `scores` inside `rect`, spread evenly in x and scaled between the
/// lowest and highest score in y (a flat history sits in the middle).
fn chart_points(scores: &[f32], rect: Rect) -> Vec<Pos2> {
    let (lo, hi) = scores
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &s| {
            (lo.min(s), hi.max(s))
        });
    let span = hi - lo;
    let last = scores.len().saturating_sub(1).max(1) as f32;
    scores
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let x = rect.left() + rect.width() * i as f32 / last;
            let t = if span > 0.0 { (s - lo) / span } else { 0.5 };
            Pos2::new(x, rect.bottom() - rect.height() * t)
        })
        .collect()
}

/// Draw the chart for `scores` (oldest first). Draws nothing for fewer than two runs.
pub fn history_chart(ui: &mut egui::Ui, scores: &[f32]) {
    if scores.len() < 2 {
        return;
    }
    let width = ui.available_width().clamp(160.0, 420.0);
    let (response, painter) = ui.allocate_painter(vec2(width, HEIGHT_PX), Sense::hover());
    let frame = response.rect;
    let visuals = ui.visuals();
    painter.rect_stroke(
        frame,
        2.0,
        visuals.widgets.noninteractive.bg_stroke,
        StrokeKind::Inside,
    );
    let plot = frame.shrink(PADDING_PX);
    let points = chart_points(scores, plot);
    let line = visuals.widgets.active.fg_stroke.color;
    painter.line(points.clone(), Stroke::new(1.5, line));
    let best = scores
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i);
    for (i, p) in points.iter().enumerate() {
        let (radius, color) = if Some(i) == best {
            (4.0, GOOD)
        } else {
            (2.5, line)
        };
        painter.circle_filled(*p, radius, color);
    }
    let font = FontId::proportional(11.0);
    let weak = Color32::from_gray(160);
    let (lo, hi) = scores
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &s| {
            (lo.min(s), hi.max(s))
        });
    painter.text(
        plot.left_top(),
        Align2::LEFT_TOP,
        format!("{hi:.0}"),
        font.clone(),
        weak,
    );
    painter.text(
        plot.left_bottom(),
        Align2::LEFT_BOTTOM,
        format!("{lo:.0}"),
        font,
        weak,
    );
    response.on_hover_text(format!(
        "Last {} runs, oldest on the left. Green: best.",
        scores.len()
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_span_the_rect_from_lowest_to_highest_score() {
        let rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(100.0, 50.0));
        let p = chart_points(&[10.0, 30.0, 20.0], rect);
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
    fn a_flat_history_sits_in_the_middle() {
        let rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(10.0, 10.0));
        let p = chart_points(&[5.0, 5.0], rect);
        assert!(p.iter().all(|p| p.y == 5.0));
    }
}
