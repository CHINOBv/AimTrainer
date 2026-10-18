//! Target motion: advances one target's position each frame according to the scenario's
//! [`Motion`] (static, VALORANT-style strafing, evasive dodging, or free drift).

use super::MIN_SPHERE_HEIGHT;
use super::def::{Motion, Shape, TargetsDef};
use crate::games::valorant::{
    CROUCH_HEIGHT_DROP_M, CROUCH_TRANSITION_SECS, JUMP_AIR_SECS, JUMP_APEX_M, Weapon,
};
use crate::math::{Range, Vec3};
use crate::movement::{self, LocalVelocity, MoveInput};
use crate::rng::Rng;

/// Area under VALORANT's counter-strafe curve: braking distance ≈ speed × this.
pub(super) const STRAFE_BRAKE_SECS: f32 = 0.075;

/// What drives a target's motion this frame: the motion (the scenario's, or a behaviour
/// rule's override), whether the target is a humanoid, and the speed multiplier.
#[derive(Debug, Clone, Copy)]
pub(super) struct Drive<'a> {
    pub(super) motion: &'a Motion,
    pub(super) humanoid: bool,
    /// In-run speed tuning (`targets.speed_scale`) times any behaviour rule's multiplier.
    pub(super) speed_k: f32,
}

impl<'a> Drive<'a> {
    /// The scenario's own motion at its own speed.
    pub(super) fn of(targets: &'a TargetsDef) -> Self {
        Self {
            motion: &targets.motion,
            humanoid: matches!(targets.shape, Shape::Humanoid { .. }),
            speed_k: targets.speed_scale,
        }
    }
}

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
    /// How far into a crouch the target is: 0 standing, 1 fully crouched.
    pub(super) crouch: f32,
    /// Whether the target wants to be crouched.
    crouching: bool,
    /// Height above the anchor while jumping, meters.
    lift: f32,
    /// Vertical speed while airborne, m/s (0 on the ground).
    rise: f32,
}

