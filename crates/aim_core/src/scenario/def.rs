//! Data-driven scenario definitions. A scenario is a TOML file composed of independent
//! building blocks (target shape, spawn area, motion, fire mode, scoring), so new
//! exercises are written as data instead of code. See `docs/scenarios.md`.

use super::phase::{FailDef, PhaseDef, RunEnd};
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
    /// Game whose parameters the scenario follows (movement physics, weapons, target
    /// sizes), used to filter the library. `None`: generic, game-agnostic aim training.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<crate::games::Game>,
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
    /// How the run ends: after `duration_secs` (default), at a number of kills, or endless
    /// until a fail condition (see `scenario::phase`).
    #[serde(default, skip_serializing_if = "RunEnd::is_time")]
    pub end: RunEnd,
    /// Fail conditions that end the run early as failed.
    #[serde(default, skip_serializing_if = "FailDef::is_empty")]
    pub fail: FailDef,
    /// Ordered phases (waves): each overrides part of `targets` and advances on a trigger.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phases: Option<Vec<PhaseDef>>,
    /// Camera options: an FOV lock for this scenario only.
    #[serde(default, skip_serializing_if = "ViewDef::is_default")]
    pub view: ViewDef,
    /// Hit feedback for this scenario: hide markers or sounds, opt into damage numbers and
    /// health bars.
    #[serde(default, skip_serializing_if = "FeedbackDef::is_default")]
    pub feedback: FeedbackDef,
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
    /// Where the player's feet start, `[x, z]` in meters (default: the origin). Written by
    /// the trainer at the end of the start countdown so the run and its replays begin where
    /// the player moved to; must lie inside the bounds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<[f32; 2]>,
    /// The player's velocity when the run starts, `[right, forward]` in m/s in the player's
    /// own frame (default: standing still). Written with `start`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_velocity: Option<[f32; 2]>,
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
            start: None,
            start_velocity: None,
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
    /// Reactive behaviour: the target answers the player's strafes, crosshair and shots.
    /// Needs a `strafe` or `dodge` motion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reactions: Option<ReactionsDef>,
    /// Conditional behaviour switching: an ordered list of rules; each frame the first
    /// rule whose conditions all hold overrides the target's motion.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub behaviors: Vec<BehaviorRule>,
    /// Perception drill: how targets are announced and revealed (see
    /// [`super::cue::Perception`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub perception: Option<super::cue::Perception>,
    /// Every kill shrinks the targets: their size is multiplied by `factor`, down to
    /// `floor` times the starting size. Spheres only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shrink_per_kill: Option<ShrinkDef>,
    /// Weighted motion profiles: each target picks one by weight when it spawns and
    /// again every `switch_secs`. When set, they replace `motion`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub motion_profiles: Vec<MotionProfile>,
    /// Spider-shot pattern: spawns alternate between a centre target, drawn from `spawn`,
    /// and a peripheral target placed around the last centre target. Needs `max_alive = 1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spider: Option<SpiderDef>,
}

/// Spider-shot spawning (`targets.spider`). The 1st, 3rd, 5th… spawn is a centre target,
/// drawn from `targets.spawn` as usual (a fixed spawn area gives a fixed centre). Every
/// other spawn is a peripheral target `ring_deg` away from the last centre target, seen
/// from the player's eye at spawn time, in a random direction (humanoids: left or right)
/// and at the same distance.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpiderDef {
    /// Angle between the centre target and a peripheral one, degrees, within (0, 60].
    pub ring_deg: Range,
}

/// Widest `targets.spider.ring_deg`, degrees: peripheral targets stay on screen.
pub const MAX_SPIDER_RING_DEG: f32 = 60.0;

/// How a target reacts to the player (`targets.reactions`). A reaction sets the target's
/// strafe direction and holds it for `react_secs`, overriding the motion's own decisions
/// for that long. Airborne `dodge` targets do not react until they land.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReactionsDef {
    /// Chance that a new player strafe makes the target strafe the same way (mirror).
    #[serde(default)]
    pub mirror_strafe_chance: f32,
    /// Chance that a new player strafe makes the target strafe the other way (oppose).
    /// `mirror_strafe_chance + oppose_strafe_chance` must be <= 1.
    #[serde(default)]
    pub oppose_strafe_chance: f32,
    /// Dodge away from the crosshair when it comes within this angle of the target's aim
    /// point, degrees (0 = off).
    #[serde(default)]
    pub dodge_within_deg: f32,
    /// Chance that the target jukes (reverses, or starts moving away from the crosshair)
    /// whenever the player fires.
    #[serde(default)]
    pub shot_juke_chance: f32,
    /// How long a reaction holds before the motion decides again, seconds.
    #[serde(default = "default_react_secs")]
    pub react_secs: Range,
}

fn default_react_secs() -> Range {
    Range::new(0.2, 0.4)
}

/// One conditional behaviour rule (`[[targets.behaviors]]`): when every condition in
/// `when` holds, the rule's overrides apply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorRule {
    /// Conditions; every one that is set must hold.
    pub when: BehaviorWhen,
    /// Motion to use instead of `targets.motion`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<Motion>,
    /// Multiplier on the target's movement speed, in [0.25, 4].
    #[serde(default = "one_f", skip_serializing_if = "is_one")]
    pub speed_scale: f32,
    /// Move straight away from the player, up to this many meters from where the target
    /// started retreating in total (0 = off).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub retreat_m: f32,
}

/// Conditions of a [`BehaviorRule`]. At least one must be set.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorWhen {
    /// Remaining health is below this fraction of the starting health, in (0, 1].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_below: Option<f32>,
    /// The target took damage within the last this many seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub damaged_within_secs: Option<f32>,
    /// The target has been visible for at least this many seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alive_for_secs: Option<f32>,
    /// The crosshair is within this angle of the target's aim point, degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crosshair_within_deg: Option<f32>,
    /// The target moves faster than this, m/s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_above: Option<f32>,
}

/// Most behaviour rules a scenario may list.
pub const MAX_BEHAVIOR_RULES: usize = 8;

/// Farthest a behaviour rule may make a target retreat, meters.
pub const MAX_RETREAT_M: f32 = 20.0;

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
    /// Upright capsule (a cylinder with rounded ends) centered on the target's position,
    /// with an optional spherical head above it. Body hits score `body`, head hits `head`.
    Capsule {
        /// Radius of the cylinder and its end caps, meters.
        radius: f32,
        /// Total height including both caps, meters (≥ 2 × radius).
        height: f32,
        /// Optional head sphere.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        head: Option<CapsuleHead>,
    },
}

impl Shape {
    /// For a capsule: the height of its aim point above its center (the head's center, or
    /// the center itself without a head) and the radius of that zone.
    pub fn capsule_aim(&self) -> Option<(f32, f32)> {
        match *self {
            Shape::Capsule { radius, head, .. } => {
                Some(head.map_or((0.0, radius), |h| (h.offset, h.radius)))
            }
            _ => None,
        }
    }
}

/// The head of a capsule target.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapsuleHead {
    /// Head sphere radius, meters.
    pub radius: f32,
    /// Height of the head's center above the capsule's center, meters.
    pub offset: f32,
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
        /// Snap spawns to a grid of `rows` × `cols` points spanning the yaw and pitch
        /// ranges (humanoids use the columns only).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        grid: Option<SpawnGrid>,
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
        /// Optional forward/back movement inside a distance band (W/S on top of A/D).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        depth: Option<DepthDef>,
        /// Optional strafe rhythm: pauses at swaps, uneven sides, balanced randomness.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rhythm: Option<Rhythm>,
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
        /// Optional forward/back movement inside a distance band (W/S on top of A/D).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        depth: Option<DepthDef>,
        /// Optional strafe rhythm: pauses at swaps, uneven sides, balanced randomness.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rhythm: Option<Rhythm>,
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
        /// Optional forward/back movement inside a distance band, at the drift speed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        depth: Option<DepthDef>,
    },
    /// Thrown under gravity: launched once, at a random speed and angle, sideways along
    /// the target's lateral axis (a random side). It falls, bounces off the floor
    /// `bounces` times, and then lands, which removes it (see `on_floor`).
    Ballistic {
        /// Launch speed, m/s.
        speed: Range,
        /// Launch angle above the horizon, degrees within [-90, 90]: 90 throws straight
        /// up, 0 sideways, negative angles downwards.
        angle_deg: Range,
        /// Downward acceleration, m/s² (> 0). Default: Earth's standard gravity.
        #[serde(default = "default_gravity")]
        gravity: f32,
        /// Floor bounces before the target lands.
        #[serde(default)]
        bounces: u32,
        /// Share of the vertical speed kept by a bounce, in (0, 1].
        #[serde(default = "default_restitution")]
        restitution: f32,
        /// What landing does: the target expires, or the run fails.
        #[serde(default)]
        on_floor: FloorAction,
    },
    /// Straight lines at a constant angular speed in a random direction, reflecting off
    /// the walls of an angular arena seen from the player's eye at spawn: a box, or a
    /// regular polygon inside it. Humanoids move horizontally only.
    LinearBounce {
        /// Angular speed, degrees per second.
        speed_deg: Range,
        /// The walls the target bounces between.
        arena: BounceArena,
    },
    /// Circles around the player (his eye at spawn) at a fixed horizontal radius and
    /// height, at an angular speed, optionally reversing on a timer.
    Orbit {
        /// Angular speed around the player, degrees per second.
        speed_deg: Range,
        /// Horizontal radius, meters (≥ 1). Default: the target's distance at spawn.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        radius: Option<f32>,
        /// Time between direction decisions, seconds. Without it the target never turns.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        change_secs: Option<Range>,
        /// Chance that a decision reverses the direction, in [0, 1].
        #[serde(default = "default_reverse_chance")]
        reverse_chance: f32,
    },
    /// Glides sideways along its lateral axis at a constant speed while weaving up and
    /// down on a periodic path, a smooth sine or a sharp zigzag, and turns back at
    /// `max_offset`. It starts at a random side and a random point of its cycle. Spheres
    /// and capsules only.
    Wave {
        /// Lateral speed, m/s.
        speed: Range,
        /// Height of the weave above and below its center line, meters (> 0).
        amplitude: Range,
        /// Full up-and-down cycles per second (> 0).
        frequency_hz: Range,
        /// Shape of the weave.
        #[serde(default)]
        form: WaveForm,
        /// Max lateral distance from the spawn point, meters.
        #[serde(default = "default_strafe_width")]
        max_offset: f32,
    },
}

