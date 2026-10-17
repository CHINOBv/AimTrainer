//! Difficulty scaling: derives easier or harder versions of a scenario from one definition.
//!
//! A level of `0` is the scenario as authored; `+1` is the hardest and `-1` the easiest
//! supported variant. Each knob is the fraction of change at level −1 (`easy`) and +1
//! (`hard`); a plain number sets both. A scenario tunes or disables knobs in its
//! `[difficulty]` table, e.g. `size = { easy = 0.5, hard = 0.1 }`. Training methods use fixed
//! offsets (drill, variant, "hard"); adaptive training moves the level continuously.

use super::def::{Motion, ScenarioDef, Shape, SpawnArea};
use crate::math::Range;
use serde::{Deserialize, Serialize};

/// Lowest supported difficulty level (easiest).
pub const MIN_LEVEL: f32 = -1.0;
/// Highest supported difficulty level (hardest).
pub const MAX_LEVEL: f32 = 1.0;

/// How far one parameter moves at the ends of the level range.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged, try_from = "RawKnob")]
pub enum Knob {
    /// The same fraction at level −1 and +1.
    Symmetric(f32),
    /// Fraction of change at level −1 (`easy`) and at +1 (`hard`).
    Bounds {
        /// Fraction of change at level -1.
        easy: f32,
        /// Fraction of change at level +1.
        hard: f32,
    },
}

/// What a knob may be written as. The table form rejects unknown keys, which an untagged
/// struct variant alone would silently drop.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawKnob {
    Symmetric(f32),
    Bounds(KnobBounds),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KnobBounds {
    easy: f32,
    hard: f32,
}

impl From<RawKnob> for Knob {
    fn from(raw: RawKnob) -> Self {
        match raw {
            RawKnob::Symmetric(c) => Knob::Symmetric(c),
            RawKnob::Bounds(KnobBounds { easy, hard }) => Knob::Bounds { easy, hard },
        }
    }
}

impl Knob {
    /// Knob that changes by the same fraction at both ends of the level range.
    pub const fn symmetric(fraction: f32) -> Self {
        Knob::Symmetric(fraction)
    }

    /// Signed fraction of change at `level` (negative when easier).
    pub fn at(self, level: f32) -> f32 {
        let (easy, hard) = self.bounds();
        if level >= 0.0 {
            hard * level
        } else {
            easy * level
        }
    }

    fn bounds(self) -> (f32, f32) {
        match self {
            Knob::Symmetric(c) => (c, c),
            Knob::Bounds { easy, hard } => (easy, hard),
        }
    }
}

/// How each scenario parameter scales with the difficulty level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct DifficultyDef {
    /// Sphere radius shrinks by this fraction when harder (grows when easier).
    pub size: Knob,
    /// Drift speed grows, and direction changes come sooner, by this fraction when harder.
    pub speed: Knob,
    /// Target lifetime shrinks by this fraction when harder.
    pub lifetime: Knob,
    /// Humanoid spawn distance grows by this fraction when harder (smaller heads).
    pub distance: Knob,
    /// Crosshair-relative flick offsets grow by this fraction when harder.
    pub offset: Knob,
    /// Targets alive at once grow by this fraction when harder (rounded, at least one).
    /// Off by default: more targets is not harder in every scenario.
    pub count: Knob,
}

impl Default for DifficultyDef {
    fn default() -> Self {
        Self {
            size: Knob::symmetric(0.3),
            speed: Knob::symmetric(0.3),
            lifetime: Knob::symmetric(0.25),
            distance: Knob::symmetric(0.25),
            offset: Knob::symmetric(0.2),
            count: Knob::symmetric(0.0),
        }
    }
}

