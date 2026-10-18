//! Post-run analysis panel (flicks, corrections, tracking).
//!
//! [`RunReport`] is the plain data the panel shows, built once per result from the core
//! analysis; the panel itself only lays it out and turns "practice" clicks into actions.

use crate::ui::UiAction;
use crate::ui::atoms::{GOOD, WARN};
use crate::ui::molecules::stat_row;
use crate::ui::organisms::decision_panel::DecisionReport;
use crate::ui::organisms::moving_panel::MovingReport;
use aim_core::analytics::{RunAnalysis, SkillProfile};
use bevy_egui::egui;

use crate::ui::organisms::pace_panel::{PaceReport, pace_panel};
use crate::ui::organisms::priority_panel::{PriorityReport, priority_panel};

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
    /// Go/no-go section, for runs with decoys.
    pub decision: Option<DecisionReport>,
    /// Firing-while-moving details, for movement scenarios.
    pub moving: Option<MovingReport>,
    pub pace: Option<PaceReport>,
    pub priority: Option<PriorityReport>,
}

pub(crate) fn ms(secs: f32) -> String {
    format!("{:.0} ms", secs * 1000.0)
}

pub(crate) fn pct(ratio: f32) -> String {
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
            push(
                "Speed match",
                t.speed_match_ratio.map(|m| {
                    let split = t
                        .speed_under_ratio
                        .zip(t.speed_over_ratio)
                        .map_or(String::new(), |(u, o)| {
                            format!(" (too slow {} / too fast {})", pct(u), pct(o))
                        });
                    format!("{}{split}", pct(m))
                }),
            );
            push("Time on the target's edge", t.edge_ratio.map(pct));
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
            decision: analysis.decision.as_ref().map(DecisionReport::new),
            moving: analysis.moving.as_ref().map(MovingReport::new),
            pace: analysis.pace.as_ref().map(PaceReport::new),
            priority: analysis
                .priority
                .as_ref()
                .map(|p| PriorityReport::new(p, profile.findings())),
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
            ui.small("Thresholds are provisional until calibrated on player data.");
        });
    if let Some(pace) = &report.pace {
        pace_panel(ui, pace);
    }
    if let Some(priority) = &report.priority {
        priority_panel(ui, priority);
    }
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
    fn tracking_runs_show_tracking_metrics() {
        let r = report("track-precise", 2);
        assert_eq!(r.title, "Tracking analysis");
        assert!(r.metrics.iter().any(|(l, _)| *l == "Time on target"));
    }

    #[test]
    fn tracking_runs_show_speed_match_split_and_edge_share() {
        let r = report("track-precise", 2);
        let value = |label: &str| {
            r.metrics
                .iter()
                .find(|(l, _)| *l == label)
                .map(|(_, v)| v.clone())
        };
        let speed = value("Speed match").expect("a moving target gives speed match");
        assert!(
            speed.contains("too slow") && speed.contains("too fast"),
            "{speed}"
        );
        assert!(value("Time on the target's edge").is_some());
    }

    #[test]
    fn only_runs_with_decoys_get_a_decision_section() {
        let go = report("go-no-go", 0);
        let d = go.decision.expect("go-no-go has decoys");
        assert!(
            d.metrics
                .iter()
                .any(|(l, _)| *l == "Decoys shot (non-match hit)")
        );
        assert!(report("flick-wide", 1).decision.is_none());
    }

    #[test]
    fn only_movement_scenarios_carry_a_moving_report() {
        assert_eq!(report("flick-wide", 2).moving, None);
        let moving = report("counter-strafe", 2)
            .moving
            .expect("movement scenario");
        assert_eq!(moving.rows[0].0, "Shots stopped / moving");
    }

    #[test]
    fn runs_with_enough_kills_or_score_show_their_pace() {
        let r = report("flick-wide", 2);
        assert!(r.pace.as_ref().unwrap().rows[0].1.contains("kills/s"));
        let r = report("track-precise", 2);
        assert!(r.pace.as_ref().unwrap().rows[0].1.contains("points/s"));
    }

    #[test]
    fn multi_target_runs_show_their_prioritization() {
        let r = report("switch-wide", 2);
        let p = r
            .priority
            .as_ref()
            .expect("switch-wide shows three targets");
        assert!(p.rows[0].1.contains("contested kills"), "{p:?}");
        // The simulated aimer always takes the nearest target: a strength with its threshold.
        assert!(
            p.verdicts
                .iter()
                .any(|(s, t)| *s && t.contains("threshold"))
        );
        assert!(report("flick-wide", 2).priority.is_none());
    }
}
