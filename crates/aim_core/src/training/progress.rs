//! Progress signals from run history: readiness before ranked, fatigue during a session, and
//! daily goals with streaks.
//!
//! These are heuristics built on the evidence in `docs/training-science.md` (warm-up
//! decrement after rest; daily gains peaking around 30–60 min, Listman et al. 2021). Their
//! thresholds are documented constants, shown to the player with the numbers behind them.

use super::benchmark::{BenchmarkDef, BenchmarkResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const SECS_PER_DAY: u64 = 86_400;

/// One finished run, as the history stores it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    /// Unix seconds when the run finished.
    pub timestamp: u64,
    /// Scenario id (run key) that was played.
    pub scenario: String,
    /// Score achieved in the run.
    pub score: f32,
    /// Run length in seconds.
    pub duration_secs: f32,
}

// ---------------------------------------------------------------- readiness

/// Today's best at or above this share of the baseline counts as warmed up.
pub const READY_RATIO: f32 = 0.95;
/// Below this share the player is still cold.
pub const COLD_RATIO: f32 = 0.85;
/// Days of history used for the baseline.
pub const BASELINE_DAYS: u64 = 30;

/// How ready the player is, relative to their recent baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Warmth {
    /// Clearly below the baseline.
    Cold,
    /// Approaching the baseline.
    Warming,
    /// At or above the ready threshold of the baseline.
    Ready,
}

/// Warm-up readiness of one scenario: today's best against the recent baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct Readiness {
    /// Scenario id the readiness refers to.
    pub scenario: String,
    /// Median of the daily best scores over the baseline window, excluding today.
    pub baseline: f32,
    /// Best score today.
    pub today_best: f32,
    /// `today_best / baseline`.
    pub ratio: f32,
    /// Warmth level derived from `ratio`.
    pub warmth: Warmth,
}

fn day_of(ts: u64, utc_offset_secs: i64) -> i64 {
    (ts as i64 + utc_offset_secs).div_euclid(SECS_PER_DAY as i64)
}

fn median(mut v: Vec<f32>) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f32::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    })
}

/// Compare today's best on `scenario` with the median of previous daily bests.
/// `None` until there is a baseline (at least 3 previous days) and a run today.
pub fn readiness(
    records: &[RunRecord],
    scenario: &str,
    now: u64,
    utc_offset_secs: i64,
) -> Option<Readiness> {
    let today = day_of(now, utc_offset_secs);
    let since = today - BASELINE_DAYS as i64;
    let mut daily_best = std::collections::BTreeMap::<i64, f32>::new();
    let mut today_best: Option<f32> = None;
    for r in records.iter().filter(|r| r.scenario == scenario) {
        let d = day_of(r.timestamp, utc_offset_secs);
        if d == today {
            today_best = Some(today_best.map_or(r.score, |b| b.max(r.score)));
        } else if d >= since && d < today {
            let e = daily_best.entry(d).or_insert(r.score);
            *e = e.max(r.score);
        }
    }
    if daily_best.len() < 3 {
        return None;
    }
    let baseline = median(daily_best.into_values().collect())?;
    let today_best = today_best?;
    let ratio = if baseline > 0.0 {
        today_best / baseline
    } else {
        1.0
    };
    let warmth = if ratio >= READY_RATIO {
        Warmth::Ready
    } else if ratio >= COLD_RATIO {
        Warmth::Warming
    } else {
        Warmth::Cold
    };
    Some(Readiness {
        scenario: scenario.to_owned(),
        baseline,
        today_best,
        ratio,
        warmth,
    })
}

// ---------------------------------------------------------------- fatigue

/// Runs closer together than this belong to the same session.
pub const SESSION_GAP_SECS: u64 = 20 * 60;
/// Past this much training in one session, extra time pays off less (Listman et al. 2021).
pub const LONG_SESSION_MINUTES: f32 = 60.0;
/// A drop of this share from the session's best scores on a scenario signals fatigue.
pub const FATIGUE_DROP: f32 = 0.10;

/// A sign that the current session has passed the point of diminishing returns.
#[derive(Debug, Clone, PartialEq)]
pub enum FatigueSignal {
    /// Session time is past the point of diminishing returns.
    LongSession {
        /// Session length in minutes.
        minutes: f32,
    },
    /// The last runs of a scenario fell clearly below its earlier peak this session.
    ScoreDrop {
        /// Scenario id whose scores dropped.
        scenario: String,
        /// Best score of the scenario earlier this session.
        peak: f32,
        /// Mean of the most recent scores of the scenario.
        recent: f32,
        /// Relative drop from `peak` to `recent` (0.10 = 10%).
        drop: f32,
    },
}

