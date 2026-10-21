//! Ground movement model calibrated to VALORANT measurements.
//!
//! Deceleration is not a guessed friction formula: it replays the stopping curves a Riot
//! designer measured (speed fraction over time after releasing a key, and after
//! counter-strafing). Partial states resume on the curve where their speed sits, so
//! tapping, releasing mid-acceleration, and counter-strafing all decelerate consistently.

use crate::math::Vec3;
use serde::{Deserialize, Serialize};

/// Monotonically decreasing speed curve: `(seconds, fraction_of_run_speed)`, starting at
/// `(0, 1)` and ending at `(_, 0)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StopCurve(pub Vec<(f32, f32)>);

impl StopCurve {
    /// Whether the curve starts at `(0, 1)`, ends at fraction 0 and is strictly increasing in time and non-increasing in speed.
    pub fn is_valid(&self) -> bool {
        let p = &self.0;
        p.len() >= 2
            && p.first() == Some(&(0.0, 1.0))
            && p.last().is_some_and(|l| l.1 == 0.0)
            && p.windows(2).all(|w| w[1].0 > w[0].0 && w[1].1 <= w[0].1)
    }

    /// Time at which the curve reaches `fraction`.
    fn time_at(&self, fraction: f32) -> f32 {
        let f = fraction.clamp(0.0, 1.0);
        for w in self.0.windows(2) {
            let ((t0, f0), (t1, f1)) = (w[0], w[1]);
            if f <= f0 && f >= f1 {
                if f0 == f1 {
                    return t0;
                }
                return t0 + (f0 - f) / (f0 - f1) * (t1 - t0);
            }
        }
        self.duration()
    }

    fn fraction_at(&self, t: f32) -> f32 {
        for w in self.0.windows(2) {
            let ((t0, f0), (t1, f1)) = (w[0], w[1]);
            if t <= t1 {
                return f0 + (f1 - f0) * ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
            }
        }
        0.0
    }

    /// Total time in seconds the curve takes to reach a full stop.
    pub fn duration(&self) -> f32 {
        self.0.last().map_or(0.0, |l| l.0)
    }

    /// Advance a speed fraction along the curve by `dt` seconds.
    pub fn advance(&self, fraction: f32, dt: f32) -> f32 {
        self.fraction_at(self.time_at(fraction) + dt)
    }
}

/// Ground movement parameters of one weapon loadout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MovementProfile {
    /// Max running speed in m/s (depends on the equipped weapon).
    pub run_speed: f32,
    /// Walk (shift) speed as a fraction of run speed.
    pub walk_fraction: f32,
    /// Ground acceleration in m/s².
    pub acceleration: f32,
    /// Speed decay after releasing all movement keys on an axis.
    pub release_curve: StopCurve,
    /// Speed decay while pressing the opposite direction (counter-strafe).
    pub counter_strafe_curve: StopCurve,
    /// Shots are fully accurate at or below this fraction of `run_speed`.
    pub accuracy_threshold: f32,
}

impl MovementProfile {
    /// VALORANT rifle movement (Vandal/Phantom: 5.4 m/s). See `docs/valorant-mechanics.md`.
    pub fn valorant_rifle() -> Self {
        Self::valorant_with_speed(5.4)
    }

    /// VALORANT knife movement (6.75 m/s).
    pub fn valorant_knife() -> Self {
        Self::valorant_with_speed(6.75)
    }

    /// Movement profile with VALORANT acceleration and stopping curves at the given run speed in m/s.
    pub fn valorant_with_speed(run_speed: f32) -> Self {
        Self {
            run_speed,
            walk_fraction: 0.6,
            acceleration: 18.75,
            release_curve: StopCurve(vec![(0.0, 1.0), (0.065, 0.61), (0.110, 0.25), (0.170, 0.0)]),
            counter_strafe_curve: StopCurve(vec![
                (0.0, 1.0),
                (0.055, 0.61),
                (0.104, 0.25),
                (0.160, 0.0),
            ]),
            accuracy_threshold: 0.275,
        }
    }

    /// Speed in m/s at or below which shots are fully accurate.
    pub fn accuracy_speed_limit(&self) -> f32 {
        self.run_speed * self.accuracy_threshold
    }

