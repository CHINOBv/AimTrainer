//! Short sessions for ranked days: a warm-up that fits a matchmaking queue, and a
//! buy-phase micro drill that keeps the hands warm between rounds.
//!
//! Warm-up order follows `docs/training-science.md`: smooth tracking first, then large
//! targets and flicks, then precision, switching and movement, with easier levels first
//! (warm-up decrement after rest, Lee & Genovese 1988). [`warmup_dose`] sizes the queue
//! warm-up from the player's own sessions instead of a fixed length.

use super::progress::RunRecord;
use super::routine::{Block, RoutineDef};
use std::collections::HashMap;

/// Id of the generated queue warm-up routine.
pub const QUEUE_WARMUP_ID: &str = "queue-warmup";
/// Length of each warm-up run, seconds.
pub const WARMUP_RUN_SECS: f32 = 30.0;
/// Length of a buy-phase micro drill, seconds (a buy phase lasts 30 s in VALORANT's
/// standard modes; leave time to buy).
pub const MICRO_DRILL_SECS: f32 = 20.0;
/// Most runs in a queue warm-up: [`DOSE_MAX_MINUTES`] of [`WARMUP_RUN_SECS`] runs, so an
/// explicit length or a personal dose up to 15 min is honoured.
pub const MAX_WARMUP_RUNS: usize = (DOSE_MAX_MINUTES * 60.0 / WARMUP_RUN_SECS) as usize;

/// `(scenario, difficulty)` in warm-up order: smooth → flicks → precision → switch → move.
const WARMUP_ORDER: [(&str, f32); 6] = [
    ("track-precise", -0.5),
    ("flick-wide", -0.25),
    ("micro-adjust", -0.25),
    ("flick-headshot", 0.0),
    ("switch-speed", 0.0),
    ("counter-strafe", 0.0),
];

/// Scenarios that suit a 20 s drill: quick to read, no movement.
const MICRO_DRILLS: [&str; 4] = [
    "micro-adjust",
    "flick-headshot",
    "static-headline",
    "track-reactive-micro",
];

/// A warm-up of about `minutes` (at least 1 run, at most [`MAX_WARMUP_RUNS`]: the first
/// pass through the warm-up order, then its later steps repeat). Runs are 30 s and kept in
/// order (no interleaving).
pub fn queue_warmup(minutes: f32) -> RoutineDef {
    let runs = ((minutes * 60.0 / WARMUP_RUN_SECS).floor() as usize).clamp(1, MAX_WARMUP_RUNS);
    let mut blocks: Vec<Block> = Vec::new();
    for i in 0..runs {
        // First pass through the order, then repeat the later (harder) steps.
        let (scenario, difficulty) = if i < WARMUP_ORDER.len() {
            WARMUP_ORDER[i]
        } else {
            WARMUP_ORDER[2 + (i - WARMUP_ORDER.len()) % (WARMUP_ORDER.len() - 2)]
        };
        blocks.push(Block {
            arena: None,
            scenario: scenario.to_owned(),
            runs: Some(1),
            minutes: None,
            note: "Warm-up: stay relaxed, build speed gradually.".to_owned(),
            difficulty,
            duration_secs: Some(WARMUP_RUN_SECS),
        });
    }
    RoutineDef {
        id: QUEUE_WARMUP_ID.to_owned(),
        name: format!("Queue warm-up ({minutes:.0} min)"),
        description: "Fits your matchmaking queue: smooth tracking first, then flicks, precision, switching and movement.".to_owned(),
        interleave_from: None,
        blocks,
    }
}

/// The `n`-th buy-phase micro drill (rotates so consecutive rounds train different things).
pub fn micro_drill(n: usize) -> (&'static str, f32) {
    (MICRO_DRILLS[n % MICRO_DRILLS.len()], MICRO_DRILL_SECS)
}

// ---------------------------------------------------------------- personal dose