/// The current session: the latest runs with no gap longer than [`SESSION_GAP_SECS`].
pub fn current_session(records: &[RunRecord]) -> &[RunRecord] {
    let mut start = records.len();
    while start > 0 {
        let prev = start - 1;
        if start < records.len()
            && records[start]
                .timestamp
                .saturating_sub(records[prev].timestamp)
                > SESSION_GAP_SECS
        {
            break;
        }
        start = prev;
    }
    &records[start..]
}

/// Signals that it may be time to stop (most important first).
pub fn fatigue(session: &[RunRecord]) -> Vec<FatigueSignal> {
    let mut out = Vec::new();
    let minutes = session.iter().map(|r| r.duration_secs).sum::<f32>() / 60.0;
    if minutes >= LONG_SESSION_MINUTES {
        out.push(FatigueSignal::LongSession { minutes });
    }
    let mut scenarios: Vec<&str> = session.iter().map(|r| r.scenario.as_str()).collect();
    scenarios.dedup();
    scenarios.sort_unstable();
    scenarios.dedup();
    for s in scenarios {
        let scores: Vec<f32> = session
            .iter()
            .filter(|r| r.scenario == s)
            .map(|r| r.score)
            .collect();
        if scores.len() < 5 {
            continue;
        }
        // Peak of 2-run averages before the last two runs vs the average of the last two.
        let (earlier, last) = scores.split_at(scores.len() - 2);
        let peak = earlier
            .windows(2)
            .map(|w| (w[0] + w[1]) / 2.0)
            .fold(f32::MIN, f32::max);
        let recent = (last[0] + last[1]) / 2.0;
        if peak > 0.0 && recent < peak * (1.0 - FATIGUE_DROP) {
            out.push(FatigueSignal::ScoreDrop {
                scenario: s.to_owned(),
                peak,
                recent,
                drop: 1.0 - recent / peak,
            });
        }
    }
    out
}

// ---------------------------------------------------------------- goals

/// Progress towards the daily training-time goal.
#[derive(Debug, Clone, PartialEq)]
pub struct DailyGoal {
    /// Minutes trained today.
    pub minutes_today: f32,
    /// Target minutes per day.
    pub goal_minutes: f32,
    /// Consecutive days (ending today, or yesterday if today has no training yet) that met
    /// the goal.
    pub streak_days: u32,
}

impl DailyGoal {
    /// Whether today's training time reached the goal.
    pub fn met_today(&self) -> bool {
        self.minutes_today >= self.goal_minutes
    }
}

/// Minutes trained today and the streak of days that reached `goal_minutes`.
pub fn daily_goal(
    records: &[RunRecord],
    goal_minutes: f32,
    now: u64,
    utc_offset_secs: i64,
) -> DailyGoal {
    let mut per_day = std::collections::BTreeMap::<i64, f32>::new();
    for r in records {
        *per_day
            .entry(day_of(r.timestamp, utc_offset_secs))
            .or_default() += r.duration_secs / 60.0;
    }
    let today = day_of(now, utc_offset_secs);
    let minutes_today = per_day.get(&today).copied().unwrap_or(0.0);
    let met = |d: i64| per_day.get(&d).is_some_and(|m| *m >= goal_minutes);
    let mut day = if met(today) { today } else { today - 1 };
    let mut streak_days = 0;
    while met(day) {
        streak_days += 1;
        day -= 1;
    }
    DailyGoal {
        minutes_today,
        goal_minutes,
        streak_days,
    }
}

// ---------------------------------------------------------------- trend

/// Personal best per scenario (`scenario id -> best score`) over runs finished at or before
/// `cutoff` (Unix seconds).
pub fn bests_until(records: &[RunRecord], cutoff: u64) -> HashMap<String, f32> {
    let mut bests = HashMap::<String, f32>::new();
    for r in records.iter().filter(|r| r.timestamp <= cutoff) {
        let e = bests.entry(r.scenario.clone()).or_insert(r.score);
        *e = e.max(r.score);
    }
    bests
}

