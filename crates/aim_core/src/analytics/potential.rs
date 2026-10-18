//! Potential score with one tip: the score a click run would have reached if its single
//! costliest weakness were fixed.
//!
//! Each weakness is turned into points with a conservative, explainable assumption:
//! seconds wasted are converted at the run's own points-per-second, and miss penalties
//! count directly. Only the largest gain is offered, so the player gets one thing to fix
//! instead of a list. See "Potential score" in `docs/training-science.md`.

use super::flick::{Flick, FlickKind};
use super::stats::median;

/// Fewer events (overshoots, misses or slow clicks) than this give no estimate for that fix.
pub const MIN_EVENTS: usize = 3;
/// Fewer clean flicks than this give no settle-time reference for overshoots.
pub const MIN_CLEAN_FLICKS: usize = 3;
/// A gain smaller than this share of the score is not worth a tip (provisional).
pub const MIN_GAIN_SHARE: f32 = 0.03;
/// Click delay the hesitation estimate assumes is achievable: the click-delay weakness
/// threshold of the rules table (provisional), seconds.
pub const HESITATION_REFERENCE_SECS: f32 = 0.20;
/// Share of misses the miss estimate assumes would have hit instead (provisional).
pub const MISS_FIX_SHARE: f32 = 0.5;

/// A weakness the potential score can price.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fix {
    /// The primary movement ended past the target and needed extra settle time.
    Overshoot,
    /// Shots that missed: their penalties and the retry time.
    Misses,
    /// Time on the target before clicking, beyond the reference delay.
    Hesitation,
}

impl Fix {
    /// Every fix, in the order ties are broken.
    pub const ALL: [Fix; 3] = [Fix::Overshoot, Fix::Misses, Fix::Hesitation];

    /// Lowercase name for "Fix …".
    pub fn label(self) -> &'static str {
        match self {
            Fix::Overshoot => "overshoot",
            Fix::Misses => "misses",
            Fix::Hesitation => "click hesitation",
        }
    }
}

/// What the run scored and how long it lasted, for converting time into points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunTotals {
    /// Final score.
    pub score: f32,
    /// Session time at the end of the run, seconds.
    pub elapsed_secs: f32,
    /// Points subtracted per missed shot (`scoring.miss_penalty`).
    pub miss_penalty: f32,
    /// The run ends on the clock (`end.kind = "time"`), so seconds saved would have bought
    /// more kills. Otherwise saved seconds are worth no points.
    pub time_limited: bool,
}

/// The estimated gain of one fix.
#[derive(Debug, Clone, PartialEq)]
pub struct Gain {
    /// The weakness fixed.
    pub fix: Fix,
    /// Overshoots, misses or slow clicks the estimate is built on.
    pub events: usize,
    /// Seconds the fix would have saved.
    pub saved_secs: f32,
    /// Miss penalties won back.
    pub penalty_points: f32,
    /// Total points gained: saved seconds at the run's points per second, plus penalties.
    pub points: f32,
    /// Reference time the fix is measured against: the clean flicks' median settle time,
    /// or [`HESITATION_REFERENCE_SECS`]. `None` for misses.
    pub reference_secs: Option<f32>,
}

impl Gain {
    /// The assumption behind the estimate, in plain English, starting with "if".
    pub fn assumption(&self) -> String {
        let ms = |secs: Option<f32>| format!("{:.0} ms", secs.unwrap_or(0.0) * 1000.0);
        match self.fix {
            Fix::Overshoot => format!(
                "if your {} overshoots settled like your clean flicks (median {})",
                self.events,
                ms(self.reference_secs)
            ),
            Fix::Misses => format!(
                "if half of your {} misses had hit the first time",
                self.events
            ),
            Fix::Hesitation => format!(
                "if your {} slow clicks came within {} of reaching the target",
                self.events,
                ms(self.reference_secs)
            ),
        }
    }
}

/// The single costliest weakness of a run and the score without it.
#[derive(Debug, Clone, PartialEq)]
pub struct PotentialTip {
    /// The run's final score.
    pub score: f32,
    /// The largest gain.
    pub gain: Gain,
}

