//! Target motion: advances one target's position each frame according to the scenario's
//! [`Motion`] (static, VALORANT-style strafing, or free drift).

use super::def::Motion;
use crate::math::Vec3;
use crate::movement::{self, LocalVelocity, MoveInput};
use crate::rng::Rng;

/// Lowest height a sphere target's center may reach.
pub(super) const MIN_SPHERE_HEIGHT: f32 = 0.3;
/// Area under VALORANT's counter-strafe curve: braking distance ≈ speed × this.
const STRAFE_BRAKE_SECS: f32 = 0.075;

/// Per-target motion state, owned by the target and advanced by [`MotionState::step`].
#[derive(Debug, Clone, PartialEq)]
pub(super) struct MotionState {
    /// Spawn point; strafing and drift stay within `max_offset` of it.
    pub(super) anchor: Vec3,
    /// Horizontal unit vector perpendicular to the line towards the player.
    pub(super) lateral: Vec3,
    pub(super) strafe: LocalVelocity,
    /// Strafe direction the target wants: -1, 0 (stop) or 1.
    pub(super) intent: f32,
    pub(super) drift: Vec3,
    pub(super) next_change_at: f32,
}

impl MotionState {
    /// A target at rest at `anchor` that picks its first motion when it becomes visible.
    pub(super) fn new(anchor: Vec3, lateral: Vec3, visible_at: f32) -> Self {
        Self {
            anchor,
            lateral,
            strafe: LocalVelocity::default(),
            intent: 0.0,
            drift: Vec3::ZERO,
            next_change_at: visible_at,
        }
    }

    pub(super) fn velocity(&self) -> Vec3 {
        self.lateral * self.strafe.right + self.drift
    }

    /// Advances one visible target by `dt` seconds at time `now`, moving `position`.
    pub(super) fn step(
        &mut self,
        position: &mut Vec3,
        motion: &Motion,
        humanoid: bool,
        rng: &mut Rng,
        now: f32,
        dt: f32,
    ) {
        match motion {
            Motion::Static => {}
            Motion::Strafe {
                weapon,
                change_secs,
                stop_chance,
                max_offset,
            } => {
                let offset = (*position - self.anchor).dot(self.lateral);
                if now >= self.next_change_at {
                    self.intent = if rng.next_f32() < *stop_chance {
                        0.0
                    } else if self.intent == 0.0 {
                        rng.sign()
                    } else {
                        -self.intent
                    };
                    self.next_change_at = now + rng.in_range(*change_secs);
                }
                let profile = weapon.movement();
                // Reverse early enough that the counter-strafe braking distance keeps the
                // target inside its lane.
                let braking = self.strafe.right.abs() * STRAFE_BRAKE_SECS;
                if self.intent != 0.0 && offset * self.intent + braking >= *max_offset {
                    self.intent = -self.intent;
                }
                let input = MoveInput {
                    right: self.intent,
                    ..Default::default()
                };
                self.strafe = movement::step(&profile, self.strafe, input, dt);
                *position += self.lateral * (self.strafe.right * dt);
            }
            Motion::Drift {
                speed,
                change_secs,
                max_offset,
            } => {
                if now >= self.next_change_at {
                    let a = rng.next_f32() * std::f32::consts::TAU;
                    let up = if humanoid { 0.0 } else { a.sin() };
                    self.drift = (self.lateral * a.cos() + Vec3::UP * up) * rng.in_range(*speed);
                    self.next_change_at = now + rng.in_range(*change_secs);
                }
                let away = *position - self.anchor;
                if away.length() >= *max_offset && away.dot(self.drift) > 0.0 {
                    self.drift = -self.drift;
                }
                *position += self.drift * dt;
                if !humanoid {
                    position.y = position.y.max(MIN_SPHERE_HEIGHT);
                }
            }
        }
    }
}
