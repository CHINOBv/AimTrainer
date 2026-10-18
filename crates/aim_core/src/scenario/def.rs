//! Data-driven scenario definitions. A scenario is a TOML file composed of independent
//! building blocks (target shape, spawn area, motion, fire mode, scoring), so new
//! exercises are written as data instead of code. See `docs/scenarios.md`.

use crate::games::valorant::Weapon;
use crate::math::Range;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Complete definition of a scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioDef {
    /// Unique, stable identifier (`[a-z0-9_-]`). Used for score history.
    pub id: String,
    /// Name shown in the UI.
    pub name: String,
    /// Training category the scenario belongs to.
    pub category: Category,
    /// Short text shown in the scenario list.
    #[serde(default)]
    pub description: String,
    /// Run length in seconds.
    #[serde(default = "default_duration")]
    pub duration_secs: f32,
    /// Weapon and firing settings.
    #[serde(default)]
    pub weapon: WeaponDef,
    /// Player movement settings.
    #[serde(default)]
    pub player: PlayerDef,
    /// Target shape, spawning and motion.
    pub targets: TargetsDef,
    /// Points awarded and subtracted.
    #[serde(default)]
    pub scoring: ScoringDef,
    /// How difficulty levels scale this scenario (see `scenario::difficulty`).
    #[serde(default)]
    pub difficulty: super::difficulty::DifficultyDef,
}

fn default_duration() -> f32 {
    60.0
}

/// Training skill a scenario exercises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Fast target-to-target flicks.
    Flicking,
    /// Small adjustments around the crosshair.
    Microcorrection,
    /// Switching between several targets.
    Switching,
    /// Following a moving target.
    Tracking,
    /// Aiming while moving.
    Movement,
    /// Reacting to targets appearing.
    Reaction,
    /// Precise, deliberate shots.
    Precision,
    /// User-written scenarios.
    Custom,
}

impl Category {
    /// Every category, in display order.
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

    /// Name shown in the UI.
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

/// Weapon and firing settings of a scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponDef {
    /// Determines run speed, fire rate, spread and damage zones.
    #[serde(default = "default_weapon")]
    pub kind: Weapon,
    /// How the weapon fires.
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
    /// Full-auto recoil (click mode, Vandal or Phantom only): holding the button keeps
    /// firing at the fire rate, and each bullet of a burst climbs, sways and spreads more
    /// until the player stops firing for the weapon's recovery time.
    #[serde(default)]
    pub recoil: bool,
}

/// Ammo economy parameters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct AmmoDef {
    /// Ammo capacity of a full clip.
    pub clip: f32,
    /// Ammo spent per shot.
    pub cost: f32,
    /// Ammo refunded per hit.
    pub refund: f32,
    /// Reload duration in seconds.
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
            recoil: false,
        }
    }
}

/// How the weapon fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FireMode {
    /// One shot per click, capped by the weapon's fire rate.
    #[default]
    Click,
    /// Continuous damage while the button is held and the crosshair is on target.
    Hold,
}

/// Player settings of a scenario.
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
    /// Allowed range of the player's feet along world z, meters.
    #[serde(default = "default_bounds")]
    pub bounds_z: Range,
    /// Height of the player's floor above the targets' floor, meters (default 0). Within
    /// [`ELEVATION_RANGE`]. Targets keep their floor at 0, so a positive value looks down
    /// on them and a negative one looks up.
    #[serde(default)]
    pub elevation: f32,
}

/// Allowed `player.elevation`, meters.
pub const ELEVATION_RANGE: Range = Range {
    min: -3.0,
    max: 6.0,
};

fn default_bounds() -> Range {
    Range::fixed(0.0)
}

impl Default for PlayerDef {
    fn default() -> Self {
        Self {
            movement: false,
            bounds_x: default_bounds(),
            bounds_z: default_bounds(),
            elevation: 0.0,
        }
    }
}