impl PotentialTip {
    /// The estimated score with the weakness fixed.
    pub fn potential_score(&self) -> f32 {
        self.score + self.gain.points
    }

    /// The gain as a share of the score.
    pub fn gain_share(&self) -> f32 {
        self.gain.points / self.score
    }
}

/// Every fix with enough events to estimate, in [`Fix::ALL`] order.
pub fn gains(flicks: &[Flick], totals: RunTotals) -> Vec<Gain> {
    // Saved time is worth the run's average pace; a penalized score makes it conservative.
    // Only a run that ends on the clock turns saved seconds into more kills.
    let points_per_sec = if totals.time_limited && totals.score > 0.0 && totals.elapsed_secs > 0.0 {
        totals.score / totals.elapsed_secs
    } else {
        0.0
    };
    let gain = |fix, events: usize, saved_secs: f32, penalty_points: f32, reference_secs| {
        (events >= MIN_EVENTS).then_some(Gain {
            fix,
            events,
            saved_secs,
            penalty_points,
            points: saved_secs * points_per_sec + penalty_points,
            reference_secs,
        })
    };
    Fix::ALL
        .iter()
        .filter_map(|&fix| match fix {
            Fix::Overshoot => {
                let reference = clean_settle_secs(flicks)?;
                let settles: Vec<f32> = flicks
                    .iter()
                    .filter(|f| f.overshot())
                    .filter_map(|f| f.settle_secs)
                    .collect();
                let saved = settles.iter().map(|s| (s - reference).max(0.0)).sum();
                gain(fix, settles.len(), saved, 0.0, Some(reference))
            }
            Fix::Misses => {
                let misses = flicks.iter().filter(|f| !f.hit).count();
                // A correction flick runs from the miss to the retry shot.
                let retry_secs: f32 = flicks
                    .iter()
                    .filter(|f| f.kind == FlickKind::Correction)
                    .filter_map(|f| Some(f.reaction_secs? + f.movement_secs?))
                    .sum();
                gain(
                    fix,
                    misses,
                    MISS_FIX_SHARE * retry_secs,
                    MISS_FIX_SHARE * misses as f32 * totals.miss_penalty,
                    None,
                )
            }
            Fix::Hesitation => {
                let excess: Vec<f32> = flicks
                    .iter()
                    .filter_map(|f| f.click_delay_secs)
                    .map(|d| d - HESITATION_REFERENCE_SECS)
                    .filter(|&e| e > 0.0)
                    .collect();
                gain(
                    fix,
                    excess.len(),
                    excess.iter().sum(),
                    0.0,
                    Some(HESITATION_REFERENCE_SECS),
                )
            }
        })
        .collect()
}

/// Median settle time of clean flicks (landed within a radius and hit), once there are
/// enough of them to compare with.
fn clean_settle_secs(flicks: &[Flick]) -> Option<f32> {
    let settles: Vec<f32> = flicks
        .iter()
        .filter(|f| f.hit && f.primary_error_radii.is_some() && !f.overshot() && !f.undershot())
        .filter_map(|f| f.settle_secs)
        .collect();
    (settles.len() >= MIN_CLEAN_FLICKS)
        .then(|| median(&settles))
        .flatten()
}