/// The benchmark evaluated at the end of each of the last `weeks` weeks (oldest first),
/// using every run up to that point.
fn weekly_results(
    benchmark: &BenchmarkDef,
    records: &[RunRecord],
    now: u64,
    weeks: u32,
) -> Vec<BenchmarkResult> {
    (0..weeks)
        .rev()
        .map(|w| {
            let cutoff = now.saturating_sub(w as u64 * 7 * SECS_PER_DAY);
            benchmark.evaluate(&bests_until(records, cutoff))
        })
        .collect()
}

/// Overall benchmark energy at the end of each of the last `weeks` weeks (oldest first),
/// using every run up to that point. `None` while not every subcategory was played.
pub fn benchmark_trend(
    benchmark: &BenchmarkDef,
    records: &[RunRecord],
    now: u64,
    weeks: u32,
) -> Vec<Option<f32>> {
    weekly_results(benchmark, records, now, weeks)
        .into_iter()
        .map(|r| r.overall)
        .collect()
}

/// One benchmark subcategory's energy over time.
#[derive(Debug, Clone, PartialEq)]
pub struct SubcategoryTrend {
    /// Display name of the subcategory.
    pub name: String,
    /// Skill key of the subcategory.
    pub skill: String,
    /// Energy at the end of each week (oldest first); `None` until it was played.
    pub energies: Vec<Option<f32>>,
}

