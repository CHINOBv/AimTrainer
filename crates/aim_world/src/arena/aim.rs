//! Bot aim: the same Fitts-law model as `aim_core::sim::SimAimer` (reaction delay, then a
//! minimum-jerk flick whose duration follows Fitts' law and whose endpoint misses by a
//! Gaussian error scaled to the target's angular size), re-implemented for a 3D target that
//! moves and is occluded by map geometry.

use aim_core::math::{Vec3, ViewAngles, angle_between_deg, wrap_degrees};
use aim_core::rng::Rng;
use aim_core::sim::AimerProfile;

/// One planned flick from `from` to the goal plus a fixed endpoint error.
#[derive(Debug, Clone, Copy)]
struct Plan {
    start_at: f32,
    duration: f32,
    from: ViewAngles,
    error: (f32, f32),
}

/// What a bot is aiming at: its eye, the target point and the target's angular radius.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Sight {
    pub eye: Vec3,
    pub point: Vec3,
    pub radius_deg: f32,
}

/// A bot's crosshair and its current flick.
#[derive(Debug, Clone)]
pub(crate) struct BotAim {
    pub view: ViewAngles,
    plan: Option<Plan>,
}

/// Standard normal sample (Box-Muller), like `SimAimer`.
pub(crate) fn gaussian(rng: &mut Rng) -> f32 {
    let u1 = rng.next_f32().max(1e-6);
    let u2 = rng.next_f32();
    (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
}

/// Angular radius in degrees of a target of half-size `half_size` meters at `distance`.
pub(crate) fn angular_radius_deg(half_size: f32, distance: f32) -> f32 {
    (half_size / distance.max(0.01)).atan().to_degrees()
}

impl BotAim {
    pub fn new(view: ViewAngles) -> Self {
        Self { view, plan: None }
    }

    pub fn is_engaged(&self) -> bool {
        self.plan.is_some()
    }

    /// Forget the target (lost sight, or the target died).
    pub fn disengage(&mut self) {
        self.plan = None;
    }

    /// Plan a flick toward the sight point that starts `reaction` seconds after `now`.
    pub fn plan(
        &mut self,
        now: f32,
        reaction: f32,
        sight: Sight,
        profile: &AimerProfile,
        rng: &mut Rng,
    ) {
        let distance = angle_between_deg(self.view.direction(), sight.point - sight.eye);
        let r = sight.radius_deg.max(0.01);
        let duration = profile.fitts_a + profile.fitts_b * (distance / (2.0 * r) + 1.0).log2();
        let sd = profile.endpoint_noise * r;
        let error = (gaussian(rng) * sd, gaussian(rng) * sd);
        self.plan = Some(Plan {
            start_at: now + reaction,
            duration: duration.max(0.01),
            from: self.view,
            error,
        });
    }

    /// Move the crosshair along the current flick toward the sight point (which may have
    /// moved). Returns true once the flick has landed.
    pub fn track(&mut self, now: f32, sight: Sight) -> bool {
        let Some(plan) = self.plan else {
            return false;
        };
        let goal = ViewAngles::from_direction(sight.point - sight.eye);
        let tau = ((now - plan.start_at) / plan.duration).clamp(0.0, 1.0);
        // Minimum-jerk profile, as in `SimAimer`.
        let s = tau * tau * tau * (10.0 - 15.0 * tau + 6.0 * tau * tau);
        let dy = wrap_degrees(goal.yaw_deg + plan.error.0 - plan.from.yaw_deg);
        let dp = goal.pitch_deg + plan.error.1 - plan.from.pitch_deg;
        self.view = plan.from.rotated(dy * s, dp * s);
        tau >= 1.0
    }
}
