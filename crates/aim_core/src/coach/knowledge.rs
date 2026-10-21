//! The coaching knowledge base as data: for each aim sub-skill, the signals that reveal a
//! weakness, the method with an external-focus cue, drill parameters, a progression rule
//! and the measure of progress.
//!
//! Evidence and confidence tags are explained under "Coaching knowledge base" in
//! `docs/training-science.md`. Every drill number is an estimate. Diagnosis thresholds are
//! not repeated here: a signal names a metric and a direction, and the threshold is the one
//! of the matching weakness rule in `content/rules/recommendations.toml`.

use serde::{Deserialize, Serialize};

use super::context::{CoachContext, snake_debug};
use crate::analytics::rules::{self, Direction, Metric, Verdict};

/// Score added to a sub-skill whose benchmark skill stands out as a weakness.
pub const BENCHMARK_WEIGHT: f32 = 0.25;

/// How well a claim is supported (see `docs/training-science.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Replicated experimental evidence or a measured game mechanic.
    Strong,
    /// A single study or indirect evidence.
    Moderate,
    /// Coaching consensus without a controlled study.
    Expert,
    /// Our own number, set by reasoning or by the simulated aimer.
    Estimate,
}

/// An aim sub-skill the coach can diagnose and train.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SubSkill {
    /// Landing a flick on the target: the accuracy-versus-speed trade-off.
    FlickAccuracy,
    /// Small corrections near the target.
    MicroAdjustment,
    /// Committing to the click once on target (hesitation).
    ClickTiming,
    /// Finding and hitting a target that appears without warning.
    ReactiveAcquisition,
    /// Following a smoothly moving target.
    SmoothTracking,
    /// Following a target that changes direction unpredictably.
    ReactiveTracking,
    /// Moving between several targets in a good order.
    TargetSwitching,
    /// Keeping the crosshair where heads will be.
    CrosshairPlacement,
    /// Stopping (counter-strafing) before shooting.
    MovingShots,
    /// Holding an angle against a peek.
    PeekAndHold,
}

impl SubSkill {
    /// Every sub-skill, in the order of the knowledge base.
    pub const ALL: [SubSkill; 10] = [
        SubSkill::FlickAccuracy,
        SubSkill::MicroAdjustment,
        SubSkill::ClickTiming,
        SubSkill::ReactiveAcquisition,
        SubSkill::SmoothTracking,
        SubSkill::ReactiveTracking,
        SubSkill::TargetSwitching,
        SubSkill::CrosshairPlacement,
        SubSkill::MovingShots,
        SubSkill::PeekAndHold,
    ];

    /// Stable kebab-case id, used in drill ids and tool arguments.
    pub fn id(self) -> &'static str {
        match self {
            SubSkill::FlickAccuracy => "flick-accuracy",
            SubSkill::MicroAdjustment => "micro-adjustment",
            SubSkill::ClickTiming => "click-timing",
            SubSkill::ReactiveAcquisition => "reactive-acquisition",
            SubSkill::SmoothTracking => "smooth-tracking",
            SubSkill::ReactiveTracking => "reactive-tracking",
            SubSkill::TargetSwitching => "target-switching",
            SubSkill::CrosshairPlacement => "crosshair-placement",
            SubSkill::MovingShots => "moving-shots",
            SubSkill::PeekAndHold => "peek-and-hold",
        }
    }

    /// The sub-skill with id `id`.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.id() == id)
    }

    /// Its knowledge-base entry.
    pub fn entry(self) -> &'static KnowledgeEntry {
        ENTRIES
            .iter()
            .find(|e| e.skill == self)
            .expect("the knowledge base has one entry per sub-skill (tested)")
    }
}

/// A metric and the side of its weakness rule that reveals a weak sub-skill.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Signal {
    /// The metric (serialized as its snake_case id).
    #[serde(serialize_with = "snake_debug")]
    pub metric: Metric,
    /// The side of the threshold that marks the weakness.
    #[serde(serialize_with = "snake_debug")]
    pub direction: Direction,
}

impl Signal {
    /// The threshold of the matching built-in weakness rule.
    pub fn threshold(&self) -> Option<f32> {
        rules::builtin::rules()
            .rules
            .iter()
            .find(|r| {
                r.metric == self.metric
                    && r.direction == self.direction
                    && r.verdict == Verdict::Weakness
            })
            .map(|r| r.threshold)
    }
}

