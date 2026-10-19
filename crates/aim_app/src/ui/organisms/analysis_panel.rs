//! Post-run analysis panel (flicks, corrections, tracking).
//!
//! [`RunReport`] is the plain data the panel shows, built once per result from the core
//! analysis; the panel itself only lays it out and turns "practice" clicks into actions.

use crate::i18n::{tr, tr_fmt};
use crate::ui::UiAction;
use crate::ui::atoms::{GOOD, WARN};
use crate::ui::molecules::gauge::GaugeModel;
use crate::ui::molecules::stat_row;
use crate::ui::organisms::audio_cue_panel::AudioCueReport;
use crate::ui::organisms::decision_panel::DecisionReport;
use crate::ui::organisms::flick_directions::{DirectionGrid, flick_directions};
use crate::ui::organisms::miss_compass::MissCompassGrid;
use crate::ui::organisms::moving_panel::MovingReport;
use crate::ui::organisms::perception_panel::{DetectionReport, RadarRecallReport};
use crate::ui::organisms::potential_tip::PotentialLine;
use crate::ui::organisms::results_visuals::{MainFix, analysis_gauges, main_fix};
use aim_core::analytics::damage::{DamageEfficiency, TtkSpread};
use aim_core::analytics::{RunAnalysis, SkillProfile, rules};
use bevy_egui::egui;

use crate::ui::organisms::pace_panel::{PaceReport, pace_panel};
use crate::ui::organisms::priority_panel::{PriorityReport, priority_panel};

