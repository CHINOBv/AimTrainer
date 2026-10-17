//! Explainable rules that turn match findings into built-in drills.
//!
//! Every threshold is provisional: chosen by judgment, not fitted to player data
//! (docs/match-analysis.md). Each rule also needs a minimum sample so one unlucky
//! round does not produce advice.

use crate::analysis::Analysis;

/// A finding that suggests drills.
#[derive(Debug, Clone, PartialEq)]
pub struct Insight {
    /// Stable rule id, e.g. `low-headshot-rate`.
    pub rule: &'static str,
    /// What was measured.
    pub metric: &'static str,
    /// The measured value (a share in 0..=1).
    pub value: f64,
    /// The provisional threshold the value crossed.
    pub threshold: f64,
    /// Why the drills help, in plain words.
    pub reason: String,
    /// Built-in scenario ids to train.
    pub drills: &'static [&'static str],
}

/// Headshot share of hits below which head-level aim is the first thing to train.
pub const LOW_HEADSHOT_RATE: f64 = 0.20;
/// Minimum hits before the headshot rule applies.
pub const MIN_HITS: u32 = 10;
/// Share of known-distance deaths above which a range is a pattern.
pub const RANGE_DEATH_SHARE: f64 = 0.50;
/// Minimum deaths with a known distance before the range rules apply.
pub const MIN_DEATHS: u32 = 3;
/// Share of opening duels lost above which the first contact is the problem.
pub const OPENING_LOST_SHARE: f64 = 0.60;
/// Minimum opening duels before the opening rule applies.
pub const MIN_OPENING_DUELS: u32 = 3;

/// Runs every rule over one player's analysis, in a fixed order.
pub fn recommend(analysis: &Analysis) -> Vec<Insight> {
    [
        low_headshot_rate(analysis),
        long_range_deaths(analysis),
        opening_duels_lost(analysis),
        close_range_deaths(analysis),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn low_headshot_rate(a: &Analysis) -> Option<Insight> {
    let rate = a.headshot_rate()?;
    (a.hits() >= MIN_HITS && rate < LOW_HEADSHOT_RATE).then(|| Insight {
        rule: "low-headshot-rate",
        metric: "headshots / hits",
        value: rate,
        threshold: LOW_HEADSHOT_RATE,
        reason: format!(
            "Only {:.0}% of your {} hits were headshots: train holding the crosshair at head height and flicking to heads.",
            rate * 100.0,
            a.hits()
        ),
        drills: &["static-headline", "flick-headshot"],
    })
}

fn death_share(a: &Analysis, count: u32) -> Option<f64> {
    let known = a.deaths_by_range.known();
    (known >= MIN_DEATHS).then(|| f64::from(count) / f64::from(known))
}

fn long_range_deaths(a: &Analysis) -> Option<Insight> {
    let share = death_share(a, a.deaths_by_range.long)?;
    (share > RANGE_DEATH_SHARE).then(|| Insight {
        rule: "long-range-deaths",
        metric: "long-range deaths / deaths with known distance",
        value: share,
        threshold: RANGE_DEATH_SHARE,
        reason: format!(
            "{:.0}% of your deaths were over 25 m: train small, precise head targets at distance.",
            share * 100.0
        ),
        drills: &["long-range-heads"],
    })
}

fn opening_duels_lost(a: &Analysis) -> Option<Insight> {
    let duels = a.opening_duels.total();
    if duels < MIN_OPENING_DUELS {
        return None;
    }
    let share = f64::from(a.opening_duels.lost) / f64::from(duels);
    (share > OPENING_LOST_SHARE).then(|| Insight {
        rule: "opening-duels-lost",
        metric: "opening duels lost / opening duels",
        value: share,
        threshold: OPENING_LOST_SHARE,
        reason: format!(
            "You lost {} of {} opening duels: train reacting to peeks and stopping before you shoot.",
            a.opening_duels.lost, duels
        ),
        drills: &["peek-reaction", "counter-strafe"],
    })
}

fn close_range_deaths(a: &Analysis) -> Option<Insight> {
    let share = death_share(a, a.deaths_by_range.close)?;
    (share > RANGE_DEATH_SHARE).then(|| Insight {
        rule: "close-range-deaths",
        metric: "close-range deaths / deaths with known distance",
        value: share,
        threshold: RANGE_DEATH_SHARE,
        reason: format!(
            "{:.0}% of your deaths were under 10 m: train tracking and hitting heads on fast strafing targets.",
            share * 100.0
        ),
        drills: &["track-reactive-strafe", "dynamic-strafe-heads"],
    })
}