/// Target settings of a scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetsDef {
    /// Shape of each target.
    pub shape: Shape,
    /// Where targets spawn.
    pub spawn: SpawnArea,
    /// How targets move after spawning.
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
    /// Multiplier on target movement speed (drift speed; strafe run speed and
    /// acceleration). Set by in-run tuning; scenario files normally leave it at 1.
    #[serde(default = "one_f", skip_serializing_if = "is_one")]
    pub speed_scale: f32,
    /// Go/no-go: chance in [0, 1) that a spawned target is a "non-match" decoy, which the
    /// player must leave alone until it expires. Needs `lifetime_secs` and click fire.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub decoy_chance: f32,
    /// Spawn-pattern controls: keep new targets away from the last spawns, from the
    /// crosshair, or out of the middle of the spawn area.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spawn_rules: Option<SpawnRules>,
    /// Health regeneration: damaged targets heal over time, so landing damage in one
    /// sustained effort beats chipping at them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regen: Option<RegenDef>,
}

/// Health regeneration of targets (`targets.regen`). Every `interval_secs` after a target
/// appears it heals `amount` of health (hits in click mode, seconds in hold mode), never
/// above its starting health.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegenDef {
    /// Seconds between heals.
    pub interval_secs: f32,
    /// Health restored per heal.
    pub amount: f32,
    /// Restart the interval whenever the target takes damage, so it only heals once it has
    /// gone `interval_secs` without being hit.
    #[serde(default)]
    pub reset_on_hit: bool,
}

/// Spawn-pattern controls (`[targets.spawn_rules]`). A candidate spawn point that breaks a
/// rule is resampled, together with `min_separation_deg`, up to a fixed number of attempts;
/// if no attempt satisfies every rule, the last candidate is used. Angles are measured from
/// the eye between aim points (a humanoid's head, a sphere's center).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnRules {
    /// Block the last spawn point and every position within this angle of it, degrees
    /// (0 = off). A wider angle also blocks the positions adjacent to the last one.
    #[serde(default)]
    pub avoid_last_deg: f32,
    /// How many of the most recent spawn points `avoid_last_deg` blocks (default 1).
    #[serde(default = "one")]
    pub avoid_recent: u32,
    /// Minimum angle between the crosshair at spawn time and the new target, degrees
    /// (0 = off): a minimum flick distance.
    #[serde(default)]
    pub min_flick_deg: f32,
    /// Exclude the middle of the spawn area: a fraction in [0, 1) of each axis' half-width
    /// around its center (0 = off). `angular` uses yaw and pitch (humanoids yaw only),
    /// `floor` uses x and z; `around_crosshair` already excludes its middle with
    /// `offset_deg.min`.
    #[serde(default)]
    pub exclude_center: f32,
}

fn one() -> u32 {
    1
}
fn one_f() -> f32 {
    1.0
}
fn is_one(v: &f32) -> bool {
    *v == 1.0
}
fn is_zero(v: &f32) -> bool {
    *v == 0.0
}
fn zero_range() -> Range {
    Range::fixed(0.0)
}

/// Target shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Shape {
    /// Sphere target.
    Sphere {
        /// Sphere radius in meters.
        radius: f32,
    },
    /// VALORANT-sized agent standing on the floor (head / body / legs zones).
    Humanoid {
        /// Only head hits count; body and leg hits are misses.
        #[serde(default)]
        head_only: bool,
    },
}

