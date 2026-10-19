//! Target prioritization section of the run analysis: nearest-target-first share and
//! missed opportunities, with the rule verdicts (value and threshold) that apply to them.

use crate::i18n::{tr, tr_fmt};
use crate::ui::atoms::{GOOD, WARN};
use crate::ui::molecules::stat_row;
use aim_core::analytics::Finding;
use aim_core::analytics::priority::{MIN_CONTESTED_KILLS, MISSED_DWELL_SECS, PriorityAnalysis};
use aim_core::analytics::profile::{Metric, Verdict};
use bevy_egui::egui;

const TITLE: &str = "Target priority";
const NEAREST: &str = "Nearest target first";
const MISSED: &str = "Missed opportunities";

/// What the prioritization section shows, built once per result.
#[derive(Debug, Clone, PartialEq)]
pub struct PriorityReport {
    pub rows: Vec<(&'static str, String)>,
    /// Rule findings on the prioritization metrics: (is a strength, explanation).
    pub verdicts: Vec<(bool, String)>,
}

impl PriorityReport {
    /// `findings` are the run's rule findings; only the prioritization ones are kept.
    pub fn new<'a>(p: &PriorityAnalysis, findings: impl IntoIterator<Item = &'a Finding>) -> Self {
        let nearest = match p.nearest_share() {
            Some(share) => tr_fmt(
                "{} of {} contested kills ({}%)",
                &[
                    &p.nearest_kills,
                    &p.contested_kills,
                    &format!("{:.0}", share * 100.0),
                ],
            ),
            None => tr_fmt(
                "{} of {} contested kills (needs {} to judge)",
                &[&p.nearest_kills, &p.contested_kills, &MIN_CONTESTED_KILLS],
            ),
        };
        let missed = tr_fmt(
            "{} (on a target ≥ {} ms without firing, then left it)",
            &[
                &p.missed_opportunities,
                &format!("{:.0}", MISSED_DWELL_SECS * 1000.0),
            ],
        );
        let verdicts = findings
            .into_iter()
            .filter(|f| {
                matches!(
                    f.metric,
                    Metric::NearestFirstShare | Metric::MissedOpportunityRate
                )
            })
            .map(|f| {
                (
                    f.verdict == Verdict::Strength,
                    f.explanation_in(crate::i18n::language()),
                )
            })
            .collect();
        Self {
            rows: vec![(NEAREST, nearest), (MISSED, missed)],
            verdicts,
        }
    }
}

/// Draw the prioritization section.
pub fn priority_panel(ui: &mut egui::Ui, report: &PriorityReport) {
    egui::CollapsingHeader::new(tr(TITLE))
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("priority_metrics")
                .num_columns(2)
                .show(ui, |ui| {
                    for (label, value) in &report.rows {
                        stat_row(ui, tr(label), value.as_str());
                    }
                });
            for (strength, text) in &report.verdicts {
                if *strength {
                    ui.colored_label(GOOD, format!("✔ {text}"));
                } else {
                    ui.colored_label(WARN, text);
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::analytics::profile::Direction;
    use aim_core::scenario::Category;

    fn finding(metric: Metric, verdict: Verdict, explanation: &str) -> Finding {
        Finding {
            category: Category::Switching,
            verdict,
            metric,
            value: 0.0,
            threshold: 0.0,
            direction: Direction::Below,
            explanation: explanation.to_owned(),
            recommendation: None,
        }
    }

    #[test]
    fn rows_show_counts_and_only_prioritization_verdicts_are_kept() {
        let p = PriorityAnalysis {
            kills: 12,
            contested_kills: 10,
            nearest_kills: 7,
            missed_opportunities: 2,
        };
        let findings = [
            finding(Metric::NearestFirstShare, Verdict::Strength, "nearest"),
            finding(Metric::OvershootRate, Verdict::Weakness, "overshoot"),
            finding(Metric::MissedOpportunityRate, Verdict::Weakness, "missed"),
        ];
        let r = PriorityReport::new(&p, &findings);
        assert_eq!(
            r.rows,
            vec![
                (NEAREST, "7 of 10 contested kills (70%)".to_owned()),
                (
                    MISSED,
                    "2 (on a target ≥ 300 ms without firing, then left it)".to_owned()
                ),
            ]
        );
        assert_eq!(
            r.verdicts,
            vec![(true, "nearest".to_owned()), (false, "missed".to_owned())]
        );
    }

    #[test]
    fn too_few_contested_kills_say_so_instead_of_a_share() {
        let p = PriorityAnalysis {
            kills: 3,
            contested_kills: 3,
            nearest_kills: 1,
            missed_opportunities: 0,
        };
        let r = PriorityReport::new(&p, []);
        assert_eq!(r.rows[0].1, "1 of 3 contested kills (needs 5 to judge)");
        assert!(r.verdicts.is_empty());
    }
}
