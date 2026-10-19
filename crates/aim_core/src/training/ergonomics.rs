//! Wrist and posture care: break prompts by continuous play, a weekly volume cap, and a short
//! stretch list.
//!
//! Musculoskeletal complaints are common among esports players (cross-sectional surveys:
//! Monma et al. 2024, PMC11380764; Kurniawan et al. 2024, fspor.2024.1372979). No trial
//! has shown that breaks or stretches prevent these complaints in esports, so every default
//! here is an **estimate**, the policy can be turned off, and the app says so
//! ([`EVIDENCE_NOTE`]). See "Breaks and wrist care" in `docs/training-science.md`.

use super::progress::RunRecord;
use serde::{Deserialize, Serialize};

const SECS_PER_WEEK: u64 = 7 * 86_400;

/// When to suggest a break and when a week has had enough aim training.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BreakPolicy {
    /// Minutes of continuous play after which a break is due (estimate: 45).
    pub continuous_minutes: u32,
    /// Suggested break length in minutes (estimate: 5).
    pub break_minutes: u32,
    /// Training minutes per rolling 7 days after which the cap is reached (estimate: 420,
    /// about 1 h a day, where Listman et al. 2021 saw daily gains peak).
    pub weekly_cap_minutes: u32,
    /// `false` turns every prompt off.
    pub enabled: bool,
}

impl Default for BreakPolicy {
    fn default() -> Self {
        Self {
            continuous_minutes: 45,
            break_minutes: 5,
            weekly_cap_minutes: 420,
            enabled: true,
        }
    }
}

impl BreakPolicy {
    /// Problems with the policy; empty when valid. Each message names the field.
    pub fn validate(&self) -> Vec<String> {
        [
            ("continuous_minutes", self.continuous_minutes),
            ("break_minutes", self.break_minutes),
            ("weekly_cap_minutes", self.weekly_cap_minutes),
        ]
        .into_iter()
        .filter(|(_, v)| *v == 0)
        .map(|(name, _)| format!("{name}: must be above 0"))
        .collect()
    }

    /// Whether a break is due after `session_minutes_since_break` of continuous play.
    pub fn break_due(&self, session_minutes_since_break: f32) -> bool {
        self.enabled && session_minutes_since_break >= self.continuous_minutes as f32
    }

    /// Whether `minutes_this_week` of training reached the weekly cap.
    pub fn weekly_cap_reached(&self, minutes_this_week: f32) -> bool {
        self.enabled && minutes_this_week >= self.weekly_cap_minutes as f32
    }
}

/// Minutes of runs that finished in the 7 days up to `now` (Unix seconds).
pub fn minutes_this_week(records: &[RunRecord], now: u64) -> f32 {
    let since = now.saturating_sub(SECS_PER_WEEK);
    records
        .iter()
        .filter(|r| r.timestamp > since && r.timestamp <= now)
        .map(|r| r.duration_secs)
        .sum::<f32>()
        / 60.0
}

/// One stretch of the break list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stretch {
    /// Short name.
    pub name: &'static str,
    /// How to do it.
    pub how: &'static str,
    /// How long to hold or repeat it, seconds.
    pub secs: u32,
}

/// Shown with the stretch list.
pub const STRETCH_DISCLAIMER: &str = "General stretches, not medical advice. Move gently and \
stop if anything hurts. See a health professional about pain that lasts.";

/// Shown with break prompts: what the evidence does and does not say.
pub const EVIDENCE_NOTE: &str = "Wrist, neck and back complaints are common among esports \
players, but no trial has shown that breaks or stretches prevent them. Breaks are offered as \
a precaution; you can change or turn them off in Settings.";

/// A short, generic stretch list for a break.
pub const STRETCHES: [Stretch; 5] = [
    Stretch {
        name: "Wrist flexor stretch",
        how: "Arm straight, palm up; gently pull the fingers back with the other hand.",
        secs: 20,
    },
    Stretch {
        name: "Wrist extensor stretch",
        how: "Arm straight, palm down; gently bend the hand down with the other hand.",
        secs: 20,
    },
    Stretch {
        name: "Finger spreads",
        how: "Spread the fingers wide, hold, then make a loose fist. Repeat.",
        secs: 20,
    },
    Stretch {
        name: "Shoulder rolls",
        how: "Roll both shoulders slowly backwards, then forwards.",
        secs: 20,
    },
    Stretch {
        name: "Look away",
        how: "Stand up and look at something far away to rest the eyes.",
        secs: 20,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_break_is_due_from_the_continuous_limit_on() {
        let p = BreakPolicy::default();
        assert!(!p.break_due(44.9));
        assert!(p.break_due(45.0));
        assert!(p.break_due(80.0));
    }

    #[test]
    fn the_weekly_cap_is_reached_at_its_minutes() {
        let p = BreakPolicy {
            weekly_cap_minutes: 300,
            ..BreakPolicy::default()
        };
        assert!(!p.weekly_cap_reached(299.0));
        assert!(p.weekly_cap_reached(300.0));
    }

    #[test]
    fn a_disabled_policy_never_prompts() {
        let p = BreakPolicy {
            enabled: false,
            ..BreakPolicy::default()
        };
        assert!(!p.break_due(1000.0));
        assert!(!p.weekly_cap_reached(100_000.0));
    }

    #[test]
    fn zero_minutes_are_rejected_by_field() {
        assert!(BreakPolicy::default().validate().is_empty());
        let p = BreakPolicy {
            continuous_minutes: 0,
            weekly_cap_minutes: 0,
            ..BreakPolicy::default()
        };
        assert_eq!(
            p.validate(),
            vec![
                "continuous_minutes: must be above 0".to_owned(),
                "weekly_cap_minutes: must be above 0".to_owned(),
            ]
        );
    }

    #[test]
    fn the_policy_round_trips_and_missing_fields_take_defaults() {
        let p = BreakPolicy {
            continuous_minutes: 30,
            enabled: false,
            ..BreakPolicy::default()
        };
        assert_eq!(
            toml::from_str::<BreakPolicy>(&toml::to_string(&p).unwrap()).unwrap(),
            p
        );
        let partial: BreakPolicy = toml::from_str("break_minutes = 10").unwrap();
        assert_eq!(partial.break_minutes, 10);
        assert_eq!(partial.continuous_minutes, 45);
    }

    #[test]
    fn weekly_minutes_count_only_the_last_seven_days() {
        let now = 1_800_000_000;
        let run = |timestamp| RunRecord {
            timestamp,
            scenario: "a".into(),
            score: 1.0,
            duration_secs: 600.0,
        };
        let r = vec![
            run(now - SECS_PER_WEEK),
            run(now - SECS_PER_WEEK + 1),
            run(now),
            run(now + 1),
        ];
        assert_eq!(minutes_this_week(&r, now), 20.0);
    }

    #[test]
    fn the_stretch_list_and_notes_are_filled_in() {
        assert!(STRETCHES.iter().all(|s| !s.name.is_empty() && s.secs > 0));
        assert!(STRETCH_DISCLAIMER.contains("not medical advice"));
        assert!(EVIDENCE_NOTE.contains("no trial"));
    }
}
