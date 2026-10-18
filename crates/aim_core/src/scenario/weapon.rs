//! Weapon handling: the fire-rate gate, the shot direction with movement spread and
//! recoil, and the ray cast that finds which target a shot hits.

use super::def::Shape;
use super::hit::{HitZone, Ray, ray_humanoid, ray_sphere};
use super::recoil::Kick;
use super::session::Target;
use crate::games::valorant::{HumanoidDims, WeaponStats};
use crate::math::{Vec3, ViewAngles};
use crate::movement::MovementProfile;
use crate::rng::Rng;

/// Whether enough time has passed since `last_shot_at` to fire again at time `now`.
pub(super) fn can_fire(weapon: &WeaponStats, now: f32, last_shot_at: f32) -> bool {
    now - last_shot_at >= 1.0 / weapon.fire_rate
}

/// The direction a shot travels and whether the player was moving too fast to be accurate.
/// With `simulate_spread`, the direction is sampled inside the weapon's spread cone, which
/// widens with the player's speed above the accuracy limit.
/// Shot direction with first-shot spread and running inaccuracy (VALORANT's accuracy
/// threshold); returns the direction and whether the shooter was moving too fast.
pub fn shot_direction(
    view: ViewAngles,
    speed: f32,
    profile: &MovementProfile,
    weapon: &WeaponStats,
    simulate_spread: bool,
    rng: &mut Rng,
) -> (Vec3, bool) {
    let moving = speed > profile.accuracy_speed_limit();
    if !simulate_spread {
        return (view.direction(), moving);
    }
    let spread = weapon.first_shot_spread_deg + run_spread(speed, profile, weapon);
    (sample_cone(view, spread, rng), moving)
}

/// Shot direction of one bullet of a recoil spray: the crosshair moved by the bullet's
/// recoil `kick`, sampled inside the bullet's spread cone (plus running inaccuracy when
/// `simulate_spread` is on). Returns the direction and whether the shooter was moving.
pub(super) fn recoil_direction(
    view: ViewAngles,
    kick: Kick,
    speed: f32,
    profile: &MovementProfile,
    weapon: &WeaponStats,
    simulate_spread: bool,
    rng: &mut Rng,
) -> (Vec3, bool) {
    let moving = speed > profile.accuracy_speed_limit();
    let mut spread = kick.spread_deg;
    if simulate_spread {
        spread += run_spread(speed, profile, weapon);
    }
    let aimed = view.rotated(kick.yaw_deg, kick.pitch_deg);
    (sample_cone(aimed, spread, rng), moving)
}

/// Extra spread for moving above the accuracy limit, growing linearly up to run speed.
fn run_spread(speed: f32, profile: &MovementProfile, weapon: &WeaponStats) -> f32 {
    let limit = profile.accuracy_speed_limit();
    if speed <= limit {
        return 0.0;
    }
    let run = profile.run_speed;
    let t = ((speed - limit) / (run - limit).max(f32::EPSILON)).clamp(0.0, 1.0);
    weapon.run_spread_deg * t
}

/// A direction sampled uniformly inside the disk of a cone of radius `spread_deg` around `view`.
fn sample_cone(view: ViewAngles, spread_deg: f32, rng: &mut Rng) -> Vec3 {
    let r = spread_deg * rng.next_f32().sqrt();
    let phi = rng.next_f32() * std::f32::consts::TAU;
    view.rotated(r * phi.cos(), r * phi.sin()).direction()
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
                Shape::Humanoid { .. } => ray_humanoid(
                    ray,
                    t.position,
                    t.facing_yaw_deg,
                    &dims.crouched(t.crouch()),
                ),
            };
            hit.map(|(d, z)| (d, i, z))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, i, z)| (i, z))
}

/// How far from the center of the zone it hit `ray` passes, as a fraction of the zone's
/// radius: the perpendicular distance from the sphere center, or from the head center
/// over half the head size. `None` for body and leg hits, which have no rings.
pub(super) fn zone_center_fraction(
    ray: Ray,
    target: &Target,
    shape: &Shape,
    dims: &HumanoidDims,
    zone: HitZone,
) -> Option<f32> {
    let (center, radius) = match (shape, zone) {
        (Shape::Sphere { radius }, _) => (target.position, *radius),
        (Shape::Humanoid { .. }, HitZone::Head) => {
            let d = dims.crouched(target.crouch());
            (
                target.position + Vec3::new(0.0, d.head_center_height(), 0.0),
                d.head_size / 2.0,
            )
        }
        _ => return None,
    };
    let to_center = center - ray.origin;
    let along = ray.dir * to_center.dot(ray.dir);
    Some((to_center - along).length() / radius.max(f32::EPSILON))
}
