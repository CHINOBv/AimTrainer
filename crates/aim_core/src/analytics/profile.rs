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
use super::rules::{self, MetricSources, RuleSet};
use super::tracking::{TrackingAnalysis, TrackingSummary};
use super::{AnalysisKind, RunAnalysis};
use crate::scenario::Category;

use super::pace::{PaceAnalysis, PaceSummary};

pub use super::rules::{Direction, Metric, Recommendation, Rule, Verdict};

/// Fewer flicks than this in a category give no flick findings.
pub const MIN_FLICKS: usize = 5;
/// Less firing time than this in a category gives no tracking findings.
pub const MIN_TRACKING_SECS: f32 = 3.0;

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
            // Each run's pace analysis already enforces its own minimum kills and duration.
            let paces: Vec<&PaceAnalysis> = runs.iter().filter_map(|r| r.pace.as_ref()).collect();
            let pace = PaceSummary::from_runs(&paces);
            categories.push(CategoryProfile {
                category,
                runs: runs.len(),
                findings: evaluate(
                    rules,
                    category,
                    &MetricSources {
                        flick: flick.as_ref(),
                        tracking: tracking.as_ref(),
                        pace: pace.as_ref(),
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
            pace: None,
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

    /// A flicking run whose only measured section is pace, with `amounts` kills per 10 s
    /// window.
    fn pace_run(amounts: [f32; crate::analytics::pace::WINDOWS]) -> RunAnalysis {
        use crate::analytics::pace::{PaceAnalysis, PaceUnit, PaceWindow};
        let mut run = analyze(&record("flick-wide", AimerProfile::PRESETS[1], 1));
        run.kind = AnalysisKind::Flick(Vec::new());
        run.pace = Some(PaceAnalysis {
            unit: PaceUnit::Kills,
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
        });
        run
    }

    #[test]
    fn a_slow_start_is_an_explained_weakness_with_a_warmup() {
        let p = SkillProfile::build(&[pace_run([1.0, 4.0, 4.0, 4.0, 4.0, 4.0])]);
        let f = p
            .recommendations()
            .into_iter()
            .find(|f| f.metric == Metric::StartRatio)
            .expect("25% opening pace fires");
        assert_eq!(
            f.explanation,
            "Opening pace (vs rest of run) 25% is below the 70% threshold: you score much slower at the start of a run than in the rest of it."
        );
        let rec = f.recommendation.as_ref().unwrap();
        assert_eq!(rec.routine_id.as_deref(), Some("quick-warmup"));
        assert!(p.findings().all(|f| f.metric != Metric::StaminaRatio));
    }

    #[test]
    fn a_stamina_drop_is_an_explained_weakness() {
        let p = SkillProfile::build(&[pace_run([4.0, 4.0, 4.0, 4.0, 2.0, 1.0])]);
        let f = p
            .recommendations()
            .into_iter()
            .find(|f| f.metric == Metric::StaminaRatio)
            .expect("38% late pace fires");
        assert_eq!(
            f.explanation,
            "Late pace (vs early run) 38% is below the 80% threshold: your scoring rate drops in the last third of the run."
        );
        assert!(p.findings().all(|f| f.metric != Metric::StartRatio));
        let even = SkillProfile::build(&[pace_run([3.0; 6])]);
        assert!(
            even.findings()
                .all(|f| f.metric != Metric::StartRatio && f.metric != Metric::StaminaRatio)
        );
    }

    /// The per-run sections added after flicks and tracking never change the findings of
    /// the original flick and tracking metrics.
    #[test]
    fn flick_and_tracking_findings_ignore_the_added_sections() {
        let mut original = rules::builtin::rules().clone();
        original
            .rules
            .retain(|r| !matches!(r.metric, Metric::StartRatio | Metric::StaminaRatio));
        for (id, preset) in [("flick-wide", 0), ("track-precise", 1), ("switch-wide", 2)] {
            let run = analyze(&record(id, AimerProfile::PRESETS[preset], 2));
            let mut bare = run.clone();
            bare.pace = None;
            assert_eq!(
                SkillProfile::build_with(&[run], &original),
                SkillProfile::build_with(&[bare], &original),
                "{id}"
            );
        }
    }

    #[test]
    fn pace_rules_use_the_pace_flag_thresholds() {
        use crate::analytics::pace::{SLOW_START_RATIO, STAMINA_DROP_RATIO};
        let threshold = |metric| {
            rules::builtin::rules()
                .rules
                .iter()
                .find(|r| r.metric == metric && r.direction == Direction::Below)
                .map(|r| r.threshold)
        };
        assert_eq!(threshold(Metric::StartRatio), Some(SLOW_START_RATIO));
        assert_eq!(threshold(Metric::StaminaRatio), Some(STAMINA_DROP_RATIO));
    }
}
