//! Adaptive difficulty: a staircase that keeps a player's success rate near 80%.
//!
//! The challenge-point framework (Guadagnoli & Lee 2004) and the "Eighty Five Percent Rule"
//! (Wilson et al. 2019) both argue that learning is fastest when tasks are hard enough to
//! fail sometimes. After each run the level moves up when success is clearly above the
//! target, down when clearly below, and the step halves at every reversal (a classic
//! adaptive staircase), so it settles instead of oscillating.

use crate::scenario::difficulty::{MAX_LEVEL, MIN_LEVEL};
use serde::{Deserialize, Serialize};

/// Success rate the staircase steers to.
pub const TARGET_SUCCESS: f32 = 0.8;
/// No change while success is within this distance of the target.
pub const DEAD_BAND: f32 = 0.05;
/// Initial level change per run.
pub const INITIAL_STEP: f32 = 0.25;
/// Smallest level change; halving stops here.
pub const MIN_STEP: f32 = 0.05;

/// Adaptive difficulty state for one scenario, persisted between sessions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Staircase {
    /// Current difficulty level (see `scenario::difficulty`).
    pub level: f32,
    /// Size of the next level change, in difficulty levels.
    pub step: f32,
    /// -1 easier, +1 harder, 0 none yet.
    pub last_move: i8,
    /// Number of runs the staircase has seen.
    pub runs: u32,
}

impl Default for Staircase {
    fn default() -> Self {
        Self {
            level: 0.0,
            step: INITIAL_STEP,
            last_move: 0,
            runs: 0,
        }
    }
}

impl Staircase {
    /// Feed one run's success rate (0..1); returns the level for the next run.
    pub fn update(&mut self, success: f32) -> f32 {
        self.runs += 1;
        let movement: i8 = if success > TARGET_SUCCESS + DEAD_BAND {
            1
        } else if success < TARGET_SUCCESS - DEAD_BAND {
            -1
        } else {
            0
        };
        if movement != 0 {
            if self.last_move != 0 && movement != self.last_move {
                self.step = (self.step * 0.5).max(MIN_STEP);
            }
            self.level = (self.level + self.step * movement as f32).clamp(MIN_LEVEL, MAX_LEVEL);
            self.last_move = movement;
        }
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::builtin;
    use crate::scenario::{FrameInput, Session};
    use crate::sim::{AimerProfile, SimAimer};

    #[test]
    fn moves_up_when_easy_down_when_hard_and_holds_in_the_band() {
        let mut s = Staircase::default();
        assert_eq!(s.update(0.95), 0.25);
        assert_eq!(s.update(0.95), 0.5);
        assert_eq!(s.update(0.5), 0.375, "reversal halves the step");
        assert_eq!(s.update(0.8), 0.375, "inside the dead band nothing changes");
        for _ in 0..50 {
            s.update(1.0);
        }
        assert_eq!(s.level, MAX_LEVEL);
        assert!(s.step >= MIN_STEP);
    }

    #[test]
    fn converges_near_the_target_against_a_smooth_skill_curve() {
        // Success falls as difficulty rises; 80% at level 0.3.
        let success = |level: f32| (0.8 - (level - 0.3) * 0.6).clamp(0.0, 1.0);
        let mut s = Staircase::default();
        for _ in 0..30 {
            let l = s.level;
            s.update(success(l));
        }
        assert!((s.level - 0.3).abs() <= 0.15, "{}", s.level);
    }

    #[test]
    fn a_simulated_player_gets_a_harder_level_on_an_easy_scenario() {
        let def = builtin::all()
            .into_iter()
            .find(|d| d.id == "flick-wide")
            .unwrap();
        let mut stair = Staircase::default();
        for seed in 0..4 {
            let mut session = Session::new(def.at_difficulty(stair.level), seed);
            let mut aimer = SimAimer::new(AimerProfile::PRESETS[3], seed);
            while !session.is_finished() {
                let input: FrameInput = aimer.input(&session, 1.0 / 120.0);
                session.update(input);
            }
            stair.update(session.success_rate());
        }
        assert!(
            stair.level > 0.0,
            "elite aimer should be pushed above level 0: {}",
            stair.level
        );
    }
}
