//! Target behaviour layered on top of motion: reactions to the player
//! (`targets.reactions`: strafe mirroring, crosshair dodges, shot jukes) and conditional
//! behaviour rules (`targets.behaviors`: switch motion, speed or retreat on conditions).
//!
//! Everything here is a pure function of the target's state, what it perceives this frame
//! and the session's seeded RNG, which it draws from only when the matching option is on.

use super::def::{BehaviorRule, BehaviorWhen, DamageJuke, Motion, ReactionsDef};
use super::motion::{MotionState, STRAFE_BRAKE_SECS};
use crate::games::valorant::Weapon;
use crate::math::Vec3;
use crate::movement::{self, LocalVelocity, MoveInput};
use crate::rng::Rng;

/// Player speed along a target's lateral axis, m/s, that the target notices as a strafe.
/// An estimate: about a fifth of the Vandal's 5.4 m/s run speed, which the player reaches
/// within a few hundredths of a second of pressing A or D, so the reaction starts almost
/// with the key press while slow drifting is ignored.
pub(super) const STRAFE_NOTICE_SPEED: f32 = 1.0;

/// What a target perceives in one frame; built by the session.
#[derive(Debug, Clone, Copy)]
pub(super) struct Situation {
    /// Session time, seconds.
    pub(super) now: f32,
    /// Remaining over starting health (NaN for unkillable targets, so `health_below`
    /// never holds).
    pub(super) health_fraction: f32,
    /// Seconds since the target last took damage (`inf` before the first).
    pub(super) since_hit_secs: f32,
    /// Seconds since the target became visible.
    pub(super) alive_secs: f32,
    /// Angle between the crosshair and the target's aim point, degrees.
    pub(super) crosshair_deg: f32,
    /// The target's offset from the crosshair along its lateral axis, at its distance,
    /// meters: positive when the target is on the `+lateral` side of the crosshair.
    pub(super) crosshair_offset: f32,
    /// The target's speed, m/s.
    pub(super) speed: f32,
    /// The player's velocity along the target's lateral axis, m/s.
    pub(super) player_lateral: f32,
}

/// Per-target behaviour state, owned by the target next to its [`MotionState`].
#[derive(Debug, Clone, PartialEq)]
pub(super) struct BehaviorState {
    /// Index of the behaviour rule in force, if any.
    active: Option<usize>,
    /// The player's strafe side last noticed along the lateral axis: -1, 0 or 1.
    player_side: f32,
    /// Whether the crosshair was inside `dodge_within_deg` last frame.
    near: bool,
    /// Horizontal unit vector away from the player at spawn.
    away: Vec3,
    /// Retreat velocity; `forward` is the speed away from the player, m/s.
    retreat: LocalVelocity,
    /// Meters retreated so far.
    retreated: f32,
    /// Damage taken towards a `damage_juke`, and when the last of it was taken.
    damage: f32,
    damaged_at: f32,
    /// Session time a triggered `damage_juke` happens.
    juke_at: Option<f32>,
    /// No new `damage_juke` starts before this session time.
    juke_ready_at: f32,
}

impl BehaviorState {
    /// A target that has not reacted yet, whose lateral axis is `lateral`.
    pub(super) fn new(lateral: Vec3) -> Self {
        Self {
            active: None,
            player_side: 0.0,
            near: false,
            // `lateral` is up × (towards the player), so up × lateral points away from him.
            away: Vec3::UP.cross(lateral),
            retreat: LocalVelocity::default(),
            retreated: 0.0,
            damage: 0.0,
            damaged_at: f32::NEG_INFINITY,
            juke_at: None,
            juke_ready_at: 0.0,
        }
    }

    /// Counts `amount` of damage taken at `now` towards a `damage_juke`, starting again from
    /// zero when the last damage is older than its `reset_secs`.
    pub(super) fn took_damage(&mut self, j: &DamageJuke, amount: f32, now: f32) {
        if now - self.damaged_at > j.reset_secs {
            self.damage = 0.0;
        }
        self.damage += amount;
        self.damaged_at = now;
    }

    /// Velocity added by retreating, m/s.
    pub(super) fn velocity(&self) -> Vec3 {
        self.away * self.retreat.forward
    }

    /// Picks the first rule whose conditions all hold and returns it. When the rule in
    /// force changes the motion, `motion` is handed over to the new one. An airborne target
    /// keeps its rule until it lands.
    pub(super) fn select<'a>(
        &mut self,
        rules: &'a [BehaviorRule],
        base: &'a Motion,
        s: &Situation,
        motion: &mut MotionState,
    ) -> Option<&'a BehaviorRule> {
        if !motion.is_airborne() {
            let next = rules.iter().position(|r| holds(&r.when, s));
            if next != self.active {
                let drives =
                    |i: Option<usize>| i.and_then(|i| rules[i].motion.as_ref()).unwrap_or(base);
                let (before, after) = (drives(self.active), drives(next));
                if before != after {
                    motion.switch_to(after, s.now);
                }
                self.active = next;
            }
        }
        self.active.map(|i| &rules[i])
    }

    /// Retreats straight away from the player while `retreat_m` (0 = no retreat) has not
    /// been covered, with the movement physics of `weapon` scaled by `speed_k`, moving
    /// `position` and the motion's anchor together so strafing continues further back.
    pub(super) fn retreat(
        &mut self,
        position: &mut Vec3,
        motion: &mut MotionState,
        retreat_m: f32,
        weapon: Weapon,
        speed_k: f32,
        dt: f32,
    ) {
        let mut profile = weapon.movement();
        profile.run_speed *= speed_k;
        profile.acceleration *= speed_k;
        // Stop pushing early enough that braking ends at `retreat_m`, as strafes do at their
        // lane edge.
        let braking = self.retreat.forward.abs() * STRAFE_BRAKE_SECS;
        let go = self.retreated + braking < retreat_m;
        let input = MoveInput {
            forward: if go { 1.0 } else { 0.0 },
            ..Default::default()
        };
        self.retreat = movement::step(&profile, self.retreat, input, dt);
        let step = self.retreat.forward * dt;
        *position += self.away * step;
        motion.anchor += self.away * step;
        self.retreated += step;
    }
}

