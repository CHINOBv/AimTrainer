//! Visual summary at the top of the results card: the score with its change against the
//! personal best, a row of gauges coloured by the recommendation thresholds, the one main
//! fix with a "Watch it" button, and the miss compass, flick directions and pace as charts.
//!
//! The builders here are pure and tested; the view only lays the models out (painting is
//! not covered by automated tests).

use crate::i18n::{tr, tr_fmt};
use crate::ui::UiAction;
use crate::ui::atoms::{NEUTRAL, Tone, big};
use crate::ui::molecules::gauge::{GaugeModel, gauge};
use crate::ui::molecules::radar::{RadarAxis, RadarSeries, radar};
use crate::ui::molecules::radial_wedges::radial_wedges;
use crate::ui::molecules::sparkline::sparkline;
use crate::ui::organisms::analysis_panel::RunReport;
use aim_core::analytics::RunAnalysis;
use aim_core::analytics::rules::{Direction, Metric, Rule, Verdict};
use bevy_egui::egui::{self, Color32};

// Every string this section shows.
const FIRST_RUN: &str = "First run recorded";
const ACCURACY: &str = "Accuracy";
const ACCURACY_HINT: &str =
    "Hits ÷ shots. No threshold colours it: accuracy only means something next to speed.";
const REACTION: &str = "Reaction";
const OVERSHOOT: &str = "Overshoot";
const CLICK_DELAY: &str = "Click delay";
const ON_TARGET: &str = "On target";
const SPEED_MATCH: &str = "Speed match";
const BEHIND: &str = "Behind";
const FIX_ICON: &str = "⚠";
const CLEAN_ICON: &str = "✔";
const CLEAN: &str = "No weakness crossed a threshold";
const CLEAN_HINT: &str = "Every measured metric is inside its thresholds for this run.";
const WATCH: &str = "▶ Watch it";
const WATCH_HINT: &str = "Replay this run and see where the crosshair went.";
const MISSES: &str = "Misses";
const MISSES_HINT: &str = "Where your misses went around the head (humanoids) or the center (spheres). Bigger wedge = more misses.";
const DIRECTIONS: &str = "Flick hit rate";
const DIRECTIONS_HINT: &str = "Hit rate of fresh flicks toward each screen quadrant; blue near, purple far. Farther out = better.";
const PACE: &str = "Pace";
const PACE_HINT: &str = "Kills or points per second in each window of the run, start to end.";

/// The far series of the flick-direction radar.
const FAR_COLOR: Color32 = Color32::from_rgb(190, 130, 240);
/// Full-ring values of the gauges whose unit is time, seconds.
const REACTION_SCALE_SECS: f32 = 0.5;
const CLICK_DELAY_SCALE_SECS: f32 = 0.4;
const SPARKLINE_WIDTH_PX: f32 = 140.0;

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreHero {
    pub score: String,
    /// "▲ +12 · new personal best", "▼ -30 vs best 480" or "First run recorded".
    pub delta: String,
    pub tone: Tone,
    /// "24 kills · 48.0/min".
    pub sub: String,
}

/// The single fix the card leads with.
#[derive(Debug, Clone, PartialEq)]
pub struct MainFix {
    pub icon: &'static str,
    /// One short line.
    pub headline: String,
    /// The explanation and the tip (hover text).
    pub hint: String,
    pub tone: Tone,
    /// `(scenario id, name)` to practice it.
    pub practice: Option<(String, String)>,
}

fn rules_of(rules: &[Rule], metric: Metric) -> impl Iterator<Item = &Rule> {
    rules.iter().filter(move |r| r.metric == metric)
}

/// Bad past a weakness threshold, good past a strength threshold; in between, warn when the
/// metric has a strength to reach and good when it only has a weakness to avoid. Neutral
/// when no rule names the metric.
pub fn metric_tone(rules: &[Rule], metric: Metric, value: f32) -> Tone {
    let fires = |verdict| rules_of(rules, metric).any(|r| r.verdict == verdict && r.fires(value));
    let has = |verdict| rules_of(rules, metric).any(|r| r.verdict == verdict);
    if rules_of(rules, metric).next().is_none() {
        Tone::Neutral
    } else if fires(Verdict::Weakness) {
        Tone::Bad
    } else if fires(Verdict::Strength) {
        Tone::Good
    } else if has(Verdict::Strength) {
        Tone::Warn
    } else {
        Tone::Good
    }
}