/// Drill parameters from the research, as `(min, max)` ranges. [`super::drills`] maps a
/// player level onto them. All are estimates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct DrillParams {
    /// Angular target radius, degrees: the larger end for low levels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_radius_deg: Option<(f32, f32)>,
    /// Angle from the crosshair (flicks) or between targets (switching), degrees.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance_deg: Option<(f32, f32)>,
    /// Target angular speed, degrees per second.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_deg_per_sec: Option<(f32, f32)>,
    /// Target lifetime, seconds: the longer end for low levels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifetime_secs: Option<(f32, f32)>,
    /// Wait before a target appears, seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spawn_delay_secs: Option<(f32, f32)>,
    /// Targets alive at once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub targets: Option<(u32, u32)>,
    /// Length of one run, seconds.
    pub run_secs: f32,
    /// Runs per session block.
    pub runs: u32,
}

impl DrillParams {
    const BASE: DrillParams = DrillParams {
        target_radius_deg: None,
        distance_deg: None,
        speed_deg_per_sec: None,
        lifetime_secs: None,
        spawn_delay_secs: None,
        targets: None,
        run_secs: 60.0,
        runs: 3,
    };
}

/// What the coach knows about one sub-skill.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct KnowledgeEntry {
    /// The sub-skill.
    pub skill: SubSkill,
    /// Display name.
    pub name: &'static str,
    /// Weakness signals; any one of them firing points at this sub-skill.
    pub signals: &'static [Signal],
    /// Benchmark skill (`clicking`, `tracking`, `switching`, `movement`) whose weakness
    /// also points here.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark_skill: Option<&'static str>,
    /// How to train it.
    pub method: &'static str,
    /// An external-focus cue to give the player.
    pub cue: &'static str,
    /// Confidence in the method.
    pub confidence: Confidence,
    /// Drill parameters (always estimates).
    pub drill: DrillParams,
    /// When to make the drill harder or easier.
    pub progression: &'static str,
    /// The number that shows progress.
    pub progress_measure: &'static str,
    /// Built-in scenarios that train it, best first.
    pub scenarios: &'static [&'static str],
    /// Built-in routines that train it.
    pub routines: &'static [&'static str],
    /// Short source keys, detailed in `docs/training-science.md`.
    pub sources: &'static [&'static str],
}

impl KnowledgeEntry {
    /// Confidence in the drill numbers: always an estimate.
    pub fn drill_confidence(&self) -> Confidence {
        Confidence::Estimate
    }
}

/// The whole knowledge base, one entry per [`SubSkill`] in [`SubSkill::ALL`] order.
pub fn entries() -> &'static [KnowledgeEntry] {
    &ENTRIES
}

const fn sig(metric: Metric, direction: Direction) -> Signal {
    Signal { metric, direction }
}

use Direction::{Above, Below};