/// Whether every condition set in `w` holds in situation `s`.
fn holds(w: &BehaviorWhen, s: &Situation) -> bool {
    w.health_below.is_none_or(|h| s.health_fraction < h)
        && w.damaged_within_secs.is_none_or(|d| s.since_hit_secs <= d)
        && w.alive_for_secs.is_none_or(|a| s.alive_secs >= a)
        && w.crosshair_within_deg.is_none_or(|c| s.crosshair_deg <= c)
        && w.speed_above.is_none_or(|v| s.speed > v)
}

/// The weapon whose movement physics a target moving with `motion` uses; motions without
/// a weapon move like the scenario default, the Vandal.
pub(super) fn motion_weapon(motion: &Motion) -> Weapon {
    match motion {
        Motion::Strafe { weapon, .. } | Motion::Dodge { weapon, .. } => *weapon,
        Motion::Static
        | Motion::Drift { .. }
        | Motion::Ballistic { .. }
        | Motion::LinearBounce { .. }
        | Motion::Orbit { .. }
        | Motion::Wave { .. } => Weapon::Vandal,
    }
}

/// Reacts to the player's strafes and crosshair this frame (shots go through
/// [`react_to_shot`]).
pub(super) fn react(
    r: &ReactionsDef,
    state: &mut BehaviorState,
    motion: &mut MotionState,
    s: &Situation,
    rng: &mut Rng,
) {
    let side = if s.player_lateral.abs() >= STRAFE_NOTICE_SPEED {
        s.player_lateral.signum()
    } else {
        0.0
    };
    let new_strafe = side != 0.0 && side != state.player_side;
    state.player_side = side;
    let near = s.crosshair_deg <= r.dodge_within_deg;
    let entered = r.dodge_within_deg > 0.0 && near && !state.near;
    state.near = near;
    if motion.is_airborne() {
        return;
    }

    let mirror_or_oppose = r.mirror_strafe_chance + r.oppose_strafe_chance;
    if new_strafe && mirror_or_oppose > 0.0 {
        let roll = rng.next_f32();
        if roll < r.mirror_strafe_chance {
            hold(r, motion, side, s.now, rng);
        } else if roll < mirror_or_oppose {
            hold(r, motion, -side, s.now, rng);
        }
    }
    if entered {
        let dir = away_from_crosshair(s, motion, rng);
        hold(r, motion, dir, s.now, rng);
    }
    if let Some(j) = &r.damage_juke {
        damage_juke(r, j, state, motion, s, rng);
    }
}

/// Triggers a `damage_juke` once the damage taken reaches its threshold (unless ignored or
/// cooling down), and performs it when its delay is over.
fn damage_juke(
    r: &ReactionsDef,
    j: &DamageJuke,
    state: &mut BehaviorState,
    motion: &mut MotionState,
    s: &Situation,
    rng: &mut Rng,
) {
    if state.juke_at.is_none() && state.damage >= j.threshold && s.now >= state.juke_ready_at {
        state.damage = 0.0;
        state.juke_ready_at = s.now + j.cooldown_secs;
        if rng.next_f32() >= j.ignore_chance {
            state.juke_at = Some(s.now + rng.in_range(j.delay_secs));
        }
    }
    if state.juke_at.is_some_and(|at| s.now >= at) {
        state.juke_at = None;
        let dir = if motion.intent != 0.0 {
            -motion.intent
        } else {
            away_from_crosshair(s, motion, rng)
        };
        hold(r, motion, dir, s.now, rng);
    }
}

/// Reacts to a shot the player just fired: with `shot_juke_chance`, reverse, or start moving
/// away from the crosshair when standing still.
pub(super) fn react_to_shot(
    r: &ReactionsDef,
    motion: &mut MotionState,
    s: &Situation,
    rng: &mut Rng,
) {
    if r.shot_juke_chance <= 0.0 || motion.is_airborne() || rng.next_f32() >= r.shot_juke_chance {
        return;
    }
    let dir = if motion.intent != 0.0 {
        -motion.intent
    } else {
        away_from_crosshair(s, motion, rng)
    };
    hold(r, motion, dir, s.now, rng);
}

/// The strafe direction that takes the target away from the crosshair; a random side, or a
/// reversal, when the crosshair is exactly in line with it.
fn away_from_crosshair(s: &Situation, motion: &MotionState, rng: &mut Rng) -> f32 {
    if s.crosshair_offset != 0.0 {
        s.crosshair_offset.signum()
    } else {
        motion.reversed(rng)
    }
}

/// Starts a reaction towards `dir` and holds it for a `react_secs` draw.
fn hold(r: &ReactionsDef, motion: &mut MotionState, dir: f32, now: f32, rng: &mut Rng) {
    motion.react(dir, now + rng.in_range(r.react_secs));
}
