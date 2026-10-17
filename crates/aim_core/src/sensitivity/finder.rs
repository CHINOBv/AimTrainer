//! Guided sensitivity finder: a bracketing search in cm/360 driven by the player's own
//! scores. Each round plays one run at a faster and one at a slower sensitivity around the
//! current center; the better one becomes the new center and the bracket shrinks to its
//! square root (×1.5 → ×1.22 → ×1.11 → ×1.05). The order alternates every round to cancel
//! warm-up and fatigue effects.
//!
//! This follows the spirit of community "sensitivity approximation" methods. Single runs are
//! noisy, so the UI recommends confirming the result with a second pass; it is a starting
//! point, not a measurement.

/// Initial bracket: test center/1.5 and center×1.5 cm/360.
pub const START_RATIO: f64 = 1.5;
/// Stop when the bracket is this tight (±3%).
pub const STOP_RATIO: f64 = 1.03;
pub const MAX_ROUNDS: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// Lower cm/360 (faster sensitivity).
    Fast,
    /// Higher cm/360 (slower sensitivity).
    Slow,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SensFinder {
    pub center_cm: f64,
    pub ratio: f64,
    pub round: u32,
    fast_score: Option<f32>,
    slow_score: Option<f32>,
    /// `(fast cm, slow cm, fast score, slow score)` per finished round.
    pub rounds: Vec<(f64, f64, f32, f32)>,
}

impl SensFinder {
    pub fn new(start_cm: f64) -> Self {
        Self {
            center_cm: start_cm,
            ratio: START_RATIO,
            round: 0,
            fast_score: None,
            slow_score: None,
            rounds: Vec::new(),
        }
    }

    pub fn is_done(&self) -> bool {
        self.round >= MAX_ROUNDS || self.ratio < STOP_RATIO
    }

    pub fn cm_for(&self, slot: Slot) -> f64 {
        match slot {
            Slot::Fast => self.center_cm / self.ratio,
            Slot::Slow => self.center_cm * self.ratio,
        }
    }

    /// Which test to play next this round (`None` when finished).
    pub fn next_slot(&self) -> Option<Slot> {
        if self.is_done() {
            return None;
        }
        // Even rounds start fast, odd rounds start slow.
        let first = if self.round.is_multiple_of(2) {
            Slot::Fast
        } else {
            Slot::Slow
        };
        let second = if first == Slot::Fast {
            Slot::Slow
        } else {
            Slot::Fast
        };
        let played = |s| match s {
            Slot::Fast => self.fast_score.is_some(),
            Slot::Slow => self.slow_score.is_some(),
        };
        [first, second].into_iter().find(|s| !played(*s))
    }

    /// Record a finished test run; the round closes once both slots have a score.
    pub fn record(&mut self, slot: Slot, score: f32) {
        if self.is_done() {
            return;
        }
        match slot {
            Slot::Fast => self.fast_score = Some(score),
            Slot::Slow => self.slow_score = Some(score),
        }
        if let (Some(f), Some(s)) = (self.fast_score, self.slow_score) {
            let (fast_cm, slow_cm) = (self.cm_for(Slot::Fast), self.cm_for(Slot::Slow));
            self.rounds.push((fast_cm, slow_cm, f, s));
            // A tie keeps the center and still narrows the bracket.
            if f > s {
                self.center_cm = fast_cm;
            } else if s > f {
                self.center_cm = slow_cm;
            }
            self.ratio = self.ratio.sqrt();
            self.round += 1;
            self.fast_score = None;
            self.slow_score = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A player whose score peaks at `best` cm/360 (log-scale distance).
    fn score_at(cm: f64, best: f64) -> f32 {
        (1000.0 - 600.0 * (cm / best).ln().abs()) as f32
    }

    #[test]
    fn converges_near_the_players_best_sensitivity() {
        for (start, best) in [(40.0, 55.0), (40.0, 30.0), (60.0, 42.0)] {
            let mut f = SensFinder::new(start);
            while let Some(slot) = f.next_slot() {
                let cm = f.cm_for(slot);
                f.record(slot, score_at(cm, best));
            }
            assert!(f.is_done());
            let err = (f.center_cm / best).ln().abs();
            assert!(
                err < 0.2,
                "start {start}, best {best} → {:.1} cm",
                f.center_cm
            );
        }
    }

    #[test]
    fn alternates_order_and_closes_rounds_only_with_both_scores() {
        let mut f = SensFinder::new(40.0);
        assert_eq!(f.next_slot(), Some(Slot::Fast));
        f.record(Slot::Fast, 10.0);
        assert_eq!(f.round, 0, "half a round does not move the center");
        assert_eq!(f.next_slot(), Some(Slot::Slow));
        f.record(Slot::Slow, 20.0);
        assert_eq!(f.round, 1);
        assert!(
            (f.center_cm - 60.0).abs() < 1e-9,
            "the slower sensitivity won"
        );
        assert_eq!(f.next_slot(), Some(Slot::Slow), "odd rounds start slow");
        assert!(f.ratio < START_RATIO);
    }

    #[test]
    fn stops_after_the_round_limit() {
        let mut f = SensFinder::new(40.0);
        for _ in 0..20 {
            for slot in [Slot::Fast, Slot::Slow] {
                f.record(slot, 1.0);
            }
        }
        assert!(f.is_done());
        assert!(f.round <= MAX_ROUNDS);
        assert_eq!(f.next_slot(), None);
    }
}