/// Recommendations shown on the results card.
pub const MAX_RECOMMENDATIONS: usize = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct Advice {
    /// The metric and its value, one short line ("Overshoot rate 42%").
    pub headline: String,
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
    /// Per-direction flick grid (click runs only).
    pub directions: Option<DirectionGrid>,
    /// Where missed shots went around the aim point, when any shot missed a target.
    pub miss_compass: Option<MissCompassGrid>,
    /// The one fix with the largest estimated gain, shown above the advice.
    pub potential: Option<PotentialLine>,
    /// Turn reaction and angular error per direction, for sound-localisation runs.
    pub audio_cue: Option<AudioCueReport>,
    /// Per-area detection and false alarms, for peripheral detection runs.
    pub detection: Option<DetectionReport>,
    /// Correct and incorrect picks, for radar-recall runs.
    pub radar_recall: Option<RadarRecallReport>,
    /// Median time to kill and its interquartile range, with enough kills.
    pub ttk_spread: Option<String>,
    /// Damage dealt against damage available, for hold runs and multi-hit targets.
    pub damage_efficiency: Option<String>,
    /// Gauges for the run's main skill, coloured by the recommendation thresholds.
    pub gauges: Vec<GaugeModel>,
    /// The one fix the results card leads with (set by [`RunReport::new`]).
    pub main_fix: Option<MainFix>,
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
        // Detection clicks are not aimed: the run has no flicks to report.
        let flicks = analysis
            .flick_summary()
            .filter(|_| analysis.detection.is_none());
        let title = if let Some(f) = flicks {
            push(
                "Flicks",
                Some(tr_fmt("{} ({} hit)", &[&f.flicks, &pct(f.hit_rate)])),
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
                    let side = t.mean_along_error_deg.map_or(String::new(), |e| {
                        tr_fmt(" (mean {}°)", &[&format!("{e:+.2}")])
                    });
                    format!("{}{side}", pct(r))
                }),
            );
            push(
                "Reaction to direction changes",
                t.median_reversal_response_secs
                    .map(|s| tr_fmt("{} over {} reversals", &[&ms(s), &t.reversals])),
            );
            push(
                "Speed match",
                t.speed_match_ratio.map(|m| {
                    let split = t
                        .speed_under_ratio
                        .zip(t.speed_over_ratio)
                        .map_or(String::new(), |(u, o)| {
                            tr_fmt(" (too slow {} / too fast {})", &[&pct(u), &pct(o)])
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
        let strengths = profile
            .strengths()
            .map(|f| f.explanation_in(crate::i18n::language()))
            .collect();
        let advice = profile
            .recommendations()
            .into_iter()
            .filter_map(|f| f.recommendation.as_ref().map(|r| (f, r)))
            .take(MAX_RECOMMENDATIONS)
            .map(|(f, r)| Advice {
                headline: format!(
                    "{} {}",
                    f.metric.label_in(crate::i18n::language()),
                    f.metric.format(f.value)
                ),
                explanation: f.explanation_in(crate::i18n::language()),
                tip: tr(&r.tip).to_owned(),
                practice: r
                    .scenario_ids
                    .iter()
                    .filter_map(|id| name_of(id).map(|n| (id.clone(), tr(&n).to_owned())))
                    .collect(),
            })
            .collect();
        let mut report = Self {
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
            directions: DirectionGrid::new(analysis),
            miss_compass: analysis.miss_compass.as_ref().map(MissCompassGrid::new),
            potential: analysis.potential.as_ref().map(PotentialLine::new),
            audio_cue: analysis.audio_cue.as_ref().map(AudioCueReport::new),
            detection: analysis.detection.as_ref().map(DetectionReport::new),
            radar_recall: analysis.radar_recall.as_ref().map(RadarRecallReport::new),
            ttk_spread: analysis.ttk_spread.as_ref().map(ttk_spread_text),
            damage_efficiency: analysis
                .damage_efficiency
                .as_ref()
                .map(damage_efficiency_text),
            gauges: analysis_gauges(analysis, &rules::builtin::rules().rules),
            main_fix: None,
        };
        report.main_fix = Some(main_fix(&report));
        report
    }
}

/// "412 ms (IQR 150 ms, 12 kills)": the median time to kill and how much it varies.
pub fn ttk_spread_text(s: &TtkSpread) -> String {
    tr_fmt(
        "{} (IQR {}, {} kills)",
        &[&ms(s.median_secs), &ms(s.iqr_secs()), &s.kills],
    )
}

/// "75% (30 of 40)": damage dealt against damage available.
pub fn damage_efficiency_text(e: &DamageEfficiency) -> String {
    tr_fmt(
        "{} ({} of {})",
        &[
            &pct(e.ratio()),
            &format!("{:.0}", e.done),
            &format!("{:.0}", e.possible),
        ],
    )
}

/// Draw the report. Returns an action when a "practice" button is clicked.
pub fn analysis_panel(ui: &mut egui::Ui, report: &RunReport) -> Option<UiAction> {
    let mut action = None;
    egui::CollapsingHeader::new(tr(report.title))
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("analysis_metrics")
                .num_columns(2)
                .show(ui, |ui| {
                    for (label, value) in &report.metrics {
                        stat_row(ui, tr(label), value.as_str());
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
                        if ui.small_button(tr_fmt("Practice: {}", &[name])).clicked() {
                            action = Some(UiAction::StartScenario(id.clone()));
                        }
                    }
                });
            }
            if report.strengths.is_empty() && report.advice.is_empty() {
                ui.small(tr("No metric crossed a threshold in this run."));
            }
            if let Some(grid) = &report.directions {
                flick_directions(ui, grid);
            }
            ui.small(tr(
                "Thresholds are provisional until calibrated on player data.",
            ));
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

    /// New per-run sections (miss compass, potential score) never add or reorder metric rows,
    /// and the unbiased simulated aimer gets no miss-compass advice.
    #[test]
    fn existing_metric_rows_and_advice_are_unchanged_by_the_new_sections() {
        let r = report("flick-wide", 0);
        let labels: Vec<&str> = r.metrics.iter().map(|(l, _)| *l).collect();
        assert_eq!(
            labels,
            [
                "Flicks",
                "Reaction (median)",
                "Movement time (median)",
                "Peak speed (median)",
                "Overshoot / undershoot",
                "Corrections per flick",
                "Settle time (median)",
                "Path efficiency",
                "Throughput (ISO 9241-9)",
                "Click delay (median)",
            ]
        );
        let t = report("track-precise", 2);
        let labels: Vec<&str> = t.metrics.iter().map(|(l, _)| *l).collect();
        assert_eq!(
            labels,
            [
                "Time on target",
                "Error from center (median)",
                "Behind the target",
                "Reaction to direction changes",
                "Speed match",
                "Time on the target's edge",
            ]
        );
        for r in [&r, &t] {
            assert!(
                r.advice
                    .iter()
                    .all(|a| !a.explanation.starts_with("Misses ")),
                "{:?}",
                r.advice
            );
        }
    }

    #[test]
    fn click_runs_with_misses_show_a_miss_compass_and_hold_runs_do_not() {
        let r = report("flick-wide", 0);
        let grid = r.miss_compass.expect("the recruit misses");
        assert!(grid.split_note.ends_with(" misses"), "{grid:?}");
        assert_eq!(report("track-precise", 2).miss_compass, None);
    }

    #[test]
    fn a_click_run_with_a_costly_weakness_shows_one_potential_score_tip() {
        let line = report("flick-wide", 0)
            .potential
            .expect("the recruit's misses are worth a tip");
        assert!(line.headline.starts_with("Fix "), "{line:?}");
        assert!(line.assumption.starts_with("Estimate: if "), "{line:?}");
        assert_eq!(report("track-precise", 2).potential, None);
    }

    #[test]
    fn only_sound_cue_runs_get_a_sound_localisation_section() {
        let r = report("sound-turn", 2)
            .audio_cue
            .expect("sound-turn plays cues");
        assert_eq!(r.metrics[0].0, "Sound cues");
        assert_eq!(r.rows[1][1], "you");
        assert_eq!(report("flick-wide", 2).audio_cue, None);
    }

    #[test]
    fn perception_runs_get_their_own_sections_and_detection_runs_report_no_flicks() {
        let detect = report("peripheral-detect", 1);
        let d = detect.detection.expect("a detection run");
        assert_eq!(d.metrics[0].0, "Flashes seen");
        assert_eq!(d.rows[1][1], "+");
        assert!(detect.metrics.is_empty(), "{:?}", detect.metrics);
        assert_eq!(detect.radar_recall, None);
        let radar = report("radar-recall", 1);
        let r = radar.radar_recall.expect("a radar run");
        assert_eq!(r.metrics[0].0, "Correct picks");
        assert!(radar.decision.is_some(), "the go/no-go section stays");
        let plain = report("flick-wide", 1);
        assert_eq!((plain.detection, plain.radar_recall), (None, None));
    }

    #[test]
    fn every_analysed_run_gets_gauges_and_one_main_fix() {
        let r = report("flick-wide", 0);
        assert!(!r.gauges.is_empty());
        let fix = r
            .main_fix
            .as_ref()
            .expect("RunReport::new sets the main fix");
        assert_eq!(*fix, main_fix(&r));
        let first = r.advice.first().expect("recruit has a weakness");
        // The headline is the metric and value that open the explanation.
        assert!(
            first
                .explanation
                .starts_with(&format!("{} is ", first.headline)),
            "{first:?}"
        );
        assert!(report("track-precise", 2).main_fix.is_some());
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

    #[test]
    fn ttk_spread_and_damage_efficiency_are_worded_for_the_results_card() {
        use aim_core::analytics::damage::{DamageEfficiency, TtkSpread};
        let spread = TtkSpread {
            kills: 12,
            median_secs: 0.412,
            q1_secs: 0.35,
            q3_secs: 0.5,
        };
        assert_eq!(ttk_spread_text(&spread), "412 ms (IQR 150 ms, 12 kills)");
        let e = DamageEfficiency {
            done: 30.0,
            possible: 40.0,
        };
        assert_eq!(damage_efficiency_text(&e), "75% (30 of 40)");
        let click = report("flick-wide", 2);
        assert!(click.ttk_spread.is_some());
        assert_eq!(click.damage_efficiency, None);
        assert!(report("track-precise", 2).damage_efficiency.is_some());
    }
}
