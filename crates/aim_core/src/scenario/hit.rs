//! Ray casts against target shapes.

use crate::games::valorant::HumanoidDims;
use crate::math::Vec3;
use serde::{Deserialize, Serialize};

/// Part of a target that was hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HitZone {
    /// Head hit.
    Head,
    /// Torso hit.
    Body,
    /// Leg hit.
    Legs,
    /// Sphere target hit.
    Sphere,
}

/// A ray with an origin and a direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    /// Start point in meters.
    pub origin: Vec3,
    /// Unit direction.
    pub dir: Vec3,
}

/// Distance along the ray to the nearest sphere intersection, if any.
pub fn ray_sphere(ray: Ray, center: Vec3, radius: f32) -> Option<f32> {
    let oc = ray.origin - center;
    let b = oc.dot(ray.dir);
    let c = oc.dot(oc) - radius * radius;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let sq = disc.sqrt();
    let t = if -b - sq >= 0.0 { -b - sq } else { -b + sq };
    (t >= 0.0).then_some(t)
}

/// Distance along the ray to the nearest intersection with the capsule around the
/// segment `a`–`b` with `radius`, if any.
pub fn ray_capsule(ray: Ray, a: Vec3, b: Vec3, radius: f32) -> Option<f32> {
    let axis = b - a;
    let len = axis.length();
    if len <= f32::EPSILON {
        return ray_sphere(ray, a, radius);
    }
    let u = axis * (1.0 / len);
    // Infinite cylinder around the axis: drop the axis component of the ray.
    let oc = ray.origin - a;
    let d_perp = ray.dir - u * ray.dir.dot(u);
    let o_perp = oc - u * oc.dot(u);
    let qa = d_perp.dot(d_perp);
    let qb = o_perp.dot(d_perp);
    let qc = o_perp.dot(o_perp) - radius * radius;
    let mut best: Option<f32> = None;
    let mut keep = |t: f32| {
        if t >= 0.0 && best.is_none_or(|b| t < b) {
            best = Some(t);
        }
    };
    if qa > 1e-12 {
        let disc = qb * qb - qa * qc;
        if disc >= 0.0 {
            let sq = disc.sqrt();
            for t in [(-qb - sq) / qa, (-qb + sq) / qa] {
                let along = (oc + ray.dir * t).dot(u);
                if (0.0..=len).contains(&along) {
                    keep(t);
                }
            }
        }
    }
    // The rounded end caps.
    for c in [a, b] {
        if let Some(t) = ray_sphere(ray, c, radius) {
            keep(t);
        }
    }
    best
}

/// Ray against an upright capsule centered on `center` (`radius`, total `height`), with
/// an optional head sphere (`radius`, center `offset` above `center`). The head wins when
/// both are hit at the same distance or the head is nearer.
pub fn ray_capsule_target(
    ray: Ray,
    center: Vec3,
    radius: f32,
    height: f32,
    head: Option<(f32, f32)>,
) -> Option<(f32, HitZone)> {
    let half = (height / 2.0 - radius).max(0.0);
    let body = ray_capsule(
        ray,
        center - Vec3::UP * half,
        center + Vec3::UP * half,
        radius,
    )
    .map(|t| (t, HitZone::Body));
    let head = head.and_then(|(r, offset)| {
        ray_sphere(ray, center + Vec3::UP * offset, r).map(|t| (t, HitZone::Head))
    });
    match (body, head) {
        (Some(b), Some(h)) => Some(if h.0 <= b.0 { h } else { b }),
        (b, h) => b.or(h),
    }
}

/// Distance along the ray to an axis-aligned box intersection, if any.
pub fn ray_aabb(ray: Ray, min: Vec3, max: Vec3) -> Option<f32> {
    let mut t0 = 0.0_f32;
    let mut t1 = f32::INFINITY;
    for (o, d, lo, hi) in [
        (ray.origin.x, ray.dir.x, min.x, max.x),
        (ray.origin.y, ray.dir.y, min.y, max.y),
        (ray.origin.z, ray.dir.z, min.z, max.z),
    ] {
        if d.abs() < 1e-9 {
            if o < lo || o > hi {
                return None;
            }
            continue;
        }
        let (mut a, mut b) = ((lo - o) / d, (hi - o) / d);
        if a > b {
            std::mem::swap(&mut a, &mut b);
        }
        t0 = t0.max(a);
        t1 = t1.min(b);
        if t0 > t1 {
            return None;
        }
    }
    Some(t0)
}