/// "Weakness above 300 ms · strength below 220 ms (provisional thresholds).", or empty when
/// no rule names the metric.
pub fn threshold_hint(rules: &[Rule], metric: Metric) -> String {
    let parts: Vec<String> = rules_of(rules, metric)
        .map(|r| {
            // One whole template per verdict and direction, so each language can order it.
            let template = match (r.verdict, r.direction) {
                (Verdict::Weakness, Direction::Above) => "weakness above {}",
                (Verdict::Weakness, Direction::Below) => "weakness below {}",
                (Verdict::Strength, Direction::Above) => "strength above {}",
                (Verdict::Strength, Direction::Below) => "strength below {}",
            };
            tr_fmt(template, &[&metric.format(r.threshold)])
        })
        .collect();
    if parts.is_empty() {
        return String::new();
    }
    let text = tr_fmt("{} (provisional thresholds).", &[&parts.join(" · ")]);
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

pub fn score_hero(
    score: f32,
    previous_best: Option<f32>,
    kills: u32,
    kills_per_min: f32,
) -> ScoreHero {
    let (delta, tone) = match previous_best {
        None => (tr(FIRST_RUN).to_owned(), Tone::Neutral),
        Some(pb) if score > pb => (
            tr_fmt(
                "▲ {} · new personal best",
                &[&format!("{:+.0}", score - pb)],
            ),
            Tone::Good,
        ),
        Some(pb) if score < pb => (
            tr_fmt(
                "▼ {} vs best {}",
                &[&format!("{:+.0}", score - pb), &format!("{pb:.0}")],
            ),
            Tone::Bad,
        ),
        Some(pb) => (tr_fmt("= your best {}", &[&format!("{pb:.0}")]), Tone::Warn),
    };
    ScoreHero {
        score: format!("{score:.0}"),
        delta,
        tone,
        sub: tr_fmt(
            "{} kills · {}/min",
            &[&kills, &format!("{kills_per_min:.1}")],
        ),
    }
}

pub fn accuracy_gauge(accuracy: f32) -> GaugeModel {
    GaugeModel {
        label: tr(ACCURACY),
        value: format!("{:.0}%", accuracy * 100.0),
        fill: accuracy.clamp(0.0, 1.0),
        tone: Tone::Neutral,
        hint: tr(ACCURACY_HINT).to_owned(),
    }
}

/// A gauge for a rule metric: the ring is `value / full` of a turn.
fn metric_gauge(
    rules: &[Rule],
    label: &'static str,
    metric: Metric,
    value: f32,
    full: f32,
) -> GaugeModel {
    GaugeModel {
        label: tr(label),
        value: metric.format(value),
        fill: (value / full).clamp(0.0, 1.0),
        tone: metric_tone(rules, metric, value),
        hint: format!(
            "{}: {}. {}",
            metric.label_in(crate::i18n::language()),
            metric.format(value),
            threshold_hint(rules, metric)
        ),
    }
}

/// Up to three gauges for the run's main skill: reaction, overshoot and click delay for
/// flick runs; time on target, speed match and lag for tracking runs.
pub fn analysis_gauges(analysis: &RunAnalysis, rules: &[Rule]) -> Vec<GaugeModel> {
    let mut out = Vec::new();
    let mut push = |label, metric, value: Option<f32>, full| {
        if let Some(v) = value {
            out.push(metric_gauge(rules, label, metric, v, full));
        }
    };
    // Detection clicks are not aimed: no flick gauges (as in the analysis panel).
    if let Some(f) = analysis
        .flick_summary()
        .filter(|_| analysis.detection.is_none())
    {
        push(
            REACTION,
            Metric::ReactionTime,
            f.median_reaction_secs,
            REACTION_SCALE_SECS,
        );
        push(
            OVERSHOOT,
            Metric::OvershootRate,
            Some(f.overshoot_rate),
            1.0,
        );
        push(
            CLICK_DELAY,
            Metric::ClickDelay,
            f.median_click_delay_secs,
            CLICK_DELAY_SCALE_SECS,
        );
    } else if let Some(t) = analysis.tracking_summary() {
        push(
            ON_TARGET,
            Metric::OnTargetRatio,
            Some(t.on_target_ratio),
            1.0,
        );
        push(SPEED_MATCH, Metric::SpeedMatch, t.speed_match_ratio, 1.0);
        push(BEHIND, Metric::LagRatio, t.lag_ratio, 1.0);
    }
    out
}

/// The potential-score tip when there is one, otherwise the most severe advice, otherwise a
/// clean bill.
pub fn main_fix(report: &RunReport) -> MainFix {
    let top = report.advice.first();
    let practice = top.and_then(|a| a.practice.first().cloned());
    if let Some(p) = &report.potential {
        let hint = match top {
            Some(a) => format!("{}\n{}", p.assumption, a.tip),
            None => p.assumption.clone(),
        };
        MainFix {
            icon: FIX_ICON,
            headline: p.headline.clone(),
            hint,
            tone: if top.is_some() { Tone::Bad } else { Tone::Warn },
            practice,
        }
    } else if let Some(a) = top {
        MainFix {
            icon: FIX_ICON,
            headline: tr_fmt("Work on: {}", &[&a.headline]),
            hint: format!("{}\n{}", a.explanation, a.tip),
            tone: Tone::Bad,
            practice,
        }
    } else {
        MainFix {
            icon: CLEAN_ICON,
            headline: tr(CLEAN).to_owned(),
            hint: tr(CLEAN_HINT).to_owned(),
            tone: Tone::Good,
            practice: None,
        }
    }
}

/// What the visual summary draws.
pub struct VisualsView<'a> {
    pub hero: &'a ScoreHero,
    /// Accuracy first, then the analysis gauges.
    pub gauges: &'a [GaugeModel],
    pub report: Option<&'a RunReport>,
    /// The run has a replay to watch.
    pub can_watch: bool,
}

