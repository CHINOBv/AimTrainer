//! The coach's view of the player: a compact, serializable summary built from the existing
//! analyses (skill profile, flick directions, miss compass, pace, potential score, benchmark
//! energy, recent runs, missions, readiness and fatigue).
//!
//! Nothing is recomputed here; every number comes from an analysis the player already sees.
//! The summary never holds raw input traces, file paths or player names, and
//! [`CoachContext::to_prompt_json`] bounds its size before it is sent to a model.

use serde::{Serialize, Serializer};

use crate::analytics::miss_compass::{Compass, MissCompass};
use crate::analytics::pace::PaceSummary;
use crate::analytics::potential::PotentialTip;
use crate::analytics::rules::{Direction, Metric, Verdict};
use crate::analytics::{BenchmarkProfile, DirectionSummary, FlickSummary, SkillProfile};
use crate::analytics::{Finding, TrackingSummary};
use crate::scenario::Category;
use crate::training::missions::MissionBook;
use crate::training::progress::{FatigueSignal, Readiness, RunRecord, Warmth};

/// Most findings sent to a model, weaknesses first.
pub const MAX_FINDINGS: usize = 8;
/// Most scenarios listed under recent runs, most recently played first.
pub const MAX_RECENT_SCENARIOS: usize = 8;
/// Most runs listed per recent scenario, newest first.
pub const RUNS_PER_SCENARIO: usize = 5;
/// Most daily missions sent to a model.
pub const MAX_MISSIONS: usize = 6;
/// Most fatigue signals sent to a model.
pub const MAX_FATIGUE: usize = 4;
/// Upper bound of [`CoachContext::to_prompt_json`], in bytes.
pub const PROMPT_JSON_MAX_BYTES: usize = 8 * 1024;
/// Longest run key kept; longer keys are dropped rather than cut.
const MAX_KEY_LEN: usize = 64;
const SECS_PER_DAY: u64 = 86_400;

/// The analyses a [`CoachContext`] is built from. Every field is optional, so a new player
/// with no history still gets a (nearly empty) context.
#[derive(Debug, Clone, Copy, Default)]
pub struct ContextInputs<'a> {
    /// Findings and per-category summaries.
    pub profile: Option<&'a SkillProfile>,
    /// Per-direction flick buckets.
    pub directions: Option<&'a DirectionSummary>,
    /// Where missed shots went.
    pub miss_compass: Option<&'a MissCompass>,
    /// Opening and late pace over recent runs.
    pub pace: Option<&'a PaceSummary>,
    /// The potential-score tip of the latest run.
    pub potential: Option<&'a PotentialTip>,
    /// Benchmark energy per subcategory.
    pub benchmark: Option<&'a BenchmarkProfile>,
    /// Run history, oldest first.
    pub records: &'a [RunRecord],
    /// Daily missions and the recent run metrics behind them.
    pub missions: Option<&'a MissionBook>,
    /// Warm-up readiness of the player's reference scenario.
    pub readiness: Option<&'a Readiness>,
    /// Fatigue signals of the current session.
    pub fatigue: &'a [FatigueSignal],
    /// Unix seconds now, used only to turn run timestamps into "days ago".
    pub now: u64,
}

/// A compact summary of the player for a coaching model.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CoachContext {
    /// Fired rules, weaknesses first, each by severity.
    pub findings: Vec<FindingSummary>,
    /// Flick and tracking aggregates per category.
    pub categories: Vec<CategorySummary>,
    /// Per-direction flick hit rates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directions: Option<DirectionsSummary>,
    /// Where missed shots went.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub miss_compass: Option<MissCompassSummary>,
    /// Opening and late pace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pace: Option<PaceTrend>,
    /// The single costliest weakness of the latest run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub potential: Option<PotentialSummary>,
    /// Benchmark energy per subcategory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark: Option<BenchmarkSummary>,
    /// The latest runs per scenario, most recently played scenario first.
    pub recent: Vec<RecentScenario>,
    /// Today's missions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missions: Option<MissionsSummary>,
    /// Warm-up readiness.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readiness: Option<ReadinessSummary>,
    /// Fatigue signals of the current session, as sentences.
    pub fatigue: Vec<String>,
}

