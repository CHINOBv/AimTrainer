//! Simulated aimer used to test that scenarios are playable and to generate provisional
//! benchmark thresholds before real player data exists.
//!
//! Model (see `docs/training-science.md`):
//! - Clicking: after a reaction delay, a minimum-jerk movement towards the target whose
//!   duration follows Fitts' law, `MT = a + b·log2(D/W + 1)`, landing with Gaussian endpoint
//!   error proportional to the target's angular size. Misses trigger a corrective sub-movement.
//! - Tracking: a first-order lag on the target's direction (`gain` per second), which yields
//!   the velocity-dependent following error real players show.

use crate::math::{Vec3, ViewAngles, angle_between_deg, wrap_degrees};
use crate::rng::Rng;
use crate::scenario::{FireMode, FrameInput, ScenarioDef, Session, Shape, Target};

/// Skill parameters of a simulated aimer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AimerProfile {
    /// Preset name.
    pub name: &'static str,
    /// Delay in seconds before starting a movement towards a new target.
    pub reaction_secs: f32,
    /// Fitts' law intercept, seconds.
    pub fitts_a: f32,
    /// Fitts' law slope, seconds per bit.
    pub fitts_b: f32,
    /// Endpoint error standard deviation as a fraction of the target's angular radius.
    pub endpoint_noise: f32,
    /// Tracking gain in 1/s (higher follows tighter).
    pub tracking_gain: f32,
}

impl AimerProfile {
    /// Built-in skill levels, from weakest to strongest.
    pub const PRESETS: [AimerProfile; 4] = [
        AimerProfile {
            name: "recruit",
            reaction_secs: 0.32,
            fitts_a: 0.15,
            fitts_b: 0.20,
            endpoint_noise: 0.9,
            tracking_gain: 6.0,
        },
        AimerProfile {
            name: "contender",
            reaction_secs: 0.27,
            fitts_a: 0.12,
            fitts_b: 0.16,
            endpoint_noise: 0.7,
            tracking_gain: 9.0,
        },
        AimerProfile {
            name: "veteran",
            reaction_secs: 0.23,
            fitts_a: 0.10,
            fitts_b: 0.13,
            endpoint_noise: 0.5,
            tracking_gain: 13.0,
        },
        AimerProfile {
            name: "elite",
            reaction_secs: 0.20,
            fitts_a: 0.08,
            fitts_b: 0.10,
            endpoint_noise: 0.35,
            tracking_gain: 18.0,
        },
    ];
}

struct Plan {
    target: u64,
    start_at: f32,
    duration: f32,
    from: ViewAngles,
    error: (f32, f32),
    shot: bool,
}

/// A simulated player that produces `FrameInput`s for a `Session`.
pub struct SimAimer {
    profile: AimerProfile,
    rng: Rng,
    view: ViewAngles,
    plan: Option<Plan>,
}

impl SimAimer {
    /// Creates an aimer with the given skill and RNG seed.
    pub fn new(profile: AimerProfile, seed: u64) -> Self {
        Self {
            profile,
            rng: Rng::new(seed ^ 0xA1A1),
            view: ViewAngles::default(),
            plan: None,
        }
    }

