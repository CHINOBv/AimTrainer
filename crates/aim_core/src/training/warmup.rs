//! Short sessions for ranked days: a warm-up that fits a matchmaking queue, and a
//! buy-phase micro drill that keeps the hands warm between rounds.
//!
//! Warm-up order follows `docs/training-science.md`: smooth tracking first, then large
//! targets and flicks, then precision, switching and movement, with easier levels first
//! (warm-up decrement after rest, Lee & Genovese 1988).

use super::routine::{Block, RoutineDef};

pub const QUEUE_WARMUP_ID: &str = "queue-warmup";
/// Length of each warm-up run, seconds.
pub const WARMUP_RUN_SECS: f32 = 30.0;
/// Length of a buy-phase micro drill, seconds (a buy phase lasts 30 s in VALORANT's
/// standard modes; leave time to buy).
pub const MICRO_DRILL_SECS: f32 = 20.0;

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

/// A warm-up of about `minutes` (at least 1 run per step until the budget runs out, and
/// never more than the warm-up order). Runs are 30 s and kept in order (no interleaving).
pub fn queue_warmup(minutes: f32) -> RoutineDef {
    let runs =
        ((minutes * 60.0 / WARMUP_RUN_SECS).floor() as usize).clamp(1, WARMUP_ORDER.len() * 2);
    let mut blocks: Vec<Block> = Vec::new();
    for i in 0..runs {
        // First pass through the order, then repeat the later (harder) steps.
        let (scenario, difficulty) = if i < WARMUP_ORDER.len() {
            WARMUP_ORDER[i]
        } else {
            WARMUP_ORDER[2 + (i - WARMUP_ORDER.len()) % (WARMUP_ORDER.len() - 2)]
        };
        blocks.push(Block {
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
}
