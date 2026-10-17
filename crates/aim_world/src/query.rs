//! Geometric queries against a map's solid boxes: rays, line of sight, head visibility and
//! collision.

use crate::map::MapDef;
use aim_core::games::valorant::HumanoidDims;
use aim_core::math::Vec3;
use aim_core::scenario::hit::{Ray, ray_aabb};

/// Tolerance for "already touching this face" in collision sweeps, in meters.
const CONTACT_EPS: f32 = 1e-4;

#[derive(Clone, Copy)]
enum Axis {
    X,
    Z,
}

impl Axis {
    fn get(self, v: Vec3) -> f32 {
        match self {
            Self::X => v.x,
            Self::Z => v.z,
        }
    }

    fn other(self) -> Self {
        match self {
            Self::X => Self::Z,
            Self::Z => Self::X,
        }
    }
}

impl MapDef {
    /// Nearest box hit by `ray` within `max_t` meters, as `(distance, box index)`. A ray
    /// that starts inside a box hits it at distance 0. The floor is not a box.
    pub fn raycast(&self, ray: Ray, max_t: f32) -> Option<(f32, usize)> {
        self.boxes
            .iter()
            .enumerate()
            .filter_map(|(i, b)| ray_aabb(ray, b.min, b.max).map(|t| (t, i)))
            .filter(|&(t, _)| t <= max_t)
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
    }

    /// True when the segment from `a` to `b` crosses no solid box. Touching a box exactly
    /// at the end point does not block.
    pub fn line_of_sight(&self, a: Vec3, b: Vec3) -> bool {
        let d = b - a;
        let len = d.length();
        if len <= f32::EPSILON {
            let inside = |bx: &crate::map::SolidBox| {
                (bx.min.x..=bx.max.x).contains(&a.x)
                    && (bx.min.y..=bx.max.y).contains(&a.y)
                    && (bx.min.z..=bx.max.z).contains(&a.z)
            };
            return !self.boxes.iter().any(inside);
        }
        let ray = Ray {
            origin: a,
            dir: d * (1.0 / len),
        };
        self.raycast(ray, len - CONTACT_EPS).is_none()
    }

    /// True when any part of a standing enemy's head (feet at `target_feet`, default
    /// [`HumanoidDims`]) is in line of sight from `eye`. Samples the head center and four
    /// points near its sides.
    pub fn visible_head(&self, eye: Vec3, target_feet: Vec3) -> bool {
        let dims = HumanoidDims::default();
        let center = target_feet + Vec3::new(0.0, dims.head_center_height(), 0.0);
        let o = dims.head_size / 2.0 * 0.9;
        [
            Vec3::ZERO,
            Vec3::new(o, 0.0, 0.0),
            Vec3::new(-o, 0.0, 0.0),
            Vec3::new(0.0, 0.0, o),
            Vec3::new(0.0, 0.0, -o),
        ]
        .iter()
        .any(|&off| self.line_of_sight(eye, center + off))
    }

    /// Move a standing player's feet by the horizontal part of `delta`, sliding along
    /// boxes and the floor edges. The body is a vertical box `2 * radius` wide and as tall
    /// as [`HumanoidDims::height`]. Each axis is swept separately (X, then Z), so the player
    /// stops at the first face in its way and keeps the motion along that face, like the
    /// trainer's bounds sliding. The vertical part of `delta` is ignored: the floor is flat.
    pub fn move_with_collision(&self, feet: Vec3, delta: Vec3, radius: f32) -> Vec3 {
        let mut p = feet;
        p.x = self.sweep(p, delta.x, radius, Axis::X);
        p.z = self.sweep(p, delta.z, radius, Axis::Z);
        p.x = clamp_inside(p.x, self.floor_x.min + radius, self.floor_x.max - radius);
        p.z = clamp_inside(p.z, self.floor_z.min + radius, self.floor_z.max - radius);
        p
    }

    /// Index of a box the player body at `feet` overlaps, if any.
    pub fn body_overlaps(&self, feet: Vec3, radius: f32) -> Option<usize> {
        let height = HumanoidDims::default().height;
        self.boxes.iter().position(|b| {
            feet.x > b.min.x - radius
                && feet.x < b.max.x + radius
                && feet.z > b.min.z - radius
                && feet.z < b.max.z + radius
                && b.min.y < feet.y + height
                && b.max.y > feet.y
        })
    }

    /// True when a player of `radius` can walk in a straight line from `a` to `b`.
    pub fn walkable(&self, a: Vec3, b: Vec3, radius: f32) -> bool {
        let d = (b - a).horizontal();
        let len = d.length();
        if len <= f32::EPSILON {
            return self.body_overlaps(a, radius).is_none();
        }
        let ray = Ray {
            origin: a,
            dir: d * (1.0 / len),
        };
        let height = HumanoidDims::default().height;
        !self.boxes.iter().any(|bx| {
            if !(bx.min.y < a.y + height && bx.max.y > a.y) {
                return false;
            }
            // Inflate horizontally by the radius and make the box unbounded on Y so only
            // the footprint matters.
            let min = Vec3::new(bx.min.x - radius, -1e9, bx.min.z - radius);
            let max = Vec3::new(bx.max.x + radius, 1e9, bx.max.z + radius);
            ray_aabb(ray, min, max).is_some_and(|t| t < len)
        })
    }