/// Shortest personal warm-up, minutes.
pub const DOSE_MIN_MINUTES: f32 = 2.0;
/// Longest personal warm-up, minutes.
pub const DOSE_MAX_MINUTES: f32 = 15.0;
/// Scores within this share of the session's best rolling mean count as plateaued (estimate).
pub const PLATEAU_TOLERANCE: f32 = 0.05;
/// Runs averaged by the rolling mean (estimate).
pub const ROLLING_RUNS: usize = 3;
/// Fewer runs than this in a session say nothing about its plateau (estimate).
pub const MIN_SESSION_RUNS: usize = 4;
/// Fewer usable sessions than this give no personal dose (estimate).
pub const MIN_DOSE_SESSIONS: usize = 3;

/// One run of a past session, for estimating the warm-up dose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WarmupSample {
    /// Minutes from the start of the session to the end of the run.
    pub minute: f32,
    /// The run's score divided by the scenario's personal best.
    pub score_ratio: f32,
}

/// How long this player takes to warm up, from their own sessions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WarmupDose {
    /// Median plateau minute across sessions, clamped to
    /// [`DOSE_MIN_MINUTES`]..=[`DOSE_MAX_MINUTES`].
    pub minutes: f32,
    /// Sessions the estimate is based on.
    pub sessions: usize,
}

/// The runs of one session as warm-up samples: minutes since the session started (the first
/// run's start) and the score over the scenario's personal best in `bests`. Runs without a
/// positive personal best are skipped.
pub fn session_samples(session: &[RunRecord], bests: &HashMap<String, f32>) -> Vec<WarmupSample> {
    let Some(first) = session.first() else {
        return Vec::new();
    };
    let start = first.timestamp as f64 - f64::from(first.duration_secs);
    session
        .iter()
        .filter_map(|r| {
            let best = *bests.get(&r.scenario)?;
            (best > 0.0).then(|| WarmupSample {
                minute: ((r.timestamp as f64 - start) / 60.0) as f32,
                score_ratio: r.score / best,
            })
        })
        .collect()
}

/// The minute after which the rolling mean of one session stays within
/// [`PLATEAU_TOLERANCE`] of its best, or `None` with fewer than [`MIN_SESSION_RUNS`] runs.
fn plateau_minute(samples: &[WarmupSample]) -> Option<f32> {
    let mut s: Vec<WarmupSample> = samples
        .iter()
        .copied()
        .filter(|x| x.minute.is_finite() && x.score_ratio.is_finite())
        .collect();
    if s.len() < MIN_SESSION_RUNS {
        return None;
    }
    s.sort_by(|a, b| a.minute.total_cmp(&b.minute));
    let rolling: Vec<f32> = s
        .windows(ROLLING_RUNS)
        .map(|w| w.iter().map(|x| x.score_ratio).sum::<f32>() / ROLLING_RUNS as f32)
        .collect();
    let best = rolling.iter().copied().fold(f32::MIN, f32::max);
    let floor = best * (1.0 - PLATEAU_TOLERANCE);
    // The first window from which every later window stays at the plateau.
    let mut first = rolling.len() - 1;
    while first > 0 && rolling[first - 1] >= floor {
        first -= 1;
    }
    Some(s[first + ROLLING_RUNS - 1].minute)
}

/// The player's warm-up dose: the median plateau minute of their past sessions. `None`
/// until [`MIN_DOSE_SESSIONS`] sessions each have [`MIN_SESSION_RUNS`] runs.
pub fn warmup_dose(sessions: &[Vec<WarmupSample>]) -> Option<WarmupDose> {
    let mut minutes: Vec<f32> = sessions.iter().filter_map(|s| plateau_minute(s)).collect();
    if minutes.len() < MIN_DOSE_SESSIONS {
        return None;
    }
    minutes.sort_by(f32::total_cmp);
    let n = minutes.len();
    let median = if n % 2 == 1 {
        minutes[n / 2]
    } else {
        (minutes[n / 2 - 1] + minutes[n / 2]) / 2.0
    };
    Some(WarmupDose {
        minutes: median.clamp(DOSE_MIN_MINUTES, DOSE_MAX_MINUTES),
        sessions: n,
    })
}

