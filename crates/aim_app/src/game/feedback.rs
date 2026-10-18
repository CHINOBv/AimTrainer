//! What the player's shots did this frame, as the app reports it to sound and hit
//! feedback. Read from the sessions' events; decides no gameplay.

use aim_core::scenario::{HitZone, SessionEvent};
use aim_world::arena::ArenaEvent;
use bevy::prelude::Message;

/// Result of one shot, from the player's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShotOutcome {
    Miss,
    Hit { head: bool },
    Kill,
}

impl ShotOutcome {
    fn rank(self) -> u8 {
        match self {
            Self::Miss => 0,
            Self::Hit { head: false } => 1,
            Self::Hit { head: true } => 2,
            Self::Kill => 3,
        }
    }

    pub fn from_session(event: &SessionEvent) -> Option<Self> {
        match *event {
            SessionEvent::Shot { hit, .. } => Some(Self::of_shot(hit.map(|(_, z)| z))),
            SessionEvent::Killed { .. } => Some(Self::Kill),
            _ => None,
        }
    }

    pub fn from_arena(event: &ArenaEvent) -> Option<Self> {
        match *event {
            ArenaEvent::Shot { hit, .. } => Some(Self::of_shot(hit.map(|(_, z)| z))),
            ArenaEvent::BotKilled { .. } => Some(Self::Kill),
            _ => None,
        }
    }

    fn of_shot(zone: Option<HitZone>) -> Self {
        match zone {
            None => Self::Miss,
            Some(z) => Self::Hit {
                head: z == HitZone::Head,
            },
        }
    }
}

/// The one outcome a frame is felt as: a kill beats a headshot, which beats a body hit,
/// which beats a miss. The killing shot's own hit is folded into the kill.
pub fn strongest(outcomes: impl IntoIterator<Item = ShotOutcome>) -> Option<ShotOutcome> {
    outcomes.into_iter().max_by_key(|o| o.rank())
}

/// Sent by the scenario and arena runs for every shot outcome.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct ShotFeedback(pub ShotOutcome);

#[cfg(test)]
mod tests {
    use super::*;
    use ShotOutcome::*;

    #[test]
    fn kills_beat_headshots_beat_hits_beat_misses() {
        assert_eq!(strongest([]), None);
        assert_eq!(strongest([Miss]), Some(Miss));
        assert_eq!(
            strongest([Miss, Hit { head: false }]),
            Some(Hit { head: false })
        );
        assert_eq!(
            strongest([Hit { head: false }, Hit { head: true }, Miss]),
            Some(Hit { head: true })
        );
        assert_eq!(strongest([Hit { head: true }, Kill, Miss]), Some(Kill));
    }

    #[test]
    fn session_events_map_to_outcomes() {
        let shot = |hit| SessionEvent::Shot { hit, moving: false };
        assert_eq!(ShotOutcome::from_session(&shot(None)), Some(Miss));
        assert_eq!(
            ShotOutcome::from_session(&shot(Some((1, HitZone::Head)))),
            Some(Hit { head: true })
        );
        assert_eq!(
            ShotOutcome::from_session(&shot(Some((1, HitZone::Sphere)))),
            Some(Hit { head: false })
        );
        let killed = SessionEvent::Killed {
            id: 1,
            time_to_kill: 0.4,
        };
        assert_eq!(ShotOutcome::from_session(&killed), Some(Kill));
        assert_eq!(
            ShotOutcome::from_session(&SessionEvent::Spawned { id: 2 }),
            None
        );
    }

    #[test]
    fn arena_events_map_to_outcomes() {
        let killed = ArenaEvent::BotKilled {
            id: 3,
            time_to_kill_secs: 1.0,
        };
        assert_eq!(ShotOutcome::from_arena(&killed), Some(Kill));
        let body = ArenaEvent::Shot {
            hit: Some((3, HitZone::Body)),
            moving: true,
        };
        assert_eq!(ShotOutcome::from_arena(&body), Some(Hit { head: false }));
        assert_eq!(ShotOutcome::from_arena(&ArenaEvent::PlayerDied), None);
    }
}