/// Shape of a `wave` motion's weave (`motion.form`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveForm {
    /// A sine: it slows into each turn.
    #[default]
    Sine,
    /// A triangle: constant vertical speed with sharp turns.
    Zigzag,
}

impl WaveForm {
    /// Height of the weave, in [-1, 1], at `phase` cycles (0 is the center line, rising).
    pub fn at(self, phase: f32) -> f32 {
        match self {
            Self::Sine => (phase * std::f32::consts::TAU).sin(),
            Self::Zigzag => 4.0 * ((phase - 0.25).rem_euclid(1.0) - 0.5).abs() - 1.0,
        }
    }
}

/// The rhythm of a `strafe` or `dodge` target (`motion.rhythm`): how it changes
/// direction, on top of `change_secs`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rhythm {
    /// Stand still for this long at every change of direction (a reversal or a turn at the
    /// lane edge) before moving the other way, seconds within [0, 2].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swap_pause_secs: Option<Range>,
    /// Multiplies the time between decisions while moving to the player's left, within
    /// [0.25, 4]. With `right_secs_mult` it makes one side's strafes longer.
    #[serde(default = "one_f")]
    pub left_secs_mult: f32,
    /// Multiplies the time between decisions while moving to the player's right.
    #[serde(default = "one_f")]
    pub right_secs_mult: f32,
    /// Balanced randomness: the times between decisions come in shuffled rounds of four,
    /// one from each quarter of the range, and the side picked from a stop alternates in
    /// shuffled pairs, so neither long streaks nor droughts happen.
    #[serde(default)]
    pub balanced: bool,
}

/// Longest `rhythm.swap_pause_secs`, seconds.
pub const MAX_SWAP_PAUSE_SECS: f32 = 2.0;
/// Range of `rhythm.left_secs_mult` and `rhythm.right_secs_mult`.
pub const SIDE_SECS_MULT: Range = Range::new(0.25, 4.0);

impl Motion {
    /// The strafe rhythm of a `strafe` or `dodge` motion.
    pub fn rhythm(&self) -> Option<&Rhythm> {
        match self {
            Self::Strafe { rhythm, .. } | Self::Dodge { rhythm, .. } => rhythm.as_ref(),
            _ => None,
        }
    }
}

impl Rhythm {
    /// Multiplier of the time between decisions while moving to `side` (-1 left, 1 right;
    /// 1 when stopped).
    pub fn secs_mult(&self, side: f32) -> f32 {
        if side < 0.0 {
            self.left_secs_mult
        } else if side > 0.0 {
            self.right_secs_mult
        } else {
            1.0
        }
    }
}

/// Forward/back movement of a `strafe`, `dodge` or `drift` target (`motion.depth`). The
/// target walks towards or away from the player, deciding again every `change_secs` and
/// turning back at the edges of the distance band.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DepthDef {
    /// Time between forward/back decisions, seconds.
    pub change_secs: Range,
    /// Horizontal distance from the player's eye (at spawn) the target stays within, meters.
    pub distance: Range,
}

/// What a `ballistic` target does when it lands (`motion.on_floor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FloorAction {
    /// It expires, like a target whose lifetime ran out.
    #[default]
    Expire,
    /// It expires and the run fails at once.
    Fail,
}

/// Walls of a `linear_bounce` motion: an angular box of yaw × pitch seen from the
/// player's eye at spawn, or, with `sides` ≥ 3, the regular polygon inscribed in it (a
/// vertex at the top).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BounceArena {
    /// Yaw range, degrees within [-180, 180].
    pub yaw_deg: Range,
    /// Pitch range, degrees within ±89 (ignored by humanoids).
    pub pitch_deg: Range,
    /// 0 for the box, or 3 to 12 for a polygon.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub sides: u32,
}

fn default_strafe_width() -> f32 {
    3.0
}

/// Standard gravity, m/s² (the conventional value of Earth's surface gravity).
fn default_gravity() -> f32 {
    9.81
}

fn default_restitution() -> f32 {
    0.5
}

fn default_reverse_chance() -> f32 {
    0.5
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
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
    /// Multiplies the final score by the run's accuracy (`linear`) or its square root
    /// (`sqrt`); `none` by default.
    #[serde(default, skip_serializing_if = "AccuracyMult::is_none")]
    pub accuracy_mult: AccuracyMult,
    /// Stop-timing reward: extra points for a hit fired soon after the player came to an
    /// accurate stop (needs `player.movement`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_window: Option<StopWindow>,
}

/// Stop-timing reward (`scoring.stop_window`). A hit fired while accurate, at most `secs`
/// after the last moment the player moved faster than the accuracy speed limit, scores
/// `bonus` extra points. Every bullet of a short burst fired inside the window earns it, so
/// a 2–4 shot burst right after a counter-strafe does and a long spray does not.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopWindow {
    /// Seconds after reaching accurate speed during which a hit earns the bonus, in (0, 2].
    pub secs: f32,
    /// Points added per hit inside the window, ≥ 0.
    pub bonus: f32,
}

/// Longest `scoring.stop_window.secs`, seconds: a design choice, well past the ≈0.16 s a
/// counter-strafe takes, so the window still means "right after the stop".
pub const MAX_STOP_WINDOW_SECS: f32 = 2.0;

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

/// How the final score depends on accuracy (`scoring.accuracy_mult`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccuracyMult {
    /// The score as earned.
    #[default]
    None,
    /// Score × accuracy.
    Linear,
    /// Score × √accuracy: a gentler penalty.
    Sqrt,
}

impl AccuracyMult {
    /// Score multiplier for a run with `accuracy` in [0, 1].
    pub fn factor(self, accuracy: f32) -> f32 {
        match self {
            AccuracyMult::None => 1.0,
            AccuracyMult::Linear => accuracy,
            AccuracyMult::Sqrt => accuracy.max(0.0).sqrt(),
        }
    }

    fn is_none(&self) -> bool {
        *self == AccuracyMult::None
    }
}

/// Per-kill shrinking (`targets.shrink_per_kill`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShrinkDef {
    /// Size multiplier applied at every kill, in (0, 1).
    pub factor: f32,
    /// Smallest size, as a fraction of the starting size, in (0, 1].
    pub floor: f32,
}

/// Grid snapping of an `angular` spawn area (`spawn.grid`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnGrid {
    /// Pitch rows, 1–16, from `pitch_deg.min` to `pitch_deg.max` (one row sits in the middle).
    pub rows: u32,
    /// Yaw columns, 1–16, from `yaw_deg.min` to `yaw_deg.max` (one column sits in the middle).
    pub cols: u32,
}

impl SpawnGrid {
    /// The value of grid line `i` of `n` across `r`.
    pub(super) fn line(r: Range, i: u32, n: u32) -> f32 {
        if n <= 1 {
            r.lerp(0.5)
        } else {
            r.lerp(i as f32 / (n - 1) as f32)
        }
    }
}

/// One weighted motion profile (`[[targets.motion_profiles]]`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionProfile {
    /// Relative chance of picking this profile, ≥ 0.
    #[serde(default = "one_f")]
    pub weight: f32,
    /// Time until the target picks a profile again, seconds; without it the pick lasts
    /// the target's life.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub switch_secs: Option<Range>,
    /// The motion, written like `targets.motion` (not `ballistic`).
    pub motion: Motion,
}

/// Most motion profiles a scenario may list.
pub const MAX_MOTION_PROFILES: usize = 8;

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
            accuracy_mult: AccuracyMult::None,
            stop_window: None,
        }
    }
}

/// Vertical FOVs, degrees, an FOV lock may use. A design choice: wide enough for every
/// common game FOV (VALORANT's is ≈70.5°), narrow enough to keep the view usable.
pub const FOV_LOCK_RANGE: Range = Range::new(30.0, 120.0);

/// Camera options of a scenario.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewDef {
    /// Vertical FOV for this scenario, degrees: a fixed value (`fov_lock = 60`) or a
    /// `[min, max]` the hipfire FOV is clamped into. `None` keeps VALORANT's FOV.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fov_lock: Option<FovLock>,
}

