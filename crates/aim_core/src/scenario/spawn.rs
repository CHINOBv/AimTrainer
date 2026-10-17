//! Target spawning: samples where a new target appears, keeps it apart from the live
//! ones, and orients it towards the player.

use super::MIN_SPHERE_HEIGHT;
use super::def::{Shape, SpawnArea, TargetsDef};
use super::session::Target;
use crate::math::{Range, Vec3, ViewAngles, angle_between_deg};
use crate::rng::Rng;
use crate::valorant::HumanoidDims;

const SPAWN_ATTEMPTS: usize = 24;

/// A spawn point for a new target, resampled up to [`SPAWN_ATTEMPTS`] times until it is at
/// least `min_separation_deg` away (seen from `eye`) from every existing target.
pub(super) fn separated_point(
    def: &TargetsDef,
    existing: &[Target],
    eye: Vec3,
    view: ViewAngles,
    dims: &HumanoidDims,
    rng: &mut Rng,
) -> Vec3 {
    let sep = def.min_separation_deg;
    let mut point = sample_point(def, eye, view, rng);
    for _ in 1..SPAWN_ATTEMPTS {
        let dir = aim_point(&def.shape, dims, point) - eye;
        let far_enough = existing
            .iter()
            .all(|t| angle_between_deg(dir, aim_point(&def.shape, dims, t.position) - eye) >= sep);
        if far_enough {
            break;
        }
        point = sample_point(def, eye, view, rng);
    }
    point
}

/// The yaw a target at `point` faces to look at the player, and its horizontal lateral axis.
pub(super) fn face_player(point: Vec3, player_pos: Vec3) -> (f32, Vec3) {
    let to_player = (player_pos - point).horizontal();
    let facing_yaw_deg = to_player.x.atan2(to_player.z).to_degrees();
    let lateral = Vec3::UP.cross(to_player).normalize_or_zero();
    let lateral = if lateral == Vec3::ZERO {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        lateral
    };
    (facing_yaw_deg, lateral)
}

fn sample_point(def: &TargetsDef, eye: Vec3, view: ViewAngles, rng: &mut Rng) -> Vec3 {
    let humanoid = matches!(def.shape, Shape::Humanoid { .. });
    let mut p = match def.spawn {
        SpawnArea::Angular {
            yaw_deg,
            pitch_deg,
            distance,
        } => {
            let yaw = rng.in_range(yaw_deg);
            let pitch = if humanoid {
                0.0
            } else {
                rng.in_range(pitch_deg)
            };
            eye + ViewAngles::new(yaw, pitch).direction() * rng.in_range(distance)
        }
        SpawnArea::AroundCrosshair {
            offset_deg,
            distance,
            pitch_limit_deg,
        } => {
            let offset = rng.in_range(offset_deg);
            let dist = rng.in_range(distance);
            if humanoid {
                let yaw = view.yaw_deg + offset * rng.sign();
                eye + ViewAngles::new(yaw, 0.0).direction() * dist
            } else {
                crosshair_sphere_point(eye, view, offset, dist, pitch_limit_deg, rng)
            }
        }
        SpawnArea::Floor { x, z, height } => {
            Vec3::new(rng.in_range(x), rng.in_range(height), rng.in_range(z))
        }
    };
    if humanoid {
        p.y = 0.0;
    } else {
        p.y = p.y.max(MIN_SPHERE_HEIGHT);
    }
    p
}

/// A point `offset` degrees away from the current aim, in a random direction, that stays
/// inside the pitch band and above the floor. Each spawn is relative to the previous aim,
/// so without the band chained flicks drift into the sky or the floor.
fn crosshair_sphere_point(
    eye: Vec3,
    view: ViewAngles,
    offset: f32,
    dist: f32,
    limit: Range,
    rng: &mut Rng,
) -> Vec3 {
    let fits = |v: ViewAngles| {
        let p = eye + v.direction() * dist;
        let in_band = (limit.min..=limit.max).contains(&v.pitch_deg);
        (in_band && p.y >= MIN_SPHERE_HEIGHT).then_some(p)
    };
    for _ in 0..SPAWN_ATTEMPTS {
        let phi = rng.next_f32() * std::f32::consts::TAU;
        let (d_yaw, d_pitch) = (offset * phi.cos(), offset * phi.sin());
        // Try the sampled direction, then its vertical mirror.
        for v in [view.rotated(d_yaw, d_pitch), view.rotated(d_yaw, -d_pitch)] {
            if let Some(p) = fits(v) {
                return p;
            }
        }
    }
    // The aim itself is outside the band (e.g. looking at the sky): flick horizontally
    // and bring the pitch back into the band.
    let yaw = view.yaw_deg + offset * rng.sign();
    let pitch = view.pitch_deg.clamp(limit.min, limit.max);
    let mut p = eye + ViewAngles::new(yaw, pitch).direction() * dist;
    p.y = p.y.max(MIN_SPHERE_HEIGHT);
    p
}

/// Where the player aims to separate targets: a humanoid's head, a sphere's center.
fn aim_point(shape: &Shape, dims: &HumanoidDims, p: Vec3) -> Vec3 {
    match shape {
        Shape::Humanoid { .. } => p + Vec3::new(0.0, dims.head_center_height(), 0.0),
        Shape::Sphere { .. } => p,
    }
}