/// The fix with the largest gain, or `None` when the run scored nothing or no gain reaches
/// [`MIN_GAIN_SHARE`] of the score. Ties go to the first fix in [`Fix::ALL`].
pub fn potential(flicks: &[Flick], totals: RunTotals) -> Option<PotentialTip> {
    if totals.score <= 0.0 {
        return None;
    }
    let best = gains(flicks, totals)
        .into_iter()
        .filter(|g| g.points >= MIN_GAIN_SHARE * totals.score)
        .fold(None, |best: Option<Gain>, g| match best {
            Some(b) if b.points >= g.points => Some(b),
            _ => Some(g),
        })?;
    Some(PotentialTip {
        score: totals.score,
        gain: best,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::test_support::record;
    use crate::analytics::{AnalysisKind, analyze};
    use crate::sim::AimerProfile;

    /// A fresh flick that landed on the target, settled in 100 ms and clicked in 100 ms.
    fn clean() -> Flick {
        Flick {
            kind: FlickKind::Fresh,
            target_id: 1,
            hit: true,
            distance_deg: 20.0,
            target_radius_deg: 1.0,
            reaction_secs: Some(0.2),
            movement_secs: Some(0.3),
            peak_speed_deg_per_sec: 200.0,
            primary_error_radii: Some(0.2),
            endpoint_error_deg: 0.0,
            effective_distance_deg: 20.0,
            corrections: 0,
            settle_secs: Some(0.1),
            path_efficiency: Some(0.95),
            click_delay_secs: Some(0.1),
            direction_deg: (20.0, 0.0),
            primary_offset_deg: Some((0.2, 0.0)),
        }
    }

    fn overshoot(settle_secs: f32) -> Flick {
        Flick {
            primary_error_radii: Some(2.0),
            corrections: 1,
            settle_secs: Some(settle_secs),
            ..clean()
        }
    }

    fn hesitant(click_delay_secs: f32) -> Flick {
        Flick {
            click_delay_secs: Some(click_delay_secs),
            ..clean()
        }
    }

    /// 100 points in 50 s: 2 points per second.
    const TOTALS: RunTotals = RunTotals {
        score: 100.0,
        elapsed_secs: 50.0,
        miss_penalty: 0.0,
        time_limited: true,
    };

    #[test]
    fn runs_that_do_not_end_on_time_price_no_saved_seconds() {
        let mut flicks = vec![clean(); 4];
        flicks.extend(vec![overshoot(0.6); 4]);
        flicks.extend(vec![
            Flick {
                hit: false,
                ..clean()
            };
            4
        ]);
        let totals = RunTotals {
            miss_penalty: 5.0,
            time_limited: false,
            ..TOTALS
        };
        let all = gains(&flicks, totals);
        assert!(!all.is_empty());
        for g in &all {
            assert!(g.saved_secs > 0.0 || g.fix == Fix::Misses, "{g:?}");
            assert!(close(g.points, g.penalty_points), "{g:?}");
        }
        let tip = potential(&flicks, totals).expect("half of 4 misses × 5 = 10% of 100");
        assert_eq!(tip.gain.fix, Fix::Misses);
        assert!(close(tip.gain.points, 10.0), "{tip:?}");
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn the_tip_is_the_fix_with_the_largest_gain() {
        // Four overshoots settle 0.5 s slower than the clean median (0.1 s): 2 s = 4 points.
        // Three clicks 0.2 s past the 0.2 s reference: 0.6 s = 1.2 points.
        let mut flicks = vec![clean(); 4];
        flicks.extend(vec![overshoot(0.6); 4]);
        flicks.extend(vec![hesitant(0.4); 3]);
        let tip = potential(&flicks, TOTALS).expect("4% is worth a tip");
        assert_eq!(tip.gain.fix, Fix::Overshoot);
        assert_eq!(tip.gain.events, 4);
        assert!(close(tip.gain.saved_secs, 2.0), "{tip:?}");
        assert!(close(tip.gain.points, 4.0), "{tip:?}");
        assert_eq!(tip.gain.reference_secs, Some(0.1));
        assert!(close(tip.potential_score(), 104.0));
        assert!(close(tip.gain_share(), 0.04));
        assert_eq!(
            tip.gain.assumption(),
            "if your 4 overshoots settled like your clean flicks (median 100 ms)"
        );
        let all = gains(&flicks, TOTALS);
        let fixes: Vec<Fix> = all.iter().map(|g| g.fix).collect();
        assert_eq!(fixes, [Fix::Overshoot, Fix::Hesitation]);
        assert!(close(all[1].points, 1.2), "{all:?}");

        // Slower clicks (1.0 s past the reference each) make hesitation the costliest: 6 points.
        let mut flicks = vec![clean(); 4];
        flicks.extend(vec![overshoot(0.6); 4]);
        flicks.extend(vec![hesitant(1.2); 3]);
        let tip = potential(&flicks, TOTALS).expect("hesitation tip");
        assert_eq!(tip.gain.fix, Fix::Hesitation);
        assert!(close(tip.gain.points, 6.0), "{tip:?}");
        assert_eq!(
            tip.gain.assumption(),
            "if your 3 slow clicks came within 200 ms of reaching the target"
        );
    }

    #[test]
    fn misses_win_back_half_their_penalties_and_retry_time() {
        // Four misses, each retried with a 0.5 s correction flick, at 5 points per miss.
        let miss = Flick {
            hit: false,
            ..clean()
        };
        let retry = Flick {
            kind: FlickKind::Correction,
            ..clean()
        };
        let mut flicks = vec![clean(); 4];
        flicks.extend(vec![miss; 4]);
        flicks.extend(vec![retry; 4]);
        let totals = RunTotals {
            miss_penalty: 5.0,
            ..TOTALS
        };
        let tip = potential(&flicks, totals).expect("misses tip");
        assert_eq!(tip.gain.fix, Fix::Misses);
        assert_eq!(tip.gain.events, 4);
        // Half of 4 × 5 points, plus half of 4 × 0.5 s at 2 points/s.
        assert!(close(tip.gain.penalty_points, 10.0), "{tip:?}");
        assert!(close(tip.gain.saved_secs, 1.0), "{tip:?}");
        assert!(close(tip.gain.points, 12.0), "{tip:?}");
        assert_eq!(tip.gain.reference_secs, None);
        assert_eq!(
            tip.gain.assumption(),
            "if half of your 4 misses had hit the first time"
        );
    }

    #[test]
    fn a_clean_run_or_a_small_gain_gives_no_tip() {
        assert_eq!(potential(&vec![clean(); 20], TOTALS), None);
        assert!(gains(&vec![clean(); 20], TOTALS).is_empty());
        // Two overshoots are too few to estimate.
        let mut flicks = vec![clean(); 4];
        flicks.extend(vec![overshoot(2.0); 2]);
        assert_eq!(potential(&flicks, TOTALS), None);
        // Three overshoots 0.2 s slow: 0.6 s = 1.2 points, under 3% of 100.
        let mut flicks = vec![clean(); 4];
        flicks.extend(vec![overshoot(0.3); 3]);
        assert_eq!(gains(&flicks, TOTALS).len(), 1);
        assert_eq!(potential(&flicks, TOTALS), None);
        // Overshoots without enough clean flicks to compare with.
        let mut flicks = vec![clean(); 2];
        flicks.extend(vec![overshoot(2.0); 4]);
        assert_eq!(potential(&flicks, TOTALS), None);
        // A run that scored nothing has nothing to scale.
        let mut flicks = vec![clean(); 4];
        flicks.extend(vec![overshoot(2.0); 4]);
        let zero = RunTotals {
            score: 0.0,
            ..TOTALS
        };
        assert_eq!(potential(&flicks, zero), None);
    }

    #[test]
    fn click_runs_carry_a_tip_built_from_their_own_score_and_hold_runs_do_not() {
        let rec = record("flick-wide", AimerProfile::PRESETS[0], 3);
        let a = analyze(&rec);
        let result = rec.replay(|_, _| {}).stats().clone();
        let AnalysisKind::Flick(flicks) = &a.kind else {
            panic!("click run");
        };
        let totals = RunTotals {
            score: result.score,
            elapsed_secs: rec.def.duration_secs,
            miss_penalty: rec.def.scoring.miss_penalty,
            time_limited: true,
        };
        let expected = potential(flicks, totals);
        assert!(close(
            a.potential.as_ref().map_or(0.0, |t| t.score),
            expected.as_ref().map_or(0.0, |t| t.score)
        ));
        assert_eq!(
            a.potential.as_ref().map(|t| t.gain.fix),
            expected.map(|t| t.gain.fix)
        );
        let rec = record("track-precise", AimerProfile::PRESETS[0], 3);
        assert_eq!(analyze(&rec).potential, None);
    }
}