    /// Final coordinate on `axis` after moving `d` from `p`, stopped at the first face.
    fn sweep(&self, p: Vec3, d: f32, radius: f32, axis: Axis) -> f32 {
        let start = axis.get(p);
        let mut target = start + d;
        if d == 0.0 {
            return start;
        }
        let height = HumanoidDims::default().height;
        let o = axis.other();
        for b in &self.boxes {
            let side = axis.other().get(p);
            let overlaps_other = side > o.get(b.min) - radius && side < o.get(b.max) + radius;
            let overlaps_vertical = b.min.y < p.y + height && b.max.y > p.y;
            if !(overlaps_other && overlaps_vertical) {
                continue;
            }
            if d > 0.0 {
                let face = axis.get(b.min) - radius;
                if start <= face + CONTACT_EPS && target > face {
                    target = face;
                }
            } else {
                let face = axis.get(b.max) + radius;
                if start >= face - CONTACT_EPS && target < face {
                    target = face;
                }
            }
        }
        target
    }
}

/// Clamp that tolerates a range narrower than the body (then it centers).
fn clamp_inside(v: f32, lo: f32, hi: f32) -> f32 {
    if lo > hi {
        (lo + hi) / 2.0
    } else {
        v.clamp(lo, hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{BoxKind, SolidBox};
    use aim_core::math::Range;

    /// A 20 x 20 m floor with one wall across X at z in [-0.2, 0.2], x in [-5, 5].
    fn walled() -> MapDef {
        MapDef {
            id: "t".into(),
            name: "T".into(),
            description: String::new(),
            floor_x: Range {
                min: -10.0,
                max: 10.0,
            },
            floor_z: Range {
                min: -10.0,
                max: 10.0,
            },
            boxes: vec![SolidBox {
                kind: BoxKind::Wall,
                min: Vec3::new(-5.0, 0.0, -0.2),
                max: Vec3::new(5.0, 3.0, 0.2),
            }],
            spawns: vec![],
            angles: vec![],
            waypoints: vec![],
        }
    }

    #[test]
    fn line_of_sight_is_blocked_by_a_wall_and_clear_in_the_open() {
        let m = walled();
        let eye = Vec3::new(0.0, 1.8, 5.0);
        assert!(!m.line_of_sight(eye, Vec3::new(0.0, 1.8, -5.0)));
        // Past the end of the wall.
        assert!(m.line_of_sight(Vec3::new(7.0, 1.8, 5.0), Vec3::new(7.0, 1.8, -5.0)));
        // Over the top of the wall.
        assert!(m.line_of_sight(Vec3::new(0.0, 3.5, 5.0), Vec3::new(0.0, 3.5, -5.0)));
        assert!(!m.visible_head(eye, Vec3::new(0.0, 0.0, -5.0)));
        assert!(m.visible_head(eye, Vec3::new(0.0, 0.0, 3.0)));
    }

    #[test]
    fn raycast_distance_is_exact_on_a_known_box() {
        let m = walled();
        let ray = Ray {
            origin: Vec3::new(1.0, 1.5, 4.2),
            dir: Vec3::new(0.0, 0.0, -1.0),
        };
        let (t, i) = m.raycast(ray, 100.0).unwrap();
        assert_eq!(i, 0);
        assert!((t - 4.0).abs() < 1e-5, "t = {t}");
        assert!(m.raycast(ray, 3.9).is_none(), "max_t bounds the cast");
        let away = Ray {
            dir: Vec3::new(0.0, 0.0, 1.0),
            ..ray
        };
        assert!(m.raycast(away, 100.0).is_none());
    }

    #[test]
    fn collision_slides_along_a_wall() {
        let m = walled();
        let r = 0.35;
        let feet = Vec3::new(0.0, 0.0, 1.0);
        // Diagonal push into the wall: Z stops at the face, X keeps moving.
        let p = m.move_with_collision(feet, Vec3::new(1.0, 0.0, -2.0), r);
        assert!((p.z - (0.2 + r)).abs() < 1e-5, "{p:?}");
        assert!((p.x - 1.0).abs() < 1e-5, "{p:?}");
        // Sliding along the face is free.
        let q = m.move_with_collision(p, Vec3::new(2.0, 0.0, 0.0), r);
        assert!(
            (q.x - 3.0).abs() < 1e-5 && (q.z - p.z).abs() < 1e-6,
            "{q:?}"
        );
    }

    #[test]
    fn a_large_step_cannot_tunnel_through_a_wall() {
        let m = walled();
        let p = m.move_with_collision(Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, -10.0), 0.35);
        assert!(p.z > 0.2, "{p:?}");
    }

    #[test]
    fn random_walk_never_enters_a_box_or_leaves_the_floor() {
        let mut m = walled();
        m.boxes.push(SolidBox {
            kind: BoxKind::LowCover,
            min: Vec3::new(-3.0, 0.0, 4.0),
            max: Vec3::new(-2.0, 1.0, 5.0),
        });
        let mut rng = aim_core::rng::Rng::new(7);
        let r = 0.35;
        let mut p = Vec3::new(0.0, 0.0, 6.0);
        for _ in 0..20_000 {
            let d = Vec3::new(rng.next_f32() * 3.0 - 1.5, 0.0, rng.next_f32() * 3.0 - 1.5);
            p = m.move_with_collision(p, d, r);
            assert!(m.body_overlaps(p, r).is_none(), "entered a box at {p:?}");
            assert!(p.x.abs() <= 10.0 - r + 1e-5 && p.z.abs() <= 10.0 - r + 1e-5);
        }
    }

    #[test]
    fn walkable_respects_the_body_radius() {
        let m = walled();
        assert!(!m.walkable(Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, -5.0), 0.35));
        // Passes 0.5 m from the wall end: fits a 0.35 m radius, not a 0.6 m one.
        let (a, b) = (Vec3::new(5.5, 0.0, 5.0), Vec3::new(5.5, 0.0, -5.0));
        assert!(m.walkable(a, b, 0.35));
        assert!(!m.walkable(a, b, 0.6));
    }
}