fn hero(ui: &mut egui::Ui, h: &ScoreHero) {
    ui.horizontal(|ui| {
        ui.label(big(&h.score, 44.0));
        ui.vertical(|ui| {
            ui.add_space(6.0);
            ui.colored_label(h.tone.color(), egui::RichText::new(&h.delta).strong());
            ui.small(&h.sub);
        });
    });
}

fn main_fix_card(ui: &mut egui::Ui, fix: &MainFix, can_watch: bool) -> Option<UiAction> {
    let mut action = None;
    egui::Frame::group(ui.style())
        .stroke(egui::Stroke::new(1.5, fix.tone.color()))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(fix.tone.color(), big(fix.icon, 22.0));
                ui.strong(&fix.headline).on_hover_text(&fix.hint);
                if can_watch
                    && fix.tone != Tone::Good
                    && ui.button(tr(WATCH)).on_hover_text(tr(WATCH_HINT)).clicked()
                {
                    action = Some(UiAction::WatchMistakes);
                }
                if let Some((id, name)) = &fix.practice
                    && ui.small_button(tr_fmt("Practice: {}", &[name])).clicked()
                {
                    action = Some(UiAction::StartScenario(id.clone()));
                }
            });
        });
    action
}

/// Quadrant axes of the flick radar, counter-clockwise from up-right (the order of
/// [`Quadrant::ALL`](aim_core::analytics::direction::Quadrant::ALL)).
const QUADRANT_AXES: [RadarAxis; 4] = [
    RadarAxis {
        angle: std::f32::consts::FRAC_PI_4,
        label: "↗",
    },
    RadarAxis {
        angle: 3.0 * std::f32::consts::FRAC_PI_4,
        label: "↖",
    },
    RadarAxis {
        angle: 5.0 * std::f32::consts::FRAC_PI_4,
        label: "↙",
    },
    RadarAxis {
        angle: 7.0 * std::f32::consts::FRAC_PI_4,
        label: "↘",
    },
];

fn charts(ui: &mut egui::Ui, report: &RunReport) {
    ui.horizontal_top(|ui| {
        if let Some(c) = &report.miss_compass {
            ui.vertical(|ui| {
                ui.small(tr(MISSES));
                let hint = format!("{}\n{}", tr(MISSES_HINT), c.split_note);
                radial_wedges(ui, &c.counts, &c.busiest_wedges, &hint);
            });
        }
        if let Some(d) = &report.directions {
            ui.vertical(|ui| {
                ui.small(tr(DIRECTIONS));
                let series = [
                    RadarSeries {
                        values: d.near_rates.to_vec(),
                        color: NEUTRAL,
                    },
                    RadarSeries {
                        values: d.far_rates.to_vec(),
                        color: FAR_COLOR,
                    },
                ];
                let hint = match &d.weakest_note {
                    Some(w) => format!("{}\n{w}", tr(DIRECTIONS_HINT)),
                    None => tr(DIRECTIONS_HINT).to_owned(),
                };
                radar(ui, &QUADRANT_AXES, &series, &hint);
            });
        }
        if let Some(p) = &report.pace {
            ui.vertical(|ui| {
                ui.small(tr(PACE));
                let warn = p.flags.iter().any(|f| !f.good);
                let color = if warn { Tone::Warn.color() } else { NEUTRAL };
                if let Some(r) = sparkline(ui, &p.rates, SPARKLINE_WIDTH_PX, color) {
                    let rows: Vec<String> = p
                        .rows
                        .iter()
                        .map(|(l, v)| format!("{}: {v}", tr(l)))
                        .collect();
                    r.on_hover_text(format!("{}\n{}", tr(PACE_HINT), rows.join("\n")));
                }
                for f in &p.flags {
                    let tone = if f.good { Tone::Good } else { Tone::Warn };
                    // "Slow start: opening pace …" → "Slow start".
                    let title = f.text.split(':').next().unwrap_or_default();
                    ui.colored_label(tone.color(), title).on_hover_text(&f.text);
                }
            });
        }
    });
}

