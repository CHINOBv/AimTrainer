//! Recommendation rules as data.
//!
//! A rule names one metric, a threshold and a direction, so every strength, weakness and
//! recommendation can say exactly which number triggered it. The built-in rules live in
//! `content/rules/recommendations.toml`; all thresholds are provisional (see "Analytics
//! metrics" in `docs/training-science.md`) until they are calibrated on real player data.

use super::flick::FlickSummary;
use super::tracking::TrackingSummary;
use serde::Deserialize;

/// A measurable quantity a rule can threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    /// Median reaction time of fresh flicks, seconds.
    ReactionTime,
    /// Share of flicks whose primary movement overshot.
    OvershootRate,
    /// Share of flicks whose primary movement undershot.
    UndershootRate,
    /// Mean corrective sub-movements per flick.
    Corrections,
    /// Median path efficiency of flicks (1 = straight).
    PathEfficiency,
    /// Share of firing time spent on a counted zone (tracking).
    OnTargetRatio,
    /// Share of moving-target time spent behind the target (tracking).
    LagRatio,
    /// Median seconds to follow a target that reversed direction (tracking).
    ReversalResponse,
}

impl Metric {
    /// Human-readable name of the metric.
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

    pub(super) fn value(
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

/// Whether a fired rule marks a strength or a weakness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// A skill the player does well.
    Strength,
    /// A skill to improve.
    Weakness,
}

/// Which side of a threshold fires a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// Fires when the value is above the threshold.
    Above,
    /// Fires when the value is below the threshold.
    Below,
}

impl Direction {
    /// The direction as a lowercase word for explanations.
    pub fn word(self) -> &'static str {
        match self {
            Direction::Above => "above",
            Direction::Below => "below",
        }
    }
}

/// What to practice when a weakness fires.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recommendation {
    /// Built-in scenario ids, best first.
    pub scenario_ids: Vec<String>,
    /// Routine to start for this weakness, if one fits.
    #[serde(default)]
    pub routine_id: Option<String>,
    /// Short advice shown to the player.
    pub tip: String,
}

/// One row of the rules table.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    /// The metric the rule examines.
    pub metric: Metric,
    /// Which side of the threshold fires the rule.
    pub direction: Direction,
    /// The threshold, in the metric's own unit.
    pub threshold: f32,
    /// Whether firing marks a strength or a weakness.
    pub verdict: Verdict,
    /// What the number means for the player.
    pub meaning: String,
    /// What to practice when the rule fires.
    #[serde(default)]
    pub recommendation: Option<Recommendation>,
}

impl Rule {
    /// Whether `value` is strictly past the threshold in the rule's direction.
    pub fn fires(&self, value: f32) -> bool {
        match self.direction {
            Direction::Above => value > self.threshold,
            Direction::Below => value < self.threshold,
        }
    }
}

/// A rules file: `content/rules/<id>.toml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleSet {
    /// Stable id (equal to the file name).
    pub id: String,
    /// The rules, applied in order.
    pub rules: Vec<Rule>,
}

impl RuleSet {
    /// Parse a rules file from TOML.
    pub fn from_toml(src: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(src)
    }

    /// Problems that make the rules unusable: non-finite thresholds, empty text, and
    /// recommended scenarios or routines that do not exist.
    pub fn validate(&self, scenario_ids: &[&str], routine_ids: &[&str]) -> Vec<String> {
        let mut errs = Vec::new();
        for (i, rule) in self.rules.iter().enumerate() {
            let at = format!("rules[{i}] ({:?})", rule.metric);
            if !rule.threshold.is_finite() {
                errs.push(format!("{at}: threshold must be finite"));
            }
            if rule.meaning.trim().is_empty() {
                errs.push(format!("{at}: meaning must not be empty"));
            }
            let Some(rec) = &rule.recommendation else {
                continue;
            };
            if rec.scenario_ids.is_empty() {
                errs.push(format!("{at}: recommendation needs at least one scenario"));
            }
            for id in &rec.scenario_ids {
                if !scenario_ids.contains(&id.as_str()) {
                    errs.push(format!("{at}: unknown scenario '{id}'"));
                }
            }
            if let Some(id) = &rec.routine_id
                && !routine_ids.contains(&id.as_str())
            {
                errs.push(format!("{at}: unknown routine '{id}'"));
            }
            if rec.tip.trim().is_empty() {
                errs.push(format!("{at}: tip must not be empty"));
            }
        }
        errs
    }
}

