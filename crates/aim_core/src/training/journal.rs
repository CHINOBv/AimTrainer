//! Ranked session journal: a one-tap entry after each match, linked to that day's warm-up
//! readiness and fatigue, with losing-streak and tilt-reset signals.
//!
//! Entries are plain serde data; the app stores them. Readiness and fatigue are captured
//! when the entry is logged (from [`super::progress::readiness`] and
//! [`super::progress::fatigue`]), so the journal can compare matches played warm or cold,
//! fresh or tired. See "Ranked journal and tilt reset" in `docs/training-science.md`.

use super::progress::Warmth;
use super::routine::{Block, RoutineDef};
use serde::{Deserialize, Serialize};

/// Losses in a row after which a tilt reset is suggested (estimate).
pub const TILT_LOSS_STREAK: u32 = 2;
/// Lowest self-rating.
pub const MIN_SELF_RATING: u8 = 1;
/// Highest self-rating.
pub const MAX_SELF_RATING: u8 = 5;

/// How a match ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    /// The match was won.
    Win,
    /// The match was lost.
    Loss,
    /// The match ended in a draw.
    Draw,
}

/// One tag the player can attach to an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JournalTag {
    /// Frustrated, playing on emotion.
    Tilted,
    /// Physically or mentally tired.
    Tired,
    /// Warmed up before the match.
    WarmedUp,
    /// Queued without warming up.
    Cold,
    /// Aim felt off.
    AimOff,
    /// Aim felt good.
    AimOn,
    /// The teams were clearly mismatched.
    TeamDiff,
    /// Anything else.
    Other,
}

impl JournalTag {
    /// Every tag, in display order.
    pub const ALL: [JournalTag; 8] = [
        JournalTag::Tilted,
        JournalTag::Tired,
        JournalTag::WarmedUp,
        JournalTag::Cold,
        JournalTag::AimOff,
        JournalTag::AimOn,
        JournalTag::TeamDiff,
        JournalTag::Other,
    ];
}

/// One logged match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalEntry {
    /// Unix seconds when the match was logged.
    pub timestamp: u64,
    /// How the match ended.
    pub outcome: Outcome,
    /// Ranked rating gained (positive) or lost (negative), if the player entered it.
    #[serde(default)]
    pub rr_change: Option<i32>,
    /// How well the player thinks they played, [`MIN_SELF_RATING`]..=[`MAX_SELF_RATING`].
    pub self_rating: u8,
    /// One optional tag.
    #[serde(default)]
    pub tag: Option<JournalTag>,
    /// Warm-up readiness when the entry was logged; `None` when it was unknown.
    #[serde(default)]
    pub readiness: Option<Warmth>,
    /// Whether a fatigue signal was showing when the entry was logged.
    #[serde(default)]
    pub fatigued: bool,
}

/// Why an entry was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JournalError {
    /// The self-rating is outside 1..=5; carries it.
    #[error("self_rating: must be between {MIN_SELF_RATING} and {MAX_SELF_RATING} (got {0})")]
    SelfRating(u8),
}

impl JournalEntry {
    /// `Ok` when the entry can be stored.
    pub fn validate(&self) -> Result<(), JournalError> {
        if (MIN_SELF_RATING..=MAX_SELF_RATING).contains(&self.self_rating) {
            Ok(())
        } else {
            Err(JournalError::SelfRating(self.self_rating))
        }
    }
}

/// Results of a group of matches.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Split {
    /// Matches in the group.
    pub matches: u32,
    /// Matches won.
    pub wins: u32,
    /// Matches lost.
    pub losses: u32,
    /// Matches drawn.
    pub draws: u32,
    /// Wins over all matches of the group (draws count as not won); `None` when empty.
    pub win_rate: Option<f32>,
    /// Mean self-rating of the group; `None` when empty.
    pub avg_self_rating: Option<f32>,
}

impl Split {
    fn of<'a>(entries: impl Iterator<Item = &'a JournalEntry>) -> Split {
        let mut s = Split::default();
        let mut rating_sum = 0u32;
        for e in entries {
            s.matches += 1;
            rating_sum += u32::from(e.self_rating);
            match e.outcome {
                Outcome::Win => s.wins += 1,
                Outcome::Loss => s.losses += 1,
                Outcome::Draw => s.draws += 1,
            }
        }
        if s.matches > 0 {
            s.win_rate = Some(s.wins as f32 / s.matches as f32);
            s.avg_self_rating = Some(rating_sum as f32 / s.matches as f32);
        }
        s
    }
}

/// Matches split by the readiness they were played at.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReadinessSplit {
    /// Played at READY.
    pub ready: Split,
    /// Played at WARMING.
    pub warming: Split,
    /// Played COLD.
    pub cold: Split,
    /// Played with readiness unknown (no baseline or no run that day).
    pub unknown: Split,
}

/// Matches split by whether a fatigue signal was showing.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FatigueSplit {
    /// No fatigue signal.
    pub fresh: Split,
    /// A fatigue signal was showing.
    pub fatigued: Split,
}