static ENTRIES: [KnowledgeEntry; 10] = [
    KnowledgeEntry {
        skill: SubSkill::FlickAccuracy,
        name: "Flick accuracy vs speed",
        signals: &[
            sig(Metric::UndershootRate, Above),
            sig(Metric::WeakestDirectionGap, Above),
            sig(Metric::PathEfficiency, Below),
        ],
        benchmark_skill: Some("clicking"),
        method: "Flick in one committed motion and let a correction finish the job; early \
                 gains come from committing and correcting, not from a faster primary \
                 movement. Train each emphasis (accuracy, then speed) in separate blocks.",
        cue: "Land on the head in one motion.",
        confidence: Confidence::Moderate,
        drill: DrillParams {
            target_radius_deg: Some((1.5, 3.0)),
            distance_deg: Some((10.0, 60.0)),
            ..DrillParams::BASE
        },
        progression: "Shrink the target 10% when the median accuracy of a block is at least \
                      85%; grow it back below 60%.",
        progress_measure: "Kills per minute at a stable accuracy.",
        scenarios: &["flick-wide", "spread-flicks", "flick-headshot"],
        routines: &["skill-clicking-static"],
        sources: &["warburton-2023", "fitts-1954"],
    },
    KnowledgeEntry {
        skill: SubSkill::MicroAdjustment,
        name: "Micro-adjustment",
        signals: &[
            sig(Metric::OvershootRate, Above),
            sig(Metric::Corrections, Above),
            sig(Metric::MissAboveShare, Above),
        ],
        benchmark_skill: Some("clicking"),
        method: "Small, fast corrections near the target; online corrections track skill \
                 closely. Keep targets head-sized and close to the crosshair.",
        cue: "Nudge onto the head and click as it settles.",
        confidence: Confidence::Moderate,
        drill: DrillParams {
            target_radius_deg: Some((0.6, 1.2)),
            distance_deg: Some((2.0, 8.0)),
            ..DrillParams::BASE
        },
        progression: "Shrink the target 10% at a median accuracy of 85% or more; grow it \
                      back below 60%.",
        progress_measure: "Accuracy and mean corrections per flick.",
        scenarios: &["micro-adjust", "static-headline"],
        routines: &["skill-clicking-static"],
        sources: &["warburton-2023"],
    },
    KnowledgeEntry {
        skill: SubSkill::ClickTiming,
        name: "Click timing (hesitation)",
        signals: &[sig(Metric::ClickDelay, Above)],
        benchmark_skill: None,
        method: "Shorten the time on target before the click; click dwell is the strongest \
                 kinematic correlate of FPS skill. Short target lifetimes force the commit.",
        cue: "Click the moment the crosshair touches the head.",
        confidence: Confidence::Moderate,
        drill: DrillParams {
            target_radius_deg: Some((1.5, 2.5)),
            distance_deg: Some((3.0, 12.0)),
            lifetime_secs: Some((0.65, 0.9)),
            ..DrillParams::BASE
        },
        progression: "Start at a 900 ms lifetime and take 50 ms off after each block that \
                      expires fewer than 10% of the targets.",
        progress_measure: "Median click delay.",
        scenarios: &["flick-wide", "peek-reaction"],
        routines: &["skill-clicking-dynamic"],
        sources: &["warburton-2023"],
    },
    KnowledgeEntry {
        skill: SubSkill::ReactiveAcquisition,
        name: "Reactive acquisition",
        signals: &[
            sig(Metric::ReactionTime, Above),
            sig(Metric::StartRatio, Below),
        ],
        benchmark_skill: None,
        method: "Targets appear after an unpredictable wait; react, then flick. Reaction \
                 time itself trains little, so score the whole acquisition, not the start.",
        cue: "See it, then snap to the head.",
        confidence: Confidence::Expert,
        drill: DrillParams {
            target_radius_deg: Some((1.2, 2.5)),
            distance_deg: Some((12.0, 30.0)),
            lifetime_secs: Some((1.5, 2.5)),
            spawn_delay_secs: Some((0.4, 2.0)),
            ..DrillParams::BASE
        },
        progression: "Shorten the lifetime 10% when fewer than 10% of the targets expire.",
        progress_measure: "Median time to kill.",
        scenarios: &["peek-reaction", "react-duel"],
        routines: &["quick-warmup"],
        sources: &["expert"],
    },
    KnowledgeEntry {
        skill: SubSkill::SmoothTracking,
        name: "Smooth tracking",
        signals: &[
            sig(Metric::OnTargetRatio, Below),
            sig(Metric::SpeedMatch, Below),
            sig(Metric::EdgeShare, Above),
        ],
        benchmark_skill: Some("tracking"),
        method: "Match the target's speed first, then centre on it; smoothness comes before \
                 precision.",
        cue: "Match the target's speed first.",
        confidence: Confidence::Expert,
        drill: DrillParams {
            target_radius_deg: Some((0.8, 1.5)),
            speed_deg_per_sec: Some((30.0, 60.0)),
            ..DrillParams::BASE
        },
        progression: "Add 10% target speed at 75% or more time on target.",
        progress_measure: "Time on target and speed match.",
        scenarios: &[
            "track-precise",
            "track-orbit",
            "control-weave",
            "vertical-rail",
        ],
        routines: &["skill-tracking-precise", "skill-tracking-control"],
        sources: &["chua-2021", "expert"],
    },
    KnowledgeEntry {
        skill: SubSkill::ReactiveTracking,
        name: "Reactive tracking",
        signals: &[
            sig(Metric::ReversalResponse, Above),
            sig(Metric::LagRatio, Above),
        ],
        benchmark_skill: Some("tracking"),
        method: "Follow unpredictable direction changes; react to the reversal instead of \
                 predicting it.",
        cue: "Stay glued to the head through every change of direction.",
        confidence: Confidence::Expert,
        drill: DrillParams::BASE,
        progression: "Shorten the time between direction changes 10% at 70% or more time on \
                      target.",
        progress_measure: "Reaction to direction changes and time on target.",
        scenarios: &[
            "track-reactive-strafe",
            "track-reactive-micro",
            "stutter-read-track",
        ],
        routines: &["skill-tracking-reactive"],
        sources: &["expert"],
    },
    KnowledgeEntry {
        skill: SubSkill::TargetSwitching,
        name: "Target switching",
        signals: &[
            sig(Metric::NearestFirstShare, Below),
            sig(Metric::MissedOpportunityRate, Above),
        ],
        benchmark_skill: Some("switching"),
        method: "Kill the nearest target first and move on without waiting; spread the \
                 targets wider as the switches get clean.",
        cue: "Next head, nearest first.",
        confidence: Confidence::Expert,
        drill: DrillParams {
            target_radius_deg: Some((1.2, 2.0)),
            distance_deg: Some((8.0, 40.0)),
            targets: Some((3, 6)),
            ..DrillParams::BASE
        },
        progression: "Add a target or widen the spacing when accuracy stays at 80% or more.",
        progress_measure: "Kills per minute and nearest-first share.",
        scenarios: &[
            "switch-wide",
            "switch-speed",
            "switch-heads",
            "steady-agents-switch",
        ],
        routines: &["skill-switching-speed", "skill-switching-stability"],
        sources: &["expert"],
    },
    KnowledgeEntry {
        skill: SubSkill::CrosshairPlacement,
        name: "Crosshair placement",
        signals: &[sig(Metric::MissBelowShare, Above)],
        benchmark_skill: None,
        method: "Keep the crosshair on the head line so most fights need only a small \
                 horizontal correction.",
        cue: "Keep the crosshair where the next head will be.",
        confidence: Confidence::Expert,
        drill: DrillParams {
            targets: Some((1, 3)),
            ..DrillParams::BASE
        },
        progression: "Spread the heads deeper and wider at 80% or more accuracy.",
        progress_measure: "Share of misses below the aim point.",
        scenarios: &["static-headline", "flick-headshot"],
        routines: &["valorant-daily"],
        sources: &["expert"],
    },
    KnowledgeEntry {
        skill: SubSkill::MovingShots,
        name: "Counter-strafe and moving shots",
        signals: &[sig(Metric::MovingShotRate, Above)],
        benchmark_skill: Some("movement"),
        method: "Counter-strafe to a stop before the shot: VALORANT adds running inaccuracy \
                 above the weapon's accuracy speed limit (docs/valorant-mechanics.md).",
        cue: "Tap the opposite key, then shoot.",
        confidence: Confidence::Strong,
        drill: DrillParams {
            lifetime_secs: Some((2.5, 4.0)),
            ..DrillParams::BASE
        },
        progression: "Shorten the target lifetime 10% when fewer than 10% of the shots are \
                      fired while moving.",
        progress_measure: "Share of shots fired while moving.",
        scenarios: &["counter-strafe", "strafe-shot"],
        routines: &["aim-and-duel"],
        sources: &["valorant-mechanics"],
    },
    KnowledgeEntry {
        skill: SubSkill::PeekAndHold,
        name: "Peek and hold",
        signals: &[sig(Metric::ReactionTime, Above)],
        benchmark_skill: None,
        method: "Hold the angle pre-aimed at head height; the peeker's advantage is about \
                 71–141 ms, so the holder must react inside a short exposure window.",
        cue: "Wait on the head line and fire as the head appears.",
        confidence: Confidence::Moderate,
        drill: DrillParams {
            lifetime_secs: Some((1.1, 1.5)),
            spawn_delay_secs: Some((0.6, 2.5)),
            ..DrillParams::BASE
        },
        progression: "Shorten the exposure 10% when fewer than 20% of the peeks escape.",
        progress_measure: "Peeks killed before they leave.",
        scenarios: &["peek-reaction", "dodge-peek"],
        routines: &["aim-and-duel"],
        sources: &["riot-netcode-2020"],
    },
];