/// One fired rule, with the number and the threshold behind it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FindingSummary {
    /// Category the finding belongs to.
    pub category: Category,
    /// Strength or weakness.
    #[serde(serialize_with = "snake_debug")]
    pub verdict: Verdict,
    /// The measured metric (serialized as its snake_case id).
    #[serde(serialize_with = "snake_debug")]
    pub metric: Metric,
    /// The measured value, in the metric's unit.
    pub value: f32,
    /// The rule's threshold, in the metric's unit.
    pub threshold: f32,
    /// Which side of the threshold fired the rule.
    #[serde(serialize_with = "snake_debug")]
    pub direction: Direction,
    /// How far past the threshold the value is, relative to it.
    pub severity: f32,
    /// The finding in one sentence, with the numbers.
    pub explanation: String,
    /// Built-in scenarios recommended for it, best first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub scenarios: Vec<String>,
    /// Routine recommended for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routine: Option<String>,
}

/// Aggregates of one category.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CategorySummary {
    /// The category.
    pub category: Category,
    /// Analyzed runs.
    pub runs: usize,
    /// Flick aggregates, for click scenarios.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flick: Option<FlickStats>,
    /// Tracking aggregates, for hold scenarios.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracking: Option<TrackingStats>,
}

/// The flick aggregates a coach needs, from [`FlickSummary`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlickStats {
    /// Analyzed flicks.
    pub flicks: usize,
    /// Share of flicks that hit.
    pub hit_rate: f32,
    /// Median reaction time of fresh flicks, seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median_reaction_secs: Option<f32>,
    /// Share of flicks whose primary movement overshot.
    pub overshoot_rate: f32,
    /// Share of flicks whose primary movement undershot.
    pub undershoot_rate: f32,
    /// Mean corrective sub-movements per flick.
    pub mean_corrections: f32,
    /// Median time from entering the target to the shot, seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median_click_delay_secs: Option<f32>,
    /// Effective throughput, bits per second.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throughput_bits_per_sec: Option<f32>,
}

/// The tracking aggregates a coach needs, from [`TrackingSummary`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TrackingStats {
    /// Seconds with fire held.
    pub hold_secs: f32,
    /// Share of firing time on target.
    pub on_target_ratio: f32,
    /// Median angular error, degrees.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median_error_deg: Option<f32>,
    /// Share of moving-target time spent behind the target.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lag_ratio: Option<f32>,
    /// Share of moving-target time with matched speed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_match_ratio: Option<f32>,
    /// Median seconds to follow a reversal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median_reversal_response_secs: Option<f32>,
}

/// Per-direction flick hit rates.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DirectionsSummary {
    /// Buckets with at least one flick.
    pub buckets: Vec<DirectionBucketSummary>,
    /// The quadrant that is clearly weaker than the others, if one is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weakest: Option<WeakestDirectionSummary>,
    /// The axis with clearly more misses, if one has.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weak_axis: Option<&'static str>,
}

/// One direction × distance bucket.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DirectionBucketSummary {
    /// Screen quadrant label.
    pub quadrant: &'static str,
    /// Near or far.
    pub distance: &'static str,
    /// Flicks in the bucket.
    pub flicks: usize,
    /// Share of them that hit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hit_rate: Option<f32>,
    /// Median reaction time, seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub median_reaction_secs: Option<f32>,
}

/// The weakest flick direction.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WeakestDirectionSummary {
    /// Screen quadrant label.
    pub quadrant: &'static str,
    /// Its hit rate.
    pub hit_rate: f32,
    /// The hit rate of every other flick.
    pub others_hit_rate: f32,
}

/// Where missed shots went.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MissCompassSummary {
    /// Misses counted.
    pub misses: u32,
    /// Non-zero counts per compass direction.
    pub counts: Vec<CompassCount>,
    /// Share of misses above the aim point.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub above_share: Option<f32>,
    /// Share of misses below the aim point.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub below_share: Option<f32>,
}

/// Misses in one compass direction.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompassCount {
    /// Compass label.
    pub direction: &'static str,
    /// Misses that went that way.
    pub count: u32,
}

/// Opening and late pace over recent runs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaceTrend {
    /// Runs with a pace analysis.
    pub runs: usize,
    /// Opening-window rate ÷ the rest of the run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_ratio: Option<f32>,
    /// Late-window rate ÷ the early windows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stamina_ratio: Option<f32>,
}

