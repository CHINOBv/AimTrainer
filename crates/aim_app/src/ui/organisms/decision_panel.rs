//! Go/no-go results: match and decoy outcomes of a run with decoys.
//!
//! [`DecisionReport`] is the plain data the panel shows, built from the core's
//! [`DecisionSummary`]; the panel only lays it out.

use crate::ui::atoms::{GOOD, WARN};
use crate::ui::molecules::stat_row;
use aim_core::analytics::decision::{DecisionSummary, FALSE_ALARM_THRESHOLD, MISS_THRESHOLD};
use bevy_egui::egui;

const TITLE: &str = "Go / no-go decisions";
const THRESHOLD_NOTE: &str = "Thresholds are estimates until calibrated on player data.";

#[derive(Debug, Clone, PartialEq)]
pub struct DecisionReport {
    pub metrics: Vec<(&'static str, String)>,
    /// `(is_warning, text)`: each verdict names its metric and threshold.
    pub verdicts: Vec<(bool, String)>,
}

fn pct(ratio: f32) -> String {
    format!("{:.0}%", ratio * 100.0)
}

impl DecisionReport {
    pub fn new(d: &DecisionSummary) -> Self {
        let c = d.counts;
        let mut metrics = vec![
            ("Decision accuracy", pct(d.accuracy)),
            ("Targets shot (match hit)", c.match_hits.to_string()),
            ("Targets missed (match wait)", c.match_waits.to_string()),
            ("Decoys shot (non-match hit)", c.nonmatch_hits.to_string()),
            (
                "Decoys left alone (non-match wait)",
                c.nonmatch_waits.to_string(),
            ),
        ];
        if let Some(rt) = d.median_match_reaction_secs {
            metrics.push((
                "Reaction to targets (median)",
                format!("{:.0} ms", rt * 1000.0),
            ));
        }
        let mut verdicts = Vec::new();
        if let Some(r) = d.false_alarm_rate {
            verdicts.push(if d.is_impulsive() {
                (
                    true,
                    format!(
                        "Decoys shot {} is above the {} threshold: confirm the target before you click.",
                        pct(r),
                        pct(FALSE_ALARM_THRESHOLD)
                    ),
                )
            } else {
                (
                    false,
                    format!(
                        "Decoys shot {} is within the {} threshold.",
                        pct(r),
                        pct(FALSE_ALARM_THRESHOLD)
                    ),
                )
            });
        }
        if let Some(r) = d.miss_rate.filter(|_| d.is_hesitant()) {
            verdicts.push((
                true,
                format!(
                    "Targets missed {} is above the {} threshold: commit to a target once you see it.",
                    pct(r),
                    pct(MISS_THRESHOLD)
                ),
            ));
        }
        Self { metrics, verdicts }
    }
}

/// Draw the go/no-go section.
pub fn decision_panel(ui: &mut egui::Ui, report: &DecisionReport) {
    egui::CollapsingHeader::new(TITLE)
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("decision_metrics")
                .num_columns(2)
                .show(ui, |ui| {
                    for (label, value) in &report.metrics {
                        stat_row(ui, label, value.as_str());
                    }
                });
            for (warn, text) in &report.verdicts {
                if *warn {
                    ui.colored_label(WARN, text);
                } else {
                    ui.colored_label(GOOD, format!("✔ {text}"));
                }
            }
            ui.small(THRESHOLD_NOTE);
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::analytics::decision::DecisionCounts;

    fn summary(nonmatch_hits: u32, match_waits: u32) -> DecisionSummary {
        let counts = DecisionCounts {
            match_hits: 6,
            match_waits,
            nonmatch_hits,
            nonmatch_waits: 3,
        };
        DecisionSummary {
            counts,
            accuracy: 0.6,
            false_alarm_rate: Some(nonmatch_hits as f32 / (nonmatch_hits + 3) as f32),
            miss_rate: Some(match_waits as f32 / (match_waits + 6) as f32),
            median_match_reaction_secs: Some(0.45),
        }
    }

    #[test]
    fn report_lists_the_four_outcomes_and_explains_each_verdict() {
        let r = DecisionReport::new(&summary(0, 4));
        let value = |label: &str| {
            r.metrics
                .iter()
                .find(|(l, _)| *l == label)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(value("Decision accuracy").as_deref(), Some("60%"));
        assert_eq!(value("Targets shot (match hit)").as_deref(), Some("6"));
        assert_eq!(value("Targets missed (match wait)").as_deref(), Some("4"));
        assert_eq!(value("Decoys shot (non-match hit)").as_deref(), Some("0"));
        assert_eq!(
            value("Decoys left alone (non-match wait)").as_deref(),
            Some("3")
        );
        assert_eq!(
            value("Reaction to targets (median)").as_deref(),
            Some("450 ms")
        );
        assert_eq!(
            r.verdicts,
            [
                (
                    false,
                    "Decoys shot 0% is within the 20% threshold.".to_owned()
                ),
                (
                    true,
                    "Targets missed 40% is above the 20% threshold: commit to a target once you see it."
                        .to_owned()
                ),
            ]
        );
    }

    #[test]
    fn shooting_many_decoys_is_flagged_as_impulsive() {
        let r = DecisionReport::new(&summary(3, 0));
        assert_eq!(
            r.verdicts,
            [(
                true,
                "Decoys shot 50% is above the 20% threshold: confirm the target before you click."
                    .to_owned()
            )]
        );
    }
}