/// Where targets spawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpawnArea {
    /// Direction relative to the starting forward view (yaw 0, pitch 0) at a distance from
    /// the eye. Humanoids ignore pitch and stand on the floor.
    Angular {
        /// Horizontal angle in degrees.
        yaw_deg: Range,
        /// Vertical angle in degrees.
        pitch_deg: Range,
        /// Distance from the eye in meters.
        distance: Range,
    },
    /// Offset from wherever the crosshair currently points (flicks, microcorrections).
    /// Humanoids only use the horizontal component.
    AroundCrosshair {
        /// Angular offset from the crosshair, in degrees.
        offset_deg: Range,
        /// Distance from the eye in meters.
        distance: Range,
        /// Absolute pitch band (degrees from the horizon, seen from the eye) that spawns
        /// must stay inside. Without it, chained flicks random-walk away from the
        /// horizon, because every spawn is relative to the previous aim.
        #[serde(default = "default_pitch_limit")]
        pitch_limit_deg: Range,
        /// Keep every spawn inside VALORANT's view as seen from the crosshair at spawn
        /// time (minus a margin, see [`crate::fov::ON_SCREEN_MARGIN_DEG`]), so a flick
        /// never starts with a search. `offset_deg.max` must then stay within
        /// [`crate::fov::valorant_on_screen_half_h_deg`].
        #[serde(default)]
        on_screen: bool,
    },
    /// World-space box on the floor. Spheres are placed at `height` above the floor.
    Floor {
        /// Range of world x, meters.
        x: Range,
        /// Range of world z, meters.
        z: Range,
        /// Height above the floor, meters.
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

/// How a target moves.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Motion {
    /// Stays where it spawned.
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
    /// Evasive duelist: the same A/D physics as `Strafe`, plus jukes (bursts of quick
    /// reversals), stop-and-go, crouches and jumps. At each decision the target jumps,
    /// crouches in place, stops or moves (reversing if it was moving), with the given
    /// chances; jumps and crouches use the VALORANT values in
    /// `docs/valorant-mechanics.md`. Spheres drop by the crouch height instead of
    /// shrinking.
    Dodge {
        /// Weapon whose run speed the target uses.
        #[serde(default = "default_weapon")]
        weapon: Weapon,
        /// Time until the next decision, seconds.
        change_secs: Range,
        /// Time until the next decision after a juke, seconds (short: a quick A-D tap).
        #[serde(default = "default_juke_secs")]
        juke_secs: Range,
        /// Probability that a decision is followed by a juke-length wait.
        #[serde(default)]
        juke_chance: f32,
        /// Probability of stopping at a decision (stop-and-go).
        #[serde(default)]
        stop_chance: f32,
        /// Probability of stopping and crouching at a decision.
        #[serde(default)]
        crouch_chance: f32,
        /// Probability of jumping at a decision. The target keeps its lateral speed in
        /// the air and decides again after landing.
        #[serde(default)]
        jump_chance: f32,
        /// Max lateral distance from the spawn point, meters.
        #[serde(default = "default_strafe_width")]
        max_offset: f32,
    },
    /// Smooth wandering in the plane facing the player (spheres / smoothness drills).
    Drift {
        /// Speed in m/s.
        speed: Range,
        /// Time in seconds between direction changes.
        change_secs: Range,
        /// Max distance from the spawn point, meters.
        #[serde(default = "default_strafe_width")]
        max_offset: f32,
    },
}

fn default_strafe_width() -> f32 {
    3.0
}

fn default_juke_secs() -> Range {
    Range::new(0.1, 0.25)
}

/// Points awarded and subtracted by a scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoringDef {
    /// Points for a body hit.
    #[serde(default = "pts_head")]
    pub head: f32,
    /// Points for a leg hit.
    #[serde(default = "pts_body")]
    pub body: f32,
    /// Points for a head hit.
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
    /// Points added per hit fired within the weapon's protected bullets (needs
    /// `weapon.recoil`). Rewards short, controlled bursts.
    #[serde(default)]
    pub burst_bonus: f32,
    /// Score-zone rings: hits near the center of a sphere or head score more.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rings: Option<ScoreRings>,
    /// Points subtracted per shot on a go/no-go decoy (see `targets.decoy_chance`).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub decoy_penalty: f32,
    /// Score decay: a target's points shrink the longer it has been visible, rewarding
    /// fast kills.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decay: Option<ScoreDecay>,
}

/// Score decay (`scoring.decay`). Points for hitting a target fall linearly from full value
/// when it appears to `floor` × full value after `window_secs`, and stay at the floor after
/// that. In hold mode the points per second decay the same way.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoreDecay {
    /// Seconds from the target appearing until its value reaches the floor.
    pub window_secs: f32,
    /// Remaining fraction of the value after the window, in [0, 1].
    pub floor: f32,
}

impl ScoreDecay {
    /// Value multiplier of a hit on a target visible for `age_secs`.
    pub fn multiplier_at(&self, age_secs: f32) -> f32 {
        let progress = (age_secs / self.window_secs).clamp(0.0, 1.0);
        1.0 - (1.0 - self.floor) * progress
    }
}

/// Score-zone rings around the center of a sphere or a humanoid head. A hit's points are
/// multiplied by the multiplier of the innermost ring it lands in, by the hit's
/// perpendicular distance from the zone center. Hits outside every ring, and body or leg
/// hits, score the zone's flat points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScoreRings {
    /// Outer radius of each ring as a fraction of the zone radius: ascending, in (0, 1].
    pub radius: Vec<f32>,
    /// Points multiplier of each ring, one per radius; every multiplier is > 0.
    pub multiplier: Vec<f32>,
}

