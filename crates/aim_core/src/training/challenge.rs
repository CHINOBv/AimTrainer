//! Daily challenge: one scenario and one seed per calendar day, the same for every player,
//! so scores are directly comparable (same target positions and timings).

use crate::scenario::ScenarioDef;

/// Scenarios eligible for the daily challenge (varied skills, all click or hold, no movement).
const POOL: [&str; 7] = [
    "micro-adjust",
    "static-headline",
    "flick-headshot",
    "dynamic-strafe-heads",
    "switch-speed",
    "switch-heads",
    "track-reactive-strafe",
];

#[derive(Debug, Clone, PartialEq)]
pub struct DailyChallenge {
    /// Days since the Unix epoch (UTC), so every player gets the same challenge on the same date.
    pub day: i64,
    pub scenario_id: String,
    pub seed: u64,
}

impl DailyChallenge {
    /// History key that keeps challenge scores apart from regular runs.
    pub fn run_key(&self) -> String {
        format!("{}#daily-{}", self.scenario_id, self.day)
    }
}

/// The challenge for `utc_day` (days since the Unix epoch), from the available scenarios.
pub fn daily(utc_day: i64, scenarios: &[ScenarioDef]) -> Option<DailyChallenge> {
    let pool: Vec<&str> = POOL
        .iter()
        .copied()
        .filter(|id| scenarios.iter().any(|d| d.id == *id))
        .collect();
    if pool.is_empty() {
        return None;
    }
    // SplitMix-style mix of the day so consecutive days do not repeat in order.
    let mut z = (utc_day as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    Some(DailyChallenge {
        day: utc_day,
        scenario_id: pool[(z % pool.len() as u64) as usize].to_owned(),
        seed: z,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::builtin;

    #[test]
    fn same_day_same_challenge_and_days_vary() {
        let lib = builtin::all();
        let a = daily(20_000, &lib).unwrap();
        assert_eq!(daily(20_000, &lib), Some(a.clone()));
        let week: std::collections::HashSet<_> = (20_000..20_007)
            .map(|d| daily(d, &lib).unwrap().scenario_id)
            .collect();
        assert!(
            week.len() >= 3,
            "a week should cover several scenarios: {week:?}"
        );
        assert_ne!(daily(20_001, &lib).unwrap().seed, a.seed);
        assert_eq!(a.run_key(), format!("{}#daily-20000", a.scenario_id));
    }

    #[test]
    fn no_challenge_without_eligible_scenarios() {
        assert_eq!(daily(1, &[]), None);
    }
}