/// One sub-skill the context points at, with the numbers behind it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Diagnosis {
    /// The weak sub-skill.
    pub skill: SubSkill,
    /// How strongly the context points at it (higher is weaker).
    pub score: f32,
    /// Each piece of evidence, with its value and threshold.
    pub evidence: Vec<String>,
}

/// The sub-skills `ctx` points at, weakest first. A weakness finding counts with its
/// severity; a benchmark skill that stands out as weak adds [`BENCHMARK_WEIGHT`].
pub fn diagnose(ctx: &CoachContext) -> Vec<Diagnosis> {
    let mut out: Vec<Diagnosis> = ENTRIES
        .iter()
        .filter_map(|entry| {
            let mut score = 0.0f32;
            let mut evidence = Vec::new();
            for f in ctx
                .findings
                .iter()
                .filter(|f| f.verdict == Verdict::Weakness)
            {
                let matches = entry
                    .signals
                    .iter()
                    .any(|s| s.metric == f.metric && s.direction == f.direction);
                if matches {
                    score = score.max(f.severity.max(0.0));
                    evidence.push(f.explanation.clone());
                }
            }
            let bench = ctx.benchmark.iter().flat_map(|b| &b.subcategories);
            for sub in bench {
                let weak = sub.verdict == Some(Verdict::Weakness);
                if weak && entry.benchmark_skill == Some(sub.skill.as_str()) {
                    score += BENCHMARK_WEIGHT;
                    evidence.push(match sub.energy {
                        Some(e) => format!("Benchmark {} is a weakness ({e:.0} energy).", sub.name),
                        None => format!("Benchmark {} is a weakness.", sub.name),
                    });
                }
            }
            (!evidence.is_empty()).then_some(Diagnosis {
                skill: entry.skill,
                score,
                evidence,
            })
        })
        .collect();
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coach::context::{BenchmarkSummary, FindingSummary, SubcategorySummary};
    use crate::scenario::{Category, builtin};
    use crate::training::routine;

    fn weakness(
        metric: Metric,
        direction: Direction,
        value: f32,
        threshold: f32,
    ) -> FindingSummary {
        let t = threshold.abs();
        FindingSummary {
            category: Category::Flicking,
            verdict: Verdict::Weakness,
            metric,
            value,
            threshold,
            direction,
            severity: match direction {
                Above => (value - threshold) / t,
                Below => (threshold - value) / t,
            },
            explanation: format!("{} {value}", metric.label()),
            scenarios: Vec::new(),
            routine: None,
        }
    }

    #[test]
    fn one_entry_per_sub_skill_in_order_with_estimated_drills() {
        let skills: Vec<SubSkill> = entries().iter().map(|e| e.skill).collect();
        assert_eq!(skills, SubSkill::ALL);
        for e in entries() {
            assert_eq!(SubSkill::from_id(e.skill.id()), Some(e.skill));
            assert_eq!(e.drill_confidence(), Confidence::Estimate);
            assert!(!e.signals.is_empty(), "{}", e.name);
            assert!(!e.cue.is_empty() && !e.method.is_empty(), "{}", e.name);
        }
    }

    #[test]
    fn every_signal_matches_a_builtin_weakness_rule() {
        for e in entries() {
            for s in e.signals {
                assert!(s.threshold().is_some(), "{}: {:?}", e.name, s);
            }
        }
    }

    #[test]
    fn referenced_scenarios_and_routines_exist() {
        let scenarios: Vec<String> = builtin::all().into_iter().map(|d| d.id).collect();
        let routines: Vec<String> = routine::builtin::all().into_iter().map(|r| r.id).collect();
        for e in entries() {
            for id in e.scenarios {
                assert!(scenarios.iter().any(|s| s == id), "{}: {id}", e.name);
            }
            for id in e.routines {
                assert!(routines.iter().any(|r| r == id), "{}: {id}", e.name);
            }
        }
    }

    #[test]
    fn diagnosis_picks_the_sub_skill_behind_the_worst_finding() {
        let ctx = CoachContext {
            findings: vec![
                weakness(Metric::ClickDelay, Above, 0.25, 0.20),
                weakness(Metric::OvershootRate, Above, 0.70, 0.35),
            ],
            ..Default::default()
        };
        let d = diagnose(&ctx);
        assert_eq!(d[0].skill, SubSkill::MicroAdjustment, "{d:?}");
        assert_eq!(d[1].skill, SubSkill::ClickTiming, "{d:?}");
        assert!(d[0].evidence[0].contains("Overshoot"), "{d:?}");
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn diagnosis_ignores_strengths_and_the_wrong_side_of_a_metric() {
        let mut strength = weakness(Metric::OvershootRate, Below, 0.05, 0.10);
        strength.verdict = Verdict::Strength;
        // lag_ratio below 0.35 is a weakness of its own (running ahead), not reactive tracking.
        let ahead = weakness(Metric::LagRatio, Below, 0.2, 0.35);
        let ctx = CoachContext {
            findings: vec![strength, ahead],
            ..Default::default()
        };
        assert!(diagnose(&ctx).is_empty(), "{:?}", diagnose(&ctx));
    }

    #[test]
    fn a_weak_benchmark_skill_points_at_its_sub_skills() {
        let ctx = CoachContext {
            benchmark: Some(BenchmarkSummary {
                average: Some(500.0),
                subcategories: vec![SubcategorySummary {
                    name: "Tracking Precise".into(),
                    skill: "tracking".into(),
                    energy: Some(300.0),
                    verdict: Some(Verdict::Weakness),
                }],
            }),
            ..Default::default()
        };
        let skills: Vec<SubSkill> = diagnose(&ctx).iter().map(|d| d.skill).collect();
        assert_eq!(
            skills,
            [SubSkill::SmoothTracking, SubSkill::ReactiveTracking]
        );
    }
}
