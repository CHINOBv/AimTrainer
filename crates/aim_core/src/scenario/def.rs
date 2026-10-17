//! Data-driven scenario definitions. A scenario is a TOML file composed of independent
//! building blocks (target shape, spawn area, motion, fire mode, scoring), so new
//! exercises are written as data instead of code. See `docs/scenarios.md`.

use crate::math::Range;
use crate::valorant::Weapon;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioDef {
    /// Unique, stable identifier (`[a-z0-9_-]`). Used for score history.
    pub id: String,
    pub name: String,
    pub category: Category,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_duration")]
    pub duration_secs: f32,
    #[serde(default)]
    pub weapon: WeaponDef,
    #[serde(default)]
    pub player: PlayerDef,
    pub targets: TargetsDef,
    #[serde(default)]
    pub scoring: ScoringDef,
    /// How difficulty levels scale this scenario (see `scenario::difficulty`).
    #[serde(default)]
    pub difficulty: super::difficulty::DifficultyDef,
}

fn default_duration() -> f32 {
    60.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Flicking,
    Microcorrection,
    Switching,
    Tracking,
    Movement,
    Reaction,
    Precision,
    Custom,
}

impl Category {
    pub const ALL: [Category; 8] = [
        Category::Flicking,
        Category::Microcorrection,
        Category::Switching,
        Category::Tracking,
        Category::Movement,
        Category::Reaction,
        Category::Precision,
        Category::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Flicking => "Flicking",
            Category::Microcorrection => "Microcorrections",
            Category::Switching => "Target switching",
            Category::Tracking => "Tracking",
            Category::Movement => "Movement",
            Category::Reaction => "Reaction",
            Category::Precision => "Precision",
            Category::Custom => "Custom",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponDef {
    /// Determines run speed, fire rate, spread and damage zones.
    #[serde(default = "default_weapon")]
    pub kind: Weapon,
    #[serde(default)]
    pub fire: FireMode,
    /// Apply first-shot spread and movement inaccuracy to shots. Off for pure aim
    /// drills, on for movement drills where stopping before shooting matters.
    #[serde(default)]
    pub simulate_spread: bool,
    /// Ammo economy that punishes spamming (clicking only): every shot costs ammo, every hit
    /// refunds some, and running dry forces a reload. Modelled on Voltaic S5 clicking.
    #[serde(default)]
    pub ammo: Option<AmmoDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct AmmoDef {
    pub clip: f32,
    pub cost: f32,
    pub refund: f32,
    pub reload_secs: f32,
}

impl Default for AmmoDef {
    /// Voltaic S5 "ww5t Intermediate" economy: clip 100, shot 30, hit refunds 37.
    fn default() -> Self {
        Self {
            clip: 100.0,
            cost: 30.0,
            refund: 37.0,
            reload_secs: 1.5,
        }
    }
}

fn default_weapon() -> Weapon {
    Weapon::Vandal
}

impl Default for WeaponDef {
    fn default() -> Self {
        Self {
            kind: default_weapon(),
            fire: FireMode::default(),
            simulate_spread: false,
            ammo: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FireMode {
    /// One shot per click, capped by the weapon's fire rate.
    #[default]
    Click,
    /// Continuous damage while the button is held and the crosshair is on target.
    Hold,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerDef {
    /// Enable WASD movement using the weapon's VALORANT movement profile.
    #[serde(default)]
    pub movement: bool,
    /// Allowed area for the player's feet (world x / z, meters). The player starts at
    /// the origin facing -Z.
    #[serde(default = "default_bounds")]
    pub bounds_x: Range,
    #[serde(default = "default_bounds")]
    pub bounds_z: Range,
}

fn default_bounds() -> Range {
    Range::fixed(0.0)
}

impl Default for PlayerDef {
    fn default() -> Self {
        Self {
            movement: false,
            bounds_x: default_bounds(),
            bounds_z: default_bounds(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetsDef {
    pub shape: Shape,
    pub spawn: SpawnArea,
    #[serde(default)]
    pub motion: Motion,
    /// Targets alive at the same time.
    #[serde(default = "one")]
    pub max_alive: u32,
    /// Click mode: hits to kill. Hold mode: seconds on target to kill (`inf` = never dies).
    #[serde(default = "one_f")]
    pub health: f32,
    /// Remove a target that was not killed in time (counts as expired).
    #[serde(default)]
    pub lifetime_secs: Option<f32>,
    /// Random delay before a newly spawned target appears (reaction drills).
    #[serde(default = "zero_range")]
    pub appear_delay_secs: Range,
    /// Minimum angle between simultaneously alive targets.
    #[serde(default)]
    pub min_separation_deg: f32,
}

fn one() -> u32 {
    1
}
fn one_f() -> f32 {
    1.0
}
fn zero_range() -> Range {
    Range::fixed(0.0)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Shape {
    Sphere {
        radius: f32,
    },
    /// VALORANT-sized agent standing on the floor (head / body / legs zones).
    Humanoid {
        /// Only head hits count; body and leg hits are misses.
        #[serde(default)]
        head_only: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpawnArea {
    /// Direction relative to the starting forward view (yaw 0, pitch 0) at a distance from
    /// the eye. Humanoids ignore pitch and stand on the floor.
    Angular {
        yaw_deg: Range,
        pitch_deg: Range,
        distance: Range,
    },
    /// Offset from wherever the crosshair currently points (flicks, microcorrections).
    /// Humanoids only use the horizontal component.
    AroundCrosshair {
        offset_deg: Range,
        distance: Range,
        /// Absolute pitch band (degrees from the horizon, seen from the eye) that spawns
        /// must stay inside. Without it, chained flicks random-walk away from the
        /// horizon, because every spawn is relative to the previous aim.
        #[serde(default = "default_pitch_limit")]
        pitch_limit_deg: Range,
    },
    /// World-space box on the floor. Spheres are placed at `height` above the floor.
    Floor {
        x: Range,
        z: Range,
        #[serde(default = "default_height")]
        height: Range,
    },
}

fn default_height() -> Range {
    Range::fixed(1.6)
}

/// Default vertical band for crosshair-relative spawns: duels in tactical shooters happen
/// close to the horizon, a little below eye level up to elevated positions.
pub fn default_pitch_limit() -> Range {
    Range::new(-15.0, 30.0)
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Motion {
    #[default]
    Static,
    /// A/D strafing perpendicular to the player, using VALORANT movement physics
    /// (acceleration and the measured stopping curves).
    Strafe {
        /// Weapon whose run speed the target uses.
        #[serde(default = "default_weapon")]
        weapon: Weapon,
        /// Time between direction decisions.
        change_secs: Range,
        /// Probability of stopping (instead of reversing) at each decision.
        #[serde(default)]
        stop_chance: f32,
        /// Max lateral distance from the spawn point, meters.
        #[serde(default = "default_strafe_width")]
        max_offset: f32,
    },
    /// Smooth wandering in the plane facing the player (spheres / smoothness drills).
    Drift {
        /// Speed in m/s.
        speed: Range,
        change_secs: Range,
        /// Max distance from the spawn point, meters.
        #[serde(default = "default_strafe_width")]
        max_offset: f32,
    },
}

fn default_strafe_width() -> f32 {
    3.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoringDef {
    #[serde(default = "pts_head")]
    pub head: f32,
    #[serde(default = "pts_body")]
    pub body: f32,
    #[serde(default = "pts_legs")]
    pub legs: f32,
    /// Points for a sphere hit.
    #[serde(default = "pts_head")]
    pub sphere: f32,
    /// Points subtracted per missed shot.
    #[serde(default)]
    pub miss_penalty: f32,
    /// Points subtracted per shot fired above the accuracy speed threshold.
    #[serde(default)]
    pub moving_shot_penalty: f32,
    /// Points subtracted when a target expires.
    #[serde(default)]
    pub expire_penalty: f32,
}

fn pts_head() -> f32 {
    100.0
}
fn pts_body() -> f32 {
    40.0
}
fn pts_legs() -> f32 {
    25.0
}

impl Default for ScoringDef {
    fn default() -> Self {
        Self {
            head: pts_head(),
            body: pts_body(),
            legs: pts_legs(),
            sphere: pts_head(),
            miss_penalty: 0.0,
            moving_shot_penalty: 0.0,
            expire_penalty: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub field: &'static str,
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl ScenarioDef {
    pub fn from_toml(src: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(src)
    }

    /// Semantic checks that serde cannot express. Returns every problem found.
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut errs = Vec::new();
        let mut check = |ok: bool, field: &'static str, msg: &str| {
            if !ok {
                errs.push(ValidationError {
                    field,
                    message: msg.to_owned(),
                });
            }
        };

        check(
            !self.id.is_empty()
                && self
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_'),
            "id",
            "must be non-empty and only contain a-z, 0-9, '-' or '_'",
        );
        check(!self.name.trim().is_empty(), "name", "must not be empty");
        check(
            self.duration_secs.is_finite() && self.duration_secs > 0.0,
            "duration_secs",
            "must be > 0",
        );
        if let Some(a) = self.weapon.ammo {
            check(
                a.cost > 0.0 && a.clip >= a.cost && a.refund >= 0.0 && a.reload_secs >= 0.0,
                "weapon.ammo",
                "needs cost > 0, clip >= cost, refund >= 0, reload_secs >= 0",
            );
        }
        check(
            self.difficulty.is_valid(),
            "difficulty",
            "coefficients must be in [0, 0.95)",
        );

        let p = &self.player;
        check(
            p.bounds_x.is_valid() && p.bounds_z.is_valid(),
            "player.bounds",
            "ranges must be [min, max]",
        );
        check(
            p.bounds_x.min <= 0.0
                && p.bounds_x.max >= 0.0
                && p.bounds_z.min <= 0.0
                && p.bounds_z.max >= 0.0,
            "player.bounds",
            "must contain the origin (player start)",
        );

        let t = &self.targets;
        check(
            (1..=32).contains(&t.max_alive),
            "targets.max_alive",
            "must be between 1 and 32",
        );
        check(
            t.health > 0.0,
            "targets.health",
            "must be > 0 (use inf for unkillable)",
        );
        check(
            t.health.is_finite() || self.weapon.fire == FireMode::Hold,
            "targets.health",
            "inf is only allowed with fire = \"hold\"",
        );
        check(
            t.lifetime_secs.is_none_or(|l| l.is_finite() && l > 0.0),
            "targets.lifetime_secs",
            "must be > 0",
        );
        check(
            t.appear_delay_secs.is_valid() && t.appear_delay_secs.min >= 0.0,
            "targets.appear_delay_secs",
            "must be a non-negative [min, max]",
        );
        check(
            (0.0..180.0).contains(&t.min_separation_deg),
            "targets.min_separation_deg",
            "must be in [0, 180)",
        );

        if let Shape::Sphere { radius } = t.shape {
            check(
                radius.is_finite() && radius > 0.0,
                "targets.shape.radius",
                "must be > 0",
            );
        }

        match &t.spawn {
            SpawnArea::Angular {
                yaw_deg,
                pitch_deg,
                distance,
            } => {
                check(
                    yaw_deg.is_valid() && pitch_deg.is_valid(),
                    "targets.spawn",
                    "ranges must be [min, max]",
                );
                check(
                    pitch_deg.min >= -89.0 && pitch_deg.max <= 89.0,
                    "targets.spawn.pitch_deg",
                    "must be within ±89",
                );
                check(
                    distance.is_valid() && distance.min >= 1.0,
                    "targets.spawn.distance",
                    "must be >= 1 m",
                );
            }
            SpawnArea::AroundCrosshair {
                offset_deg,
                distance,
                pitch_limit_deg,
            } => {
                check(
                    pitch_limit_deg.is_valid()
                        && pitch_limit_deg.min >= -89.0
                        && pitch_limit_deg.max <= 89.0,
                    "targets.spawn.pitch_limit_deg",
                    "must be a [min, max] within ±89",
                );
                check(
                    offset_deg.is_valid() && offset_deg.min >= 0.0 && offset_deg.max <= 180.0,
                    "targets.spawn.offset_deg",
                    "must be within [0, 180]",
                );
                check(
                    distance.is_valid() && distance.min >= 1.0,
                    "targets.spawn.distance",
                    "must be >= 1 m",
                );
            }
            SpawnArea::Floor { x, z, height } => {
                check(
                    x.is_valid() && z.is_valid() && height.is_valid(),
                    "targets.spawn",
                    "ranges must be [min, max]",
                );
            }
        }

        match &t.motion {
            Motion::Static => {}
            Motion::Strafe {
                change_secs,
                stop_chance,
                max_offset,
                ..
            } => {
                check(
                    change_secs.is_valid() && change_secs.min > 0.0,
                    "targets.motion.change_secs",
                    "must be a positive [min, max]",
                );
                check(
                    (0.0..=1.0).contains(stop_chance),
                    "targets.motion.stop_chance",
                    "must be in [0, 1]",
                );
                check(
                    *max_offset > 0.0,
                    "targets.motion.max_offset",
                    "must be > 0",
                );
            }
            Motion::Drift {
                speed,
                change_secs,
                max_offset,
            } => {
                check(
                    speed.is_valid() && speed.min >= 0.0,
                    "targets.motion.speed",
                    "must be a non-negative [min, max]",
                );
                check(
                    change_secs.is_valid() && change_secs.min > 0.0,
                    "targets.motion.change_secs",
                    "must be a positive [min, max]",
                );
                check(
                    *max_offset > 0.0,
                    "targets.motion.max_offset",
                    "must be > 0",
                );
            }
        }
        errs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
        id = "mini"
        name = "Mini"
        category = "flicking"
        [targets]
        shape = { kind = "sphere", radius = 0.2 }
        spawn = { kind = "around_crosshair", offset_deg = [10, 40], distance = [10, 10] }
    "#;

    #[test]
    fn minimal_definition_uses_defaults() {
        let d = ScenarioDef::from_toml(MINIMAL).unwrap();
        assert_eq!(d.duration_secs, 60.0);
        assert_eq!(d.weapon.kind, Weapon::Vandal);
        assert_eq!(d.weapon.fire, FireMode::Click);
        assert_eq!(d.targets.max_alive, 1);
        assert_eq!(d.targets.motion, Motion::Static);
        assert!(!d.player.movement);
        assert!(d.validate().is_empty(), "{:?}", d.validate());
    }

    #[test]
    fn unknown_fields_are_rejected_to_catch_typos() {
        let src = MINIMAL.replace("name = \"Mini\"", "name = \"Mini\"\ndurration_secs = 3");
        assert!(ScenarioDef::from_toml(&src).is_err());
    }

    #[test]
    fn validation_reports_every_problem() {
        let mut d = ScenarioDef::from_toml(MINIMAL).unwrap();
        d.id = "Bad Id".into();
        d.duration_secs = 0.0;
        d.targets.health = f32::INFINITY;
        let fields: Vec<_> = d.validate().into_iter().map(|e| e.field).collect();
        assert_eq!(fields, ["id", "duration_secs", "targets.health"]);
    }
}
