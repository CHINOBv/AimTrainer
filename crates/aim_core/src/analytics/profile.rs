//! Skill profile and explainable recommendations.
//!
//! Recent run analyses are grouped by scenario category; each group is summarized and a
//! [`RuleSet`] (by default the built-in `content/rules/recommendations.toml`, see
//! [`super::rules`]) is applied to it. A rule names one metric, a threshold and a
//! direction, so every strength, weakness and recommendation can say exactly which number
//! triggered it. All thresholds are provisional (see "Analytics metrics" in
//! `docs/training-science.md`) until they are calibrated on real player data. The
//! per-benchmark-subcategory profile is in [`super::skills`].

use super::flick::{Flick, FlickSummary};
use super::moving::MovingAnalysis;
use super::rules::{self, MetricSources, RuleSet};
use super::tracking::{TrackingAnalysis, TrackingSummary};
use super::{AnalysisKind, RunAnalysis};
use crate::scenario::Category;

pub use super::rules::{Direction, Metric, Recommendation, Rule, Verdict};

/// Fewer flicks than this in a category give no flick findings.
pub const MIN_FLICKS: usize = 5;
/// Less firing time than this in a category gives no tracking findings.
pub const MIN_TRACKING_SECS: f32 = 3.0;
/// Fewer shots than this in a movement category give no firing-while-moving findings.
pub const MIN_MOVING_SHOTS: usize = 10;

/// One strength or weakness found by applying a rule to a category's metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// Category whose runs were measured.
    pub category: Category,
    /// Whether the metric is a strength or a weakness.
    pub verdict: Verdict,
    /// The metric the rule examined.
    pub metric: Metric,
    /// The measured value, in the metric's own unit.
    pub value: f32,
    /// The rule's threshold, in the metric's own unit.
    pub threshold: f32,
    /// Which side of the threshold fires the rule.
    pub direction: Direction,
    /// "Overshoot rate 42% is above the 35% threshold: …"
    pub explanation: String,
    /// What to practice, when the rule carries a recommendation.
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

/// Summary and findings for one scenario category.
#[derive(Debug, Clone, PartialEq)]
pub struct CategoryProfile {
    /// The summarized category.
    pub category: Category,
    /// Number of analyzed runs in the category.
    pub runs: usize,
    /// Flick aggregates, if enough flicks were recorded.
    pub flick: Option<FlickSummary>,
    /// Tracking aggregates, if enough firing time was recorded.
    pub tracking: Option<TrackingSummary>,
    /// Findings whose rules fired, in rules-table order.
    pub findings: Vec<Finding>,
}

/// Findings per scenario category, built from run analyses.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkillProfile {
    /// One entry per category present in the analyses, in `Category::ALL` order.
    pub categories: Vec<CategoryProfile>,
}

fn evaluate(rules: &RuleSet, category: Category, sources: &MetricSources) -> Vec<Finding> {
    rules
        .rules
        .iter()
        .filter_map(|rule| {
            let value = rule.metric.value(sources)?;
            rule.fires(value).then(|| Finding {
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
                    rule.direction.word(),
                    rule.metric.format(rule.threshold),
                    rule.meaning,
                ),
                recommendation: rule.recommendation.clone(),
            })
        })
        .collect()
}

impl SkillProfile {
    /// Profile with the built-in recommendation rules.
    pub fn build(analyses: &[RunAnalysis]) -> Self {
        Self::build_with(analyses, rules::builtin::rules())
    }

    /// Profile with any rules table.
    pub fn build_with(analyses: &[RunAnalysis], rules: &RuleSet) -> Self {
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
            let mut moving = MovingAnalysis::default();
            for m in runs.iter().filter_map(|r| r.moving.as_ref()) {
                moving.merge(m);
            }
            let moving = (moving.shots.len() >= MIN_MOVING_SHOTS).then(|| moving.summary());
            categories.push(CategoryProfile {
                category,
                runs: runs.len(),
                findings: evaluate(
                    rules,
                    category,
                    &MetricSources {
                        flick: flick.as_ref(),
                        tracking: tracking.as_ref(),
                        moving: moving.as_ref(),
                    },
                ),
                flick,
                tracking,
            });
        }
        Self { categories }
    }

    /// Every finding across all categories.
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

    /// Findings that are strengths.
    pub fn strengths(&self) -> impl Iterator<Item = &Finding> {
        self.findings().filter(|f| f.verdict == Verdict::Strength)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::analyze;
    use crate::analytics::test_support::record;
    use crate::sim::AimerProfile;

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
            moving: None,
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
            overshoot.recommendation.as_ref().unwrap().scenario_ids[0],
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
                        .as_ref()
                        .unwrap()
                        .scenario_ids
                        .iter()
                        .any(|s| s == "peek-reaction")),
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

    /// The TOML rules find exactly what the former Rust table found, for the same runs.
    /// Both run in this process: simulated values differ slightly between platforms' math
    /// libraries, so pinning their text would make the test pass on one OS only.
    #[test]
    fn simulated_findings_are_unchanged_by_the_rules_file() {
        let former = crate::analytics::rules::tests::former_rule_set();
        let cases = [
            ("flick-wide", 0),
            ("flick-wide", 3),
            ("track-precise", 1),
            ("track-reactive-strafe", 0),
            ("micro-adjust", 1),
        ];
        for (id, preset) in cases {
            let runs = [analyze(&record(id, AimerProfile::PRESETS[preset], 2))];
            let now = SkillProfile::build(&runs);
            assert_eq!(
                now,
                SkillProfile::build_with(&runs, &former),
                "{id} preset {preset}"
            );
            assert!(
                now.findings().next().is_some(),
                "{id} preset {preset}: no findings"
            );
        }
    }

    fn movement_run(shots: usize) -> RunAnalysis {
        let shot = |i: usize| crate::analytics::moving::MovingShot {
            time_secs: i as f32,
            speed: if i.is_multiple_of(2) { 5.4 } else { 0.0 },
            accurate: !i.is_multiple_of(2),
            hit: !i.is_multiple_of(2),
            after_movement: true,
            stop_to_shot_secs: None,
        };
        RunAnalysis {
            scenario_id: "counter-strafe".into(),
            category: Category::Movement,
            kind: AnalysisKind::Flick(Vec::new()),
            moving: Some(crate::analytics::MovingAnalysis {
                accuracy_speed_limit: 1.485,
                shots: (0..shots).map(shot).collect(),
            }),
        }
    }

    #[test]
    fn many_shots_while_moving_recommend_counter_strafing() {
        let p = SkillProfile::build(&[movement_run(MIN_MOVING_SHOTS)]);
        let recs = p.recommendations();
        let f = recs
            .iter()
            .find(|f| f.metric == Metric::MovingShotRate)
            .expect("50% moving shots fires");
        assert_eq!(f.category, Category::Movement);
        assert_eq!(f.value, 0.5);
        assert_eq!(f.threshold, 0.2);
        assert_eq!(
            f.explanation,
            "Shots fired while moving 50% is above the 20% threshold: you often fire before you have stopped."
        );
        let rec = f.recommendation.as_ref().unwrap();
        assert_eq!(rec.scenario_ids, vec!["counter-strafe", "strafe-shot"]);

        let few = SkillProfile::build(&[movement_run(MIN_MOVING_SHOTS - 1)]);
        assert!(few.findings().all(|f| f.metric != Metric::MovingShotRate));
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