impl ScoreRings {
    /// Multiplier for a hit `fraction` of the zone radius away from its center.
    pub fn multiplier_at(&self, fraction: f32) -> f32 {
        self.radius
            .iter()
            .zip(&self.multiplier)
            .find(|(r, _)| fraction <= **r)
            .map_or(1.0, |(_, m)| *m)
    }

    fn is_valid(&self) -> bool {
        !self.radius.is_empty()
            && self.radius.len() == self.multiplier.len()
            && self.radius.iter().all(|r| *r > 0.0 && *r <= 1.0)
            && self.radius.windows(2).all(|w| w[0] < w[1])
            && self.multiplier.iter().all(|m| m.is_finite() && *m > 0.0)
    }
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
            burst_bonus: 0.0,
            rings: None,
            decoy_penalty: 0.0,
            decay: None,
        }
    }
}

/// A semantic problem found by validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// Name of the offending field.
    pub field: &'static str,
    /// Human-readable description of the problem.
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl ScenarioDef {
    /// Parses a scenario from TOML source.
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
            !self.weapon.recoil
                || (self.weapon.kind.recoil().is_some() && self.weapon.fire == FireMode::Click),
            "weapon.recoil",
            "needs kind = \"vandal\" or \"phantom\" and fire = \"click\"",
        );
        check(
            self.scoring.burst_bonus.is_finite()
                && self.scoring.burst_bonus >= 0.0
                && (self.scoring.burst_bonus == 0.0 || self.weapon.recoil),
            "scoring.burst_bonus",
            "must be >= 0 and needs weapon.recoil = true",
        );
        check(
            self.scoring.rings.as_ref().is_none_or(ScoreRings::is_valid),
            "scoring.rings",
            "needs ascending radius fractions in (0, 1] and one multiplier > 0 per radius",
        );
        check(
            self.scoring.decoy_penalty.is_finite() && self.scoring.decoy_penalty >= 0.0,
            "scoring.decoy_penalty",
            "must be >= 0",
        );
        let decoys = self.targets.decoy_chance;
        check(
            (0.0..1.0).contains(&decoys)
                && (decoys == 0.0
                    || (self.targets.lifetime_secs.is_some()
                        && self.weapon.fire == FireMode::Click)),
            "targets.decoy_chance",
            "must be in [0, 1) and needs targets.lifetime_secs and fire = \"click\"",
        );
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
        check(
            p.elevation.is_finite()
                && (ELEVATION_RANGE.min..=ELEVATION_RANGE.max).contains(&p.elevation),
            "player.elevation",
            "must be within [-3, 6] m",
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
        check(
            (0.25..=4.0).contains(&t.speed_scale),
            "targets.speed_scale",
            "must be in [0.25, 4]",
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
                on_screen,
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
                let on_screen_limit = crate::fov::valorant_on_screen_half_h_deg() as f32;
                check(
                    !on_screen || offset_deg.max <= on_screen_limit,
                    "targets.spawn.offset_deg",
                    &format!("with on_screen, must stay within {on_screen_limit}°"),
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
        if let Some(r) = t.regen {
            check(
                r.interval_secs.is_finite()
                    && r.interval_secs > 0.0
                    && r.amount.is_finite()
                    && r.amount > 0.0
                    && t.health.is_finite(),
                "targets.regen",
                "needs interval_secs > 0, a finite amount > 0 and a finite targets.health",
            );
        }
        if let Some(d) = self.scoring.decay {
            check(
                d.window_secs.is_finite() && d.window_secs > 0.0 && (0.0..=1.0).contains(&d.floor),
                "scoring.decay",
                "needs window_secs > 0 and floor in [0, 1]",
            );
        }
        if let Some(r) = &t.spawn_rules {
            check(
                (0.0..180.0).contains(&r.avoid_last_deg),
                "targets.spawn_rules.avoid_last_deg",
                "must be in [0, 180)",
            );
            check(
                (1..=16).contains(&r.avoid_recent),
                "targets.spawn_rules.avoid_recent",
                "must be between 1 and 16",
            );
            check(
                (0.0..180.0).contains(&r.min_flick_deg),
                "targets.spawn_rules.min_flick_deg",
                "must be in [0, 180)",
            );
            check(
                (0.0..1.0).contains(&r.exclude_center)
                    && (r.exclude_center == 0.0
                        || !matches!(t.spawn, SpawnArea::AroundCrosshair { .. })),
                "targets.spawn_rules.exclude_center",
                "must be in [0, 1); with around_crosshair use offset_deg.min instead",
            );
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
            Motion::Dodge {
                change_secs,
                juke_secs,
                juke_chance,
                stop_chance,
                crouch_chance,
                jump_chance,
                max_offset,
                ..
            } => {
                check(
                    change_secs.is_valid() && change_secs.min > 0.0,
                    "targets.motion.change_secs",
                    "must be a positive [min, max]",
                );
                check(
                    juke_secs.is_valid() && juke_secs.min > 0.0,
                    "targets.motion.juke_secs",
                    "must be a positive [min, max]",
                );
                for (chance, field) in [
                    (juke_chance, "targets.motion.juke_chance"),
                    (stop_chance, "targets.motion.stop_chance"),
                    (crouch_chance, "targets.motion.crouch_chance"),
                    (jump_chance, "targets.motion.jump_chance"),
                ] {
                    check((0.0..=1.0).contains(chance), field, "must be in [0, 1]");
                }
                let shared = stop_chance + crouch_chance + jump_chance;
                check(
                    [stop_chance, crouch_chance, jump_chance]
                        .iter()
                        .any(|c| !(0.0..=1.0).contains(*c))
                        || shared <= 1.0 + 1e-6,
                    "targets.motion",
                    "stop_chance + crouch_chance + jump_chance must be <= 1",
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
    fn on_screen_defaults_off_and_rejects_offsets_past_the_screen_edge() {
        let d = ScenarioDef::from_toml(MINIMAL).unwrap();
        let SpawnArea::AroundCrosshair { on_screen, .. } = d.targets.spawn else {
            panic!("wrong spawn kind")
        };
        assert!(!on_screen);

        let src = MINIMAL.replace(
            "distance = [10, 10] }",
            "distance = [10, 10], on_screen = true }",
        );
        let ok = ScenarioDef::from_toml(&src).unwrap();
        assert!(ok.validate().is_empty(), "{:?}", ok.validate());

        let wide = src.replace("[10, 40]", "[20, 70]");
        let fields: Vec<_> = ScenarioDef::from_toml(&wide)
            .unwrap()
            .validate()
            .into_iter()
            .map(|e| e.field)
            .collect();
        assert_eq!(fields, ["targets.spawn.offset_deg"]);
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

    #[test]
    fn player_elevation_defaults_to_zero_parses_and_is_range_checked() {
        let d = ScenarioDef::from_toml(MINIMAL).unwrap();
        assert_eq!(d.player.elevation, 0.0);
        let src = format!("{MINIMAL}\n[player]\nelevation = 1.5\n");
        let raised = ScenarioDef::from_toml(&src).unwrap();
        assert_eq!(raised.player.elevation, 1.5);
        assert!(raised.validate().is_empty(), "{:?}", raised.validate());
        for bad in [-3.5, 6.5, f32::NAN] {
            let mut b = d.clone();
            b.player.elevation = bad;
            let fields: Vec<_> = b.validate().into_iter().map(|e| e.field).collect();
            assert_eq!(fields, ["player.elevation"], "{bad}");
        }
        for ok in [ELEVATION_RANGE.min, ELEVATION_RANGE.max] {
            let mut b = d.clone();
            b.player.elevation = ok;
            assert!(b.validate().is_empty(), "{ok}");
        }
    }

    #[test]
    fn dodge_motion_parses_with_defaults_and_validates_its_chances() {
        let src = format!(
            "{MINIMAL}motion = {{ kind = \"dodge\", change_secs = [0.3, 0.6], crouch_chance = 0.2 }}\n"
        );
        let d = ScenarioDef::from_toml(&src).unwrap();
        let Motion::Dodge {
            weapon,
            juke_chance,
            stop_chance,
            crouch_chance,
            jump_chance,
            max_offset,
            ..
        } = d.targets.motion
        else {
            panic!("wrong motion kind")
        };
        assert_eq!(weapon, Weapon::Vandal);
        assert_eq!(
            (juke_chance, stop_chance, crouch_chance, jump_chance),
            (0.0, 0.0, 0.2, 0.0)
        );
        assert_eq!(max_offset, 3.0);
        assert!(d.validate().is_empty(), "{:?}", d.validate());

        let typo = src.replace("crouch_chance", "crouch_chanse");
        assert!(ScenarioDef::from_toml(&typo).is_err());

        let fields = |src: String| -> Vec<&'static str> {
            ScenarioDef::from_toml(&src)
                .unwrap()
                .validate()
                .into_iter()
                .map(|e| e.field)
                .collect()
        };
        assert_eq!(
            fields(src.replace("crouch_chance = 0.2", "crouch_chance = 1.5")),
            ["targets.motion.crouch_chance"]
        );
        assert_eq!(
            fields(src.replace(
                "crouch_chance = 0.2",
                "crouch_chance = 0.5, stop_chance = 0.4, jump_chance = 0.3"
            )),
            ["targets.motion"],
            "stop, crouch and jump share one decision, so they must sum to <= 1"
        );
        assert_eq!(
            fields(src.replace("crouch_chance = 0.2", "juke_secs = [0.0, 0.1]")),
            ["targets.motion.juke_secs"]
        );
    }

    #[test]
    fn recoil_and_burst_bonus_default_off_and_are_validated() {
        let d = ScenarioDef::from_toml(MINIMAL).unwrap();
        assert!(!d.weapon.recoil);
        assert_eq!(d.scoring.burst_bonus, 0.0);

        let mut bad = d.clone();
        bad.weapon.kind = Weapon::Guardian;
        bad.weapon.recoil = true;
        bad.scoring.burst_bonus = 10.0;
        let fields: Vec<_> = bad.validate().into_iter().map(|e| e.field).collect();
        assert_eq!(fields, ["weapon.recoil"]);

        let mut no_recoil = d.clone();
        no_recoil.scoring.burst_bonus = 10.0;
        let fields: Vec<_> = no_recoil.validate().into_iter().map(|e| e.field).collect();
        assert_eq!(fields, ["scoring.burst_bonus"]);

        let mut hold = d;
        hold.weapon.recoil = true;
        hold.weapon.fire = FireMode::Hold;
        let fields: Vec<_> = hold.validate().into_iter().map(|e| e.field).collect();
        assert_eq!(fields, ["weapon.recoil"]);
    }

    #[test]
    fn score_rings_default_off_parse_and_are_validated() {
        let d = ScenarioDef::from_toml(MINIMAL).unwrap();
        assert_eq!(d.scoring.rings, None);

        let src = format!(
            "{MINIMAL}\n[scoring]\nrings = {{ radius = [0.3, 0.6, 1.0], multiplier = [3.0, 2.0, 1.0] }}\n"
        );
        let ringed = ScenarioDef::from_toml(&src).unwrap();
        assert_eq!(
            ringed.scoring.rings,
            Some(ScoreRings {
                radius: vec![0.3, 0.6, 1.0],
                multiplier: vec![3.0, 2.0, 1.0],
            })
        );
        assert!(ringed.validate().is_empty(), "{:?}", ringed.validate());
        assert!(ScenarioDef::from_toml(&src.replace("multiplier", "multiplyer")).is_err());

        let fields = |radius: Vec<f32>, multiplier: Vec<f32>| -> Vec<&'static str> {
            let mut b = d.clone();
            b.scoring.rings = Some(ScoreRings { radius, multiplier });
            b.validate().into_iter().map(|e| e.field).collect()
        };
        let bad = ["scoring.rings"];
        assert_eq!(fields(vec![], vec![]), bad, "empty");
        assert_eq!(fields(vec![0.5, 0.3], vec![2.0, 1.0]), bad, "descending");
        assert_eq!(fields(vec![0.5, 0.5], vec![2.0, 1.0]), bad, "repeated");
        assert_eq!(fields(vec![0.0, 0.5], vec![2.0, 1.0]), bad, "zero radius");
        assert_eq!(fields(vec![0.5, 1.2], vec![2.0, 1.0]), bad, "past the edge");
        assert_eq!(fields(vec![0.5], vec![0.0]), bad, "zero multiplier");
        assert_eq!(fields(vec![0.5], vec![f32::NAN]), bad, "NaN multiplier");
        assert_eq!(fields(vec![0.5, 1.0], vec![2.0]), bad, "lengths differ");
        assert!(fields(vec![1.0], vec![0.5]).is_empty());
    }

    /// `MINIMAL` with an `angular` spawn area instead of `around_crosshair`.
    fn angular_minimal() -> String {
        MINIMAL.replace(
            r#"spawn = { kind = "around_crosshair", offset_deg = [10, 40], distance = [10, 10] }"#,
            r#"spawn = { kind = "angular", yaw_deg = [-40, 40], pitch_deg = [-5, 10], distance = [10, 10] }"#,
        )
    }

    fn fields_of(d: &ScenarioDef) -> Vec<&'static str> {
        d.validate().into_iter().map(|e| e.field).collect()
    }

    #[test]
    fn spawn_rules_default_off_stay_out_of_saved_files_parse_and_are_validated() {
        let d = ScenarioDef::from_toml(MINIMAL).unwrap();
        assert_eq!(d.targets.spawn_rules, None);
        assert!(!toml::to_string(&d).unwrap().contains("spawn_rules"));

        let src = format!(
            "{}\n[targets.spawn_rules]\navoid_last_deg = 12\nmin_flick_deg = 15\nexclude_center = 0.3\n",
            angular_minimal()
        );
        let ruled = ScenarioDef::from_toml(&src).unwrap();
        assert_eq!(
            ruled.targets.spawn_rules,
            Some(SpawnRules {
                avoid_last_deg: 12.0,
                avoid_recent: 1,
                min_flick_deg: 15.0,
                exclude_center: 0.3,
            })
        );
        assert!(ruled.validate().is_empty(), "{:?}", ruled.validate());
        assert!(ScenarioDef::from_toml(&src.replace("min_flick_deg", "min_flik_deg")).is_err());

        let with = |f: &dyn Fn(&mut SpawnRules)| {
            let mut b = ruled.clone();
            f(b.targets.spawn_rules.as_mut().unwrap());
            fields_of(&b)
        };
        for bad in [-1.0, 180.0, f32::NAN] {
            assert_eq!(
                with(&|r| r.avoid_last_deg = bad),
                ["targets.spawn_rules.avoid_last_deg"],
                "{bad}"
            );
            assert_eq!(
                with(&|r| r.min_flick_deg = bad),
                ["targets.spawn_rules.min_flick_deg"],
                "{bad}"
            );
        }
        for bad in [0, 17] {
            assert_eq!(
                with(&|r| r.avoid_recent = bad),
                ["targets.spawn_rules.avoid_recent"],
                "{bad}"
            );
        }
        for bad in [-0.1, 1.0, f32::NAN] {
            assert_eq!(
                with(&|r| r.exclude_center = bad),
                ["targets.spawn_rules.exclude_center"],
                "{bad}"
            );
        }

        let mut around = d.clone();
        around.targets.spawn_rules = ruled.targets.spawn_rules.clone();
        assert_eq!(
            fields_of(&around),
            ["targets.spawn_rules.exclude_center"],
            "around_crosshair excludes its middle with offset_deg.min"
        );
        around.targets.spawn_rules.as_mut().unwrap().exclude_center = 0.0;
        assert!(fields_of(&around).is_empty());
    }

    #[test]
    fn regen_and_score_decay_default_off_stay_out_of_saved_files_parse_and_are_validated() {
        let d = ScenarioDef::from_toml(MINIMAL).unwrap();
        assert_eq!((d.targets.regen, d.scoring.decay), (None, None));
        let saved = toml::to_string(&d).unwrap();
        assert!(
            !saved.contains("regen") && !saved.contains("decay"),
            "{saved}"
        );

        let src = MINIMAL.replace(
            "distance = [10, 10] }",
            "distance = [10, 10] }\nhealth = 3\nregen = { interval_secs = 0.5, amount = 1, reset_on_hit = true }\n[scoring]\ndecay = { window_secs = 1.5, floor = 0.3 }",
        );
        let r = ScenarioDef::from_toml(&src).unwrap();
        assert_eq!(
            r.targets.regen,
            Some(RegenDef {
                interval_secs: 0.5,
                amount: 1.0,
                reset_on_hit: true,
            })
        );
        assert_eq!(
            r.scoring.decay,
            Some(ScoreDecay {
                window_secs: 1.5,
                floor: 0.3,
            })
        );
        assert!(r.validate().is_empty(), "{:?}", r.validate());
        assert!(ScenarioDef::from_toml(&src.replace("reset_on_hit", "reset_on_hits")).is_err());
        assert!(ScenarioDef::from_toml(&src.replace("floor = 0.3", "flor = 0.3")).is_err());
        let no_reset = src.replace(", reset_on_hit = true", "");
        let defaulted = ScenarioDef::from_toml(&no_reset).unwrap();
        assert!(!defaulted.targets.regen.unwrap().reset_on_hit);

        for (interval, amount) in [
            (0.0, 1.0),
            (f32::NAN, 1.0),
            (0.5, 0.0),
            (0.5, f32::INFINITY),
        ] {
            let mut b = r.clone();
            b.targets.regen = Some(RegenDef {
                interval_secs: interval,
                amount,
                reset_on_hit: false,
            });
            assert_eq!(fields_of(&b), ["targets.regen"], "{interval} {amount}");
        }
        let mut immortal = r.clone();
        immortal.weapon.fire = FireMode::Hold;
        immortal.targets.health = f32::INFINITY;
        assert_eq!(
            fields_of(&immortal),
            ["targets.regen"],
            "an unkillable target has nothing to heal"
        );

        for (window, floor) in [(0.0, 0.3), (f32::NAN, 0.3), (1.0, -0.1), (1.0, 1.1)] {
            let mut b = r.clone();
            b.scoring.decay = Some(ScoreDecay {
                window_secs: window,
                floor,
            });
            assert_eq!(fields_of(&b), ["scoring.decay"], "{window} {floor}");
        }
    }

    #[test]
    fn score_decay_falls_linearly_from_full_value_to_the_floor() {
        let d = ScoreDecay {
            window_secs: 2.0,
            floor: 0.25,
        };
        for (age, expected) in [
            (-1.0, 1.0),
            (0.0, 1.0),
            (1.0, 0.625),
            (2.0, 0.25),
            (9.0, 0.25),
        ] {
            assert!((d.multiplier_at(age) - expected).abs() < 1e-6, "{age}");
        }
    }

    #[test]
    fn decoys_default_off_stay_out_of_saved_files_and_are_validated() {
        let d = ScenarioDef::from_toml(MINIMAL).unwrap();
        assert_eq!(
            (d.targets.decoy_chance, d.scoring.decoy_penalty),
            (0.0, 0.0)
        );
        let saved = toml::to_string(&d).unwrap();
        assert!(
            !saved.contains("decoy") && !saved.contains("rings"),
            "{saved}"
        );

        let src = MINIMAL.replace(
            "distance = [10, 10] }",
            "distance = [10, 10] }\nlifetime_secs = 1.5\ndecoy_chance = 0.3\n[scoring]\ndecoy_penalty = 150",
        );
        let go = ScenarioDef::from_toml(&src).unwrap();
        assert_eq!(
            (go.targets.decoy_chance, go.scoring.decoy_penalty),
            (0.3, 150.0)
        );
        assert!(go.validate().is_empty(), "{:?}", go.validate());
        assert!(ScenarioDef::from_toml(&src.replace("decoy_chance", "decoy_chanse")).is_err());

        let fields = |b: ScenarioDef| -> Vec<&'static str> {
            b.validate().into_iter().map(|e| e.field).collect()
        };
        for bad in [-0.1, 1.0, f32::NAN] {
            let mut b = go.clone();
            b.targets.decoy_chance = bad;
            assert_eq!(fields(b), ["targets.decoy_chance"], "{bad}");
        }
        let mut forever = go.clone();
        forever.targets.lifetime_secs = None;
        assert_eq!(
            fields(forever),
            ["targets.decoy_chance"],
            "a decoy must be able to leave"
        );
        let mut hold = go.clone();
        hold.weapon.fire = FireMode::Hold;
        assert_eq!(fields(hold), ["targets.decoy_chance"]);
        let mut negative = go;
        negative.scoring.decoy_penalty = -5.0;
        assert_eq!(fields(negative), ["scoring.decoy_penalty"]);
    }
}