/// The potential-score tip.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PotentialSummary {
    /// The run's score.
    pub score: f32,
    /// The estimated score with the weakness fixed.
    pub potential_score: f32,
    /// The weakness to fix.
    pub fix: &'static str,
    /// The gain as a share of the score, when the score is positive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gain_share: Option<f32>,
    /// The assumption behind the estimate.
    pub assumption: String,
}

/// Benchmark energy per subcategory.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BenchmarkSummary {
    /// Mean energy of the played subcategories.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average: Option<f32>,
    /// One entry per subcategory.
    pub subcategories: Vec<SubcategorySummary>,
}

/// One benchmark subcategory.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SubcategorySummary {
    /// Subcategory name.
    pub name: String,
    /// Skill it belongs to (clicking, tracking, switching, movement).
    pub skill: String,
    /// Current energy, if played.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub energy: Option<f32>,
    /// Whether it stands out as a strength or a weakness.
    #[serde(
        serialize_with = "snake_debug_opt",
        skip_serializing_if = "Option::is_none"
    )]
    pub verdict: Option<Verdict>,
}

/// The latest runs of one scenario (run key).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecentScenario {
    /// Run key (scenario id plus level or tuning suffixes).
    pub scenario: String,
    /// Newest first.
    pub runs: Vec<RecentRun>,
    /// Mean accuracy over the recent runs the missions keep, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mean_accuracy: Option<f32>,
}

/// One recent run.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct RecentRun {
    /// Score.
    pub score: f32,
    /// Whole days since the run.
    pub days_ago: u64,
}

/// Today's missions.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MissionsSummary {
    /// The mission day.
    pub date: String,
    /// The player's commitment level.
    pub commitment: &'static str,
    /// The missions.
    pub missions: Vec<MissionSummary>,
}

/// One daily mission.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MissionSummary {
    /// Scenario id.
    pub scenario: String,
    /// The conditions, as text.
    pub conditions: Vec<String>,
    /// Whether it is a mastery mission.
    pub mastery: bool,
    /// Whether it is done.
    pub done: bool,
}

/// Warm-up readiness.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReadinessSummary {
    /// Scenario id.
    pub scenario: String,
    /// Today's best ÷ the baseline.
    pub ratio: f32,
    /// cold, warming or ready.
    pub warmth: &'static str,
}

impl CoachContext {
    /// Summarize `inputs`. Lists are kept whole except recent runs, which are already
    /// bounded to [`MAX_RECENT_SCENARIOS`] × [`RUNS_PER_SCENARIO`].
    pub fn build(inputs: &ContextInputs) -> Self {
        let profile = inputs.profile;
        let mut findings: Vec<FindingSummary> = profile
            .into_iter()
            .flat_map(|p| p.findings())
            .map(summarize_finding)
            .collect();
        // Weaknesses first; inside each verdict the furthest past its threshold first.
        findings.sort_by(|a, b| {
            let rank = |f: &FindingSummary| u8::from(f.verdict != Verdict::Weakness);
            rank(a)
                .cmp(&rank(b))
                .then(b.severity.total_cmp(&a.severity))
        });
        Self {
            findings,
            categories: profile
                .into_iter()
                .flat_map(|p| &p.categories)
                .map(|c| CategorySummary {
                    category: c.category,
                    runs: c.runs,
                    flick: c.flick.as_ref().map(flick_stats),
                    tracking: c.tracking.as_ref().map(tracking_stats),
                })
                .collect(),
            directions: inputs.directions.map(directions),
            miss_compass: inputs.miss_compass.map(miss_compass),
            pace: inputs.pace.map(|p| PaceTrend {
                runs: p.runs,
                start_ratio: p.start_ratio.map(round3),
                stamina_ratio: p.stamina_ratio.map(round3),
            }),
            potential: inputs.potential.map(potential),
            benchmark: inputs.benchmark.map(benchmark),
            recent: recent(inputs),
            missions: inputs.missions.and_then(missions),
            readiness: inputs.readiness.and_then(|r| {
                Some(ReadinessSummary {
                    scenario: sanitize_key(&r.scenario)?,
                    ratio: round3(r.ratio),
                    warmth: match r.warmth {
                        Warmth::Cold => "cold",
                        Warmth::Warming => "warming",
                        Warmth::Ready => "ready",
                    },
                })
            }),
            fatigue: inputs.fatigue.iter().filter_map(describe_fatigue).collect(),
        }
    }

