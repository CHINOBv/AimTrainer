//! Sound cues of perception drills, placed around the player with stereo panning.
//!
//! Bevy 0.20's spatial audio only attenuates each ear by its distance to the source, so a
//! sound behind the player sounds the same as one in front. Cues are therefore rendered
//! as stereo clips here: equal-power panning by the horizontal angle, plus a duller,
//! quieter sound for sources behind the player (a simplified version of the head-shadow
//! cue listeners use to tell front from back). The values are estimates, tuned by ear.

use aim_core::math::{Vec3, ViewAngles, wrap_degrees};
use aim_core::scenario::cue::CueSound;
use bevy::prelude::Message;

/// Distance at which a cue plays at full level, meters; farther cues get quieter.
const FULL_LEVEL_M: f32 = 6.0;
/// Quietest a far cue gets, as a fraction of full level.
const MIN_DISTANCE_GAIN: f32 = 0.35;
/// Level lost by a source straight behind the player.
const REAR_ATTENUATION: f32 = 0.3;
/// How much a source straight behind is low-passed: 0 leaves it untouched, 1 silences it.
const REAR_MUFFLE: f32 = 0.85;

/// A cue to play this frame, as heard from the player's view.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct CueHeard {
    pub sound: CueSound,
    /// Horizontal angle from the view to the source, degrees (positive to the right).
    pub offset_deg: f32,
    /// Distance from the eye to the source, meters.
    pub distance_m: f32,
}

impl CueHeard {
    /// The cue of a sound made at `position`, heard from `eye` looking along `view`.
    pub fn new(sound: CueSound, eye: Vec3, view: ViewAngles, position: Vec3) -> Self {
        let to = position - eye;
        Self {
            sound,
            offset_deg: wrap_degrees(ViewAngles::from_direction(to).yaw_deg - view.yaw_deg),
            distance_m: to.length(),
        }
    }
}

/// Per-channel gains and rear filtering of a cue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub left: f32,
    pub right: f32,
    /// One-pole low-pass amount: 0 for sources in front, up to [`REAR_MUFFLE`] behind.
    pub muffle: f32,
}

/// Where a cue sits in the stereo image.
pub fn place(cue: &CueHeard) -> Placement {
    let a = cue.offset_deg.to_radians();
    // sin: -1 hard left, 1 hard right; equal-power law keeps the loudness constant.
    let theta = (a.sin() + 1.0) * std::f32::consts::FRAC_PI_4;
    let behind = (-a.cos()).max(0.0);
    let distance = (FULL_LEVEL_M / cue.distance_m.max(0.1)).clamp(MIN_DISTANCE_GAIN, 1.0);
    let level = distance * (1.0 - REAR_ATTENUATION * behind);
    Placement {
        left: theta.cos() * level,
        right: theta.sin() * level,
        muffle: REAR_MUFFLE * behind,
    }
}

/// Interleaved stereo samples (left, right, left, …) of a mono clip placed at `p`.
pub fn stereo(mono: &[f32], p: Placement) -> Vec<f32> {
    let keep = 1.0 - p.muffle;
    let mut low = 0.0f32;
    let mut out = Vec::with_capacity(mono.len() * 2);
    for s in mono {
        low += keep * (s - low);
        out.push(low * p.left);
        out.push(low * p.right);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(offset_deg: f32, distance_m: f32) -> CueHeard {
        CueHeard {
            sound: CueSound::Footsteps,
            offset_deg,
            distance_m,
        }
    }

    #[test]
    fn the_offset_is_measured_from_the_view_positive_to_the_right() {
        let eye = Vec3::new(0.0, 1.6, 0.0);
        // Forward is -Z: a source at +X is on the right of a player looking forward.
        let right = CueHeard::new(
            CueSound::Gunshot,
            eye,
            ViewAngles::new(0.0, 0.0),
            Vec3::new(5.0, 1.6, 0.0),
        );
        assert!((right.offset_deg - 90.0).abs() < 1e-3, "{right:?}");
        assert!((right.distance_m - 5.0).abs() < 1e-4);
        // Turned to face it, the same source is straight ahead.
        let ahead = CueHeard::new(
            CueSound::Gunshot,
            eye,
            ViewAngles::new(90.0, 0.0),
            Vec3::new(5.0, 1.6, 0.0),
        );
        assert!(ahead.offset_deg.abs() < 1e-3, "{ahead:?}");
    }

    #[test]
    fn sources_pan_towards_their_side_with_constant_power() {
        let front = place(&cue(0.0, 6.0));
        assert!((front.left - front.right).abs() < 1e-5);
        assert!((front.left.powi(2) + front.right.powi(2) - 1.0).abs() < 1e-4);
        let right = place(&cue(90.0, 6.0));
        assert!(right.right > 0.99 && right.left < 1e-3, "{right:?}");
        let left = place(&cue(-60.0, 6.0));
        assert!(left.left > left.right, "{left:?}");
    }

    #[test]
    fn sources_behind_are_duller_and_quieter_than_the_same_side_in_front() {
        let front = place(&cue(30.0, 6.0));
        let back = place(&cue(150.0, 6.0));
        // Same left/right balance (sin 30° = sin 150°), but muffled and quieter behind.
        assert!((front.right / front.left - back.right / back.left).abs() < 1e-3);
        assert!(back.right < front.right);
        assert_eq!(front.muffle, 0.0);
        assert!(back.muffle > 0.5, "{back:?}");
    }

    #[test]
    fn far_sources_are_quieter_down_to_a_floor() {
        let near = place(&cue(0.0, 3.0));
        let far = place(&cue(0.0, 12.0));
        let very_far = place(&cue(0.0, 1000.0));
        assert!(far.left < near.left);
        let floor = MIN_DISTANCE_GAIN * std::f32::consts::FRAC_1_SQRT_2;
        assert!((very_far.left - floor).abs() < 1e-4, "{very_far:?}");
    }

    #[test]
    fn stereo_interleaves_the_panned_channels_and_filters_rear_sources() {
        let mono = [1.0, 1.0, 1.0, 1.0];
        let hard_right = stereo(&mono, place(&cue(90.0, 6.0)));
        assert_eq!(hard_right.len(), 8);
        assert!(
            hard_right
                .chunks(2)
                .all(|lr| lr[0].abs() < 1e-3 && lr[1] > 0.99)
        );
        // A muffled source rises slowly towards the input level.
        let rear = stereo(&mono, place(&cue(180.0, 6.0)));
        assert!(rear[1] < rear[7] && rear[7] < 1.0, "{rear:?}");
    }
}