/// The chances and timings a [`Motion::Dodge`] target decides with.
struct DodgeRules {
    weapon: Weapon,
    change_secs: Range,
    juke_secs: Range,
    juke_chance: f32,
    stop_chance: f32,
    crouch_chance: f32,
    jump_chance: f32,
    max_offset: f32,
    /// In-run speed tuning applied to the ground movement.
    speed_k: f32,
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
            crouch: 0.0,
            crouching: false,
            lift: 0.0,
            rise: 0.0,
        }
    }

    pub(super) fn velocity(&self) -> Vec3 {
        self.lateral * self.strafe.right + self.drift + Vec3::UP * self.rise
    }

    /// Points the target in strafe direction `dir` (-1, 0 or 1) and holds it, standing,
    /// until `until`, when the motion decides again: a reaction to the player.
    pub(super) fn react(&mut self, dir: f32, until: f32) {
        self.intent = dir;
        self.crouching = false;
        self.next_change_at = until;
    }

    /// Hands the target over to `motion` (a behaviour rule switched it) at `now`: the new
    /// motion decides at once, and movement it has no part in stops.
    pub(super) fn switch_to(&mut self, motion: &Motion, now: f32) {
        self.next_change_at = now;
        self.crouching = false;
        self.crouch = 0.0;
        match motion {
            Motion::Static => {
                self.intent = 0.0;
                self.strafe = LocalVelocity::default();
                self.drift = Vec3::ZERO;
            }
            Motion::Strafe { .. } | Motion::Dodge { .. } => self.drift = Vec3::ZERO,
            Motion::Drift { .. } => {
                self.intent = 0.0;
                self.strafe = LocalVelocity::default();
            }
        }
    }

    /// Advances one visible target by `dt` seconds at time `now`, moving `position`. Every
    /// motion honors `drive.speed_k` (in-run speed tuning, times any behaviour rule's
    /// multiplier) by scaling its speed.
    pub(super) fn step(
        &mut self,
        position: &mut Vec3,
        drive: Drive<'_>,
        rng: &mut Rng,
        now: f32,
        dt: f32,
    ) {
        let Drive {
            motion,
            humanoid,
            speed_k,
        } = drive;
        match motion {
            Motion::Static => {}
            Motion::Strafe {
                weapon,
                change_secs,
                stop_chance,
                max_offset,
            } => {
                if now >= self.next_change_at {
                    self.intent = if rng.next_f32() < *stop_chance {
                        0.0
                    } else {
                        self.reversed(rng)
                    };
                    self.next_change_at = now + rng.in_range(*change_secs);
                }
                self.strafe(position, *weapon, *max_offset, speed_k, dt);
            }
            Motion::Dodge {
                weapon,
                change_secs,
                juke_secs,
                juke_chance,
                stop_chance,
                crouch_chance,
                jump_chance,
                max_offset,
            } => {
                let rules = DodgeRules {
                    weapon: *weapon,
                    change_secs: *change_secs,
                    juke_secs: *juke_secs,
                    juke_chance: *juke_chance,
                    stop_chance: *stop_chance,
                    crouch_chance: *crouch_chance,
                    jump_chance: *jump_chance,
                    max_offset: *max_offset,
                    speed_k,
                };
                if now >= self.next_change_at && !self.is_airborne() {
                    self.decide_dodge(&rules, rng, now);
                }
                self.dodge(position, &rules, humanoid, dt);
            }
            Motion::Drift {
                speed,
                change_secs,
                max_offset,
            } => {
                if now >= self.next_change_at {
                    let a = rng.next_f32() * std::f32::consts::TAU;
                    let up = if humanoid { 0.0 } else { a.sin() };
                    self.drift =
                        (self.lateral * a.cos() + Vec3::UP * up) * (rng.in_range(*speed) * speed_k);
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

    pub(super) fn is_airborne(&self) -> bool {
        self.rise != 0.0 || self.lift > 0.0
    }

    /// The direction a moving decision picks: a reversal, or a random side from a stop.
    pub(super) fn reversed(&self, rng: &mut Rng) -> f32 {
        if self.intent == 0.0 {
            rng.sign()
        } else {
            -self.intent
        }
    }

    /// Ground A/D movement towards `intent` with the weapon's VALORANT physics.
    fn strafe(
        &mut self,
        position: &mut Vec3,
        weapon: Weapon,
        max_offset: f32,
        speed_k: f32,
        dt: f32,
    ) {
        let offset = (*position - self.anchor).dot(self.lateral);
        let mut profile = weapon.movement();
        // Scaling run speed and acceleration together keeps the strafe's timing.
        profile.run_speed *= speed_k;
        profile.acceleration *= speed_k;
        // Reverse early enough that the counter-strafe braking distance keeps the target
        // inside its lane.
        let braking = self.strafe.right.abs() * STRAFE_BRAKE_SECS;
        if self.intent != 0.0 && offset * self.intent + braking >= max_offset {
            self.intent = -self.intent;
        }
        let input = MoveInput {
            right: self.intent,
            ..Default::default()
        };
        self.strafe = movement::step(&profile, self.strafe, input, dt);
        *position += self.lateral * (self.strafe.right * dt);
    }

    /// One grounded dodge decision: jump, crouch in place, stop, or move (reversing).
    fn decide_dodge(&mut self, r: &DodgeRules, rng: &mut Rng, now: f32) {
        let roll = rng.next_f32();
        self.crouching = false;
        if roll < r.jump_chance {
            // Take-off speed that, with the matching gravity, reaches the apex height and
            // lands after the air time.
            self.rise = 4.0 * JUMP_APEX_M / JUMP_AIR_SECS;
        } else if roll < r.jump_chance + r.crouch_chance {
            self.intent = 0.0;
            self.crouching = true;
        } else if roll < r.jump_chance + r.crouch_chance + r.stop_chance {
            self.intent = 0.0;
        } else {
            self.intent = self.reversed(rng);
        }
        let wait = if rng.next_f32() < r.juke_chance {
            r.juke_secs
        } else {
            r.change_secs
        };
        self.next_change_at = now + rng.in_range(wait);
    }

    fn dodge(&mut self, position: &mut Vec3, r: &DodgeRules, humanoid: bool, dt: f32) {
        let goal = if self.crouching { 1.0 } else { 0.0 };
        let rate = dt / CROUCH_TRANSITION_SECS;
        self.crouch += (goal - self.crouch).clamp(-rate, rate);

        if self.is_airborne() {
            // No air control: the target keeps its take-off speed, stopping at the lane edge.
            let gravity = 8.0 * JUMP_APEX_M / (JUMP_AIR_SECS * JUMP_AIR_SECS);
            self.lift += self.rise * dt - 0.5 * gravity * dt * dt;
            self.rise -= gravity * dt;
            if self.lift <= 0.0 {
                self.lift = 0.0;
                self.rise = 0.0;
            }
            let next = (*position - self.anchor).dot(self.lateral) + self.strafe.right * dt;
            if next.abs() >= r.max_offset {
                self.strafe.right = 0.0;
            }
            *position += self.lateral * (self.strafe.right * dt);
        } else {
            self.strafe(position, r.weapon, r.max_offset, r.speed_k, dt);
        }

        position.y = self.anchor.y + self.lift;
        if !humanoid {
            // Spheres have no hitbox to compress: they drop by the crouch height instead.
            position.y -= CROUCH_HEIGHT_DROP_M * self.crouch;
            position.y = position.y.max(MIN_SPHERE_HEIGHT);
        }
    }
}
