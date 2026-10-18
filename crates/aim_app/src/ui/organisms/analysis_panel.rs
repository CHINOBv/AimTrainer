//! Post-run analysis panel (flicks, corrections, tracking).
//!
//! [`RunReport`] is the plain data the panel shows, built once per result from the core
//! analysis; the panel itself only lays it out and turns "practice" clicks into actions.

use crate::ui::UiAction;
use crate::ui::atoms::{GOOD, WARN};
use crate::ui::molecules::stat_row;
use crate::ui::organisms::flick_directions::{DirectionGrid, flick_directions};
use aim_core::analytics::{RunAnalysis, SkillProfile};
use bevy_egui::egui;

/// Recommendations shown on the results card.
pub const MAX_RECOMMENDATIONS: usize = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct Advice {
    /// Metric, value and threshold that triggered it, in plain English.
    pub explanation: String,
    pub tip: String,
    /// `(scenario id, display name)` to practice, best first.
    pub practice: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunReport {
    pub title: &'static str,
    pub metrics: Vec<(&'static str, String)>,
    pub strengths: Vec<String>,
    pub advice: Vec<Advice>,
    /// Per-direction flick grid (click runs only).
    pub directions: Option<DirectionGrid>,
}

fn ms(secs: f32) -> String {
    format!("{:.0} ms", secs * 1000.0)
}

fn pct(ratio: f32) -> String {
    format!("{:.0}%", ratio * 100.0)
}

impl RunReport {
    /// `name_of` maps a scenario id to its display name; ids it does not know are skipped.
    pub fn new(analysis: &RunAnalysis, name_of: impl Fn(&str) -> Option<String>) -> Self {
        let mut metrics = Vec::new();
        let mut push = |label, value: Option<String>| {
            if let Some(v) = value {
                metrics.push((label, v));
            }
        };
        let title = if let Some(f) = analysis.flick_summary() {
            push(
                "Flicks",
                Some(format!("{} ({} hit)", f.flicks, pct(f.hit_rate))),
            );
            push("Reaction (median)", f.median_reaction_secs.map(ms));
            push("Movement time (median)", f.median_movement_secs.map(ms));
            push(
                "Peak speed (median)",
                f.median_peak_speed_deg_per_sec
                    .map(|v| format!("{v:.0}°/s")),
            );
            push(
                "Overshoot / undershoot",
                Some(format!(
                    "{} / {}",
                    pct(f.overshoot_rate),
                    pct(f.undershoot_rate)
                )),
            );
            push(
                "Corrections per flick",
                Some(format!("{:.2}", f.mean_corrections)),
            );
            push("Settle time (median)", f.median_settle_secs.map(ms));
            push("Path efficiency", f.median_path_efficiency.map(pct));
            push(
                "Throughput (ISO 9241-9)",
                f.throughput_bits_per_sec.map(|v| format!("{v:.1} bits/s")),
            );
            push("Click delay (median)", f.median_click_delay_secs.map(ms));
            "Flick analysis"
        } else if let Some(t) = analysis.tracking_summary() {
            push("Time on target", Some(pct(t.on_target_ratio)));
            push(
                "Error from center (median)",
                t.median_error_deg.map(|v| format!("{v:.2}°")),
            );
            push(
                "Behind the target",
                t.lag_ratio.map(|r| {
                    let side = t
                        .mean_along_error_deg
                        .map_or(String::new(), |e| format!(" (mean {e:+.2}°)"));
                    format!("{}{side}", pct(r))
                }),
            );
            push(
                "Reaction to direction changes",
                t.median_reversal_response_secs
                    .map(|s| format!("{} over {} reversals", ms(s), t.reversals)),
            );
            "Tracking analysis"
        } else {
            "Analysis"
        };

        let profile = SkillProfile::build(std::slice::from_ref(analysis));
        let strengths = profile.strengths().map(|f| f.explanation.clone()).collect();
        let advice = profile
            .recommendations()
            .into_iter()
            .filter_map(|f| f.recommendation.as_ref().map(|r| (f, r)))
            .take(MAX_RECOMMENDATIONS)
            .map(|(f, r)| Advice {
                explanation: f.explanation.clone(),
                tip: r.tip.clone(),
                practice: r
                    .scenario_ids
                    .iter()
                    .filter_map(|id| name_of(id).map(|n| (id.clone(), n)))
                    .collect(),
            })
            .collect();
        Self {
            title,
            metrics,
            strengths,
            advice,
            directions: DirectionGrid::new(analysis),
        }
    }
}

/// Draw the report. Returns an action when a "practice" button is clicked.
pub fn analysis_panel(ui: &mut egui::Ui, report: &RunReport) -> Option<UiAction> {
    let mut action = None;
    egui::CollapsingHeader::new(report.title)
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("analysis_metrics")
                .num_columns(2)
                .show(ui, |ui| {
                    for (label, value) in &report.metrics {
                        stat_row(ui, label, value.as_str());
                    }
                });
            for s in &report.strengths {
                ui.colored_label(GOOD, format!("✔ {s}"));
            }
            for a in &report.advice {
                ui.add_space(4.0);
                ui.colored_label(WARN, &a.explanation);
                ui.label(a.tip.as_str());
                ui.horizontal_wrapped(|ui| {
                    for (id, name) in &a.practice {
                        if ui.small_button(format!("Practice: {name}")).clicked() {
                            action = Some(UiAction::StartScenario(id.clone()));
                        }
                    }
                });
            }
            if report.strengths.is_empty() && report.advice.is_empty() {
                ui.small("No metric crossed a threshold in this run.");
            }
            if let Some(grid) = &report.directions {
                flick_directions(ui, grid);
            }
            ui.small("Thresholds are provisional until calibrated on player data.");
        });
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::analytics::analyze;
    use aim_core::scenario::{Recording, builtin};
    use aim_core::sim::{AimerProfile, SimAimer};

    fn report(id: &str, profile: usize) -> RunReport {
        let def = builtin::all().into_iter().find(|d| d.id == id).unwrap();
        let mut rec = Recording::new(def, 3);
        let mut live = rec.start();
        let mut aimer = SimAimer::new(AimerProfile::PRESETS[profile], 3);
        while !live.is_finished() {
            let i = aimer.input(&live, 1.0 / 240.0);
            rec.inputs.push(i);
            live.update(i);
        }
        let names = builtin::all();
        RunReport::new(&analyze(&rec), |id| {
            names.iter().find(|d| d.id == id).map(|d| d.name.clone())
        })
    }

    #[test]
    fn a_slow_flicker_gets_explained_advice_with_practice_scenarios() {
        let r = report("flick-wide", 0);
        assert_eq!(r.title, "Flick analysis");
        assert!(r.metrics.iter().any(|(l, _)| *l == "Reaction (median)"));
        let first = r.advice.first().expect("recruit has a weakness");
        assert!(first.explanation.contains("threshold"), "{first:?}");
        assert!(!first.practice.is_empty());
        assert!(r.advice.len() <= MAX_RECOMMENDATIONS);
    }

    #[test]
    fn flick_runs_show_the_median_click_delay_after_the_existing_metrics() {
        let r = report("flick-wide", 2);
        let labels: Vec<&str> = r.metrics.iter().map(|(l, _)| *l).collect();
        let click = labels
            .iter()
            .position(|l| *l == "Click delay (median)")
            .expect("click delay row");
        let throughput = labels
            .iter()
            .position(|l| *l == "Throughput (ISO 9241-9)")
            .expect("existing rows are kept");
        assert!(click > throughput, "{labels:?}");
        assert!(
            r.metrics[click].1.ends_with(" ms"),
            "{:?}",
            r.metrics[click]
        );
    }

    #[test]
    fn tracking_runs_show_tracking_metrics() {
        let r = report("track-precise", 2);
        assert_eq!(r.title, "Tracking analysis");
        assert!(r.metrics.iter().any(|(l, _)| *l == "Time on target"));
    }
}