    /// The context as JSON for a model prompt, at most [`PROMPT_JSON_MAX_BYTES`] long:
    /// the top [`MAX_FINDINGS`] findings, [`MAX_MISSIONS`] missions and so on, halved again
    /// while the text is still too long.
    pub fn to_prompt_json(&self) -> String {
        let mut limits = Limits::PROMPT;
        loop {
            let json = serde_json::to_string(&self.bounded(limits))
                .expect("a context holds only strings, numbers and lists");
            if json.len() <= PROMPT_JSON_MAX_BYTES || limits == Limits::MINIMAL {
                return json;
            }
            limits = limits.halved();
        }
    }

    fn bounded(&self, l: Limits) -> Self {
        let mut out = self.clone();
        out.findings.truncate(l.findings);
        out.recent.truncate(l.scenarios);
        for r in &mut out.recent {
            r.runs.truncate(l.runs);
        }
        if let Some(m) = out.missions.as_mut() {
            m.missions.truncate(l.missions);
        }
        if let Some(b) = out.benchmark.as_mut() {
            b.subcategories.truncate(l.subcategories);
        }
        out.fatigue.truncate(l.fatigue);
        out
    }
}

/// How many items of each list [`CoachContext::to_prompt_json`] sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Limits {
    findings: usize,
    scenarios: usize,
    runs: usize,
    missions: usize,
    subcategories: usize,
    fatigue: usize,
}

impl Limits {
    const PROMPT: Limits = Limits {
        findings: MAX_FINDINGS,
        scenarios: MAX_RECENT_SCENARIOS,
        runs: RUNS_PER_SCENARIO,
        missions: MAX_MISSIONS,
        subcategories: 16,
        fatigue: MAX_FATIGUE,
    };
    const MINIMAL: Limits = Limits {
        findings: 1,
        scenarios: 1,
        runs: 1,
        missions: 1,
        subcategories: 1,
        fatigue: 1,
    };

    fn halved(self) -> Self {
        let h = |n: usize| (n / 2).max(1);
        Limits {
            findings: h(self.findings),
            scenarios: h(self.scenarios),
            runs: h(self.runs),
            missions: h(self.missions),
            subcategories: h(self.subcategories),
            fatigue: h(self.fatigue),
        }
    }
}

/// Round to three decimals: enough for rates and seconds, and shorter prompts.
fn round3(v: f32) -> f32 {
    (v * 1000.0).round() / 1000.0
}

/// A run key as stored by the history (`id@+0.50~20s%s80d120v150^+1.50m`), or `None` when
/// it holds anything else, such as a path separator, spaces or capitals, which could carry
/// a file path or a name to the model.
fn sanitize_key(key: &str) -> Option<String> {
    let allowed = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || "-_@.+~%^".contains(c);
    (!key.is_empty() && key.len() <= MAX_KEY_LEN && key.chars().all(allowed))
        .then(|| key.to_owned())
}

fn summarize_finding(f: &Finding) -> FindingSummary {
    let rec = f.recommendation.as_ref();
    FindingSummary {
        category: f.category,
        verdict: f.verdict,
        metric: f.metric,
        value: round3(f.value),
        threshold: f.threshold,
        direction: f.direction,
        severity: round3(f.severity()),
        explanation: f.explanation.clone(),
        scenarios: rec.map(|r| r.scenario_ids.clone()).unwrap_or_default(),
        routine: rec.and_then(|r| r.routine_id.clone()),
    }
}

fn flick_stats(f: &FlickSummary) -> FlickStats {
    FlickStats {
        flicks: f.flicks,
        hit_rate: round3(f.hit_rate),
        median_reaction_secs: f.median_reaction_secs.map(round3),
        overshoot_rate: round3(f.overshoot_rate),
        undershoot_rate: round3(f.undershoot_rate),
        mean_corrections: round3(f.mean_corrections),
        median_click_delay_secs: f.median_click_delay_secs.map(round3),
        throughput_bits_per_sec: f.throughput_bits_per_sec.map(round3),
    }
}