/// Hitbox boxes in the agent's local frame (feet at origin, facing +Z), as `(min, max, zone)`.
pub fn humanoid_boxes(d: &HumanoidDims) -> [(Vec3, Vec3, HitZone); 3] {
    let (hw, bw, bd) = (d.head_size / 2.0, d.body_width / 2.0, d.body_depth / 2.0);
    let neck = d.height - d.head_size;
    let lw = bw * 0.8;
    [
        (
            Vec3::new(-hw, neck, -hw),
            Vec3::new(hw, d.height, hw),
            HitZone::Head,
        ),
        (
            Vec3::new(-bw, d.waist_height, -bd),
            Vec3::new(bw, neck, bd),
            HitZone::Body,
        ),
        (
            Vec3::new(-lw, 0.0, -bd),
            Vec3::new(lw, d.waist_height, bd),
            HitZone::Legs,
        ),
    ]
}

/// Ray against an agent standing at `feet`, rotated `facing_yaw_deg` around Y.
pub fn ray_humanoid(
    ray: Ray,
    feet: Vec3,
    facing_yaw_deg: f32,
    dims: &HumanoidDims,
) -> Option<(f32, HitZone)> {
    // Move the ray into the agent's local frame.
    let (s, c) = (-facing_yaw_deg).to_radians().sin_cos();
    let rot = |v: Vec3| Vec3::new(c * v.x + s * v.z, v.y, -s * v.x + c * v.z);
    let local = Ray {
        origin: rot(ray.origin - feet),
        dir: rot(ray.dir),
    };
    humanoid_boxes(dims)
        .into_iter()
        .filter_map(|(min, max, zone)| ray_aabb(local, min, max).map(|t| (t, zone)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ray(origin: Vec3, target: Vec3) -> Ray {
        Ray {
            origin,
            dir: (target - origin).normalize_or_zero(),
        }
    }

    #[test]
    fn sphere_hit_and_miss() {
        let c = Vec3::new(0.0, 0.0, -10.0);
        assert!((ray_sphere(ray(Vec3::ZERO, c), c, 0.5).unwrap() - 9.5).abs() < 1e-4);
        let off = Ray {
            origin: Vec3::ZERO,
            dir: Vec3::new(0.0, 0.1, -1.0).normalize_or_zero(),
        };
        assert!(ray_sphere(off, c, 0.5).is_none());
    }

    #[test]
    fn humanoid_zones_by_height() {
        let d = HumanoidDims::default();
        let feet = Vec3::new(0.0, 0.0, -15.0);
        let eye = Vec3::new(0.0, d.eye_height, 0.0);
        let zone_at =
            |h: f32| ray_humanoid(ray(eye, Vec3::new(0.0, h, -15.0)), feet, 0.0, &d).map(|x| x.1);
        assert_eq!(zone_at(d.head_center_height()), Some(HitZone::Head));
        assert_eq!(zone_at(1.4), Some(HitZone::Body));
        assert_eq!(zone_at(0.5), Some(HitZone::Legs));
        assert_eq!(zone_at(2.3), None);
    }

    #[test]
    fn level_crosshair_from_equal_height_hits_the_head() {
        let d = HumanoidDims::default();
        let eye = Vec3::new(0.0, d.eye_height, 0.0);
        let r = Ray {
            origin: eye,
            dir: Vec3::FORWARD,
        };
        let hit = ray_humanoid(r, Vec3::new(0.0, 0.0, -20.0), 37.0, &d);
        assert_eq!(hit.map(|h| h.1), Some(HitZone::Head));
    }

    #[test]
    fn capsule_distances_cover_the_cylinder_and_the_caps() {
        let (a, b) = (Vec3::new(0.0, 0.0, -10.0), Vec3::new(0.0, 1.0, -10.0));
        let side = ray(Vec3::new(0.0, 0.5, 0.0), Vec3::new(0.0, 0.5, -10.0));
        assert!((ray_capsule(side, a, b, 0.5).unwrap() - 9.5).abs() < 1e-4);
        // Straight down onto the top cap.
        let down = Ray {
            origin: Vec3::new(0.0, 5.0, -10.0),
            dir: Vec3::new(0.0, -1.0, 0.0),
        };
        assert!((ray_capsule(down, a, b, 0.5).unwrap() - 3.5).abs() < 1e-4);
        let past = ray(Vec3::new(0.0, 0.5, 0.0), Vec3::new(0.6, 0.5, -10.0));
        assert!(ray_capsule(past, a, b, 0.5).is_none());
        // A head in front of the body wins.
        let head = ray_capsule_target(side, a, 0.5, 2.0, Some((0.2, 0.5)));
        assert_eq!(head.map(|h| h.1), Some(HitZone::Body), "the body is nearer");
        let level = ray(Vec3::new(0.0, 1.9, 0.0), Vec3::new(0.0, 1.9, -10.0));
        let hit = ray_capsule_target(level, a, 0.5, 2.0, Some((0.2, 1.9)));
        assert_eq!(hit.map(|h| h.1), Some(HitZone::Head));
    }
}