impl DifficultyDef {
    /// Whether every knob is finite.
    pub fn is_valid(&self) -> bool {
        [
            self.size,
            self.speed,
            self.lifetime,
            self.distance,
            self.offset,
            self.count,
        ]
        .iter()
        .all(|k| {
            let (easy, hard) = k.bounds();
            [easy, hard]
                .iter()
                .all(|c| c.is_finite() && (0.0..0.95).contains(c))
        })
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

/// Inverse of [`run_key`] (and of [`ScenarioDef::with_duration`]): base scenario id and
/// level (`0` when the key has no level). A `~20s` duration suffix is dropped.
pub fn parse_run_key(key: &str) -> (&str, f32) {
    let (base, level, _) = parse_run_key_full(key);
    (base, level)
}

/// Base id, level and shortened duration (`~20s`) encoded in a run key.
pub fn parse_run_key_full(key: &str) -> (&str, f32, Option<f32>) {
    let (rest, secs) = match key.rsplit_once('~') {
        Some((rest, d)) => match d.strip_suffix('s').and_then(|n| n.parse::<f32>().ok()) {
            Some(secs) if secs.is_finite() && secs > 0.0 => (rest, Some(secs)),
            _ => (key, None),
        },
        None => (key, None),
    };
    match rest.rsplit_once('@') {
        Some((id, level)) => match level.parse::<f32>() {
            Ok(l) if l.is_finite() => (id, l, secs),
            _ => (rest, 0.0, secs),
        },
        None => (rest, 0.0, secs),
    }
}

impl ScenarioDef {
    /// This scenario shortened (or lengthened) to `secs`. The id gains a `~20s` suffix so
    /// short drills never mix with full-length scores.
    pub fn with_duration(&self, secs: f32) -> ScenarioDef {
        let mut out = self.clone();
        if (secs - self.duration_secs).abs() >= 0.5 && secs > 0.0 {
            out.duration_secs = secs;
            out.id = format!("{}~{}s", self.id, secs.round() as u32);
        }
        out
    }

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
            *radius *= 1.0 - c.size.at(d);
        }
        if let Some(l) = &mut out.targets.lifetime_secs {
            *l *= 1.0 - c.lifetime.at(d);
        }
        match &mut out.targets.motion {
            Motion::Static => {}
            Motion::Strafe { change_secs, .. } => {
                *change_secs = scale(*change_secs, 1.0 - c.speed.at(d))
            }
            Motion::Drift {
                speed, change_secs, ..
            } => {
                *speed = scale(*speed, 1.0 + c.speed.at(d));
                *change_secs = scale(*change_secs, 1.0 - c.speed.at(d));
            }
        }
        let count = c.count.at(d);
        if count != 0.0 {
            let alive = out.targets.max_alive as f32 * (1.0 + count);
            out.targets.max_alive = (alive.round() as u32).clamp(1, 32);
        }
        let dist_k = if humanoid {
            1.0 + c.distance.at(d)
        } else {
            1.0
        };
        match &mut out.targets.spawn {
            SpawnArea::Angular { distance, .. } => *distance = scale(*distance, dist_k),
            SpawnArea::AroundCrosshair {
                offset_deg,
                distance,
                ..
            } => {
                *distance = scale(*distance, dist_k);
                let k = 1.0 + c.offset.at(d);
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
        let short = d.at_difficulty(0.25).with_duration(20.0);
        assert_eq!(short.id, "micro-adjust@+0.25~20s");
        assert_eq!(short.duration_secs, 20.0);
        assert_eq!(
            parse_run_key_full(&short.id),
            ("micro-adjust", 0.25, Some(20.0))
        );
        assert_eq!(
            parse_run_key_full("micro-adjust~20s"),
            ("micro-adjust", 0.0, Some(20.0))
        );
        assert_eq!(
            d.with_duration(d.duration_secs),
            d,
            "same duration keeps the id"
        );
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
        d.difficulty.size = Knob::symmetric(0.0);
        assert_eq!(d.at_difficulty(1.0).targets.shape, d.targets.shape);
    }

    fn radius(d: &ScenarioDef) -> f32 {
        match d.targets.shape {
            Shape::Sphere { radius } => radius,
            _ => panic!("sphere scenario"),
        }
    }

    #[test]
    fn knobs_take_separate_easy_and_hard_bounds() {
        let src = r#"
            [difficulty]
            size = { easy = 0.5, hard = 0.1 }
        "#;
        let k: DifficultyDef = toml::from_str::<toml::Table>(src).unwrap()["difficulty"]
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(
            k.size,
            Knob::Bounds {
                easy: 0.5,
                hard: 0.1
            }
        );
        assert_eq!(
            k.speed,
            DifficultyDef::default().speed,
            "unset knobs keep defaults"
        );

        let mut d = find("micro-adjust");
        d.difficulty = k;
        let r = radius(&d);
        assert!((radius(&d.at_difficulty(1.0)) - r * 0.9).abs() < 1e-6);
        assert!((radius(&d.at_difficulty(-1.0)) - r * 1.5).abs() < 1e-6);
        assert!((radius(&d.at_difficulty(-0.5)) - r * 1.25).abs() < 1e-6);
    }

    #[test]
    fn a_plain_number_is_the_same_bound_both_ways() {
        let k: Knob = toml::Value::Float(0.3).try_into().unwrap();
        assert_eq!(k, Knob::symmetric(0.3));
        assert_eq!(k.at(1.0), 0.3);
        assert_eq!(k.at(-0.5), -0.15);
    }

    #[test]
    fn the_count_knob_adds_targets_when_harder_and_is_off_by_default() {
        let mut d = find("micro-adjust");
        assert_eq!(d.at_difficulty(1.0).targets.max_alive, d.targets.max_alive);
        d.targets.max_alive = 3;
        d.difficulty.count = Knob::Bounds {
            easy: 0.4,
            hard: 0.7,
        };
        assert_eq!(d.at_difficulty(1.0).targets.max_alive, 5, "3 × 1.7 rounded");
        assert_eq!(
            d.at_difficulty(-1.0).targets.max_alive,
            2,
            "3 × 0.6 rounded"
        );
        d.targets.max_alive = 1;
        d.difficulty.count = Knob::symmetric(0.9);
        assert_eq!(
            d.at_difficulty(-1.0).targets.max_alive,
            1,
            "never below one"
        );
    }

    #[test]
    fn misspelled_keys_inside_a_knob_are_rejected() {
        let typo = toml::Value::Table(toml::from_str("easy = 0.5\nhrad = 0.1").unwrap());
        assert!(typo.try_into::<Knob>().is_err());
        let extra =
            toml::Value::Table(toml::from_str("easy = 0.5\nhard = 0.1\nmid = 0.2").unwrap());
        assert!(extra.try_into::<Knob>().is_err());
    }

    #[test]
    fn invalid_bounds_are_rejected() {
        let mut k = DifficultyDef::default();
        assert!(k.is_valid());
        k.lifetime = Knob::Bounds {
            easy: 0.2,
            hard: 0.95,
        };
        assert!(!k.is_valid());
        k.lifetime = Knob::Bounds {
            easy: -0.1,
            hard: 0.2,
        };
        assert!(!k.is_valid());
    }
}