fn tracking_stats(t: &TrackingSummary) -> TrackingStats {
    TrackingStats {
        hold_secs: round3(t.hold_secs),
        on_target_ratio: round3(t.on_target_ratio),
        median_error_deg: t.median_error_deg.map(round3),
        lag_ratio: t.lag_ratio.map(round3),
        speed_match_ratio: t.speed_match_ratio.map(round3),
        median_reversal_response_secs: t.median_reversal_response_secs.map(round3),
    }
}

fn directions(d: &DirectionSummary) -> DirectionsSummary {
    DirectionsSummary {
        buckets: d
            .buckets
            .iter()
            .filter(|b| b.flicks > 0)
            .map(|b| DirectionBucketSummary {
                quadrant: b.quadrant.label(),
                distance: b.distance.label(),
                flicks: b.flicks,
                hit_rate: b.hit_rate().map(round3),
                median_reaction_secs: b.median_reaction_secs.map(round3),
            })
            .collect(),
        weakest: d.weakest.map(|w| WeakestDirectionSummary {
            quadrant: w.quadrant.label(),
            hit_rate: round3(w.hit_rate),
            others_hit_rate: round3(w.others_hit_rate),
        }),
        weak_axis: d.weak_axis.map(|a| a.label()),
    }
}

fn miss_compass(m: &MissCompass) -> MissCompassSummary {
    MissCompassSummary {
        misses: m.misses(),
        counts: Compass::ALL
            .iter()
            .map(|&c| CompassCount {
                direction: c.label(),
                count: m.count(c),
            })
            .filter(|c| c.count > 0)
            .collect(),
        above_share: m.above_share().map(round3),
        below_share: m.below_share().map(round3),
    }
}

fn potential(p: &PotentialTip) -> PotentialSummary {
    PotentialSummary {
        score: round3(p.score),
        potential_score: round3(p.potential_score()),
        fix: p.gain.fix.label(),
        gain_share: (p.score > 0.0).then(|| round3(p.gain_share())),
        assumption: p.gain.assumption(),
    }
}

fn benchmark(b: &BenchmarkProfile) -> BenchmarkSummary {
    BenchmarkSummary {
        average: b.average.map(round3),
        subcategories: b
            .subcategories
            .iter()
            .map(|s| SubcategorySummary {
                name: s.name.clone(),
                skill: s.skill.clone(),
                energy: s.energy.map(round3),
                verdict: s.verdict,
            })
            .collect(),
    }
}

/// The latest runs per run key, most recently played key first.
fn recent(inputs: &ContextInputs) -> Vec<RecentScenario> {
    let mut out: Vec<RecentScenario> = Vec::new();
    for r in inputs.records.iter().rev() {
        let Some(key) = sanitize_key(&r.scenario) else {
            continue;
        };
        let run = RecentRun {
            score: round3(r.score),
            days_ago: inputs.now.saturating_sub(r.timestamp) / SECS_PER_DAY,
        };
        match out.iter().position(|s| s.scenario == key) {
            Some(i) if out[i].runs.len() < RUNS_PER_SCENARIO => out[i].runs.push(run),
            Some(_) => {}
            None if out.len() < MAX_RECENT_SCENARIOS => out.push(RecentScenario {
                scenario: key,
                runs: vec![run],
                mean_accuracy: None,
            }),
            None => {}
        }
    }
    if let Some(book) = inputs.missions {
        for s in &mut out {
            let runs = book.recent.iter().find(|r| r.scenario == s.scenario);
            s.mean_accuracy = runs.filter(|r| !r.runs.is_empty()).map(|r| {
                round3(r.runs.iter().map(|m| m.accuracy).sum::<f32>() / r.runs.len() as f32)
            });
        }
    }
    out
}

fn missions(book: &MissionBook) -> Option<MissionsSummary> {
    let day = book.today.as_ref()?;
    Some(MissionsSummary {
        date: day.date.clone(),
        commitment: day.commitment.label(),
        missions: day
            .missions
            .iter()
            .filter_map(|m| {
                Some(MissionSummary {
                    scenario: sanitize_key(&m.scenario)?,
                    conditions: m.conditions.iter().map(|c| c.describe()).collect(),
                    mastery: m.mastery,
                    done: m.done,
                })
            })
            .collect(),
    })
}