    fn gaussian(&mut self) -> f32 {
        let u1 = self.rng.next_f32().max(1e-6);
        let u2 = self.rng.next_f32();
        (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
    }

    /// Point the aimer wants to hit and its angular radius (degrees).
    fn aim(session: &Session, t: &Target) -> (Vec3, f32) {
        let eye = session.eye_position();
        match session.def().targets.shape {
            Shape::Sphere { radius } => {
                let d = (t.position - eye).length();
                (t.position, (radius / d).atan().to_degrees())
            }
            Shape::Humanoid { .. } => {
                let dims = session.dims();
                let head = t.position + Vec3::new(0.0, dims.head_center_height(), 0.0);
                let d = (head - eye).length();
                (head, (dims.head_size / 2.0 / d).atan().to_degrees())
            }
        }
    }

    fn angles_to(session: &Session, p: Vec3) -> ViewAngles {
        ViewAngles::from_direction(p - session.eye_position())
    }

    fn nearest(&self, session: &Session) -> Option<Target> {
        let dir = self.view.direction();
        session
            .targets()
            .min_by(|a, b| {
                let da = angle_between_deg(dir, Self::aim(session, a).0 - session.eye_position());
                let db = angle_between_deg(dir, Self::aim(session, b).0 - session.eye_position());
                da.total_cmp(&db)
            })
            .cloned()
    }

    /// Produce this frame's input.
    pub fn input(&mut self, session: &Session, dt: f32) -> FrameInput {
        match session.def().weapon.fire {
            FireMode::Hold => self.track(session, dt),
            FireMode::Click => self.click(session, dt),
        }
    }

    fn track(&mut self, session: &Session, dt: f32) -> FrameInput {
        if let Some(t) = self.nearest(session) {
            let goal = Self::angles_to(session, Self::aim(session, &t).0);
            let k = 1.0 - (-self.profile.tracking_gain * dt).exp();
            let dy = wrap_degrees(goal.yaw_deg - self.view.yaw_deg);
            let dp = goal.pitch_deg - self.view.pitch_deg;
            self.view = self.view.rotated(dy * k, dp * k);
        }
        FrameInput {
            dt,
            view: self.view,
            fire_held: true,
            ..Default::default()
        }
    }

    fn click(&mut self, session: &Session, dt: f32) -> FrameInput {
        let now = session.time();
        let current = self
            .plan
            .as_ref()
            .and_then(|p| session.targets().find(|t| t.id == p.target).cloned());
        let target = match current {
            Some(t) => t,
            None => {
                self.plan = None;
                match self.nearest(session) {
                    Some(t) => t,
                    None => {
                        return FrameInput {
                            dt,
                            view: self.view,
                            ..Default::default()
                        };
                    }
                }
            }
        };

        let (point, radius) = Self::aim(session, &target);
        let goal = Self::angles_to(session, point);
        if self.plan.as_ref().is_none_or(|p| p.shot) {
            // Plan a (corrective) sub-movement. A correction reacts faster than a fresh target.
            let corrective = self.plan.is_some();
            let distance = angle_between_deg(self.view.direction(), goal.direction());
            let fitts = self.profile.fitts_a
                + self.profile.fitts_b * (distance / (2.0 * radius) + 1.0).log2();
            let sd = self.profile.endpoint_noise * radius;
            let error = (self.gaussian() * sd, self.gaussian() * sd);
            let reaction = if corrective {
                self.profile.reaction_secs * 0.5
            } else {
                self.profile.reaction_secs
            };
            let start_at = (target.visible_at.max(now - dt)) + reaction;
            self.plan = Some(Plan {
                target: target.id,
                start_at,
                duration: fitts,
                from: self.view,
                error,
                shot: false,
            });
        }

        let plan = self.plan.as_mut().expect("plan set above");
        let tau = ((now - plan.start_at) / plan.duration).clamp(0.0, 1.0);
        let s = tau * tau * tau * (10.0 - 15.0 * tau + 6.0 * tau * tau);
        let dy = wrap_degrees(goal.yaw_deg + plan.error.0 - plan.from.yaw_deg);
        let dp = goal.pitch_deg + plan.error.1 - plan.from.pitch_deg;
        self.view = plan.from.rotated(dy * s, dp * s);
        // With recoil on, every profile except the recruit waits for the gun to recover so
        // the shot is a fresh first bullet (the habit the burst drills teach).
        let recovering = self.profile.name != "recruit"
            && session.recoil_recovery_remaining().is_some_and(|r| r > 0.0);
        let fire = tau >= 1.0 && !recovering;
        if fire {
            plan.shot = true;
        }
        FrameInput {
            dt,
            view: self.view,
            fire_pressed: fire,
            fire_held: fire,
            ..Default::default()
        }
    }
}

/// Run a whole scenario with a simulated aimer at a fixed timestep.
pub fn simulate(def: &ScenarioDef, profile: AimerProfile, seed: u64) -> crate::scenario::RunResult {
    const DT: f32 = 1.0 / 240.0;
    let mut session = Session::new(def.clone(), seed);
    let mut aimer = SimAimer::new(profile, seed);
    while !session.is_finished() {
        let input = aimer.input(&session, DT);
        session.update(input);
    }
    session.result()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::builtin;

    #[test]
    fn every_builtin_is_playable_and_better_aimers_score_higher() {
        for def in builtin::all() {
            let scores: Vec<f32> = AimerProfile::PRESETS
                .iter()
                .map(|p| simulate(&def, *p, 11).score)
                .collect();
            assert!(
                scores[1] > 0.0,
                "{}: contender scored nothing ({scores:?})",
                def.id
            );
            assert!(
                scores[3] > scores[0],
                "{}: elite should beat recruit ({scores:?})",
                def.id
            );
        }
    }

    #[test]
    fn simulation_is_deterministic_per_seed() {
        let def = builtin::all()
            .into_iter()
            .find(|d| d.id == "micro-adjust")
            .unwrap();
        let p = AimerProfile::PRESETS[1];
        assert_eq!(simulate(&def, p, 5), simulate(&def, p, 5));
    }
}
