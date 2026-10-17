//! Skill profile and explainable recommendations.
//!
//! Recent run analyses are grouped by scenario category; each group is summarized and the
//! [`RULES`] table is applied to it. A rule names one metric, a threshold and a direction,
//! so every strength, weakness and recommendation can say exactly which number triggered
//! it. All thresholds are provisional (see "Analytics metrics" in
//! `docs/training-science.md`) until they are calibrated on real player data.

use super::flick::{Flick, FlickSummary};
use super::tracking::{TrackingAnalysis, TrackingSummary};
use super::{AnalysisKind, RunAnalysis};
use crate::scenario::Category;

/// Fewer flicks than this in a category give no flick findings.
pub const MIN_FLICKS: usize = 5;
/// Less firing time than this in a category gives no tracking findings.
pub const MIN_TRACKING_SECS: f32 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    ReactionTime,
    OvershootRate,
    UndershootRate,
    Corrections,
    PathEfficiency,
    OnTargetRatio,
    LagRatio,
    ReversalResponse,
}

impl Metric {
    pub fn label(self) -> &'static str {
        match self {
            Metric::ReactionTime => "Median reaction time",
            Metric::OvershootRate => "Overshoot rate",
            Metric::UndershootRate => "Undershoot rate",
            Metric::Corrections => "Corrections per flick",
            Metric::PathEfficiency => "Path efficiency",
            Metric::OnTargetRatio => "Time on target",
            Metric::LagRatio => "Time behind the target",
            Metric::ReversalResponse => "Reaction to direction changes",
        }
    }

    /// Value formatted with its unit.
    pub fn format(self, value: f32) -> String {
        match self {
            Metric::ReactionTime | Metric::ReversalResponse => {
                format!("{:.0} ms", value * 1000.0)
            }
            Metric::Corrections => format!("{value:.2}"),
            _ => format!("{:.0}%", value * 100.0),
        }
    }

    fn value(
        self,
        flick: Option<&FlickSummary>,
        tracking: Option<&TrackingSummary>,
    ) -> Option<f32> {
        match self {
            Metric::ReactionTime => flick?.median_reaction_secs,
            Metric::OvershootRate => Some(flick?.overshoot_rate),
            Metric::UndershootRate => Some(flick?.undershoot_rate),
            Metric::Corrections => Some(flick?.mean_corrections),
            Metric::PathEfficiency => flick?.median_path_efficiency,
            Metric::OnTargetRatio => Some(tracking?.on_target_ratio),
            Metric::LagRatio => tracking?.lag_ratio,
            Metric::ReversalResponse => tracking?.median_reversal_response_secs,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Strength,
    Weakness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Above,
    Below,
}

/// What to practice when a weakness fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recommendation {
    /// Built-in scenario ids, best first.
    pub scenario_ids: &'static [&'static str],
    pub routine_id: Option<&'static str>,
    pub tip: &'static str,
}

/// One row of the rules table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rule {
    pub metric: Metric,
    pub direction: Direction,
    pub threshold: f32,
    pub verdict: Verdict,
    /// What the number means for the player.
    pub meaning: &'static str,
    pub recommendation: Option<Recommendation>,
}