/// Draw the visual summary. Returns "Watch it" or "Practice" when clicked.
pub fn results_visuals(ui: &mut egui::Ui, v: &VisualsView) -> Option<UiAction> {
    let mut action = None;
    hero(ui, v.hero);
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        for g in v.gauges {
            gauge(ui, g);
        }
    });
    if let Some(report) = v.report {
        ui.add_space(4.0);
        if let Some(fix) = &report.main_fix
            && let Some(a) = main_fix_card(ui, fix, v.can_watch)
        {
            action = Some(a);
        }
        ui.add_space(4.0);
        charts(ui, report);
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::organisms::analysis_panel::Advice;
    use crate::ui::organisms::potential_tip::PotentialLine;
    use aim_core::analytics::analyze;
    use aim_core::analytics::rules::builtin;
    use aim_core::scenario::{Recording, builtin as scenarios};
    use aim_core::sim::{AimerProfile, SimAimer};

    fn rules() -> &'static [Rule] {
        &builtin::rules().rules
    }

    fn analysis(id: &str, profile: usize) -> RunAnalysis {
        let def = scenarios::all().into_iter().find(|d| d.id == id).unwrap();
        let mut rec = Recording::new(def, 3);
        let mut live = rec.start();
        let mut aimer = SimAimer::new(AimerProfile::PRESETS[profile], 3);
        while !live.is_finished() {
            let i = aimer.input(&live, 1.0 / 240.0);
            rec.inputs.push(i);
            live.update(i);
        }
        analyze(&rec)
    }

    #[test]
    fn a_metric_is_bad_past_a_weakness_good_past_a_strength_and_warn_between() {
        // Reaction: weakness above 300 ms, strength below 220 ms.
        assert_eq!(metric_tone(rules(), Metric::ReactionTime, 0.35), Tone::Bad);
        assert_eq!(metric_tone(rules(), Metric::ReactionTime, 0.20), Tone::Good);
        assert_eq!(metric_tone(rules(), Metric::ReactionTime, 0.25), Tone::Warn);
        // Click delay has only a weakness rule: under it is simply good.
        assert_eq!(metric_tone(rules(), Metric::ClickDelay, 0.10), Tone::Good);
        assert_eq!(metric_tone(rules(), Metric::ClickDelay, 0.30), Tone::Bad);
        assert_eq!(metric_tone(&[], Metric::ClickDelay, 0.30), Tone::Neutral);
    }

    #[test]
    fn the_threshold_hint_lists_every_rule_of_the_metric() {
        assert_eq!(
            threshold_hint(rules(), Metric::ReactionTime),
            "Weakness above 300 ms · strength below 220 ms (provisional thresholds)."
        );
        assert_eq!(threshold_hint(&[], Metric::ReactionTime), "");
    }

    #[test]
    fn the_score_hero_shows_the_change_against_the_personal_best() {
        let pb = score_hero(512.4, Some(500.0), 24, 48.0);
        assert_eq!(pb.score, "512");
        assert_eq!(pb.delta, "▲ +12 · new personal best");
        assert_eq!(pb.tone, Tone::Good);
        assert_eq!(pb.sub, "24 kills · 48.0/min");
        let below = score_hero(450.0, Some(480.0), 0, 0.0);
        assert_eq!(below.delta, "▼ -30 vs best 480");
        assert_eq!(below.tone, Tone::Bad);
        let equal = score_hero(480.0, Some(480.0), 0, 0.0);
        assert_eq!(equal.delta, "= your best 480");
        assert_eq!(equal.tone, Tone::Warn);
        let first = score_hero(300.0, None, 0, 0.0);
        assert_eq!(first.delta, "First run recorded");
        assert_eq!(first.tone, Tone::Neutral);
    }

    #[test]
    fn accuracy_fills_its_ring_and_has_no_threshold_colour() {
        let g = accuracy_gauge(0.634);
        assert_eq!((g.label, g.value.as_str()), ("Accuracy", "63%"));
        assert!((g.fill - 0.634).abs() < 1e-6);
        assert_eq!(g.tone, Tone::Neutral);
    }

    #[test]
    fn flick_runs_gauge_reaction_overshoot_and_click_delay() {
        let gauges = analysis_gauges(&analysis("flick-wide", 0), rules());
        let labels: Vec<&str> = gauges.iter().map(|g| g.label).collect();
        assert_eq!(labels, ["Reaction", "Overshoot", "Click delay"]);
        let reaction = &gauges[0];
        assert!(reaction.value.ends_with(" ms"), "{reaction:?}");
        assert!(
            reaction.hint.contains("Weakness above 300 ms"),
            "{reaction:?}"
        );
        assert!(gauges.iter().all(|g| (0.0..=1.0).contains(&g.fill)));
        assert!(gauges.iter().all(|g| g.tone != Tone::Neutral));
    }

    #[test]
    fn tracking_runs_gauge_time_on_target_speed_match_and_lag() {
        let gauges = analysis_gauges(&analysis("track-precise", 2), rules());
        let labels: Vec<&str> = gauges.iter().map(|g| g.label).collect();
        assert_eq!(labels, ["On target", "Speed match", "Behind"]);
    }

    fn advice(headline: &str) -> Advice {
        Advice {
            headline: headline.into(),
            explanation: "Overshoot rate 42% is above the 35% threshold.".into(),
            tip: "Slow the end of the flick.".into(),
            practice: vec![("micro-adjust".into(), "Micro Adjust".into())],
        }
    }

    #[test]
    fn the_main_fix_prefers_the_potential_tip_then_the_top_advice() {
        let mut report = RunReport {
            advice: vec![advice("Overshoot rate 42%")],
            ..RunReport::default()
        };
        let fix = main_fix(&report);
        assert_eq!(fix.headline, "Work on: Overshoot rate 42%");
        assert_eq!(fix.tone, Tone::Bad);
        assert!(fix.hint.contains("35% threshold") && fix.hint.contains("Slow the end"));
        assert_eq!(
            fix.practice,
            Some(("micro-adjust".into(), "Micro Adjust".into()))
        );

        report.potential = Some(PotentialLine {
            headline: "Fix overshoot → about 104 points (+4%)".into(),
            assumption: "Estimate: if your 4 overshoots settled like your clean flicks.".into(),
        });
        let fix = main_fix(&report);
        assert_eq!(fix.headline, "Fix overshoot → about 104 points (+4%)");
        assert!(fix.hint.starts_with("Estimate: "), "{fix:?}");
        assert!(fix.hint.contains("Slow the end"), "{fix:?}");
    }

    #[test]
    fn the_results_screen_reads_in_spanish_and_english_is_unchanged() {
        use aim_core::i18n::{Lang, tr_fmt_in, tr_in, tr_named_in};
        assert_eq!(tr_in(Lang::Es, FIRST_RUN), "Primera partida registrada");
        assert_eq!(
            tr_fmt_in(Lang::Es, "▲ {} · new personal best", &[&"+12"]),
            "▲ +12 · nuevo récord personal"
        );
        assert_eq!(
            tr_fmt_in(Lang::Es, "{} kills · {}/min", &[&24, &"48.0"]),
            "24 bajas · 48.0/min"
        );
        assert_eq!(tr_in(Lang::Es, "Details"), "Detalles");
        assert_eq!(tr_in(Lang::Es, "Coach's take"), "Opinión del entrenador");
        assert_eq!(
            tr_fmt_in(Lang::Es, "Above {} · below {} of {} misses", &[&1, &2, &4]),
            "Arriba 1 · abajo 2 de 4 fallos"
        );
        assert_eq!(
            tr_named_in(
                Lang::Es,
                "{routine} · run {n}/{total}",
                &[("routine", &"R"), ("n", &2), ("total", &5)]
            ),
            "R · partida 2/5"
        );
        // Keys written with a line continuation in the source.
        assert_ne!(
            tr_in(
                Lang::Es,
                "Error {} s after each sound (or at your first shot); turn time until the \
                 crosshair moves towards it at {}°/s. Estimates."
            ),
            tr_in(
                Lang::En,
                "Error {} s after each sound (or at your first shot); turn time until the \
                 crosshair moves towards it at {}°/s. Estimates."
            )
        );
        assert_eq!(tr_in(Lang::En, FIRST_RUN), FIRST_RUN);
        assert_eq!(score_hero(300.0, None, 0, 0.0).delta, "First run recorded");
    }

    #[test]
    fn a_run_without_weakness_gets_a_clean_main_fix() {
        let fix = main_fix(&RunReport::default());
        assert_eq!(fix.headline, "No weakness crossed a threshold");
        assert_eq!(fix.tone, Tone::Good);
        assert_eq!(fix.practice, None);
    }
}
