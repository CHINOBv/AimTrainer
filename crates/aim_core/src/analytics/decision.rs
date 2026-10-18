//! Go/no-go decisions: how often the player shot match targets, let them expire, shot
//! decoys or correctly waited them out. A pure function of the traced [`Sample`]s.

use super::stats::median;
use super::trace::Sample;

/// Decoys shot above this share of all decoys mark an impulsive run. An estimate, not
/// calibrated on player data yet.
pub const FALSE_ALARM_THRESHOLD: f32 = 0.2;
/// Match targets missed above this share of all match targets mark a hesitant run. An
/// estimate, not calibrated on player data yet.
pub const MISS_THRESHOLD: f32 = 0.2;

/// Go/no-go outcomes: per frame in a [`Sample`], or summed over a run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DecisionCounts {
    /// Match targets shot down ("matchHit").
    pub match_hits: u32,
    /// Match targets that expired unshot ("matchWait", a miss).
    pub match_waits: u32,
    /// Decoys shot ("nonmatchHit", a false alarm).
    pub nonmatch_hits: u32,
    /// Decoys left alone until they expired ("nonmatchWait", a correct wait).
    pub nonmatch_waits: u32,
}

impl DecisionCounts {
    /// Every decision made.
    pub fn total(&self) -> u32 {
        self.match_hits + self.match_waits + self.nonmatch_hits + self.nonmatch_waits
    }

    fn add(&mut self, other: &Self) {
        self.match_hits += other.match_hits;
        self.match_waits += other.match_waits;
        self.nonmatch_hits += other.nonmatch_hits;
        self.nonmatch_waits += other.nonmatch_waits;
    }
}

/// The go/no-go result of a run.
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionSummary {
    /// Outcome counts.
    pub counts: DecisionCounts,
    /// Correct decisions (match hits plus decoy waits) over all decisions; 0 without any.
    pub accuracy: f32,
    /// Decoys shot over all decoys, if any decoy was resolved.
    pub false_alarm_rate: Option<f32>,
    /// Match targets missed over all match targets, if any was resolved.
    pub miss_rate: Option<f32>,
    /// Median seconds from a match target appearing to the shot that killed it.
    pub median_match_reaction_secs: Option<f32>,
}

impl DecisionSummary {
    /// Whether decoys were shot more often than [`FALSE_ALARM_THRESHOLD`].
    pub fn is_impulsive(&self) -> bool {
        self.false_alarm_rate
            .is_some_and(|r| r > FALSE_ALARM_THRESHOLD)
    }

    /// Whether match targets were missed more often than [`MISS_THRESHOLD`].
    pub fn is_hesitant(&self) -> bool {
        self.miss_rate.is_some_and(|r| r > MISS_THRESHOLD)
    }
}

fn ratio(part: u32, whole: u32) -> Option<f32> {
    (whole > 0).then(|| part as f32 / whole as f32)
}

/// Summarize the go/no-go decisions of a run, or `None` when it had no decoys.
pub fn analyze(samples: &[Sample]) -> Option<DecisionSummary> {
    let has_decoys = samples.iter().any(|s| s.targets.iter().any(|t| t.decoy));
    if !has_decoys {
        return None;
    }
    let mut counts = DecisionCounts::default();
    let mut reactions = Vec::new();
    for s in samples {
        counts.add(&s.decision);
        if s.decision.match_hits > 0
            && let Some(t) = s.aimed_target()
        {
            reactions.push(s.time_secs - t.visible_at_secs);
        }
    }
    let c = counts;
    Some(DecisionSummary {
        counts: c,
        accuracy: ratio(c.match_hits + c.nonmatch_waits, c.total()).unwrap_or(0.0),
        false_alarm_rate: ratio(c.nonmatch_hits, c.nonmatch_hits + c.nonmatch_waits),
        miss_rate: ratio(c.match_waits, c.match_hits + c.match_waits),
        median_match_reaction_secs: median(&reactions),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::test_support::record;
    use crate::analytics::trace::trace;
    use crate::sim::AimerProfile;

    #[test]
    fn go_no_go_counts_match_the_session_and_rates_follow_from_them() {
        let rec = record("go-no-go", AimerProfile::PRESETS[0], 2);
        let stats = rec.replay(|_, _| {}).stats().clone();
        let d = analyze(&trace(&rec)).expect("go-no-go has decoys");
        let c = d.counts;
        assert_eq!(
            (
                c.match_hits,
                c.match_waits,
                c.nonmatch_hits,
                c.nonmatch_waits
            ),
            (
                stats.kills,
                stats.expired,
                stats.decoy_hits,
                stats.decoy_waits
            )
        );
        assert!(c.nonmatch_hits > 0 && c.nonmatch_waits > 0, "{c:?}");
        let total = c.total() as f32;
        assert!((d.accuracy - (c.match_hits + c.nonmatch_waits) as f32 / total).abs() < 1e-6);
        let decoys = (c.nonmatch_hits + c.nonmatch_waits) as f32;
        assert_eq!(d.false_alarm_rate, Some(c.nonmatch_hits as f32 / decoys));
        let rt = d.median_match_reaction_secs.expect("some match was shot");
        assert!((0.2..1.6).contains(&rt), "{rt}");
    }

    #[test]
    fn runs_without_decoys_have_no_decision_summary() {
        let rec = record("micro-adjust", AimerProfile::PRESETS[1], 2);
        assert_eq!(analyze(&trace(&rec)), None);
    }

    #[test]
    fn verdicts_compare_the_rates_with_their_thresholds() {
        let summary = |false_alarm_rate, miss_rate| DecisionSummary {
            counts: DecisionCounts::default(),
            accuracy: 0.0,
            false_alarm_rate,
            miss_rate,
            median_match_reaction_secs: None,
        };
        assert!(summary(Some(0.3), Some(0.1)).is_impulsive());
        assert!(!summary(Some(0.3), Some(0.1)).is_hesitant());
        assert!(summary(Some(0.2), Some(0.25)).is_hesitant());
        assert!(
            !summary(Some(0.2), None).is_impulsive(),
            "at the threshold is fine"
        );
        assert!(!summary(None, None).is_impulsive());
    }
}
