//! Pace and stamina section of the run analysis: the rate per time window, the best and
//! worst window, and the start and stamina flags with the threshold behind each.

use crate::i18n::{tr, tr_fmt};
use crate::ui::atoms::{GOOD, WARN};
use crate::ui::molecules::stat_row;
use aim_core::analytics::pace::{
    PaceAnalysis, PaceUnit, PaceWindow, SLOW_START_RATIO, STAMINA_DROP_RATIO, STRONG_START_RATIO,
};
use bevy_egui::egui;

const TITLE: &str = "Pace";
const BEST: &str = "Best window";
const WORST: &str = "Worst window";
const PER_WINDOW: &str = "Rate per window";
const EVEN: &str = "Even pace: no start or stamina flag crossed its threshold.";

/// One pace flag, already worded with its value and threshold.
#[derive(Debug, Clone, PartialEq)]
pub struct PaceFlag {
    /// A good sign (strong start) rather than a weakness.
    pub good: bool,
    pub text: String,
}

/// What the pace section shows, built once per result.
#[derive(Debug, Clone, PartialEq)]
pub struct PaceReport {
    pub rows: Vec<(&'static str, String)>,
    pub flags: Vec<PaceFlag>,
    /// Rate per second of each window, in time order (for the sparkline).
    pub rates: Vec<f32>,
}

fn rate(unit: PaceUnit, per_sec: f32) -> String {
    match unit {
        PaceUnit::Kills => format!("{per_sec:.2} {}/s", unit.noun()),
        PaceUnit::Score => format!("{per_sec:.0} {}/s", unit.noun()),
    }
}

fn window(unit: PaceUnit, w: &PaceWindow) -> String {
    format!(
        "{:.0}–{:.0} s · {}",
        w.start_secs,
        w.end_secs,
        rate(unit, w.rate_per_sec)
    )
}

fn pct(ratio: f32) -> String {
    format!("{:.0}%", ratio * 100.0)
}

impl PaceReport {
    pub fn new(pace: &PaceAnalysis) -> Self {
        let unit = pace.unit;
        let per_window = pace
            .windows
            .iter()
            .map(|w| match unit {
                PaceUnit::Kills => format!("{:.2}", w.rate_per_sec),
                PaceUnit::Score => format!("{:.0}", w.rate_per_sec),
            })
            .collect::<Vec<_>>()
            .join(" · ");
        let rows = vec![
            (BEST, window(unit, pace.best())),
            (WORST, window(unit, pace.worst())),
            (PER_WINDOW, format!("{per_window} {}/s", unit.noun())),
        ];
        let mut flags = Vec::new();
        if let Some(r) = pace.start_ratio() {
            if pace.slow_start() {
                flags.push(PaceFlag {
                    good: false,
                    text: tr_fmt(
                        "Slow start: opening pace {} of the rest of the run (below the {} threshold).",
                        &[&pct(r), &pct(SLOW_START_RATIO)],
                    ),
                });
            } else if pace.strong_start() {
                flags.push(PaceFlag {
                    good: true,
                    text: tr_fmt(
                        "Strong start: opening pace {} of the rest of the run (above the {} threshold).",
                        &[&pct(r), &pct(STRONG_START_RATIO)],
                    ),
                });
            }
        }
        if let Some(r) = pace.stamina_ratio().filter(|_| pace.stamina_drop()) {
            flags.push(PaceFlag {
                good: false,
                text: tr_fmt(
                    "Stamina drop: late pace {} of the early run (below the {} threshold).",
                    &[&pct(r), &pct(STAMINA_DROP_RATIO)],
                ),
            });
        }
        Self {
            rows,
            flags,
            rates: pace.windows.iter().map(|w| w.rate_per_sec).collect(),
        }
    }
}

/// Draw the pace section.
pub fn pace_panel(ui: &mut egui::Ui, report: &PaceReport) {
    egui::CollapsingHeader::new(tr(TITLE))
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("pace_metrics")
                .num_columns(2)
                .show(ui, |ui| {
                    for (label, value) in &report.rows {
                        stat_row(ui, tr(label), value.as_str());
                    }
                });
            for f in &report.flags {
                ui.colored_label(if f.good { GOOD } else { WARN }, &f.text);
            }
            if report.flags.is_empty() {
                ui.small(tr(EVEN));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pace(unit: PaceUnit, amounts: [f32; 6]) -> PaceAnalysis {
        PaceAnalysis {
            unit,
            windows: amounts
                .iter()
                .enumerate()
                .map(|(i, &amount)| PaceWindow {
                    start_secs: i as f32 * 10.0,
                    end_secs: (i + 1) as f32 * 10.0,
                    amount,
                    rate_per_sec: amount / 10.0,
                })
                .collect(),
        }
    }

    #[test]
    fn best_and_worst_windows_show_their_time_range_and_rate() {
        let r = PaceReport::new(&pace(PaceUnit::Kills, [1.0, 4.0, 4.0, 4.0, 4.0, 4.0]));
        assert_eq!(
            r.rows,
            vec![
                (BEST, "10–20 s · 0.40 kills/s".to_owned()),
                (WORST, "0–10 s · 0.10 kills/s".to_owned()),
                (
                    PER_WINDOW,
                    "0.10 · 0.40 · 0.40 · 0.40 · 0.40 · 0.40 kills/s".to_owned()
                ),
            ]
        );
        assert_eq!(
            r.flags,
            vec![PaceFlag {
                good: false,
                text:
                    "Slow start: opening pace 25% of the rest of the run (below the 70% threshold)."
                        .to_owned(),
            }]
        );
        assert_eq!(r.rates, [0.1, 0.4, 0.4, 0.4, 0.4, 0.4]);
    }

    #[test]
    fn a_fading_run_flags_a_strong_start_and_a_stamina_drop() {
        let r = PaceReport::new(&pace(PaceUnit::Score, [40.0, 40.0, 40.0, 40.0, 20.0, 10.0]));
        assert_eq!(r.rows[0].1, "0–10 s · 4 points/s");
        let texts: Vec<(bool, &str)> = r.flags.iter().map(|f| (f.good, f.text.as_str())).collect();
        assert_eq!(
            texts,
            vec![
                (
                    true,
                    "Strong start: opening pace 133% of the rest of the run (above the 130% threshold)."
                ),
                (
                    false,
                    "Stamina drop: late pace 38% of the early run (below the 80% threshold)."
                ),
            ]
        );
    }

    #[test]
    fn an_even_run_has_no_flags() {
        assert!(
            PaceReport::new(&pace(PaceUnit::Kills, [3.0; 6]))
                .flags
                .is_empty()
        );
    }
}