/// Provisional rules (documented in `docs/training-science.md`, "Analytics metrics").
pub const RULES: &[Rule] = &[
    Rule {
        metric: Metric::ReactionTime,
        direction: Direction::Above,
        threshold: 0.30,
        verdict: Verdict::Weakness,
        meaning: "you start moving late after a target appears",
        recommendation: Some(Recommendation {
            scenario_ids: &["peek-reaction"],
            routine_id: Some("quick-warmup"),
            tip: "Keep the crosshair where targets appear and react to the first pixel, not the whole target.",
        }),
    },
    Rule {
        metric: Metric::ReactionTime,
        direction: Direction::Below,
        threshold: 0.22,
        verdict: Verdict::Strength,
        meaning: "you start moving quickly after a target appears",
        recommendation: None,
    },
    Rule {
        metric: Metric::OvershootRate,
        direction: Direction::Above,
        threshold: 0.35,
        verdict: Verdict::Weakness,
        meaning: "your first movement often ends past the target's far edge",
        recommendation: Some(Recommendation {
            scenario_ids: &["micro-adjust", "static-headline"],
            routine_id: None,
            tip: "Slow your flicks slightly and aim to land on the target in one motion; if it persists, check that your sensitivity matches VALORANT.",
        }),
    },
    Rule {
        metric: Metric::UndershootRate,
        direction: Direction::Above,
        threshold: 0.35,
        verdict: Verdict::Weakness,
        meaning: "your first movement often stops short of the target",
        recommendation: Some(Recommendation {
            scenario_ids: &["flick-wide"],
            routine_id: None,
            tip: "Commit to the full distance with your arm instead of finishing the flick with small wrist corrections.",
        }),
    },
    Rule {
        metric: Metric::Corrections,
        direction: Direction::Above,
        threshold: 1.0,
        verdict: Verdict::Weakness,
        meaning: "you usually need more than one correction after the first movement",
        recommendation: Some(Recommendation {
            scenario_ids: &["micro-adjust", "flick-headshot"],
            routine_id: None,
            tip: "Make the first movement more accurate rather than faster; corrections cost more time than a slightly slower flick.",
        }),
    },
    Rule {
        metric: Metric::PathEfficiency,
        direction: Direction::Below,
        threshold: 0.85,
        verdict: Verdict::Weakness,
        meaning: "your crosshair travels a curved or back-and-forth path to the target",
        recommendation: Some(Recommendation {
            scenario_ids: &["flick-headshot"],
            routine_id: None,
            tip: "Move in a straight line to the target; keep head height so flicks stay horizontal.",
        }),
    },
    Rule {
        metric: Metric::OvershootRate,
        direction: Direction::Below,
        threshold: 0.10,
        verdict: Verdict::Strength,
        meaning: "your first movement rarely ends past the target",
        recommendation: None,
    },
    Rule {
        metric: Metric::OnTargetRatio,
        direction: Direction::Below,
        threshold: 0.50,
        verdict: Verdict::Weakness,
        meaning: "you are off the target for more than half the time you fire",
        recommendation: Some(Recommendation {
            scenario_ids: &["track-precise"],
            routine_id: None,
            tip: "Prioritize smooth, steady mouse movement over chasing the target; speed up only once you stay on it.",
        }),
    },
    Rule {
        metric: Metric::OnTargetRatio,
        direction: Direction::Above,
        threshold: 0.75,
        verdict: Verdict::Strength,
        meaning: "you stay on the target most of the time you fire",
        recommendation: None,
    },
    Rule {
        metric: Metric::LagRatio,
        direction: Direction::Above,
        threshold: 0.65,
        verdict: Verdict::Weakness,
        meaning: "you trail behind moving targets most of the time",
        recommendation: Some(Recommendation {
            scenario_ids: &["track-precise", "track-reactive-micro"],
            routine_id: None,
            tip: "Match the target's speed first, then close the gap; watch the target, not the crosshair.",
        }),
    },
    Rule {
        metric: Metric::LagRatio,
        direction: Direction::Below,
        threshold: 0.35,
        verdict: Verdict::Weakness,
        meaning: "you run ahead of moving targets most of the time",
        recommendation: Some(Recommendation {
            scenario_ids: &["track-reactive-micro"],
            routine_id: None,
            tip: "Stop predicting; follow the target and let it lead you.",
        }),
    },
    Rule {
        metric: Metric::ReversalResponse,
        direction: Direction::Above,
        threshold: 0.25,
        verdict: Verdict::Weakness,
        meaning: "you take long to follow a target that changes direction",
        recommendation: Some(Recommendation {
            scenario_ids: &["track-reactive-strafe", "track-reactive-micro"],
            routine_id: None,
            tip: "React to strafes instead of anticipating them; relax your grip so reversals are quicker.",
        }),
    },
];

