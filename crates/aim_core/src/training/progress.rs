//! Progress signals from run history: readiness before ranked, fatigue during a session, and
//! daily goals with streaks.
//!
//! These are heuristics built on the evidence in `docs/training-science.md` (warm-up
//! decrement after rest; daily gains peaking around 30–60 min, Listman et al. 2021). Their
//! thresholds are documented constants, shown to the player with the numbers behind them.

use serde::{Deserialize, Serialize};

const SECS_PER_DAY: u64 = 86_400;

/// One finished run, as the history stores it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    /// Unix seconds when the run finished.
    pub timestamp: u64,
    pub scenario: String,
    pub score: f32,
    pub duration_secs: f32,
}

// ---------------------------------------------------------------- readiness

/// Today's best at or above this share of the baseline counts as warmed up.
pub const READY_RATIO: f32 = 0.95;
/// Below this share the player is still cold.
pub const COLD_RATIO: f32 = 0.85;
/// Days of history used for the baseline.
pub const BASELINE_DAYS: u64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Warmth {
    Cold,
    Warming,
    Ready,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Readiness {
    pub scenario: String,
    /// Median of the daily best scores over the baseline window, excluding today.
    pub baseline: f32,
    /// Best score today.
    pub today_best: f32,
    pub ratio: f32,
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

#[derive(Debug, Clone, PartialEq)]
pub enum FatigueSignal {
    /// Session time is past the point of diminishing returns.
    LongSession { minutes: f32 },
    /// The last runs of a scenario fell clearly below its earlier peak this session.
    ScoreDrop {
        scenario: String,
        peak: f32,
        recent: f32,
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

#[derive(Debug, Clone, PartialEq)]
pub struct DailyGoal {
    pub minutes_today: f32,
    pub goal_minutes: f32,
    /// Consecutive days (ending today, or yesterday if today has no training yet) that met
    /// the goal.
    pub streak_days: u32,
}

impl DailyGoal {
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
    fn days_follow_the_local_offset() {
        // 23:30 UTC is the next day at UTC+1.
        assert_eq!(day_of(DAY - 1800, 3600), 1);
        assert_eq!(day_of(DAY - 1800, 0), 0);
    }
}
