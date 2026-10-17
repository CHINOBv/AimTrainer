//! Minimal vector math so the domain stays engine-agnostic.
//!
//! World convention (matches Bevy): right-handed, +Y up, -Z forward, meters.

use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Mul, Neg, Sub};

/// A 3D vector or point in world space, in meters.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Vec3 {
    /// Right (+X) component.
    pub x: f32,
    /// Up (+Y) component.
    pub y: f32,
    /// Backward (+Z) component; forward is -Z.
    pub z: f32,
}

impl Vec3 {
    /// The zero vector.
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    /// Unit vector pointing up (+Y).
    pub const UP: Self = Self::new(0.0, 1.0, 0.0);
    /// Unit vector pointing forward (-Z).
    pub const FORWARD: Self = Self::new(0.0, 0.0, -1.0);

    /// Builds a vector from its components.
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Dot product.
    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    /// Cross product (right-handed).
    pub fn cross(self, o: Self) -> Self {
        Self::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    /// Euclidean length.
    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    /// Unit-length copy, or zero when the length is negligible.
    pub fn normalize_or_zero(self) -> Self {
        let len = self.length();
        if len > f32::EPSILON {
            self * (1.0 / len)
        } else {
            Self::ZERO
        }
    }

    /// Copy with the vertical (Y) component set to zero.
    pub fn horizontal(self) -> Self {
        Self::new(self.x, 0.0, self.z)
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, o: Self) {
        *self = *self + o;
    }
}

impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, s: f32) -> Self {
        Self::new(self.x * s, self.y * s, self.z * s)
    }
}

impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

/// Camera orientation in degrees. Yaw 0 looks down -Z, positive yaw turns right.
/// Positive pitch looks up.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ViewAngles {
    /// Horizontal rotation in degrees.
    pub yaw_deg: f32,
    /// Vertical rotation in degrees.
    pub pitch_deg: f32,
}

impl ViewAngles {
    /// Largest absolute pitch allowed, in degrees, to avoid flipping over the vertical.
    pub const PITCH_LIMIT_DEG: f32 = 89.0;

    /// Builds view angles from yaw and pitch in degrees.
    pub fn new(yaw_deg: f32, pitch_deg: f32) -> Self {
        Self { yaw_deg, pitch_deg }
    }

    /// Unit direction the camera is looking at.
    pub fn direction(self) -> Vec3 {
        let (sy, cy) = self.yaw_deg.to_radians().sin_cos();
        let (sp, cp) = self.pitch_deg.to_radians().sin_cos();
        Vec3::new(sy * cp, sp, -cy * cp)
    }

    /// Angles that look along `dir` (any non-zero vector).
    pub fn from_direction(dir: Vec3) -> Self {
        let d = dir.normalize_or_zero();
        let yaw = d.x.atan2(-d.z).to_degrees();
        let pitch = d.y.clamp(-1.0, 1.0).asin().to_degrees();
        Self::new(yaw, pitch)
    }

    /// Apply a rotation in degrees, clamping pitch like an FPS camera.
    pub fn rotated(self, d_yaw_deg: f32, d_pitch_deg: f32) -> Self {
        Self::new(
            wrap_degrees(self.yaw_deg + d_yaw_deg),
            (self.pitch_deg + d_pitch_deg).clamp(-Self::PITCH_LIMIT_DEG, Self::PITCH_LIMIT_DEG),
        )
    }
}

/// Wrap an angle into (-180, 180].
pub fn wrap_degrees(a: f32) -> f32 {
    let mut a = a % 360.0;
    if a <= -180.0 {
        a += 360.0;
    } else if a > 180.0 {
        a -= 360.0;
    }
    a
}

/// Angle in degrees between two directions.
pub fn angle_between_deg(a: Vec3, b: Vec3) -> f32 {
    let a = a.normalize_or_zero();
    let b = b.normalize_or_zero();
    a.dot(b).clamp(-1.0, 1.0).acos().to_degrees()
}

/// Inclusive range used throughout scenario definitions, written as `[min, max]` in TOML.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(from = "[f32; 2]", into = "[f32; 2]")]
pub struct Range {
    /// Lower bound (inclusive).
    pub min: f32,
    /// Upper bound (inclusive).
    pub max: f32,
}

impl Range {
    /// Builds a range from its bounds.
    pub const fn new(min: f32, max: f32) -> Self {
        Self { min, max }
    }

    /// Degenerate range that always yields `v`.
    pub const fn fixed(v: f32) -> Self {
        Self { min: v, max: v }
    }

    /// Whether both bounds are finite and `min <= max`.
    pub fn is_valid(&self) -> bool {
        self.min.is_finite() && self.max.is_finite() && self.min <= self.max
    }

    /// Linear interpolation from `min` (t = 0) to `max` (t = 1).
    pub fn lerp(&self, t: f32) -> f32 {
        self.min + (self.max - self.min) * t
    }
}

impl From<[f32; 2]> for Range {
    fn from([min, max]: [f32; 2]) -> Self {
        Self { min, max }
    }
}

impl From<Range> for [f32; 2] {
    fn from(r: Range) -> Self {
        [r.min, r.max]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn direction_round_trips_through_angles() {
        for (yaw, pitch) in [(0.0, 0.0), (90.0, 0.0), (-45.0, 30.0), (170.0, -60.0)] {
            let a = ViewAngles::new(yaw, pitch);
            let b = ViewAngles::from_direction(a.direction());
            assert!(
                close(a.yaw_deg, b.yaw_deg) && close(a.pitch_deg, b.pitch_deg),
                "{a:?} vs {b:?}"
            );
        }
    }

    #[test]
    fn yaw_zero_looks_forward_and_positive_yaw_turns_right() {
        let f = ViewAngles::new(0.0, 0.0).direction();
        assert!(close(f.z, -1.0));
        let r = ViewAngles::new(90.0, 0.0).direction();
        assert!(close(r.x, 1.0));
    }

    #[test]
    fn rotated_wraps_yaw_and_clamps_pitch() {
        let a = ViewAngles::new(170.0, 80.0).rotated(20.0, 20.0);
        assert!(close(a.yaw_deg, -170.0));
        assert!(close(a.pitch_deg, ViewAngles::PITCH_LIMIT_DEG));
    }
}
