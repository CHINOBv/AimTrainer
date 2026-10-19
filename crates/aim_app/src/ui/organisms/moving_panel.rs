//! Firing-while-moving panel for movement scenarios: speed at the shot, stopped vs moving
//! accuracy and the counter-strafe check.
//!
//! [`MovingReport`] is the plain data, built from the core analysis; the panel only lays
//! it out. The rule that judges these numbers is shown with the other findings in the
//! analysis panel.

use crate::i18n::{tr, tr_fmt};
use crate::ui::molecules::stat_row;
use crate::ui::organisms::analysis_panel::{ms, pct};
use aim_core::analytics::MovingAnalysis;
use aim_core::analytics::moving::AFTER_MOVEMENT_WINDOW_SECS;
use bevy_egui::egui;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MovingReport {
    /// `(label, value)` rows, in display order.
    pub rows: Vec<(&'static str, String)>,
    /// How "moving" and "after movement" are defined, with their thresholds.
    pub definition: String,
}

impl MovingReport {
    pub fn new(analysis: &MovingAnalysis) -> Self {
        let s = analysis.summary();
        let limit = analysis.accuracy_speed_limit;
        let mut rows = Vec::new();
        rows.push((
            "Shots stopped / moving",
            format!("{} / {}", s.accurate_shots, s.shots - s.accurate_shots),
        ));
        if let Some(v) = s.median_shot_speed {
            rows.push(("Speed at the shot (median)", format!("{v:.2} m/s")));
        }
        if s.shots > 0 {
            let rate = |r: Option<f32>| r.map_or("—".to_owned(), pct);
            rows.push((
                "Hit rate stopped / moving",
                format!("{} / {}", rate(s.stopped_hit_rate), rate(s.moving_hit_rate)),
            ));
        }
        if let Some(share) = s.counter_strafe_share {
            rows.push((
                "Stopped before firing after moving",
                tr_fmt(
                    "{} of {} ({})",
                    &[
                        &s.counter_strafed_shots,
                        &s.after_movement_shots,
                        &pct(share),
                    ],
                ),
            ));
        }
        if let Some(d) = s.median_stop_to_shot_secs {
            rows.push(("Stop to shot (median)", ms(d)));
        }
        Self {
            rows,
            definition: tr_fmt(
                "Moving = above {} m/s (the accuracy limit). After moving = moving within {} ms before the shot.",
                &[
                    &format!("{limit:.2}"),
                    &format!("{:.0}", AFTER_MOVEMENT_WINDOW_SECS * 1000.0),
                ],
            ),
        }
    }
}

/// Draw the report.
pub fn moving_panel(ui: &mut egui::Ui, report: &MovingReport) {
    egui::CollapsingHeader::new(tr("Firing while moving"))
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("moving_metrics")
                .num_columns(2)
                .show(ui, |ui| {
                    for (label, value) in &report.rows {
                        stat_row(ui, tr(label), value.as_str());
                    }
                });
            ui.small(report.definition.as_str());
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::analytics::moving::MovingShot;

    fn shot(speed: f32, accurate: bool, hit: bool, stop_to_shot_secs: Option<f32>) -> MovingShot {
        MovingShot {
            time_secs: 0.0,
            speed,
            accurate,
            hit,
            after_movement: speed > 0.0 || stop_to_shot_secs.is_some(),
            stop_to_shot_secs,
        }
    }

    #[test]
    fn the_report_shows_stopped_vs_moving_and_the_counter_strafe_check() {
        let analysis = MovingAnalysis {
            accuracy_speed_limit: 1.5,
            shots: vec![
                shot(5.4, false, false, None),
                shot(0.0, true, true, Some(0.12)),
                shot(0.0, true, false, None),
                shot(0.5, true, true, Some(0.08)),
            ],
        };
        let r = MovingReport::new(&analysis);
        let rows: Vec<(&str, &str)> = r.rows.iter().map(|(l, v)| (*l, v.as_str())).collect();
        assert_eq!(
            rows,
            vec![
                ("Shots stopped / moving", "3 / 1"),
                ("Speed at the shot (median)", "0.25 m/s"),
                ("Hit rate stopped / moving", "67% / 0%"),
                ("Stopped before firing after moving", "2 of 3 (67%)"),
                ("Stop to shot (median)", "100 ms"),
            ]
        );
        assert_eq!(
            r.definition,
            "Moving = above 1.50 m/s (the accuracy limit). After moving = moving within 500 ms before the shot."
        );
    }

    #[test]
    fn a_run_without_shots_only_counts_zero_shots() {
        let r = MovingReport::new(&MovingAnalysis::default());
        assert_eq!(r.rows, vec![("Shots stopped / moving", "0 / 0".to_owned())]);
    }
}