/// Queue warm-up length: the personal dose when there is one, else the player's setting.
/// Pass the result to [`queue_warmup`].
pub fn queue_warmup_minutes(dose: Option<&WarmupDose>, setting_minutes: f32) -> f32 {
    dose.map_or(setting_minutes, |d| d.minutes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::builtin;

    #[test]
    fn warmup_fits_the_queue_time_and_starts_easy() {
        let lib = builtin::all();
        for minutes in [1.0, 3.0, 5.0, 8.0] {
            let r = queue_warmup(minutes);
            assert!(r.validate(&lib).is_empty(), "{:?}", r.validate(&lib));
            let m = r.minutes(&lib);
            assert!(
                m <= minutes + 1e-3 && m >= minutes.min(6.0) - 0.5,
                "{minutes} → {m}"
            );
            let plan = r.plan(&lib, 0);
            assert_eq!(plan[0].scenario_id, "track-precise");
            assert!(plan[0].difficulty < 0.0);
            assert!(
                plan.iter()
                    .all(|p| p.duration_secs == Some(WARMUP_RUN_SECS))
            );
        }
    }

    #[test]
    fn explicit_lengths_up_to_fifteen_minutes_are_honoured() {
        let lib = builtin::all();
        for minutes in [10.0, 12.5, 15.0] {
            let r = queue_warmup(minutes);
            assert!(r.validate(&lib).is_empty(), "{:?}", r.validate(&lib));
            assert_eq!(r.blocks.len(), (minutes * 2.0) as usize, "{minutes}");
            assert!((r.minutes(&lib) - minutes).abs() < 1e-3, "{minutes}");
        }
        assert_eq!(queue_warmup(DOSE_MAX_MINUTES).blocks.len(), MAX_WARMUP_RUNS);
        assert_eq!(queue_warmup(30.0).blocks.len(), MAX_WARMUP_RUNS, "capped");
    }

    #[test]
    fn warmups_of_six_minutes_or_less_are_unchanged() {
        // The order before the cap was raised: the six steps, then the later four repeat.
        let expected = [
            "track-precise",
            "flick-wide",
            "micro-adjust",
            "flick-headshot",
            "switch-speed",
            "counter-strafe",
            "micro-adjust",
            "flick-headshot",
            "switch-speed",
            "counter-strafe",
            "micro-adjust",
            "flick-headshot",
        ];
        for half_minutes in 0..=12 {
            let minutes = half_minutes as f32 / 2.0;
            let r = queue_warmup(minutes);
            let runs = half_minutes.max(1);
            let ids: Vec<&str> = r.blocks.iter().map(|b| b.scenario.as_str()).collect();
            assert_eq!(ids, expected[..runs], "{minutes} min");
        }
    }

    #[test]
    fn micro_drills_rotate_and_exist() {
        let lib = builtin::all();
        let ids: Vec<_> = (0..8).map(|n| micro_drill(n).0).collect();
        assert_ne!(ids[0], ids[1]);
        assert_eq!(ids[0], ids[4]);
        for id in ids {
            assert!(lib.iter().any(|d| d.id == id), "{id}");
        }
        assert!(micro_drill(0).1 < 30.0);
    }

    /// One sample per minute from minute 1 to 12, rising 5% a minute to a plateau at 1.0
    /// reached at `plateau`; the 3-run rolling mean is within 5% of it from `plateau` on.
    fn rising_session(plateau: f32) -> Vec<WarmupSample> {
        (1..=12)
            .map(|m| WarmupSample {
                minute: m as f32,
                score_ratio: (1.0 - 0.05 * (plateau - m as f32)).min(1.0),
            })
            .collect()
    }

    #[test]
    fn the_dose_is_the_median_plateau_minute_of_past_sessions() {
        let dose = warmup_dose(&[
            rising_session(4.0),
            rising_session(9.0),
            rising_session(6.0),
        ])
        .expect("three usable sessions");
        assert_eq!(dose.minutes, 6.0);
        assert_eq!(dose.sessions, 3);
        let even = warmup_dose(&[
            rising_session(4.0),
            rising_session(5.0),
            rising_session(7.0),
            rising_session(9.0),
        ])
        .unwrap();
        assert_eq!(even.minutes, 6.0, "mean of the two middle sessions");
    }

    #[test]
    fn a_later_dip_moves_the_plateau_past_it() {
        let mut s = rising_session(4.0);
        s[8].score_ratio = 0.5; // minute 9 dips; the rolling mean recovers by minute 12
        let late = plateau_minute(&s).unwrap();
        assert_eq!(late, 12.0, "{s:?}");
        // Order of the samples does not matter.
        let mut shuffled = rising_session(6.0);
        shuffled.reverse();
        assert_eq!(plateau_minute(&shuffled), Some(6.0));
    }

    #[test]
    fn the_dose_is_clamped_and_needs_enough_data() {
        // 30 s runs already at the plateau: the first rolling window ends at minute 1.5.
        let flat = || -> Vec<WarmupSample> {
            (1..=8)
                .map(|i| WarmupSample {
                    minute: i as f32 * 0.5,
                    score_ratio: 1.0,
                })
                .collect()
        };
        assert_eq!(plateau_minute(&flat()), Some(1.5));
        let quick = warmup_dose(&[flat(), flat(), flat()]).unwrap();
        assert_eq!(quick.minutes, DOSE_MIN_MINUTES);
        let slow: Vec<_> = (0..3)
            .map(|_| {
                (1..=30)
                    .map(|m| WarmupSample {
                        minute: m as f32,
                        score_ratio: m as f32 / 30.0,
                    })
                    .collect()
            })
            .collect();
        assert_eq!(warmup_dose(&slow).unwrap().minutes, DOSE_MAX_MINUTES);
        assert_eq!(
            warmup_dose(&[rising_session(5.0), rising_session(5.0)]),
            None
        );
        let short = rising_session(5.0)[..MIN_SESSION_RUNS - 1].to_vec();
        assert_eq!(
            warmup_dose(&[rising_session(5.0), rising_session(5.0), short]),
            None,
            "a session with too few runs does not count"
        );
    }

    #[test]
    fn the_dose_overrides_the_setting_only_when_known() {
        let dose = WarmupDose {
            minutes: 8.0,
            sessions: 4,
        };
        assert_eq!(queue_warmup_minutes(Some(&dose), 5.0), 8.0);
        assert_eq!(queue_warmup_minutes(None, 5.0), 5.0);
    }

    #[test]
    fn session_samples_use_minutes_from_the_start_and_the_personal_best() {
        let run = |timestamp, scenario: &str, score| RunRecord {
            timestamp,
            scenario: scenario.into(),
            score,
            duration_secs: 60.0,
        };
        let session = [
            run(1_060, "a", 50.0),
            run(1_180, "b", 30.0),
            run(1_300, "a", 100.0),
            run(1_400, "unknown", 10.0),
        ];
        let bests = HashMap::from([("a".to_owned(), 100.0), ("b".to_owned(), 60.0)]);
        let s = session_samples(&session, &bests);
        assert_eq!(
            s,
            vec![
                WarmupSample {
                    minute: 1.0,
                    score_ratio: 0.5
                },
                WarmupSample {
                    minute: 3.0,
                    score_ratio: 0.5
                },
                WarmupSample {
                    minute: 5.0,
                    score_ratio: 1.0
                },
            ]
        );
        assert!(session_samples(&[], &bests).is_empty());
    }
}