/// Every logged match, oldest first.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Journal {
    /// Entries in the order they were logged.
    pub entries: Vec<JournalEntry>,
}

impl Journal {
    /// Log a match, after validating it.
    pub fn add(&mut self, entry: JournalEntry) -> Result<(), JournalError> {
        entry.validate()?;
        self.entries.push(entry);
        Ok(())
    }

    /// Losses in a row at the end of the journal; a win or a draw ends a streak.
    pub fn losing_streak(&self) -> u32 {
        self.entries
            .iter()
            .rev()
            .take_while(|e| e.outcome == Outcome::Loss)
            .count() as u32
    }

    /// Whether the last [`TILT_LOSS_STREAK`] matches were all lost.
    pub fn tilt_reset_suggested(&self) -> bool {
        self.losing_streak() >= TILT_LOSS_STREAK
    }

    /// Win rate and self-rating by readiness label.
    pub fn by_readiness(&self) -> ReadinessSplit {
        let group = |w: Option<Warmth>| Split::of(self.entries.iter().filter(|e| e.readiness == w));
        ReadinessSplit {
            ready: group(Some(Warmth::Ready)),
            warming: group(Some(Warmth::Warming)),
            cold: group(Some(Warmth::Cold)),
            unknown: group(None),
        }
    }

    /// Win rate and self-rating with and without a fatigue signal.
    pub fn by_fatigue(&self) -> FatigueSplit {
        let group = |f: bool| Split::of(self.entries.iter().filter(|e| e.fatigued == f));
        FatigueSplit {
            fresh: group(false),
            fatigued: group(true),
        }
    }
}

/// Id of the tilt-reset routine.
pub const TILT_RESET_ID: &str = "tilt-reset";
/// Length of the paced-breathing timer, seconds (estimate).
pub const BREATHING_SECS: u32 = 60;
/// Breathing pace of the timer, breaths per minute (estimate: slow breathing around 6/min).
pub const BREATHS_PER_MINUTE: u32 = 6;
/// Length of the easy tracking run after the breathing timer, seconds.
pub const TILT_TRACKING_SECS: f32 = 120.0;
/// Difficulty of the easy tracking run (see `scenario::difficulty`).
pub const TILT_TRACKING_DIFFICULTY: f32 = -0.5;

/// A three-minute reset between matches: a paced-breathing timer, then easy smooth tracking.
#[derive(Debug, Clone, PartialEq)]
pub struct TiltReset {
    /// Length of the breathing timer, seconds.
    pub breathing_secs: u32,
    /// Breaths per minute the timer paces.
    pub breaths_per_minute: u32,
    /// Text shown during the breathing timer.
    pub breathing_note: String,
    /// The routine played after the timer.
    pub routine: RoutineDef,
}

impl TiltReset {
    /// Seconds per breath (inhale plus exhale).
    pub fn breath_secs(&self) -> f32 {
        60.0 / self.breaths_per_minute as f32
    }

    /// Total length in minutes: the timer plus the routine's runs.
    pub fn minutes(&self) -> f32 {
        let runs: f32 = self
            .routine
            .blocks
            .iter()
            .filter_map(|b| b.duration_secs)
            .sum();
        (self.breathing_secs as f32 + runs) / 60.0
    }
}

