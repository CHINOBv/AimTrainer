//! What the player's shots did this frame, as the app reports it to sound and hit
//! feedback. Read from the sessions' events; decides no gameplay.

use aim_core::scenario::{HitZone, SessionEvent};
use aim_world::arena::ArenaEvent;
use bevy::prelude::Message;
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;

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

/// Mark drawn around the crosshair for a moment after a hit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HitMarkerStyle {
    Off,
    /// The crosshair itself turns white.
    Flash,
    Dot,
    /// A small diagonal X.
    #[default]
    Cross,
    Ring,
    Brackets,
}

impl HitMarkerStyle {
    pub const ALL: [Self; 6] = [
        Self::Off,
        Self::Flash,
        Self::Dot,
        Self::Cross,
        Self::Ring,
        Self::Brackets,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Flash => "Crosshair flash",
            Self::Dot => "Dot",
            Self::Cross => "Cross (X)",
            Self::Ring => "Ring",
            Self::Brackets => "Brackets",
        }
    }
}

/// Color of the hit marker.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MarkerColor {
    #[default]
    White,
    Red,
    Yellow,
    Cyan,
    /// The crosshair's own color.
    Crosshair,
}

impl MarkerColor {
    pub const ALL: [Self; 5] = [
        Self::White,
        Self::Red,
        Self::Yellow,
        Self::Cyan,
        Self::Crosshair,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::White => "White",
            Self::Red => "Red",
            Self::Yellow => "Yellow",
            Self::Cyan => "Cyan",
            Self::Crosshair => "Crosshair color",
        }
    }

    /// sRGB bytes, or `None` for the crosshair's color.
    pub fn rgb(self) -> Option<[u8; 3]> {
        match self {
            Self::White => Some([255, 255, 255]),
            Self::Red => Some([255, 70, 70]),
            Self::Yellow => Some([255, 220, 60]),
            Self::Cyan => Some([80, 230, 255]),
            Self::Crosshair => None,
        }
    }
}

/// What a hit target does for a moment. A killed target plays it before disappearing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TargetEffect {
    Off,
    /// Briefly turns white.
    #[default]
    Flash,
    /// Swells slightly and settles back.
    Pop,
    /// Turns see-through for a moment (a killed target fades out).
    Fade,
    /// A thin ring expands from the hit and fades.
    Ring,
}

impl TargetEffect {
    pub const ALL: [Self; 5] = [Self::Off, Self::Flash, Self::Pop, Self::Fade, Self::Ring];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Flash => "Flash",
            Self::Pop => "Pop",
            Self::Fade => "Fade",
            Self::Ring => "Ring burst",
        }
    }

    /// Length of the effect. Short on purpose: a killed target lingers this long.
    pub fn duration_secs(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Flash => 0.08,
            Self::Pop => 0.12,
            Self::Fade => 0.12,
            Self::Ring => 0.2,
        }
    }
}

/// Opacity of the hit marker at the start: semi-transparent so it never hides a target.
pub const MARKER_PEAK_ALPHA: f32 = 0.8;
/// How far a popping target swells, as a fraction of its size.
const POP_AMOUNT: f32 = 0.12;
/// Lowest opacity of a fading target that survives the hit.
const FADE_MIN_ALPHA: f32 = 0.3;
/// How far a ring burst grows, as a fraction of its starting radius.
const RING_GROWTH: f32 = 0.8;
/// Opacity of a ring burst at the start.
const RING_PEAK_ALPHA: f32 = 0.6;

/// Hit-marker opacity at `t`, the elapsed fraction (0–1) of its duration: an ease-out.
pub fn marker_alpha(t: f32) -> f32 {
    MARKER_PEAK_ALPHA * (1.0 - t.clamp(0.0, 1.0)).powi(2)
}

/// Scale factor of a popping target at `t`: up and back to its size.
pub fn pop_scale(t: f32) -> f32 {
    1.0 + POP_AMOUNT * (PI * t.clamp(0.0, 1.0)).sin()
}

/// Opacity of a fading target at `t`: a surviving target dips and recovers, a killed one
/// fades out.
pub fn fade_alpha(t: f32, killed: bool) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if killed {
        (1.0 - FADE_MIN_ALPHA) * (1.0 - t)
    } else {
        FADE_MIN_ALPHA + (1.0 - FADE_MIN_ALPHA) * t
    }
}

/// Radius factor and opacity of a ring burst at `t`.
pub fn ring_scale_alpha(t: f32) -> (f32, f32) {
    let t = t.clamp(0.0, 1.0);
    (1.0 + RING_GROWTH * t, RING_PEAK_ALPHA * (1.0 - t))
}

/// Index of the pre-built material (of `steps`, from faintest to most opaque) closest to
/// `alpha` (0–1), so effects never create materials while playing.
pub fn alpha_step(alpha: f32, steps: usize) -> usize {
    let i = (alpha.clamp(0.0, 1.0) * steps as f32).ceil() as usize;
    i.clamp(1, steps) - 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use ShotOutcome::*;

    #[test]
    fn hit_marker_fades_out_from_a_semi_transparent_start() {
        assert_eq!(marker_alpha(0.0), MARKER_PEAK_ALPHA);
        const { assert!(MARKER_PEAK_ALPHA < 1.0, "subtle: never fully opaque") };
        assert_eq!(marker_alpha(1.0), 0.0);
        assert_eq!(marker_alpha(2.0), 0.0, "clamped past the end");
        let samples: Vec<f32> = (0..=10).map(|i| marker_alpha(i as f32 / 10.0)).collect();
        assert!(samples.windows(2).all(|w| w[1] < w[0]), "{samples:?}");
    }

    #[test]
    fn pop_returns_to_the_original_size() {
        assert_eq!(pop_scale(0.0), 1.0);
        assert!((pop_scale(0.5) - (1.0 + POP_AMOUNT)).abs() < 1e-6);
        assert!((pop_scale(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn fade_recovers_on_hits_and_vanishes_on_kills() {
        assert_eq!(fade_alpha(0.0, false), FADE_MIN_ALPHA);
        assert_eq!(fade_alpha(1.0, false), 1.0);
        assert!(fade_alpha(0.0, true) < 1.0);
        assert_eq!(fade_alpha(1.0, true), 0.0);
    }

    #[test]
    fn ring_grows_while_fading() {
        assert_eq!(ring_scale_alpha(0.0), (1.0, RING_PEAK_ALPHA));
        let (scale, alpha) = ring_scale_alpha(1.0);
        assert_eq!((scale, alpha), (1.0 + RING_GROWTH, 0.0));
    }

    #[test]
    fn alpha_steps_cover_the_prebuilt_materials() {
        assert_eq!(alpha_step(0.0, 4), 0);
        assert_eq!(alpha_step(0.25, 4), 0);
        assert_eq!(alpha_step(0.3, 4), 1);
        assert_eq!(alpha_step(1.0, 4), 3);
        assert_eq!(alpha_step(7.0, 4), 3);
    }

    #[test]
    fn effects_are_short_and_off_is_instant() {
        assert_eq!(TargetEffect::Off.duration_secs(), 0.0);
        for e in &TargetEffect::ALL[1..] {
            let d = e.duration_secs();
            assert!(d > 0.0 && d <= 0.25, "{e:?}: {d} s");
        }
    }

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
