//! Difficulty scaling: derives easier or harder versions of a scenario from one definition.
//!
//! A level of `0` is the scenario as authored; `+1` is the hardest and `-1` the easiest
//! supported variant. Each knob has a coefficient (fraction of change at level ±1) that a
//! scenario can tune or disable in its `[difficulty]` table. Training methods use fixed
//! offsets (drill, variant, "hard"); adaptive training moves the level continuously.

use super::def::{Motion, ScenarioDef, Shape, SpawnArea};
use crate::math::Range;
use serde::{Deserialize, Serialize};

/// Supported level range.
pub const MIN_LEVEL: f32 = -1.0;
pub const MAX_LEVEL: f32 = 1.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct DifficultyDef {
    /// Sphere radius shrinks by this fraction at level +1 (grows at −1).
    pub size: f32,
    /// Drift speed grows, and direction changes come sooner, by this fraction at +1.
    pub speed: f32,
    /// Target lifetime shrinks by this fraction at +1.
    pub lifetime: f32,
    /// Humanoid spawn distance grows by this fraction at +1 (smaller heads).
    pub distance: f32,
    /// Crosshair-relative flick offsets grow by this fraction at +1.
    pub offset: f32,
}

impl Default for DifficultyDef {
    fn default() -> Self {
        Self {
            size: 0.3,
            speed: 0.3,
            lifetime: 0.25,
            distance: 0.25,
            offset: 0.2,
        }
    }
}

impl DifficultyDef {
    pub fn is_valid(&self) -> bool {
        [
            self.size,
            self.speed,
            self.lifetime,
            self.distance,
            self.offset,
        ]
        .iter()
        .all(|c| c.is_finite() && (0.0..0.95).contains(c))
    }
}

fn scale(r: Range, k: f32) -> Range {
    Range::new(r.min * k, r.max * k)
}

/// Key used to store results: plain id at level 0, `id@+0.50` otherwise, so scores at
/// different difficulties never mix.
pub fn run_key(id: &str, level: f32) -> String {
    if level.abs() < 1e-3 {
        id.to_owned()
    } else {
        format!("{id}@{level:+.2}")
    }
}

/// Inverse of [`run_key`]: base scenario id and level (`0` when the key has no level).
pub fn parse_run_key(key: &str) -> (&str, f32) {
    match key.rsplit_once('@') {
        Some((id, level)) => match level.parse::<f32>() {
            Ok(l) if l.is_finite() => (id, l),
            _ => (key, 0.0),
        },
        None => (key, 0.0),
    }
}