/// Per-subcategory (per-skill) energy at the end of each of the last `weeks` weeks, with
/// the same weekly cutoffs as [`benchmark_trend`]. One entry per subcategory, in the
/// benchmark's order.
pub fn subcategory_trend(
    benchmark: &BenchmarkDef,
    records: &[RunRecord],
    now: u64,
    weeks: u32,
) -> Vec<SubcategoryTrend> {
    let weekly = weekly_results(benchmark, records, now, weeks);
    benchmark
        .subcategories
        .iter()
        .enumerate()
        .map(|(i, sub)| SubcategoryTrend {
            name: sub.name.clone(),
            skill: sub.skill.clone(),
            energies: weekly.iter().map(|r| r.subcategories[i].energy).collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = SECS_PER_DAY;
    const NOW: u64 = 1_800_000_000; // a fixed "now"

    fn rec(ts: u64, scenario: &str, score: f32) -> RunRecord {
        RunRecord {
            timestamp: ts,
            scenario: scenario.into(),
            score,
            duration_secs: 60.0,
        }
    }

    #[test]
    fn readiness_compares_today_with_the_median_of_daily_bests() {
        let mut r = vec![];
        for d in 1..=5 {
            r.push(rec(NOW - d * DAY, "x", 1000.0 + d as f32));
            r.push(rec(NOW - d * DAY + 60, "x", 900.0));
        }
        assert_eq!(readiness(&r, "x", NOW, 0), None, "no run today yet");
        r.push(rec(NOW, "x", 800.0));
        let cold = readiness(&r, "x", NOW, 0).unwrap();
        assert_eq!(cold.baseline, 1003.0);
        assert_eq!(cold.warmth, Warmth::Cold);
        r.push(rec(NOW + 60, "x", 900.0));
        assert_eq!(
            readiness(&r, "x", NOW + 60, 0).unwrap().warmth,
            Warmth::Warming
        );
        r.push(rec(NOW + 120, "x", 990.0));
        assert_eq!(
            readiness(&r, "x", NOW + 120, 0).unwrap().warmth,
            Warmth::Ready
        );
    }

    #[test]
    fn readiness_needs_three_days_of_baseline() {
        let r = vec![
            rec(NOW - DAY, "x", 1.0),
            rec(NOW - 2 * DAY, "x", 1.0),
            rec(NOW, "x", 1.0),
        ];
        assert_eq!(readiness(&r, "x", NOW, 0), None);
    }

    #[test]
    fn sessions_split_on_long_gaps() {
        let r = vec![
            rec(0, "a", 1.0),
            rec(100, "a", 1.0),
            rec(100 + SESSION_GAP_SECS + 1, "a", 1.0),
            rec(100 + SESSION_GAP_SECS + 61, "a", 1.0),
        ];
        assert_eq!(current_session(&r).len(), 2);
        assert_eq!(current_session(&r[..2]).len(), 2);
        assert!(current_session(&[]).is_empty());
    }

    #[test]
    fn fatigue_flags_long_sessions_and_clear_score_drops_only() {
        let steady: Vec<_> = (0..6).map(|i| rec(i * 60, "a", 1000.0)).collect();
        assert!(fatigue(&steady).is_empty());
        let mut dropping = steady.clone();
        dropping.push(rec(400, "a", 850.0));
        dropping.push(rec(460, "a", 860.0));
        let f = fatigue(&dropping);
        assert!(
            matches!(&f[..], [FatigueSignal::ScoreDrop { scenario, .. }] if scenario == "a"),
            "{f:?}"
        );
        let long: Vec<_> = (0..61).map(|i| rec(i * 60, "b", 1.0)).collect();
        assert!(matches!(
            fatigue(&long)[0],
            FatigueSignal::LongSession { .. }
        ));
    }

    #[test]
    fn daily_goal_counts_today_and_the_streak() {
        let mut r = vec![];
        for d in 1..=3 {
            for i in 0..30 {
                r.push(rec(NOW - d * DAY + i * 60, "a", 1.0));
            }
        }
        let g = daily_goal(&r, 30.0, NOW, 0);
        assert_eq!(g.minutes_today, 0.0);
        assert_eq!(
            g.streak_days, 3,
            "today has no training yet, so the streak is kept"
        );
        for i in 0..30 {
            r.push(rec(NOW + i * 60, "a", 1.0));
        }
        let g = daily_goal(&r, 30.0, NOW + 1800, 0);
        assert!(g.met_today());
        assert_eq!(g.streak_days, 4);
        assert_eq!(daily_goal(&r, 31.0, NOW + 1800, 0).streak_days, 0);
    }

    #[test]
    fn benchmark_trend_rises_as_scores_improve() {
        let b = crate::training::benchmark::builtin::all().remove(0);
        let mut r = vec![];
        for (week, factor) in [(3u64, 0.5f32), (1, 1.0)] {
            for sub in &b.subcategories {
                let s = &sub.scenarios[0];
                // One hour after each weekly cutoff, so week 3's runs land after the oldest point.
                r.push(rec(
                    NOW - week * 7 * DAY + 3600,
                    &s.id,
                    s.thresholds[1] * factor,
                ));
            }
        }
        let trend = benchmark_trend(&b, &r, NOW, 4);
        assert_eq!(trend.len(), 4);
        assert_eq!(trend[0], None, "nothing was played by the oldest cutoff");
        let (early, late) = (trend[1].unwrap(), trend[3].unwrap());
        assert!(late > early, "{early} → {late}");
    }

    #[test]
    fn subcategory_trend_follows_each_subcategory_with_the_overall_cutoffs() {
        let b = crate::training::benchmark::builtin::all().remove(0);
        let first = &b.subcategories[0].scenarios[0];
        let second = &b.subcategories[1].scenarios[0];
        let r = vec![
            rec(NOW - 2 * 7 * DAY + 3600, &first.id, first.thresholds[0]),
            rec(NOW - 7 * DAY + 3600, &first.id, first.thresholds[1]),
            rec(NOW - 3600, &second.id, second.thresholds[2]),
        ];
        let trend = subcategory_trend(&b, &r, NOW, 3);
        assert_eq!(trend.len(), b.subcategories.len());
        assert_eq!(trend[0].name, b.subcategories[0].name);
        assert_eq!(trend[0].skill, b.subcategories[0].skill);
        assert_eq!(trend[0].energies, vec![None, Some(100.0), Some(200.0)]);
        assert_eq!(trend[1].energies, vec![None, None, Some(300.0)]);
        assert!(trend[2..].iter().all(|t| t.energies == vec![None; 3]));
        // The same cutoffs as the overall trend: it exists exactly when every subcategory does.
        let overall = benchmark_trend(&b, &r, NOW, 3);
        for (w, point) in overall.iter().enumerate() {
            assert_eq!(
                point.is_some(),
                trend.iter().all(|t| t.energies[w].is_some())
            );
        }
    }

    #[test]
    fn bests_until_keeps_the_best_score_up_to_the_cutoff() {
        let r = vec![rec(10, "a", 5.0), rec(20, "a", 9.0), rec(30, "a", 12.0)];
        let b = bests_until(&r, 20);
        assert_eq!(b.get("a"), Some(&9.0));
        assert!(bests_until(&r, 5).is_empty());
    }

    #[test]
    fn days_follow_the_local_offset() {
        // 23:30 UTC is the next day at UTC+1.
        assert_eq!(day_of(DAY - 1800, 3600), 1);
        assert_eq!(day_of(DAY - 1800, 0), 0);
    }
}