#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub category: Category,
    pub verdict: Verdict,
    pub metric: Metric,
    pub value: f32,
    pub threshold: f32,
    pub direction: Direction,
    /// "Overshoot rate 42% is above the 35% threshold: …"
    pub explanation: String,
    pub recommendation: Option<Recommendation>,
}

impl Finding {
    /// How far past the threshold the value is, relative to the threshold (for ranking).
    pub fn severity(&self) -> f32 {
        let t = self.threshold.abs().max(1e-6);
        match self.direction {
            Direction::Above => (self.value - self.threshold) / t,
            Direction::Below => (self.threshold - self.value) / t,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CategoryProfile {
    pub category: Category,
    pub runs: usize,
    pub flick: Option<FlickSummary>,
    pub tracking: Option<TrackingSummary>,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkillProfile {
    /// One entry per category present in the analyses, in `Category::ALL` order.
    pub categories: Vec<CategoryProfile>,
}

fn evaluate(
    category: Category,
    flick: Option<&FlickSummary>,
    tracking: Option<&TrackingSummary>,
) -> Vec<Finding> {
    RULES
        .iter()
        .filter_map(|rule| {
            let value = rule.metric.value(flick, tracking)?;
            let fires = match rule.direction {
                Direction::Above => value > rule.threshold,
                Direction::Below => value < rule.threshold,
            };
            fires.then(|| Finding {
                category,
                verdict: rule.verdict,
                metric: rule.metric,
                value,
                threshold: rule.threshold,
                direction: rule.direction,
                explanation: format!(
                    "{} {} is {} the {} threshold: {}.",
                    rule.metric.label(),
                    rule.metric.format(value),
                    match rule.direction {
                        Direction::Above => "above",
                        Direction::Below => "below",
                    },
                    rule.metric.format(rule.threshold),
                    rule.meaning,
                ),
                recommendation: rule.recommendation,
            })
        })
        .collect()
}

impl SkillProfile {
    pub fn build(analyses: &[RunAnalysis]) -> Self {
        let mut categories = Vec::new();
        for category in Category::ALL {
            let runs: Vec<&RunAnalysis> =
                analyses.iter().filter(|a| a.category == category).collect();
            if runs.is_empty() {
                continue;
            }
            let mut flicks: Vec<Flick> = Vec::new();
            let mut tracking: Option<TrackingAnalysis> = None;
            for run in &runs {
                match &run.kind {
                    AnalysisKind::Flick(f) => flicks.extend_from_slice(f),
                    AnalysisKind::Tracking(t) => tracking
                        .get_or_insert_with(TrackingAnalysis::default)
                        .merge(t),
                }
            }
            let flick = (flicks.len() >= MIN_FLICKS).then(|| FlickSummary::from_flicks(&flicks));
            let tracking = tracking
                .map(|t| t.summary())
                .filter(|t| t.hold_secs >= MIN_TRACKING_SECS);
            categories.push(CategoryProfile {
                category,
                runs: runs.len(),
                findings: evaluate(category, flick.as_ref(), tracking.as_ref()),
                flick,
                tracking,
            });
        }
        Self { categories }
    }

    pub fn findings(&self) -> impl Iterator<Item = &Finding> {
        self.categories.iter().flat_map(|c| c.findings.iter())
    }

    /// Weaknesses with a recommendation, most severe first.
    pub fn recommendations(&self) -> Vec<&Finding> {
        let mut v: Vec<&Finding> = self
            .findings()
            .filter(|f| f.verdict == Verdict::Weakness && f.recommendation.is_some())
            .collect();
        v.sort_by(|a, b| b.severity().total_cmp(&a.severity()));
        v
    }

    pub fn strengths(&self) -> impl Iterator<Item = &Finding> {
        self.findings().filter(|f| f.verdict == Verdict::Strength)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::analyze;
    use crate::analytics::test_support::record;
    use crate::scenario::builtin;
    use crate::sim::AimerProfile;

    #[test]
    fn every_recommended_scenario_and_routine_is_built_in() {
        let scenarios: Vec<String> = builtin::all().into_iter().map(|d| d.id).collect();
        let routines: Vec<String> = crate::training::routine::builtin::all()
            .into_iter()
            .map(|r| r.id)
            .collect();
        for rule in RULES {
            if let Some(r) = rule.recommendation {
                assert!(!r.scenario_ids.is_empty());
                for id in r.scenario_ids {
                    assert!(scenarios.iter().any(|s| s == id), "unknown scenario {id}");
                }
                if let Some(id) = r.routine_id {
                    assert!(routines.iter().any(|s| s == id), "unknown routine {id}");
                }
            }
        }
    }

    #[test]
    fn findings_state_the_metric_value_and_threshold() {
        let flicks = vec![
            Flick {
                kind: crate::analytics::flick::FlickKind::Fresh,
                target_id: 1,
                hit: true,
                distance_deg: 20.0,
                target_radius_deg: 1.0,
                reaction_secs: Some(0.4),
                movement_secs: Some(0.3),
                peak_speed_deg_per_sec: 200.0,
                primary_error_radii: Some(2.0),
                endpoint_error_deg: 0.0,
                effective_distance_deg: 20.0,
                corrections: 1,
                settle_secs: Some(0.1),
                path_efficiency: Some(0.9),
            };
            MIN_FLICKS
        ];
        let run = RunAnalysis {
            scenario_id: "flick-wide".into(),
            category: Category::Flicking,
            kind: AnalysisKind::Flick(flicks),
        };
        let p = SkillProfile::build(&[run]);
        let recs = p.recommendations();
        let overshoot = recs
            .iter()
            .find(|f| f.metric == Metric::OvershootRate)
            .expect("100% overshoot fires");
        assert_eq!(
            overshoot.explanation,
            "Overshoot rate 100% is above the 35% threshold: your first movement often ends past the target's far edge."
        );
        assert_eq!(
            overshoot.recommendation.unwrap().scenario_ids[0],
            "micro-adjust"
        );
        assert!(recs.iter().any(|f| f.metric == Metric::ReactionTime));
        // Overshoot (+186%) is further past its threshold than reaction (+33%).
        assert_eq!(recs[0].metric, Metric::OvershootRate);
    }

    #[test]
    fn slow_simulated_reaction_recommends_peek_reaction_and_elite_shows_a_strength() {
        let recruit = analyze(&record("flick-wide", AimerProfile::PRESETS[0], 2));
        let p = SkillProfile::build(&[recruit]);
        assert!(
            p.recommendations()
                .iter()
                .any(|f| f.metric == Metric::ReactionTime
                    && f.recommendation
                        .unwrap()
                        .scenario_ids
                        .contains(&"peek-reaction")),
            "{p:?}"
        );
        let elite = analyze(&record("flick-wide", AimerProfile::PRESETS[3], 2));
        let p = SkillProfile::build(&[elite]);
        // Measured onset adds ~45 ms to the simulated 0.20 s delay, so elite reaction is
        // neither a weakness nor (at 0.22 s) a strength; its clean flicks are.
        assert!(
            p.strengths().any(|f| f.metric == Metric::OvershootRate),
            "{p:?}"
        );
        assert!(
            p.recommendations()
                .iter()
                .all(|f| f.metric != Metric::ReactionTime)
        );
    }

    #[test]
    fn categories_are_profiled_separately() {
        let runs = [
            analyze(&record("flick-wide", AimerProfile::PRESETS[1], 1)),
            analyze(&record("track-precise", AimerProfile::PRESETS[1], 1)),
        ];
        let p = SkillProfile::build(&runs);
        let cats: Vec<Category> = p.categories.iter().map(|c| c.category).collect();
        assert_eq!(cats, vec![Category::Flicking, Category::Tracking]);
        assert!(p.categories[0].flick.is_some() && p.categories[0].tracking.is_none());
        assert!(p.categories[1].tracking.is_some() && p.categories[1].flick.is_none());
    }
}