fn describe_fatigue(f: &FatigueSignal) -> Option<String> {
    Some(match f {
        FatigueSignal::LongSession { minutes } => {
            format!("The session has lasted {minutes:.0} minutes.")
        }
        FatigueSignal::ScoreDrop {
            scenario,
            peak,
            recent,
            drop,
        } => format!(
            "{} scores fell {:.0}% from a session peak of {peak:.0} to {recent:.0}.",
            sanitize_key(scenario)?,
            drop * 100.0
        ),
    })
}

fn snake_debug<T: std::fmt::Debug, S: Serializer>(v: &T, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&snake_case(&format!("{v:?}")))
}

fn snake_debug_opt<T: std::fmt::Debug, S: Serializer>(
    v: &Option<T>,
    s: S,
) -> Result<S::Ok, S::Error> {
    match v {
        Some(v) => snake_debug(v, s),
        None => s.serialize_none(),
    }
}

/// `CamelCase` → `snake_case`, matching the serde ids of the rules file.
fn snake_case(camel: &str) -> String {
    let mut out = String::with_capacity(camel.len() + 4);
    for (i, c) in camel.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::profile::CategoryProfile;
    use crate::training::missions::{Commitment, Condition, Mission, MissionDay};

    const DAY: u64 = 86_400;
    const NOW: u64 = 10_000 * DAY;

    fn finding(metric: Metric, value: f32, threshold: f32, verdict: Verdict) -> Finding {
        Finding {
            category: Category::Flicking,
            verdict,
            metric,
            value,
            threshold,
            direction: Direction::Above,
            explanation: format!("{} is {value}", metric.label()),
            recommendation: None,
        }
    }

    fn profile(findings: Vec<Finding>) -> SkillProfile {
        SkillProfile {
            categories: vec![CategoryProfile {
                category: Category::Flicking,
                runs: 4,
                flick: Some(FlickSummary::from_flicks(&[])),
                tracking: None,
                findings,
            }],
        }
    }

    fn record(scenario: &str, score: f32, days_ago: u64) -> RunRecord {
        RunRecord {
            timestamp: NOW - days_ago * DAY,
            scenario: scenario.to_owned(),
            score,
            duration_secs: 60.0,
        }
    }

    #[test]
    fn findings_keep_value_and_threshold_with_weaknesses_first_by_severity() {
        let p = profile(vec![
            finding(Metric::ReactionTime, 0.20, 0.22, Verdict::Strength),
            finding(Metric::OvershootRate, 0.40, 0.35, Verdict::Weakness),
            finding(Metric::Corrections, 2.0, 1.0, Verdict::Weakness),
        ]);
        let ctx = CoachContext::build(&ContextInputs {
            profile: Some(&p),
            ..Default::default()
        });
        let order: Vec<Metric> = ctx.findings.iter().map(|f| f.metric).collect();
        assert_eq!(
            order,
            [
                Metric::Corrections,
                Metric::OvershootRate,
                Metric::ReactionTime
            ]
        );
        assert_eq!(ctx.findings[1].value, 0.4);
        assert_eq!(ctx.findings[1].threshold, 0.35);
        assert_eq!(ctx.categories.len(), 1);
        let json = ctx.to_prompt_json();
        assert!(json.contains("\"metric\":\"overshoot_rate\""), "{json}");
        assert!(json.contains("\"verdict\":\"weakness\""), "{json}");
        assert!(json.contains("\"threshold\":0.35"), "{json}");
    }

    #[test]
    fn recent_runs_are_grouped_newest_first_and_bounded() {
        let mut records = Vec::new();
        for i in 0..(MAX_RECENT_SCENARIOS + 3) {
            for d in 0..(RUNS_PER_SCENARIO as u64 + 2) {
                // Scenario 0 is the most recently played.
                records.push(record(&format!("s{i}"), d as f32, 50 + i as u64 * 10 + d));
            }
        }
        records.sort_by_key(|r| r.timestamp);
        let ctx = CoachContext::build(&ContextInputs {
            records: &records,
            now: NOW,
            ..Default::default()
        });
        assert_eq!(ctx.recent.len(), MAX_RECENT_SCENARIOS);
        assert_eq!(ctx.recent[0].scenario, "s0");
        assert_eq!(ctx.recent[0].runs.len(), RUNS_PER_SCENARIO);
        assert_eq!(ctx.recent[0].runs[0].days_ago, 50, "newest run first");
        assert_eq!(ctx.recent[0].runs[1].days_ago, 51);
    }

    #[test]
    fn paths_and_odd_keys_never_reach_the_context() {
        let records = [
            record("C:\\Users\\David\\drills\\mine", 10.0, 1),
            record("/home/david/.config/aimtrainer/x", 10.0, 1),
            record("David's Drill", 10.0, 1),
            record("micro-adjust@+0.50", 20.0, 2),
        ];
        let ctx = CoachContext::build(&ContextInputs {
            records: &records,
            now: NOW,
            ..Default::default()
        });
        let keys: Vec<&str> = ctx.recent.iter().map(|r| r.scenario.as_str()).collect();
        assert_eq!(keys, ["micro-adjust@+0.50"]);
        let json = ctx.to_prompt_json();
        for banned in ["\\", "/home", "Users", "David", "errors_deg", "samples"] {
            assert!(!json.contains(banned), "{banned} leaked: {json}");
        }
    }

    #[test]
    fn missions_readiness_and_fatigue_are_summarized() {
        let book = MissionBook {
            today: Some(MissionDay {
                date: "2026-10-18".into(),
                commitment: Commitment::Medium,
                missions: vec![Mission {
                    scenario: "flick-wide".into(),
                    conditions: vec![Condition {
                        metric: crate::training::missions::Metric::Accuracy,
                        target: 0.75,
                    }],
                    mastery: false,
                    done: true,
                    best_met: 1,
                }],
            }),
            ..Default::default()
        };
        let readiness = Readiness {
            scenario: "flick-wide".into(),
            baseline: 100.0,
            today_best: 80.0,
            ratio: 0.8,
            warmth: Warmth::Cold,
        };
        let fatigue = [FatigueSignal::LongSession { minutes: 75.0 }];
        let ctx = CoachContext::build(&ContextInputs {
            missions: Some(&book),
            readiness: Some(&readiness),
            fatigue: &fatigue,
            ..Default::default()
        });
        let m = ctx.missions.as_ref().unwrap();
        assert_eq!(m.missions[0].scenario, "flick-wide");
        assert!(m.missions[0].done);
        assert_eq!(ctx.readiness.as_ref().unwrap().warmth, "cold");
        assert_eq!(ctx.fatigue.len(), 1);
        assert!(ctx.fatigue[0].contains("75"), "{:?}", ctx.fatigue);
    }

    #[test]
    fn prompt_json_is_bounded_however_large_the_history() {
        // Long explanations force the halving step past the default limits.
        let findings = (0..60)
            .map(|i| Finding {
                explanation: "x".repeat(1_500),
                ..finding(Metric::Corrections, 1.0 + i as f32, 1.0, Verdict::Weakness)
            })
            .collect();
        let p = profile(findings);
        let records: Vec<RunRecord> = (0..2_000)
            .map(|i| record(&format!("custom-drill-{i}"), i as f32, 2_000 - i))
            .collect();
        let ctx = CoachContext::build(&ContextInputs {
            profile: Some(&p),
            records: &records,
            now: NOW + 5_000 * DAY,
            ..Default::default()
        });
        assert_eq!(ctx.findings.len(), 60, "build keeps every finding");
        let json = ctx.to_prompt_json();
        assert!(json.len() <= PROMPT_JSON_MAX_BYTES, "{} bytes", json.len());
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let sent = value["findings"].as_array().unwrap().len();
        assert!((1..MAX_FINDINGS).contains(&sent), "{sent} findings sent");
        assert_eq!(
            value["findings"][0]["value"], 60.0,
            "the most severe finding survives the cut"
        );
    }

    #[test]
    fn an_empty_history_gives_an_empty_but_valid_context() {
        let ctx = CoachContext::build(&ContextInputs::default());
        assert_eq!(ctx, CoachContext::default());
        let value: serde_json::Value = serde_json::from_str(&ctx.to_prompt_json()).unwrap();
        assert!(value["findings"].as_array().unwrap().is_empty());
    }
}