/// The rule sets shipped with the trainer.
pub mod builtin {
    use super::RuleSet;
    use std::sync::LazyLock;

    /// Built-in rules files as `(file name, TOML source)`.
    pub const SOURCES: &[(&str, &str)] = &[(
        "recommendations.toml",
        include_str!("../../../../content/rules/recommendations.toml"),
    )];

    static RULES: LazyLock<RuleSet> = LazyLock::new(|| {
        let (file, src) = SOURCES[0];
        RuleSet::from_toml(src).unwrap_or_else(|e| panic!("built-in {file}: {e}"))
    });

    /// The built-in recommendation rules, parsed once.
    pub fn rules() -> &'static RuleSet {
        &RULES
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    type Expected = (
        Metric,
        Direction,
        f32,
        Verdict,
        &'static str,
        Option<(&'static [&'static str], Option<&'static str>, &'static str)>,
    );

    /// The table that was a Rust `const RULES` before the rules became data.
    const FORMER_RULES: &[Expected] = &[
        (
            Metric::ReactionTime,
            Direction::Above,
            0.30,
            Verdict::Weakness,
            "you start moving late after a target appears",
            Some((
                &["peek-reaction"],
                Some("quick-warmup"),
                "Keep the crosshair where targets appear and react to the first pixel, not the whole target.",
            )),
        ),
        (
            Metric::ReactionTime,
            Direction::Below,
            0.22,
            Verdict::Strength,
            "you start moving quickly after a target appears",
            None,
        ),
        (
            Metric::OvershootRate,
            Direction::Above,
            0.35,
            Verdict::Weakness,
            "your first movement often ends past the target's far edge",
            Some((
                &["micro-adjust", "static-headline"],
                None,
                "Slow your flicks slightly and aim to land on the target in one motion; if it persists, check that your sensitivity matches VALORANT.",
            )),
        ),
        (
            Metric::UndershootRate,
            Direction::Above,
            0.35,
            Verdict::Weakness,
            "your first movement often stops short of the target",
            Some((
                &["flick-wide"],
                None,
                "Commit to the full distance with your arm instead of finishing the flick with small wrist corrections.",
            )),
        ),
        (
            Metric::Corrections,
            Direction::Above,
            1.0,
            Verdict::Weakness,
            "you usually need more than one correction after the first movement",
            Some((
                &["micro-adjust", "flick-headshot"],
                None,
                "Make the first movement more accurate rather than faster; corrections cost more time than a slightly slower flick.",
            )),
        ),
        (
            Metric::PathEfficiency,
            Direction::Below,
            0.85,
            Verdict::Weakness,
            "your crosshair travels a curved or back-and-forth path to the target",
            Some((
                &["flick-headshot"],
                None,
                "Move in a straight line to the target; keep head height so flicks stay horizontal.",
            )),
        ),
        (
            Metric::OvershootRate,
            Direction::Below,
            0.10,
            Verdict::Strength,
            "your first movement rarely ends past the target",
            None,
        ),
        (
            Metric::OnTargetRatio,
            Direction::Below,
            0.50,
            Verdict::Weakness,
            "you are off the target for more than half the time you fire",
            Some((
                &["track-precise"],
                None,
                "Prioritize smooth, steady mouse movement over chasing the target; speed up only once you stay on it.",
            )),
        ),
        (
            Metric::OnTargetRatio,
            Direction::Above,
            0.75,
            Verdict::Strength,
            "you stay on the target most of the time you fire",
            None,
        ),
        (
            Metric::LagRatio,
            Direction::Above,
            0.65,
            Verdict::Weakness,
            "you trail behind moving targets most of the time",
            Some((
                &["track-precise", "track-reactive-micro"],
                None,
                "Match the target's speed first, then close the gap; watch the target, not the crosshair.",
            )),
        ),
        (
            Metric::LagRatio,
            Direction::Below,
            0.35,
            Verdict::Weakness,
            "you run ahead of moving targets most of the time",
            Some((
                &["track-reactive-micro"],
                None,
                "Stop predicting; follow the target and let it lead you.",
            )),
        ),
        (
            Metric::ReversalResponse,
            Direction::Above,
            0.25,
            Verdict::Weakness,
            "you take long to follow a target that changes direction",
            Some((
                &["track-reactive-strafe", "track-reactive-micro"],
                None,
                "React to strafes instead of anticipating them; relax your grip so reversals are quicker.",
            )),
        ),
    ];

    /// The recommendation table as it was written in Rust before it moved to TOML.
    pub(crate) fn former_rule_set() -> RuleSet {
        let rules = FORMER_RULES
            .iter()
            .map(
                |(metric, direction, threshold, verdict, meaning, rec)| Rule {
                    metric: *metric,
                    direction: *direction,
                    threshold: *threshold,
                    verdict: *verdict,
                    meaning: (*meaning).to_owned(),
                    recommendation: rec.map(|(ids, routine, tip)| Recommendation {
                        scenario_ids: ids.iter().map(|s| (*s).to_owned()).collect(),
                        routine_id: routine.map(str::to_owned),
                        tip: tip.to_owned(),
                    }),
                },
            )
            .collect();
        RuleSet {
            id: "recommendations".to_owned(),
            rules,
        }
    }

    #[test]
    fn builtin_rules_equal_the_former_rust_table() {
        assert_eq!(builtin::rules().rules, former_rule_set().rules);
    }

    #[test]
    fn builtin_rules_validate_against_builtin_content() {
        let scenarios: Vec<String> = crate::scenario::builtin::all()
            .into_iter()
            .map(|d| d.id)
            .collect();
        let routines: Vec<String> = crate::training::routine::builtin::all()
            .into_iter()
            .map(|r| r.id)
            .collect();
        let s: Vec<&str> = scenarios.iter().map(String::as_str).collect();
        let r: Vec<&str> = routines.iter().map(String::as_str).collect();
        let errs = builtin::rules().validate(&s, &r);
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn builtin_file_name_equals_its_id() {
        for (file, src) in builtin::SOURCES {
            let set = RuleSet::from_toml(src).unwrap();
            assert_eq!(format!("{}.toml", set.id), *file);
        }
    }

    const ONE_RULE: &str = r#"
id = "test"
[[rules]]
metric = "lag_ratio"
direction = "above"
threshold = 0.5
verdict = "weakness"
meaning = "trailing"
[rules.recommendation]
scenario_ids = ["known", "missing-scenario"]
routine_id = "missing-routine"
tip = "follow it"
"#;

    #[test]
    fn validate_reports_unknown_ids_and_non_finite_thresholds() {
        let mut set = RuleSet::from_toml(ONE_RULE).unwrap();
        assert_eq!(
            set.validate(&["known"], &[]),
            vec![
                "rules[0] (LagRatio): unknown scenario 'missing-scenario'",
                "rules[0] (LagRatio): unknown routine 'missing-routine'",
            ]
        );
        set.rules[0].threshold = f32::NAN;
        set.rules[0].recommendation = None;
        assert_eq!(
            set.validate(&[], &[]),
            vec!["rules[0] (LagRatio): threshold must be finite"]
        );
    }

    #[test]
    fn unknown_fields_and_metrics_are_rejected() {
        let typo = ONE_RULE.replace("meaning =", "meanin =");
        assert!(RuleSet::from_toml(&typo).is_err());
        let metric = ONE_RULE.replace("lag_ratio", "lag");
        assert!(RuleSet::from_toml(&metric).is_err());
    }

    #[test]
    fn a_rule_fires_strictly_past_its_threshold() {
        let rule = &RuleSet::from_toml(ONE_RULE).unwrap().rules[0];
        assert!(rule.fires(0.51));
        assert!(!rule.fires(0.5));
    }
}
