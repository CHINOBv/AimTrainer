//! Ray casts against target shapes.

use crate::math::Vec3;
use crate::valorant::HumanoidDims;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HitZone {
    Head,
    Body,
    Legs,
    Sphere,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub origin: Vec3,
    /// Unit direction.
    pub dir: Vec3,
}

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
}