    /// Whether every field is within its allowed range.
    pub fn is_valid(&self) -> bool {
        self.run_speed > 0.0
            && self.acceleration > 0.0
            && (0.0..=1.0).contains(&self.walk_fraction)
            && (0.0..=1.0).contains(&self.accuracy_threshold)
            && self.release_curve.is_valid()
            && self.counter_strafe_curve.is_valid()
    }
}

/// What the player is pressing this frame, in the player's local frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct MoveInput {
    /// -1 (A) .. 1 (D).
    pub right: f32,
    /// -1 (S) .. 1 (W).
    pub forward: f32,
    /// Walk (shift) held: moves at the walk fraction of run speed.
    pub walk: bool,
    /// Crouch held: moves at the crouch fraction of run speed (slower than walking).
    #[serde(default)]
    pub crouch: bool,
}

/// Horizontal velocity in the player's local frame (x = right, y = forward), in m/s.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LocalVelocity {
    /// Velocity along the player's right axis, in m/s.
    pub right: f32,
    /// Velocity along the player's forward axis, in m/s.
    pub forward: f32,
}

impl LocalVelocity {
    /// Magnitude of the horizontal velocity in m/s.
    pub fn speed(&self) -> f32 {
        (self.right * self.right + self.forward * self.forward).sqrt()
    }

    /// Convert to world space given the camera yaw in degrees.
    pub fn to_world(&self, yaw_deg: f32) -> Vec3 {
        let (s, c) = yaw_deg.to_radians().sin_cos();
        let forward = Vec3::new(s, 0.0, -c);
        let right = Vec3::new(c, 0.0, s);
        right * self.right + forward * self.forward
    }

    /// Inverse of [`LocalVelocity::to_world`] for a horizontal world velocity.
    pub fn from_world(world: Vec3, yaw_deg: f32) -> Self {
        let (s, c) = yaw_deg.to_radians().sin_cos();
        let forward = Vec3::new(s, 0.0, -c);
        let right = Vec3::new(c, 0.0, s);
        Self {
            right: world.dot(right),
            forward: world.dot(forward),
        }
    }
}

/// Advance velocity by one step. Axes are handled independently, like strafing in
/// tactical shooters; diagonal input is normalized so it never exceeds max speed.
pub fn step(
    profile: &MovementProfile,
    v: LocalVelocity,
    input: MoveInput,
    dt: f32,
) -> LocalVelocity {
    let max = profile.run_speed
        * if input.crouch {
            crate::games::valorant::CROUCH_SPEED_FRACTION
        } else if input.walk {
            profile.walk_fraction
        } else {
            1.0
        };
    let (mut wr, mut wf) = (input.right.clamp(-1.0, 1.0), input.forward.clamp(-1.0, 1.0));
    let len = (wr * wr + wf * wf).sqrt();
    if len > 1.0 {
        wr /= len;
        wf /= len;
    }
    LocalVelocity {
        right: step_axis(profile, v.right, wr * max, dt),
        forward: step_axis(profile, v.forward, wf * max, dt),
    }
}

