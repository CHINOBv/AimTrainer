//! Weapon handling: the fire-rate gate, the shot direction with movement spread, and the
//! ray cast that finds which target a shot hits.

use super::def::Shape;
use super::hit::{HitZone, Ray, ray_humanoid, ray_sphere};
use super::session::Target;
use crate::math::{Vec3, ViewAngles};
use crate::movement::MovementProfile;
use crate::rng::Rng;
use crate::valorant::{HumanoidDims, WeaponStats};

/// Whether enough time has passed since `last_shot_at` to fire again at time `now`.
pub(super) fn can_fire(weapon: &WeaponStats, now: f32, last_shot_at: f32) -> bool {
    now - last_shot_at >= 1.0 / weapon.fire_rate
}

/// The direction a shot travels and whether the player was moving too fast to be accurate.
/// With `simulate_spread`, the direction is sampled inside the weapon's spread cone, which
/// widens with the player's speed above the accuracy limit.
pub(super) fn shot_direction(
    view: ViewAngles,
    speed: f32,
    profile: &MovementProfile,
    weapon: &WeaponStats,
    simulate_spread: bool,
    rng: &mut Rng,
) -> (Vec3, bool) {
    let limit = profile.accuracy_speed_limit();
    let moving = speed > limit;
    let mut dir = view.direction();
    if simulate_spread {
        let mut spread = weapon.first_shot_spread_deg;
        if moving {
            let run = profile.run_speed;
            let t = ((speed - limit) / (run - limit).max(f32::EPSILON)).clamp(0.0, 1.0);
            spread += weapon.run_spread_deg * t;
        }
        // Uniform sample inside the spread cone's disk.
        let r = spread * rng.next_f32().sqrt();
        let phi = rng.next_f32() * std::f32::consts::TAU;
        let v = view.rotated(r * phi.cos(), r * phi.sin());
        dir = v.direction();
    }
    (dir, moving)
}

/// The index and zone of the nearest target visible at `now` that `ray` hits.
pub(super) fn raycast(
    ray: Ray,
    targets: &[Target],
    shape: &Shape,
    dims: &HumanoidDims,
    now: f32,
) -> Option<(usize, HitZone)> {
    targets
        .iter()
        .enumerate()
        .filter(|(_, t)| t.is_visible(now))
        .filter_map(|(i, t)| {
            let hit = match *shape {
                Shape::Sphere { radius } => {
                    ray_sphere(ray, t.position, radius).map(|d| (d, HitZone::Sphere))
                }
                Shape::Humanoid { .. } => ray_humanoid(ray, t.position, t.facing_yaw_deg, dims),
            };
            hit.map(|(d, z)| (d, i, z))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, i, z)| (i, z))
}