impl ViewDef {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Vertical FOV the camera uses, degrees: VALORANT's hipfire FOV with the lock applied,
    /// or, for `zoom` > 1, that FOV narrowed the way VALORANT zooms (horizontal FOV / zoom).
    pub fn vertical_fov_deg(&self, zoom: Option<f64>) -> f64 {
        use crate::fov;
        let zoom = zoom.filter(|z| *z > 1.0);
        let Some(lock) = self.fov_lock else {
            return zoom.map_or_else(
                fov::valorant_vertical_fov_deg,
                fov::valorant_zoomed_vertical_fov_deg,
            );
        };
        let hip = lock.clamp(fov::valorant_vertical_fov_deg());
        match zoom {
            None => hip,
            Some(z) => {
                let aspect = fov::VALORANT_MAX_ASPECT;
                fov::vertical_from_horizontal(
                    fov::horizontal_from_vertical(hip, aspect) / z,
                    aspect,
                )
            }
        }
    }
}

/// A scenario's FOV lock, vertical degrees.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FovLock {
    /// Always this FOV.
    Fixed(f32),
    /// The hipfire FOV clamped into `[min, max]`.
    Range(Range),
}

impl FovLock {
    /// The allowed FOVs as a range (a fixed lock is a one-value range).
    pub fn bounds(self) -> Range {
        match self {
            FovLock::Fixed(v) => Range::fixed(v),
            FovLock::Range(r) => r,
        }
    }

    /// `vfov_deg` clamped into the lock.
    pub fn clamp(self, vfov_deg: f64) -> f64 {
        let b = self.bounds();
        vfov_deg.clamp(b.min as f64, b.max as f64)
    }

    fn is_valid(self) -> bool {
        let b = self.bounds();
        b.is_valid() && b.min >= FOV_LOCK_RANGE.min && b.max <= FOV_LOCK_RANGE.max
    }
}

/// Per-scenario hit feedback. Markers and sounds follow the player's settings unless the
/// scenario hides them; damage numbers and health bars exist only per scenario (opt-in).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FeedbackDef {
    /// `false` hides hit markers in this scenario.
    pub hit_markers: bool,
    /// `false` mutes hit and kill sounds in this scenario.
    pub hit_sounds: bool,
    /// Floating damage numbers at each hit (click scenarios).
    pub damage_numbers: bool,
    /// A small bar over each target whose starting health is above 1.
    pub health_bars: bool,
}

impl Default for FeedbackDef {
    fn default() -> Self {
        Self {
            hit_markers: true,
            hit_sounds: true,
            damage_numbers: false,
            health_bars: false,
        }
    }
}

impl FeedbackDef {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Whether hit markers show, given whether the player's own markers are on.
    pub fn hit_markers_shown(&self, player_on: bool) -> bool {
        player_on && self.hit_markers
    }