/// The tilt-reset routine suggested after [`TILT_LOSS_STREAK`] losses in a row.
pub fn tilt_reset() -> TiltReset {
    TiltReset {
        breathing_secs: BREATHING_SECS,
        breaths_per_minute: BREATHS_PER_MINUTE,
        breathing_note: "Breathe in for 5 seconds and out for 5 seconds. Let your hand rest."
            .to_owned(),
        routine: RoutineDef {
            id: TILT_RESET_ID.to_owned(),
            name: "Tilt reset".to_owned(),
            description:
                "A short pause before the next queue: slow breathing, then easy smooth tracking."
                    .to_owned(),
            interleave_from: None,
            blocks: vec![Block {
                arena: None,
                scenario: "track-precise".to_owned(),
                runs: Some(1),
                minutes: None,
                note: "Easy pace: smooth and relaxed, no score to chase.".to_owned(),
                difficulty: TILT_TRACKING_DIFFICULTY,
                duration_secs: Some(TILT_TRACKING_SECS),
            }],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::builtin;

    fn entry(outcome: Outcome, rating: u8) -> JournalEntry {
        JournalEntry {
            timestamp: 0,
            outcome,
            rr_change: None,
            self_rating: rating,
            tag: None,
            readiness: None,
            fatigued: false,
        }
    }

    fn journal(entries: Vec<JournalEntry>) -> Journal {
        let mut j = Journal::default();
        for e in entries {
            j.add(e).unwrap();
        }
        j
    }

    #[test]
    fn the_losing_streak_counts_the_latest_losses_only() {
        use Outcome::*;
        assert_eq!(Journal::default().losing_streak(), 0);
        let j = journal(vec![
            entry(Loss, 3),
            entry(Win, 3),
            entry(Loss, 3),
            entry(Loss, 2),
        ]);
        assert_eq!(j.losing_streak(), 2);
        let j = journal(vec![entry(Loss, 3), entry(Loss, 3), entry(Draw, 3)]);
        assert_eq!(j.losing_streak(), 0, "a draw ends the streak");
    }

    #[test]
    fn a_tilt_reset_is_suggested_after_two_losses_in_a_row() {
        use Outcome::*;
        assert!(!journal(vec![entry(Loss, 3)]).tilt_reset_suggested());
        assert!(
            !journal(vec![entry(Loss, 3), entry(Win, 3), entry(Loss, 3)]).tilt_reset_suggested()
        );
        assert!(
            journal(vec![entry(Win, 3), entry(Loss, 3), entry(Loss, 3)]).tilt_reset_suggested()
        );
        assert!(
            journal(vec![entry(Loss, 3), entry(Loss, 3), entry(Loss, 1)]).tilt_reset_suggested()
        );
    }

    #[test]
    fn win_rate_and_rating_split_by_readiness_and_fatigue() {
        use Outcome::*;
        let at = |o, r, w, f| JournalEntry {
            readiness: w,
            fatigued: f,
            ..entry(o, r)
        };
        let j = journal(vec![
            at(Win, 4, Some(Warmth::Ready), false),
            at(Win, 5, Some(Warmth::Ready), false),
            at(Loss, 3, Some(Warmth::Ready), true),
            at(Loss, 2, Some(Warmth::Cold), true),
            at(Draw, 3, Some(Warmth::Cold), false),
            at(Win, 4, None, false),
        ]);
        let r = j.by_readiness();
        assert_eq!((r.ready.matches, r.ready.wins, r.ready.losses), (3, 2, 1));
        assert_eq!(r.ready.win_rate, Some(2.0 / 3.0));
        assert_eq!(r.ready.avg_self_rating, Some(4.0));
        assert_eq!((r.cold.matches, r.cold.draws), (2, 1));
        assert_eq!(r.cold.win_rate, Some(0.0), "a draw is not a win");
        assert_eq!(r.cold.avg_self_rating, Some(2.5));
        assert_eq!(r.warming, Split::default());
        assert_eq!(r.warming.win_rate, None);
        assert_eq!(r.unknown.matches, 1);
        let f = j.by_fatigue();
        assert_eq!((f.fresh.matches, f.fresh.wins), (4, 3));
        assert_eq!((f.fatigued.matches, f.fatigued.losses), (2, 2));
        assert_eq!(f.fatigued.win_rate, Some(0.0));
        assert_eq!(f.fatigued.avg_self_rating, Some(2.5));
    }

    #[test]
    fn self_ratings_outside_one_to_five_are_rejected() {
        let mut j = Journal::default();
        assert_eq!(
            j.add(entry(Outcome::Win, 0)),
            Err(JournalError::SelfRating(0))
        );
        assert_eq!(
            j.add(entry(Outcome::Win, 6)),
            Err(JournalError::SelfRating(6))
        );
        assert!(j.entries.is_empty(), "rejected entries are not stored");
        assert!(j.add(entry(Outcome::Win, 1)).is_ok());
        assert!(j.add(entry(Outcome::Win, 5)).is_ok());
        assert_eq!(j.entries.len(), 2);
    }

    #[test]
    fn entries_round_trip_with_kebab_case_tags() {
        let e = JournalEntry {
            timestamp: 1_800_000_000,
            outcome: Outcome::Loss,
            rr_change: Some(-18),
            self_rating: 2,
            tag: Some(JournalTag::AimOff),
            readiness: Some(Warmth::Warming),
            fatigued: true,
        };
        let j = Journal {
            entries: vec![e.clone()],
        };
        let src = toml::to_string(&j).unwrap();
        assert!(
            src.contains("\"aim-off\"") && src.contains("\"warming\""),
            "{src}"
        );
        assert_eq!(toml::from_str::<Journal>(&src).unwrap(), j);
        let minimal: JournalEntry =
            toml::from_str("timestamp = 1\noutcome = \"win\"\nself_rating = 4").unwrap();
        assert_eq!(minimal.tag, None);
        assert!(!minimal.fatigued);
        assert_eq!(JournalTag::ALL.len(), 8);
    }

    #[test]
    fn the_tilt_reset_is_three_minutes_of_breathing_then_easy_tracking() {
        let t = tilt_reset();
        assert_eq!(t.breathing_secs, 60);
        assert_eq!(t.breath_secs(), 10.0);
        assert!((t.minutes() - 3.0).abs() < 1e-6, "{}", t.minutes());
        let lib = builtin::all();
        assert!(
            t.routine.validate(&lib).is_empty(),
            "{:?}",
            t.routine.validate(&lib)
        );
        let plan = t.routine.plan(&lib, 0);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].scenario_id, "track-precise");
        assert!(plan[0].difficulty < 0.0, "easier than the authored level");
        assert_eq!(plan[0].duration_secs, Some(TILT_TRACKING_SECS));
    }
}