fn step_axis(p: &MovementProfile, v: f32, target: f32, dt: f32) -> f32 {
    let run = p.run_speed;
    let decay = |curve: &StopCurve, v: f32, floor: f32| {
        let f = curve.advance(v.abs() / run, dt) * run;
        v.signum() * f.max(floor)
    };

    if target == 0.0 {
        return decay(&p.release_curve, v, 0.0);
    }
    if v != 0.0 && v.signum() != target.signum() {
        // Counter-strafe: brake on the counter curve, carry any leftover time into acceleration.
        let curve = &p.counter_strafe_curve;
        let t_left = curve.duration() - curve.time_at(v.abs() / run);
        if dt < t_left {
            return decay(&p.counter_strafe_curve, v, 0.0);
        }
        let rest = dt - t_left;
        return target.signum() * (p.acceleration * rest).min(target.abs());
    }
    if v.abs() > target.abs() {
        // Slowing to a lower cap (e.g. started walking while running).
        return decay(&p.release_curve, v, target.abs());
    }
    target.signum() * (v.abs() + p.acceleration * dt).min(target.abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 1000.0;

    fn time_until(
        profile: &MovementProfile,
        mut v: LocalVelocity,
        input: MoveInput,
        done: impl Fn(LocalVelocity) -> bool,
    ) -> f32 {
        let mut t = 0.0;
        while !done(v) {
            v = step(profile, v, input, DT);
            t += DT;
            assert!(t < 5.0, "never reached condition");
        }
        t
    }

    fn running() -> LocalVelocity {
        LocalVelocity {
            right: 5.4,
            forward: 0.0,
        }
    }

    #[test]
    fn releasing_reaches_accuracy_threshold_and_stop_at_measured_times() {
        let p = MovementProfile::valorant_rifle();
        let limit = p.accuracy_speed_limit();
        let to_accurate = time_until(&p, running(), MoveInput::default(), |v| v.speed() <= limit);
        let to_stop = time_until(&p, running(), MoveInput::default(), |v| v.speed() == 0.0);
        // Riot measured 25% at 0.110 s; 27.5% lands slightly earlier.
        assert!((0.10..=0.110).contains(&to_accurate), "{to_accurate}");
        assert!((to_stop - 0.170).abs() < 2.0 * DT, "{to_stop}");
    }

    #[test]
    fn counter_strafing_stops_faster_then_accelerates_the_other_way() {
        let p = MovementProfile::valorant_rifle();
        let left = MoveInput {
            right: -1.0,
            ..Default::default()
        };
        let to_stop = time_until(&p, running(), left, |v| v.right <= 0.0);
        assert!((to_stop - 0.160).abs() < 2.0 * DT, "{to_stop}");
        let mut v = running();
        for _ in 0..1000 {
            v = step(&p, v, left, DT);
        }
        assert!((v.right + 5.4).abs() < 1e-4, "{v:?}");
    }

    #[test]
    fn acceleration_reaches_run_speed_in_about_0_29_s() {
        let p = MovementProfile::valorant_rifle();
        let right = MoveInput {
            right: 1.0,
            ..Default::default()
        };
        let t = time_until(&p, LocalVelocity::default(), right, |v| v.right >= 5.4);
        assert!((t - 5.4 / 18.75).abs() < 2.0 * DT, "{t}");
    }

    #[test]
    fn crouching_caps_speed_below_walking() {
        use crate::games::valorant::CROUCH_SPEED_FRACTION;
        let p = MovementProfile::valorant_rifle();
        let top = |input: MoveInput| {
            let mut v = running();
            for _ in 0..1000 {
                v = step(&p, v, input, DT);
            }
            v.right
        };
        let crouch = MoveInput {
            right: 1.0,
            crouch: true,
            ..Default::default()
        };
        assert!((top(crouch) - 5.4 * CROUCH_SPEED_FRACTION).abs() < 1e-4);
        // Crouching wins over walking: the slower of the two.
        let both = MoveInput {
            walk: true,
            ..crouch
        };
        assert!((top(both) - 5.4 * CROUCH_SPEED_FRACTION).abs() < 1e-4);
        assert!(CROUCH_SPEED_FRACTION < p.walk_fraction);
    }

    #[test]
    fn walking_caps_speed_and_diagonals_do_not_exceed_max() {
        let p = MovementProfile::valorant_rifle();
        let mut v = running();
        let walk = MoveInput {
            right: 1.0,
            walk: true,
            ..Default::default()
        };
        for _ in 0..1000 {
            v = step(&p, v, walk, DT);
        }
        assert!((v.right - 5.4 * 0.6).abs() < 1e-4);

        let mut v = LocalVelocity::default();
        let diag = MoveInput {
            right: 1.0,
            forward: 1.0,
            walk: false,
            crouch: false,
        };
        for _ in 0..1000 {
            v = step(&p, v, diag, DT);
        }
        assert!((v.speed() - 5.4).abs() < 1e-3, "{}", v.speed());
    }

    #[test]
    fn local_world_conversion_round_trips() {
        let v = LocalVelocity {
            right: 1.5,
            forward: -2.0,
        };
        for yaw in [0.0, 37.0, -120.0] {
            let back = LocalVelocity::from_world(v.to_world(yaw), yaw);
            assert!((back.right - v.right).abs() < 1e-5 && (back.forward - v.forward).abs() < 1e-5);
        }
    }

    #[test]
    fn default_curves_are_valid() {
        assert!(MovementProfile::valorant_rifle().is_valid());
        assert!(MovementProfile::valorant_knife().is_valid());
    }
}