    /// Whether hit and kill sounds play, given whether the player's own sounds are on.
    pub fn hit_sounds_played(&self, player_on: bool) -> bool {
        player_on && self.hit_sounds
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

    /// A stable 64-bit hash of the definition's content (FNV-1a over its canonical TOML), so
    /// run history can tell an edited scenario from the original. Formatting and comments
    /// in the source file do not change it; any changed value does. Blocks left at their
    /// defaults are not serialized, so adding an optional block to the engine keeps the
    /// hashes of scenarios that do not use it.
    pub fn content_hash(&self) -> u64 {
        // Every definition serializes (replays embed it the same way); the debug form is a
        // stable fallback that still changes with every value.
        let hashed = self.hashed();
        let canonical = toml::to_string(&*hashed).unwrap_or_else(|_| format!("{hashed:?}"));
        fnv1a_64(canonical.as_bytes())
    }

    /// The definition as hashed: the `game` tag does not change gameplay, so it is left
    /// out and tagging a scenario keeps its replays verifiable and its lab id.
    pub(crate) fn hashed(&self) -> std::borrow::Cow<'_, Self> {
        if self.game.is_none() {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut untagged = self.clone();
        untagged.game = None;
        std::borrow::Cow::Owned(untagged)
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
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
                && !is_windows_device_name(&self.id),
            "id",
            "must be non-empty, only contain a-z, 0-9, '-' or '_', and not be a Windows \
             device name (con, nul, aux, prn, com1-9, lpt1-9)",
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
        check(
            p.start.is_none_or(|[x, z]| {
                (p.bounds_x.min..=p.bounds_x.max).contains(&x)
                    && (p.bounds_z.min..=p.bounds_z.max).contains(&z)
            }),
            "player.start",
            "must be inside the player bounds",
        );
        check(
            p.start_velocity
                .is_none_or(|v| v.iter().all(|c| c.is_finite())),
            "player.start_velocity",
            "must be finite",
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
        if let Shape::Capsule {
            radius,
            height,
            head,
        } = t.shape
        {
            check(
                radius.is_finite() && radius > 0.0 && height.is_finite() && height >= 2.0 * radius,
                "targets.shape",
                "a capsule needs radius > 0 and height >= 2 × radius",
            );
            check(
                head.is_none_or(|h| {
                    h.radius.is_finite()
                        && h.radius > 0.0
                        && h.offset.is_finite()
                        && h.offset >= 0.0
                }),
                "targets.shape.head",
                "needs radius > 0 and offset >= 0",
            );
        }

        match &t.spawn {
            SpawnArea::Angular {
                yaw_deg,
                pitch_deg,
                distance,
                ..
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
        if let Some(w) = self.scoring.stop_window {
            check(
                w.secs.is_finite()
                    && w.secs > 0.0
                    && w.secs <= MAX_STOP_WINDOW_SECS
                    && w.bonus.is_finite()
                    && w.bonus >= 0.0,
                "scoring.stop_window",
                "needs secs in (0, 2] and a finite bonus >= 0",
            );
            check(
                self.player.movement && self.weapon.fire == FireMode::Click,
                "scoring.stop_window",
                "rewards shots after the player stops: needs player.movement = true and \
                 weapon.fire = \"click\"",
            );
        }
        if let Some(s) = t.spider {
            check(
                s.ring_deg.is_valid()
                    && s.ring_deg.min > 0.0
                    && s.ring_deg.max <= MAX_SPIDER_RING_DEG,
                "targets.spider.ring_deg",
                "must be a [min, max] within (0, 60]",
            );
            check(
                t.max_alive == 1 && t.decoy_chance == 0.0 && t.perception.is_none(),
                "targets.spider",
                "alternates one target at a time: needs max_alive = 1, no decoys and no \
                 perception drill",
            );
        }
        if let Some(r) = &t.spawn_rules {
            check(
                (0.0..180.0).contains(&r.avoid_last_deg),
                "targets.spawn_rules.avoid_last_deg",
                "must be in [0, 180)",
            );
            check(
                (1..=16).contains(&r.avoid_recent)
                    && (r.avoid_recent == 1 || r.avoid_last_deg > 0.0),
                "targets.spawn_rules.avoid_recent",
                "must be between 1 and 16; more than 1 needs avoid_last_deg > 0",
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

        for (field, msg) in motion_problems(&t.motion) {
            check(false, field, msg);
        }
        if let Some(s) = t.shrink_per_kill {
            check(
                s.factor > 0.0 && s.factor < 1.0 && s.floor > 0.0 && s.floor <= 1.0,
                "targets.shrink_per_kill",
                "needs factor in (0, 1) and floor in (0, 1]",
            );
            check(
                matches!(t.shape, Shape::Sphere { .. }),
                "targets.shrink_per_kill",
                "needs a sphere shape",
            );
        }
        if let SpawnArea::Angular { grid: Some(g), .. } = &t.spawn {
            check(
                (1..=16).contains(&g.rows) && (1..=16).contains(&g.cols),
                "targets.spawn.grid",
                "rows and cols must be between 1 and 16",
            );
        }
        let profiles = &t.motion_profiles;
        check(
            profiles.len() <= MAX_MOTION_PROFILES,
            "targets.motion_profiles",
            &format!("at most {MAX_MOTION_PROFILES} profiles"),
        );
        check(
            profiles.is_empty()
                || (profiles
                    .iter()
                    .all(|p| p.weight.is_finite() && p.weight >= 0.0)
                    && profiles.iter().map(|p| p.weight).sum::<f32>() > 0.0),
            "targets.motion_profiles.weight",
            "weights must be >= 0 and add up to more than 0",
        );
        check(
            profiles.is_empty() || t.motion == Motion::Static,
            "targets.motion",
            "leave it out when motion_profiles is set: the profiles replace it",
        );
        for p in profiles {
            check(
                p.switch_secs.is_none_or(|s| s.is_valid() && s.min > 0.0),
                "targets.motion_profiles.switch_secs",
                "must be a positive [min, max]",
            );
            check(
                !matches!(p.motion, Motion::Ballistic { .. }),
                "targets.motion_profiles.motion",
                "ballistic is a whole flight: use it as targets.motion",
            );
            for (field, msg) in motion_problems(&p.motion) {
                let name = field.strip_prefix("targets.motion.").unwrap_or("chances");
                check(
                    false,
                    "targets.motion_profiles.motion",
                    &format!("{name}: {msg}"),
                );
            }
        }
        if let Some(r) = &t.reactions {
            for (chance, field) in [
                (
                    r.mirror_strafe_chance,
                    "targets.reactions.mirror_strafe_chance",
                ),
                (
                    r.oppose_strafe_chance,
                    "targets.reactions.oppose_strafe_chance",
                ),
                (r.shot_juke_chance, "targets.reactions.shot_juke_chance"),
            ] {
                check((0.0..=1.0).contains(&chance), field, "must be in [0, 1]");
            }
            // Reported only when both chances are in range, so one bad value is one error.
            let (mirror, oppose) = (r.mirror_strafe_chance, r.oppose_strafe_chance);
            let in_range = |c: f32| (0.0..=1.0).contains(&c);
            check(
                !(in_range(mirror) && in_range(oppose)) || mirror + oppose <= 1.0 + 1e-6,
                "targets.reactions",
                "mirror_strafe_chance + oppose_strafe_chance must be <= 1",
            );
            check(
                (0.0..90.0).contains(&r.dodge_within_deg),
                "targets.reactions.dodge_within_deg",
                "must be in [0, 90)",
            );
            check(
                r.react_secs.is_valid() && r.react_secs.min > 0.0,
                "targets.reactions.react_secs",
                "must be a positive [min, max]",
            );
            let reacts = |m: &Motion| matches!(m, Motion::Strafe { .. } | Motion::Dodge { .. });
            check(
                if t.motion_profiles.is_empty() {
                    reacts(&t.motion)
                } else {
                    t.motion_profiles.iter().all(|p| reacts(&p.motion))
                },
                "targets.reactions",
                "needs motion kind = \"strafe\" or \"dodge\" (every motion profile's, with profiles)",
            );
            // Options that could never act are refused rather than silently ignored.
            check(
                (mirror == 0.0 && oppose == 0.0) || self.player.movement,
                "targets.reactions",
                "mirror_strafe_chance and oppose_strafe_chance watch the player's strafe: \
                 they need player.movement = true",
            );
            check(
                r.shot_juke_chance == 0.0 || self.weapon.fire == FireMode::Click,
                "targets.reactions.shot_juke_chance",
                "reacts to click shots: needs weapon.fire = \"click\"",
            );
        }
        check(
            t.behaviors.len() <= MAX_BEHAVIOR_RULES,
            "targets.behaviors",
            &format!("at most {MAX_BEHAVIOR_RULES} rules"),
        );
        for rule in &t.behaviors {
            check(
                behavior_when_is_valid(&rule.when),
                "targets.behaviors.when",
                "needs at least one condition: health_below in (0, 1], damaged_within_secs > 0, \
                 alive_for_secs >= 0, crosshair_within_deg in (0, 180) or speed_above >= 0",
            );
            check(
                rule.motion.is_some() || rule.speed_scale != 1.0 || rule.retreat_m != 0.0,
                "targets.behaviors",
                "each rule needs motion, speed_scale or retreat_m",
            );
            check(
                (0.25..=4.0).contains(&rule.speed_scale),
                "targets.behaviors.speed_scale",
                "must be in [0.25, 4]",
            );
            check(
                (0.0..=MAX_RETREAT_M).contains(&rule.retreat_m),
                "targets.behaviors.retreat_m",
                &format!("must be in [0, {MAX_RETREAT_M}] m"),
            );
            for (field, msg) in rule.motion.iter().flat_map(motion_problems) {
                let name = field.strip_prefix("targets.motion.").unwrap_or("chances");
                check(false, "targets.behaviors.motion", &format!("{name}: {msg}"));
            }
            check(
                !matches!(rule.motion, Some(Motion::Ballistic { .. })),
                "targets.behaviors.motion",
                "ballistic is a whole flight: use it as targets.motion",
            );
        }
        let ballistic = matches!(t.motion, Motion::Ballistic { .. });
        check(
            !ballistic || t.behaviors.is_empty(),
            "targets.behaviors",
            "not available with a ballistic motion: its flight cannot be switched",
        );
        let humanoid = matches!(t.shape, Shape::Humanoid { .. });
        check(
            !humanoid
                || !matches!(t.motion, Motion::LinearBounce { arena, .. } if arena.sides != 0),
            "targets.motion.arena.sides",
            "humanoids bounce horizontally only: use the box (sides = 0)",
        );
        let waves = |m: &Motion| matches!(m, Motion::Wave { .. });
        check(
            !humanoid || !waves(&t.motion),
            "targets.motion",
            "wave weaves up and down: use a sphere or capsule shape",
        );
        check(
            !humanoid
                || !t.motion_profiles.iter().any(|p| waves(&p.motion))
                    && !t.behaviors.iter().flat_map(|b| &b.motion).any(waves),
            "targets.motion_profiles.motion",
            "wave weaves up and down: use a sphere or capsule shape",
        );
        if let Some(p) = &t.perception {
            for (field, msg) in p.problems(self) {
                check(false, field, msg);
            }
        }
        check(
            self.view.fov_lock.is_none_or(FovLock::is_valid),
            "view.fov_lock",
            "must be a vertical FOV, or a [min, max] with min <= max, within 30–120 degrees",
        );
        check(
            !self.feedback.damage_numbers || self.weapon.fire == FireMode::Click,
            "feedback.damage_numbers",
            "needs weapon.fire = \"click\": hold mode fires no shots to number",
        );
        errs.extend(self.run_rule_problems());
        errs
    }
}

/// Whether `id` is a reserved Windows device name, which cannot be a file name there.
fn is_windows_device_name(id: &str) -> bool {
    matches!(id, "con" | "nul" | "aux" | "prn")
        || ((id.starts_with("com") || id.starts_with("lpt"))
            && id.len() == 4
            && id.as_bytes()[3].is_ascii_digit()
            && id.as_bytes()[3] != b'0')
}

/// Whether a behaviour rule sets at least one condition and every set condition is in range.
fn behavior_when_is_valid(w: &BehaviorWhen) -> bool {
    let set = [
        w.health_below,
        w.damaged_within_secs,
        w.alive_for_secs,
        w.crosshair_within_deg,
        w.speed_above,
    ];
    set.iter().any(Option::is_some)
        && w.health_below.is_none_or(|h| h > 0.0 && h <= 1.0)
        && w.damaged_within_secs
            .is_none_or(|s| s.is_finite() && s > 0.0)
        && w.alive_for_secs.is_none_or(|s| s.is_finite() && s >= 0.0)
        && w.crosshair_within_deg.is_none_or(|a| a > 0.0 && a < 180.0)
        && w.speed_above.is_none_or(|v| v.is_finite() && v >= 0.0)
}

/// Problems with a motion's parameters, as `(field, message)`; the fields are named as
/// under `targets.motion`.
fn motion_problems(m: &Motion) -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    let mut check = |ok: bool, field: &'static str, msg: &'static str| {
        if !ok {
            out.push((field, msg));
        }
    };
    match m {
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
            ..
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
        Motion::Ballistic {
            speed,
            angle_deg,
            gravity,
            bounces,
            restitution,
            ..
        } => {
            check(
                speed.is_valid() && speed.min >= 0.0 && speed.max.is_finite(),
                "targets.motion.speed",
                "must be a non-negative [min, max]",
            );
            check(
                angle_deg.is_valid() && angle_deg.min >= -90.0 && angle_deg.max <= 90.0,
                "targets.motion.angle_deg",
                "must be a [min, max] within [-90, 90]",
            );
            check(
                gravity.is_finite() && *gravity > 0.0,
                "targets.motion.gravity",
                "must be > 0",
            );
            check(
                *bounces <= MAX_BOUNCES,
                "targets.motion.bounces",
                "must be at most 16",
            );
            check(
                *restitution > 0.0 && *restitution <= 1.0,
                "targets.motion.restitution",
                "must be in (0, 1]",
            );
        }
        Motion::LinearBounce { speed_deg, arena } => {
            check(
                speed_deg.is_valid() && speed_deg.min > 0.0 && speed_deg.max.is_finite(),
                "targets.motion.speed_deg",
                "must be a positive [min, max]",
            );
            check(
                arena.yaw_deg.is_valid()
                    && arena.yaw_deg.min < arena.yaw_deg.max
                    && arena.yaw_deg.min >= -180.0
                    && arena.yaw_deg.max <= 180.0
                    && arena.pitch_deg.is_valid()
                    && arena.pitch_deg.min >= -89.0
                    && arena.pitch_deg.max <= 89.0,
                "targets.motion.arena",
                "needs a yaw_deg [min, max] (min < max) within [-180, 180] and a pitch_deg \
                 [min, max] within ±89",
            );
            check(
                arena.sides == 0 || (3..=12).contains(&arena.sides),
                "targets.motion.arena.sides",
                "must be 0 (box) or 3 to 12",
            );
            check(
                arena.sides == 0 || arena.pitch_deg.min < arena.pitch_deg.max,
                "targets.motion.arena.pitch_deg",
                "a polygon needs a pitch range with min < max",
            );
        }
        Motion::Orbit {
            speed_deg,
            radius,
            change_secs,
            reverse_chance,
        } => {
            check(
                speed_deg.is_valid() && speed_deg.min > 0.0 && speed_deg.max.is_finite(),
                "targets.motion.speed_deg",
                "must be a positive [min, max]",
            );
            check(
                radius.is_none_or(|r| r.is_finite() && r >= 1.0),
                "targets.motion.radius",
                "must be >= 1 m",
            );
            check(
                change_secs.is_none_or(|c| c.is_valid() && c.min > 0.0),
                "targets.motion.change_secs",
                "must be a positive [min, max]",
            );
            check(
                (0.0..=1.0).contains(reverse_chance),
                "targets.motion.reverse_chance",
                "must be in [0, 1]",
            );
        }
        Motion::Wave {
            speed,
            amplitude,
            frequency_hz,
            max_offset,
            ..
        } => {
            check(
                speed.is_valid() && speed.min >= 0.0 && speed.max.is_finite(),
                "targets.motion.speed",
                "must be a non-negative [min, max]",
            );
            check(
                amplitude.is_valid() && amplitude.min > 0.0 && amplitude.max.is_finite(),
                "targets.motion.amplitude",
                "must be a positive [min, max] in meters",
            );
            check(
                frequency_hz.is_valid() && frequency_hz.min > 0.0 && frequency_hz.max <= 10.0,
                "targets.motion.frequency_hz",
                "must be a positive [min, max] of at most 10 Hz",
            );
            check(
                max_offset.is_finite() && *max_offset > 0.0,
                "targets.motion.max_offset",
                "must be > 0",
            );
        }
    }
    if let Some(d) = motion_depth(m) {
        check(
            d.change_secs.is_valid() && d.change_secs.min > 0.0,
            "targets.motion.depth.change_secs",
            "must be a positive [min, max]",
        );
        check(
            d.distance.is_valid() && d.distance.min >= 1.0 && d.distance.max.is_finite(),
            "targets.motion.depth.distance",
            "must be a [min, max] of at least 1 m",
        );
    }
    if let Some(r) = m.rhythm() {
        if let Some(p) = r.swap_pause_secs {
            check(
                p.is_valid() && p.min >= 0.0 && p.max <= MAX_SWAP_PAUSE_SECS,
                "targets.motion.rhythm.swap_pause_secs",
                "must be a [min, max] within [0, 2] seconds",
            );
        }
        check(
            (SIDE_SECS_MULT.min..=SIDE_SECS_MULT.max).contains(&r.left_secs_mult),
            "targets.motion.rhythm.left_secs_mult",
            "must be in [0.25, 4]",
        );
        check(
            (SIDE_SECS_MULT.min..=SIDE_SECS_MULT.max).contains(&r.right_secs_mult),
            "targets.motion.rhythm.right_secs_mult",
            "must be in [0.25, 4]",
        );
    }
    out
}

/// 64-bit FNV-1a: tiny, dependency-free and identical on every platform and release.
fn fnv1a_64(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes
        .iter()
        .fold(OFFSET, |h, &b| (h ^ b as u64).wrapping_mul(PRIME))
}

/// Most floor bounces a `ballistic` motion may have.
pub const MAX_BOUNCES: u32 = 16;

/// The forward/back block of a motion, when it has one.
pub(super) fn motion_depth(m: &Motion) -> Option<&DepthDef> {
    match m {
        Motion::Strafe { depth, .. }
        | Motion::Dodge { depth, .. }
        | Motion::Drift { depth, .. } => depth.as_ref(),
        _ => None,
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
    fn windows_device_names_are_not_valid_ids() {
        let mut d = ScenarioDef::from_toml(MINIMAL).unwrap();
        for bad in ["con", "nul", "aux", "prn", "com1", "lpt9"] {
            d.id = bad.into();
            assert_eq!(
                d.validate()
                    .into_iter()
                    .map(|e| e.field)
                    .collect::<Vec<_>>(),
                ["id"],
                "{bad} names a device on Windows, so its file could not be saved"
            );
        }
        d.id = "console-flicks".into();
        assert!(d.validate().is_empty());
    }

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
    fn game_tag_defaults_to_generic_and_names_a_known_game() {
        let generic = ScenarioDef::from_toml(MINIMAL).unwrap();
        assert_eq!(generic.game, None);
        assert!(!toml::to_string(&generic).unwrap().contains("game"));

        let tagged = MINIMAL.replace("name = \"Mini\"", "name = \"Mini\"\ngame = \"valorant\"");
        let d = ScenarioDef::from_toml(&tagged).unwrap();
        assert_eq!(d.game, Some(crate::games::Game::Valorant));
        assert!(d.validate().is_empty());
        let cs = MINIMAL.replace("name = \"Mini\"", "name = \"Mini\"\ngame = \"cs2\"");
        assert_eq!(
            ScenarioDef::from_toml(&cs).unwrap().game,
            Some(crate::games::Game::Cs2)
        );

        let unknown = MINIMAL.replace("name = \"Mini\"", "name = \"Mini\"\ngame = \"quake\"");
        assert!(ScenarioDef::from_toml(&unknown).is_err());
    }

    #[test]
    fn the_game_tag_does_not_change_the_content_hash() {
        // Tagging a scenario must keep its replays verifiable and its lab id.
        let generic = ScenarioDef::from_toml(MINIMAL).unwrap();
        let mut tagged = generic.clone();
        tagged.game = Some(crate::games::Game::Valorant);
        assert_eq!(tagged.content_hash(), generic.content_hash());
        assert_eq!(
            crate::analytics::proof::content_hash(&tagged).unwrap(),
            crate::analytics::proof::content_hash(&generic).unwrap()
        );
        let mut changed = generic.clone();
        changed.duration_secs = 30.0;
        assert_ne!(changed.content_hash(), generic.content_hash());
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
    fn player_start_fields_are_optional_parse_and_must_stay_inside_the_bounds() {
        let d = ScenarioDef::from_toml(MINIMAL).unwrap();
        assert_eq!((d.player.start, d.player.start_velocity), (None, None));
        let plain = toml::to_string(&d).unwrap();
        assert!(!plain.contains("start"), "absent fields are not written");
        let src = format!(
            "{MINIMAL}\n[player]\nmovement = true\nbounds_x = [-5, 5]\nbounds_z = [-1, 1]\n\
             start = [2.5, -0.5]\nstart_velocity = [3.0, 0.25]\n"
        );
        let moved = ScenarioDef::from_toml(&src).unwrap();
        assert_eq!(moved.player.start, Some([2.5, -0.5]));
        assert_eq!(moved.player.start_velocity, Some([3.0, 0.25]));
        assert!(moved.validate().is_empty(), "{:?}", moved.validate());
        for bad in [[5.5, 0.0], [0.0, -1.5], [f32::NAN, 0.0]] {
            let mut b = moved.clone();
            b.player.start = Some(bad);
            let fields: Vec<_> = b.validate().into_iter().map(|e| e.field).collect();
            assert_eq!(fields, ["player.start"], "{bad:?}");
        }
        for bad in [[f32::INFINITY, 0.0], [0.0, f32::NAN]] {
            let mut b = moved.clone();
            b.player.start_velocity = Some(bad);
            let fields: Vec<_> = b.validate().into_iter().map(|e| e.field).collect();
            assert_eq!(fields, ["player.start_velocity"], "{bad:?}");
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
        assert_eq!(
            with(&|r| {
                r.avoid_last_deg = 0.0;
                r.avoid_recent = 4;
            }),
            ["targets.spawn_rules.avoid_recent"],
            "avoid_recent only widens avoid_last_deg, so alone it would do nothing"
        );
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

    /// `MINIMAL` with a strafe motion, which reactions need.
    fn strafing_minimal() -> String {
        format!("{MINIMAL}motion = {{ kind = \"strafe\", change_secs = [0.3, 0.6] }}\n")
    }

    #[test]
    fn reactions_default_off_stay_out_of_saved_files_parse_and_are_validated() {
        let d = ScenarioDef::from_toml(MINIMAL).unwrap();
        assert_eq!(d.targets.reactions, None);
        assert!(!toml::to_string(&d).unwrap().contains("reactions"));

        let src = format!(
            "{}reactions = {{ mirror_strafe_chance = 0.3, oppose_strafe_chance = 0.2, dodge_within_deg = 3, shot_juke_chance = 0.5 }}\n[player]\nmovement = true\n",
            strafing_minimal()
        );
        let r = ScenarioDef::from_toml(&src).unwrap();

        // Options that could never act are refused, not silently ignored.
        let mut still = r.clone();
        still.player.movement = false;
        assert_eq!(
            fields_of(&still),
            ["targets.reactions"],
            "mirror/oppose watch the player's strafe, which needs movement"
        );
        let mut held = r.clone();
        held.weapon.fire = FireMode::Hold;
        assert!(
            fields_of(&held).contains(&"targets.reactions.shot_juke_chance"),
            "{:?}",
            fields_of(&held)
        );
        assert_eq!(
            r.targets.reactions,
            Some(ReactionsDef {
                mirror_strafe_chance: 0.3,
                oppose_strafe_chance: 0.2,
                dodge_within_deg: 3.0,
                shot_juke_chance: 0.5,
                react_secs: Range::new(0.2, 0.4),
            })
        );
        assert!(r.validate().is_empty(), "{:?}", r.validate());
        assert!(ScenarioDef::from_toml(&src.replace("shot_juke_chance", "shot_juke")).is_err());

        let with = |f: &dyn Fn(&mut ReactionsDef)| {
            let mut b = r.clone();
            f(b.targets.reactions.as_mut().unwrap());
            fields_of(&b)
        };
        for bad in [-0.1, 1.1, f32::NAN] {
            assert_eq!(
                with(&|x| x.mirror_strafe_chance = bad),
                ["targets.reactions.mirror_strafe_chance"],
                "{bad}"
            );
            assert_eq!(
                with(&|x| x.oppose_strafe_chance = bad),
                ["targets.reactions.oppose_strafe_chance"],
                "{bad}"
            );
            assert_eq!(
                with(&|x| x.shot_juke_chance = bad),
                ["targets.reactions.shot_juke_chance"],
                "{bad}"
            );
        }
        assert_eq!(
            with(&|x| {
                x.mirror_strafe_chance = 0.6;
                x.oppose_strafe_chance = 0.6;
            }),
            ["targets.reactions"],
            "mirror and oppose share one roll"
        );
        for bad in [-1.0, 90.0, f32::NAN] {
            assert_eq!(
                with(&|x| x.dodge_within_deg = bad),
                ["targets.reactions.dodge_within_deg"],
                "{bad}"
            );
        }
        assert_eq!(
            with(&|x| x.react_secs = Range::new(0.0, 0.3)),
            ["targets.reactions.react_secs"]
        );

        let mut still = r.clone();
        still.targets.motion = Motion::Static;
        assert_eq!(
            fields_of(&still),
            ["targets.reactions"],
            "a reaction steers a strafe"
        );
    }

    #[test]
    fn behavior_rules_default_empty_stay_out_of_saved_files_parse_and_are_validated() {
        let d = ScenarioDef::from_toml(MINIMAL).unwrap();
        assert!(d.targets.behaviors.is_empty());
        assert!(!toml::to_string(&d).unwrap().contains("behaviors"));

        let src = format!(
            "{MINIMAL}health = 4\n\
             [[targets.behaviors]]\n\
             when = {{ health_below = 0.5 }}\n\
             retreat_m = 4\n\
             [[targets.behaviors]]\n\
             when = {{ damaged_within_secs = 1.2, crosshair_within_deg = 5 }}\n\
             motion = {{ kind = \"strafe\", change_secs = [0.2, 0.4] }}\n\
             speed_scale = 1.3\n\
             [[targets.behaviors]]\n\
             when = {{ alive_for_secs = 2, speed_above = 1.5 }}\n\
             speed_scale = 0.5\n"
        );
        let b = ScenarioDef::from_toml(&src).unwrap();
        let rules = &b.targets.behaviors;
        assert_eq!(rules.len(), 3);
        assert_eq!(
            rules[0],
            BehaviorRule {
                when: BehaviorWhen {
                    health_below: Some(0.5),
                    ..Default::default()
                },
                motion: None,
                speed_scale: 1.0,
                retreat_m: 4.0,
            }
        );
        assert_eq!(rules[1].when.damaged_within_secs, Some(1.2));
        assert_eq!(rules[1].when.crosshair_within_deg, Some(5.0));
        assert!(matches!(rules[1].motion, Some(Motion::Strafe { .. })));
        assert_eq!(rules[1].speed_scale, 1.3);
        assert_eq!(
            (rules[2].when.alive_for_secs, rules[2].when.speed_above),
            (Some(2.0), Some(1.5))
        );
        assert!(b.validate().is_empty(), "{:?}", b.validate());
        assert_eq!(
            ScenarioDef::from_toml(&toml::to_string(&b).unwrap()).unwrap(),
            b
        );
        assert!(ScenarioDef::from_toml(&src.replace("health_below", "health_under")).is_err());
        assert!(ScenarioDef::from_toml(&src.replace("retreat_m", "retreat")).is_err());

        let with = |f: &dyn Fn(&mut BehaviorRule)| {
            let mut c = b.clone();
            f(&mut c.targets.behaviors[0]);
            fields_of(&c)
        };
        assert_eq!(
            with(&|r| r.when = BehaviorWhen::default()),
            ["targets.behaviors.when"],
            "a rule needs a condition"
        );
        for when in [
            BehaviorWhen {
                health_below: Some(0.0),
                ..Default::default()
            },
            BehaviorWhen {
                health_below: Some(1.5),
                ..Default::default()
            },
            BehaviorWhen {
                damaged_within_secs: Some(0.0),
                ..Default::default()
            },
            BehaviorWhen {
                alive_for_secs: Some(-1.0),
                ..Default::default()
            },
            BehaviorWhen {
                crosshair_within_deg: Some(180.0),
                ..Default::default()
            },
            BehaviorWhen {
                speed_above: Some(f32::NAN),
                ..Default::default()
            },
        ] {
            assert_eq!(
                with(&|r| r.when = when.clone()),
                ["targets.behaviors.when"],
                "{when:?}"
            );
        }
        assert_eq!(
            with(&|r| r.retreat_m = 0.0),
            ["targets.behaviors"],
            "a rule needs an override"
        );
        for bad in [-1.0, 25.0, f32::NAN] {
            assert_eq!(
                with(&|r| r.retreat_m = bad),
                ["targets.behaviors.retreat_m"],
                "{bad}"
            );
        }
        for bad in [0.2, 4.5] {
            assert_eq!(
                with(&|r| r.speed_scale = bad),
                ["targets.behaviors.speed_scale"],
                "{bad}"
            );
        }
        let broken = Motion::Strafe {
            weapon: Weapon::Vandal,
            change_secs: Range::new(0.0, 0.3),
            stop_chance: 0.0,
            max_offset: 3.0,
            depth: None,
            rhythm: None,
        };
        let problems: Vec<ValidationError> = {
            let mut c = b.clone();
            c.targets.behaviors[0].motion = Some(broken);
            c.validate()
        };
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(problems[0].field, "targets.behaviors.motion");
        assert!(
            problems[0].message.contains("change_secs"),
            "{}",
            problems[0].message
        );

        let mut many = b.clone();
        many.targets.behaviors = vec![b.targets.behaviors[0].clone(); MAX_BEHAVIOR_RULES + 1];
        assert_eq!(fields_of(&many), ["targets.behaviors"]);
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

    /// Fields of the validation problems of `MINIMAL` with `motion` and `extra` lines.
    fn motion_fields(motion: &str, extra: &str) -> Vec<&'static str> {
        let src = format!("{MINIMAL}motion = {motion}\n{extra}");
        ScenarioDef::from_toml(&src)
            .unwrap_or_else(|e| panic!("{motion}: {e}"))
            .validate()
            .into_iter()
            .map(|e| e.field)
            .collect()
    }

    #[test]
    fn ballistic_motion_parses_with_defaults_and_validates() {
        let src = format!(
            "{MINIMAL}motion = {{ kind = \"ballistic\", speed = [6, 9], angle_deg = [40, 80] }}\n"
        );
        let d = ScenarioDef::from_toml(&src).unwrap();
        assert_eq!(
            d.targets.motion,
            Motion::Ballistic {
                speed: Range::new(6.0, 9.0),
                angle_deg: Range::new(40.0, 80.0),
                gravity: 9.81,
                bounces: 0,
                restitution: 0.5,
                on_floor: FloorAction::Expire,
            }
        );
        assert!(d.validate().is_empty(), "{:?}", d.validate());
        let typo = src.replace("angle_deg", "angel_deg");
        assert!(ScenarioDef::from_toml(&typo).is_err());

        let ballistic = |rest: &str| format!("{{ kind = \"ballistic\", speed = [6, 9], {rest} }}");
        for (rest, field) in [
            ("angle_deg = [40, 100]", "targets.motion.angle_deg"),
            (
                "angle_deg = [40, 80], gravity = 0",
                "targets.motion.gravity",
            ),
            (
                "angle_deg = [40, 80], bounces = 17",
                "targets.motion.bounces",
            ),
            (
                "angle_deg = [40, 80], restitution = 1.5",
                "targets.motion.restitution",
            ),
            (
                "angle_deg = [40, 80], restitution = 0",
                "targets.motion.restitution",
            ),
        ] {
            assert_eq!(motion_fields(&ballistic(rest), ""), [field], "{rest}");
        }
        assert_eq!(
            motion_fields(&ballistic("angle_deg = [40, 80], on_floor = \"fail\""), ""),
            Vec::<&str>::new()
        );
        let flight = ballistic("angle_deg = [40, 80]");
        assert!(
            motion_fields(&flight, "[fail]\nmax_expired = 3\n").is_empty(),
            "landing ballistic targets expire, so max_expired needs no lifetime"
        );
        assert_eq!(
            motion_fields(
                &flight,
                "[[targets.behaviors]]\nwhen = { alive_for_secs = 1 }\nspeed_scale = 2\n"
            ),
            ["targets.behaviors"]
        );
        let base = "{ kind = \"static\" }";
        let rule =
            format!("[[targets.behaviors]]\nwhen = {{ alive_for_secs = 1 }}\nmotion = {flight}\n");
        assert_eq!(motion_fields(base, &rule), ["targets.behaviors.motion"]);
    }

    #[test]
    fn depth_bounce_and_orbit_blocks_validate_their_ranges() {
        let depth = |band: &str| {
            format!(
                "{{ kind = \"strafe\", change_secs = [0.3, 0.6], depth = {{ change_secs = [0.4, 0.8], distance = {band} }} }}"
            )
        };
        assert!(motion_fields(&depth("[8, 14]"), "").is_empty());
        assert_eq!(
            motion_fields(&depth("[0.5, 14]"), ""),
            ["targets.motion.depth.distance"]
        );
        let zero = "{ kind = \"drift\", speed = [1, 2], change_secs = [1, 2], depth = { change_secs = [0, 1], distance = [8, 9] } }";
        assert_eq!(
            motion_fields(zero, ""),
            ["targets.motion.depth.change_secs"]
        );

        let bounce = |arena: &str| {
            format!("{{ kind = \"linear_bounce\", speed_deg = [20, 30], arena = {arena} }}")
        };
        assert!(
            motion_fields(&bounce("{ yaw_deg = [-20, 20], pitch_deg = [0, 15] }"), "").is_empty()
        );
        assert!(
            motion_fields(
                &bounce("{ yaw_deg = [-20, 20], pitch_deg = [0, 15], sides = 6 }"),
                ""
            )
            .is_empty()
        );
        for (arena, field) in [
            (
                "{ yaw_deg = [20, 20], pitch_deg = [0, 15] }",
                "targets.motion.arena",
            ),
            (
                "{ yaw_deg = [-20, 20], pitch_deg = [0, 95] }",
                "targets.motion.arena",
            ),
            (
                "{ yaw_deg = [-20, 20], pitch_deg = [0, 15], sides = 2 }",
                "targets.motion.arena.sides",
            ),
            (
                "{ yaw_deg = [-20, 20], pitch_deg = [5, 5], sides = 4 }",
                "targets.motion.arena.pitch_deg",
            ),
        ] {
            assert_eq!(motion_fields(&bounce(arena), ""), [field], "{arena}");
        }
        assert_eq!(
            motion_fields(
                "{ kind = \"linear_bounce\", speed_deg = [0, 30], arena = { yaw_deg = [-20, 20], pitch_deg = [0, 15] } }",
                ""
            ),
            ["targets.motion.speed_deg"]
        );
        let humanoid = format!(
            "{}motion = {}\n",
            MINIMAL.replace(
                "{ kind = \"sphere\", radius = 0.2 }",
                "{ kind = \"humanoid\" }"
            ),
            bounce("{ yaw_deg = [-20, 20], pitch_deg = [0, 15], sides = 5 }")
        );
        let fields: Vec<_> = ScenarioDef::from_toml(&humanoid)
            .unwrap()
            .validate()
            .into_iter()
            .map(|e| e.field)
            .collect();
        assert_eq!(fields, ["targets.motion.arena.sides"]);

        assert!(motion_fields("{ kind = \"orbit\", speed_deg = [30, 50] }", "").is_empty());
        for (motion, field) in [
            (
                "{ kind = \"orbit\", speed_deg = [30, 50], radius = 0.5 }",
                "targets.motion.radius",
            ),
            (
                "{ kind = \"orbit\", speed_deg = [0, 50] }",
                "targets.motion.speed_deg",
            ),
            (
                "{ kind = \"orbit\", speed_deg = [30, 50], change_secs = [0, 1] }",
                "targets.motion.change_secs",
            ),
            (
                "{ kind = \"orbit\", speed_deg = [30, 50], reverse_chance = 2 }",
                "targets.motion.reverse_chance",
            ),
        ] {
            assert_eq!(motion_fields(motion, ""), [field], "{motion}");
        }
    }

    /// Fields of the validation problems of `MINIMAL` with `lines` added under `[targets]`
    /// and `scoring` lines under `[scoring]`.
    fn extra_fields(lines: &str, scoring: &str) -> Vec<&'static str> {
        let src = format!("{MINIMAL}{lines}\n[scoring]\n{scoring}\n");
        ScenarioDef::from_toml(&src)
            .unwrap_or_else(|e| panic!("{lines}: {e}"))
            .validate()
            .into_iter()
            .map(|e| e.field)
            .collect()
    }

    #[test]
    fn scoring_and_spawn_extras_parse_and_validate() {
        let none: Vec<&str> = Vec::new();
        assert_eq!(extra_fields("", "accuracy_mult = \"sqrt\""), none);
        assert!(
            ScenarioDef::from_toml(&format!(
                "{MINIMAL}\n[scoring]\naccuracy_mult = \"cubic\"\n"
            ))
            .is_err()
        );

        let shrink = |s: &str| extra_fields(&format!("shrink_per_kill = {s}"), "");
        assert_eq!(shrink("{ factor = 0.9, floor = 0.4 }"), none);
        for bad in [
            "{ factor = 1.0, floor = 0.4 }",
            "{ factor = 0.9, floor = 0 }",
            "{ factor = 0.9, floor = 1.2 }",
        ] {
            assert_eq!(shrink(bad), ["targets.shrink_per_kill"], "{bad}");
        }
        let humanoid = format!(
            "{}shrink_per_kill = {{ factor = 0.9, floor = 0.4 }}\n",
            MINIMAL.replace(
                "{ kind = \"sphere\", radius = 0.2 }",
                "{ kind = \"humanoid\" }"
            )
        );
        let fields: Vec<_> = ScenarioDef::from_toml(&humanoid)
            .unwrap()
            .validate()
            .into_iter()
            .map(|e| e.field)
            .collect();
        assert_eq!(fields, ["targets.shrink_per_kill"], "spheres only");

        let grid = |g: &str| {
            let src = MINIMAL.replace(
                "spawn = { kind = \"around_crosshair\", offset_deg = [10, 40], distance = [10, 10] }",
                &format!("spawn = {{ kind = \"angular\", yaw_deg = [-20, 20], pitch_deg = [0, 10], distance = [10, 10], grid = {g} }}"),
            );
            ScenarioDef::from_toml(&src)
                .unwrap()
                .validate()
                .into_iter()
                .map(|e| e.field)
                .collect::<Vec<_>>()
        };
        assert_eq!(grid("{ rows = 2, cols = 3 }"), none);
        assert_eq!(grid("{ rows = 0, cols = 3 }"), ["targets.spawn.grid"]);
        assert_eq!(grid("{ rows = 2, cols = 17 }"), ["targets.spawn.grid"]);
    }

    #[test]
    fn motion_profiles_validate_weights_timers_and_motions() {
        let profile = |weight: &str, extra: &str, motion: &str| {
            format!(
                "\n[[targets.motion_profiles]]\nweight = {weight}\n{extra}\nmotion = {motion}\n"
            )
        };
        let still = "{ kind = \"static\" }";
        let orbit = "{ kind = \"orbit\", speed_deg = [20, 30] }";
        let ok = format!(
            "{}{}",
            profile("1", "switch_secs = [1, 2]", still),
            profile("2", "", orbit)
        );
        let none: Vec<&str> = Vec::new();
        let fields = |profiles: &str| {
            ScenarioDef::from_toml(&format!("{MINIMAL}{profiles}"))
                .unwrap_or_else(|e| panic!("{profiles}: {e}"))
                .validate()
                .into_iter()
                .map(|e| e.field)
                .collect::<Vec<_>>()
        };
        assert_eq!(fields(&ok), none);
        assert_eq!(
            fields(&format!(
                "{}{}",
                profile("0", "", still),
                profile("0", "", orbit)
            )),
            ["targets.motion_profiles.weight"]
        );
        assert_eq!(
            fields(&profile("-1", "", still)),
            ["targets.motion_profiles.weight"]
        );
        assert_eq!(
            fields(&profile("1", "switch_secs = [0, 1]", still)),
            ["targets.motion_profiles.switch_secs"]
        );
        assert_eq!(
            fields(&profile(
                "1",
                "",
                "{ kind = \"ballistic\", speed = [5, 6], angle_deg = [40, 60] }"
            )),
            ["targets.motion_profiles.motion"]
        );
        assert_eq!(
            fields(&profile(
                "1",
                "",
                "{ kind = \"orbit\", speed_deg = [0, 30] }"
            )),
            ["targets.motion_profiles.motion"]
        );
        let with_motion = format!("motion = {orbit}\n{ok}");
        assert_eq!(
            fields(&with_motion),
            ["targets.motion"],
            "the profiles replace targets.motion"
        );
        let nine: String = (0..9).map(|_| profile("1", "", still)).collect();
        assert_eq!(fields(&nine), ["targets.motion_profiles"]);
    }

    #[test]
    fn capsule_shapes_parse_and_validate() {
        let shape = |s: &str| {
            let src = MINIMAL.replace("{ kind = \"sphere\", radius = 0.2 }", s);
            ScenarioDef::from_toml(&src)
                .unwrap_or_else(|e| panic!("{s}: {e}"))
                .validate()
                .into_iter()
                .map(|e| e.field)
                .collect::<Vec<_>>()
        };
        let none: Vec<&str> = Vec::new();
        assert_eq!(
            shape("{ kind = \"capsule\", radius = 0.3, height = 1.6 }"),
            none
        );
        assert_eq!(
            shape(
                "{ kind = \"capsule\", radius = 0.3, height = 1.6, head = { radius = 0.15, offset = 0.95 } }"
            ),
            none
        );
        assert_eq!(
            shape("{ kind = \"capsule\", radius = 0.3, height = 0.5 }"),
            ["targets.shape"]
        );
        assert_eq!(
            shape("{ kind = \"capsule\", radius = 0, height = 1 }"),
            ["targets.shape"]
        );
        assert_eq!(
            shape(
                "{ kind = \"capsule\", radius = 0.3, height = 1.6, head = { radius = 0, offset = 1 } }"
            ),
            ["targets.shape.head"]
        );
        let typo = MINIMAL.replace(
            "{ kind = \"sphere\", radius = 0.2 }",
            "{ kind = \"capsule\", radius = 0.3, hieght = 1.6 }",
        );
        assert!(ScenarioDef::from_toml(&typo).is_err());
    }

    #[test]
    fn without_a_view_block_the_camera_keeps_valorants_fov() {
        let d = ScenarioDef::from_toml(MINIMAL).unwrap();
        assert_eq!(d.view, ViewDef::default());
        assert_eq!(
            d.view.vertical_fov_deg(None),
            crate::fov::valorant_vertical_fov_deg()
        );
        assert_eq!(
            d.view.vertical_fov_deg(Some(1.25)),
            crate::fov::valorant_zoomed_vertical_fov_deg(1.25)
        );
        assert!(
            !toml::to_string(&d).unwrap().contains("[view]"),
            "an unused block is not written, so content hashes stay put"
        );
    }

    #[test]
    fn a_fixed_fov_lock_replaces_the_fov_and_a_range_clamps_it() {
        let fixed = ScenarioDef::from_toml(&format!("{MINIMAL}\n[view]\nfov_lock = 60\n")).unwrap();
        assert_eq!(fixed.view.fov_lock, Some(FovLock::Fixed(60.0)));
        assert!(fixed.validate().is_empty(), "{:?}", fixed.validate());
        assert_eq!(fixed.view.vertical_fov_deg(None), 60.0);
        // Zoom narrows the locked FOV the way it narrows VALORANT's: horizontal / zoom.
        let zoomed = fixed.view.vertical_fov_deg(Some(2.0));
        let h = crate::fov::horizontal_from_vertical(60.0, 16.0 / 9.0) / 2.0;
        assert!((zoomed - crate::fov::vertical_from_horizontal(h, 16.0 / 9.0)).abs() < 1e-9);

        let narrow =
            ScenarioDef::from_toml(&format!("{MINIMAL}\n[view]\nfov_lock = [40, 55]\n")).unwrap();
        assert_eq!(
            narrow.view.vertical_fov_deg(None),
            55.0,
            "70.5° clamps down to max"
        );
        let wide =
            ScenarioDef::from_toml(&format!("{MINIMAL}\n[view]\nfov_lock = [80, 100]\n")).unwrap();
        assert_eq!(
            wide.view.vertical_fov_deg(None),
            80.0,
            "70.5° clamps up to min"
        );
        let around =
            ScenarioDef::from_toml(&format!("{MINIMAL}\n[view]\nfov_lock = [60, 90]\n")).unwrap();
        assert_eq!(
            around.view.vertical_fov_deg(None),
            crate::fov::valorant_vertical_fov_deg(),
            "inside the range VALORANT's FOV is kept"
        );
    }

    #[test]
    fn fov_lock_ranges_are_validated_and_typos_rejected() {
        let fields = |lock: &str| -> Vec<&'static str> {
            ScenarioDef::from_toml(&format!("{MINIMAL}\n[view]\nfov_lock = {lock}\n"))
                .unwrap()
                .validate()
                .into_iter()
                .map(|e| e.field)
                .collect()
        };
        let none: [&str; 0] = [];
        assert_eq!(fields("30"), none);
        assert_eq!(fields("120"), none);
        assert_eq!(fields("[30, 120]"), none);
        for bad in ["29", "121", "0", "[90, 60]", "[20, 60]", "[60, 130]", "nan"] {
            assert_eq!(fields(bad), ["view.fov_lock"], "{bad}");
        }
        let typo = format!("{MINIMAL}\n[view]\nfov_lok = 60\n");
        assert!(ScenarioDef::from_toml(&typo).is_err());
    }

    #[test]
    fn feedback_defaults_leave_the_players_settings_in_charge() {
        let d = ScenarioDef::from_toml(MINIMAL).unwrap();
        assert_eq!(d.feedback, FeedbackDef::default());
        for player in [true, false] {
            assert_eq!(d.feedback.hit_markers_shown(player), player);
            assert_eq!(d.feedback.hit_sounds_played(player), player);
        }
        assert!(!d.feedback.damage_numbers && !d.feedback.health_bars);
        assert!(!toml::to_string(&d).unwrap().contains("[feedback]"));
    }

    #[test]
    fn feedback_can_only_hide_markers_and_sounds_and_opt_into_numbers_and_bars() {
        let src = format!(
            "{MINIMAL}\n[feedback]\nhit_markers = false\nhit_sounds = false\ndamage_numbers = true\nhealth_bars = true\n"
        );
        let d = ScenarioDef::from_toml(&src).unwrap();
        assert!(d.validate().is_empty(), "{:?}", d.validate());
        for player in [true, false] {
            assert!(!d.feedback.hit_markers_shown(player));
            assert!(!d.feedback.hit_sounds_played(player));
        }
        // `true` cannot force on what the player turned off.
        let on = ScenarioDef::from_toml(&format!(
            "{MINIMAL}\n[feedback]\nhit_markers = true\nhit_sounds = true\n"
        ))
        .unwrap();
        assert!(!on.feedback.hit_markers_shown(false));
        assert!(!on.feedback.hit_sounds_played(false));
        assert!(d.feedback.damage_numbers && d.feedback.health_bars);

        let typo = format!("{MINIMAL}\n[feedback]\nhit_marker = false\n");
        assert!(ScenarioDef::from_toml(&typo).is_err());
    }

    #[test]
    fn damage_numbers_need_shots_to_show() {
        let mut d =
            ScenarioDef::from_toml(&format!("{MINIMAL}\n[feedback]\ndamage_numbers = true\n"))
                .unwrap();
        d.weapon.fire = FireMode::Hold;
        d.targets.health = 2.0;
        let fields: Vec<_> = d.validate().into_iter().map(|e| e.field).collect();
        assert_eq!(fields, ["feedback.damage_numbers"]);
    }

    #[test]
    fn the_content_hash_is_stable_and_changes_with_any_edit() {
        let a = ScenarioDef::from_toml(MINIMAL).unwrap();
        let b = ScenarioDef::from_toml(MINIMAL).unwrap();
        assert_eq!(
            a.content_hash(),
            b.content_hash(),
            "same definition, same hash"
        );
        assert_eq!(a.content_hash(), a.clone().content_hash());
        // Formatting and comments are not content.
        let reformatted = format!("# a comment\n{}", MINIMAL.replace("    ", "  "));
        assert_eq!(
            ScenarioDef::from_toml(&reformatted).unwrap().content_hash(),
            a.content_hash()
        );
        let mut edited = a.clone();
        edited.targets.max_alive = 2;
        assert_ne!(edited.content_hash(), a.content_hash());
        let mut faster = a.clone();
        faster.duration_secs = 30.0;
        assert_ne!(faster.content_hash(), a.content_hash());
        let locked =
            ScenarioDef::from_toml(&format!("{MINIMAL}\n[view]\nfov_lock = 60\n")).unwrap();
        assert_ne!(locked.content_hash(), a.content_hash());
        // Every built-in scenario hashes, and no two share a hash.
        let hashes: std::collections::HashSet<u64> = crate::scenario::builtin::all()
            .iter()
            .map(ScenarioDef::content_hash)
            .collect();
        assert_eq!(hashes.len(), crate::scenario::builtin::all().len());
    }
    #[test]
    fn stop_window_defaults_off_stays_out_of_saved_files_parses_and_is_validated() {
        let d = ScenarioDef::from_toml(MINIMAL).unwrap();
        assert_eq!(d.scoring.stop_window, None);
        assert!(!toml::to_string(&d).unwrap().contains("stop_window"));

        let src = format!(
            "{MINIMAL}
[player]
movement = true
[scoring]
stop_window = {{ secs = 0.4, bonus = 50 }}
"
        );
        let w = ScenarioDef::from_toml(&src).unwrap();
        assert_eq!(
            w.scoring.stop_window,
            Some(StopWindow {
                secs: 0.4,
                bonus: 50.0
            })
        );
        assert!(w.validate().is_empty(), "{:?}", w.validate());
        assert!(
            ScenarioDef::from_toml(&src.replace("bonus = 50", "bonus = 50, extra = 1")).is_err()
        );

        for (secs, bonus) in [(0.0, 50.0), (2.5, 50.0), (f32::NAN, 50.0), (0.4, -1.0)] {
            let mut b = w.clone();
            b.scoring.stop_window = Some(StopWindow { secs, bonus });
            assert_eq!(fields_of(&b), ["scoring.stop_window"], "{secs} {bonus}");
        }
        let mut still = w.clone();
        still.player.movement = false;
        assert_eq!(fields_of(&still), ["scoring.stop_window"]);
        let mut hold = w.clone();
        hold.weapon.fire = FireMode::Hold;
        assert_eq!(fields_of(&hold), ["scoring.stop_window"]);
    }

    #[test]
    fn spider_defaults_off_stays_out_of_saved_files_parses_and_is_validated() {
        let d = ScenarioDef::from_toml(MINIMAL).unwrap();
        assert_eq!(d.targets.spider, None);
        assert!(!toml::to_string(&d).unwrap().contains("spider"));

        let src = MINIMAL.replace(
            "distance = [10, 10] }",
            "distance = [10, 10] }
spider = { ring_deg = [5, 12] }",
        );
        let s = ScenarioDef::from_toml(&src).unwrap();
        assert_eq!(
            s.targets.spider,
            Some(SpiderDef {
                ring_deg: Range::new(5.0, 12.0)
            })
        );
        assert!(s.validate().is_empty(), "{:?}", s.validate());
        assert!(ScenarioDef::from_toml(&src.replace("ring_deg", "ring")).is_err());

        for ring in [
            Range::new(0.0, 12.0),
            Range::new(12.0, 5.0),
            Range::new(5.0, 61.0),
        ] {
            let mut b = s.clone();
            b.targets.spider = Some(SpiderDef { ring_deg: ring });
            assert_eq!(fields_of(&b), ["targets.spider.ring_deg"], "{ring:?}");
        }
        let mut crowd = s.clone();
        crowd.targets.max_alive = 2;
        assert_eq!(fields_of(&crowd), ["targets.spider"]);
    }
}