impl ScenarioDef {
    /// This scenario at difficulty `level` (clamped to [`MIN_LEVEL`], [`MAX_LEVEL`]).
    /// The id becomes [`run_key`] so history keeps difficulties apart.
    pub fn at_difficulty(&self, level: f32) -> ScenarioDef {
        let d = level.clamp(MIN_LEVEL, MAX_LEVEL);
        let mut out = self.clone();
        if d == 0.0 {
            return out;
        }
        let c = &self.difficulty;
        out.id = run_key(&self.id, d);
        let humanoid = matches!(out.targets.shape, Shape::Humanoid { .. });

        if let Shape::Sphere { radius } = &mut out.targets.shape {
            *radius *= 1.0 - c.size * d;
        }
        if let Some(l) = &mut out.targets.lifetime_secs {
            *l *= 1.0 - c.lifetime * d;
        }
        match &mut out.targets.motion {
            Motion::Static => {}
            Motion::Strafe { change_secs, .. } => {
                *change_secs = scale(*change_secs, 1.0 - c.speed * d)
            }
            Motion::Drift {
                speed, change_secs, ..
            } => {
                *speed = scale(*speed, 1.0 + c.speed * d);
                *change_secs = scale(*change_secs, 1.0 - c.speed * d);
            }
        }
        let dist_k = if humanoid { 1.0 + c.distance * d } else { 1.0 };
        match &mut out.targets.spawn {
            SpawnArea::Angular { distance, .. } => *distance = scale(*distance, dist_k),
            SpawnArea::AroundCrosshair {
                offset_deg,
                distance,
                ..
            } => {
                *distance = scale(*distance, dist_k);
                let k = 1.0 + c.offset * d;
                *offset_deg = Range::new(
                    (offset_deg.min * k).min(180.0),
                    (offset_deg.max * k).min(180.0),
                );
            }
            SpawnArea::Floor { z, .. } => {
                // Floor boxes extend away from the player along -Z.
                *z = scale(*z, dist_k);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::builtin;

    fn find(id: &str) -> ScenarioDef {
        builtin::all().into_iter().find(|d| d.id == id).unwrap()
    }

    #[test]
    fn level_zero_is_identity_and_keys_encode_level() {
        let d = find("micro-adjust");
        assert_eq!(d.at_difficulty(0.0), d);
        assert_eq!(run_key("x", 0.0), "x");
        assert_eq!(run_key("x", 0.5), "x@+0.50");
        assert_eq!(run_key("x", -0.25), "x@-0.25");
        assert_eq!(d.at_difficulty(0.5).id, "micro-adjust@+0.50");
        assert_eq!(parse_run_key("micro-adjust@+0.50"), ("micro-adjust", 0.5));
        assert_eq!(parse_run_key("micro-adjust@-0.25"), ("micro-adjust", -0.25));
        assert_eq!(parse_run_key("micro-adjust"), ("micro-adjust", 0.0));
        assert_eq!(parse_run_key("odd@name"), ("odd@name", 0.0));
    }

    #[test]
    fn harder_shrinks_spheres_speeds_up_motion_and_shortens_lifetimes() {
        let d = find("track-reactive-micro");
        let h = d.at_difficulty(1.0);
        let (Shape::Sphere { radius: r0 }, Shape::Sphere { radius: r1 }) =
            (&d.targets.shape, &h.targets.shape)
        else {
            panic!()
        };
        assert!((r1 / r0 - 0.7).abs() < 1e-5);
        let (Motion::Drift { speed: s0, .. }, Motion::Drift { speed: s1, .. }) =
            (&d.targets.motion, &h.targets.motion)
        else {
            panic!()
        };
        assert!((s1.max / s0.max - 1.3).abs() < 1e-5);
        let dynamic = find("dynamic-strafe-heads");
        let hard = dynamic.at_difficulty(1.0);
        assert!(hard.targets.lifetime_secs.unwrap() < dynamic.targets.lifetime_secs.unwrap());
        assert!(
            dynamic.at_difficulty(-1.0).targets.lifetime_secs.unwrap()
                > dynamic.targets.lifetime_secs.unwrap()
        );
    }

    #[test]
    fn levels_clamp_and_every_builtin_stays_valid_across_the_range() {
        for d in builtin::all() {
            for level in [-1.0, -0.5, 0.25, 0.5, 1.0, 3.0] {
                let v = d.at_difficulty(level);
                // Derived ids carry the level (`id@+0.50`), so only the id rule may fail.
                let other: Vec<_> = v
                    .validate()
                    .into_iter()
                    .filter(|e| e.field != "id")
                    .collect();
                assert!(other.is_empty(), "{} at {level}: {other:?}", d.id);
            }
            assert_eq!(d.at_difficulty(3.0), d.at_difficulty(1.0));
        }
    }

    #[test]
    fn simulated_scores_drop_as_difficulty_rises() {
        use crate::sim::{AimerProfile, simulate};
        let p = AimerProfile::PRESETS[1];
        for id in ["micro-adjust", "switch-speed", "track-reactive-micro"] {
            let d = find(id);
            let easy = simulate(&d.at_difficulty(-1.0), p, 3).score;
            let base = simulate(&d, p, 3).score;
            let hard = simulate(&d.at_difficulty(1.0), p, 3).score;
            assert!(easy > base && base > hard, "{id}: {easy} / {base} / {hard}");
        }
    }

    #[test]
    fn zero_coefficients_disable_a_knob() {
        let mut d = find("micro-adjust");
        d.difficulty.size = 0.0;
        assert_eq!(d.at_difficulty(1.0).targets.shape, d.targets.shape);
    }
}
