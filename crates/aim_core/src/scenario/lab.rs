//! Movement Lab: a scenario generated from a handful of player-chosen settings (enemy
//! motion, count, spawn distance and frequency, player movement area, weapon). The lab only
//! composes existing building blocks, so the generated definition passes
//! [`ScenarioDef::validate`] like any file in `content/scenarios/`.
//!
//! Every generated definition gets a deterministic id `lab-<hash>`, so lab runs are kept in
//! history (and personal bests) apart from the built-in scenarios and from each other.

use super::def::{
    Category, FireMode, Motion, PlayerDef, ScenarioDef, ScoringDef, Shape, SpawnArea, TargetsDef,
    WeaponDef,
};
use super::difficulty::DifficultyDef;
use super::phase::{FailDef, RunEnd};
use crate::games::valorant::Weapon;
use crate::math::Range;
use serde::{Deserialize, Serialize};

/// Prefix of every generated lab scenario id.
pub const LAB_PREFIX: &str = "lab-";

/// Name every lab scenario is shown with.
pub const LAB_NAME: &str = "Movement Lab";

/// Weapons the lab offers, in the order the UI lists them.
pub const LAB_WEAPONS: [Weapon; 5] = [
    Weapon::Classic,
    Weapon::Vandal,
    Weapon::Phantom,
    Weapon::Guardian,
    Weapon::Operator,
];

/// Allowed enemy speed, percent of the weapon's VALORANT run speed.
pub const SPEED_PCT: (u32, u32) = (50, 150);
/// Allowed mean time between an enemy's direction decisions, seconds.
pub const CHANGE_SECS: (f32, f32) = (0.2, 2.0);
/// Allowed enemies alive at once.
pub const MAX_ALIVE: (u32, u32) = (1, 6);
/// Allowed spawn distance from the player's start, meters.
pub const DISTANCE_M: (f32, f32) = (5.0, 50.0);
/// Allowed lateral spread of spawns (half-width), meters.
pub const SPREAD_M: (f32, f32) = (1.0, 20.0);
/// Allowed longest appear delay, seconds.
pub const APPEAR_DELAY_SECS: (f32, f32) = (0.0, 3.0);
/// Allowed enemy lifetime, seconds.
pub const LIFETIME_SECS: (f32, f32) = (0.5, 10.0);
/// Allowed half-width of the player's movement area, meters.
pub const AREA_HALF_WIDTH_M: (f32, f32) = (2.0, 20.0);
/// Allowed depth of the player's movement area, meters.
pub const AREA_DEPTH_M: (f32, f32) = (0.0, 12.0);
/// Allowed run length, seconds.
pub const DURATION_SECS: (f32, f32) = (15.0, 180.0);

/// Lateral distance a strafing or dodging enemy may move away from its spawn point, meters
/// (the scenario default for strafing targets).
const STRAFE_WIDTH_M: f32 = 3.0;
/// Minimum angle between enemies alive at once when there are several, degrees.
const SEPARATION_DEG: f32 = 6.0;
/// Points lost per miss, per shot fired while moving and per enemy that leaves; the values
/// of the built-in counter-strafe drills.
const MISS_PENALTY: f32 = 10.0;
const MOVING_SHOT_PENALTY: f32 = 50.0;
const EXPIRE_PENALTY: f32 = 50.0;

/// How the lab's enemies move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabMotion {
    /// They stand still.
    Static,
    /// A/D strafing with VALORANT movement physics.
    #[default]
    Strafe,
    /// Strafing plus jukes, stop-and-go and crouches (see [`DodgePreset`]).
    Dodge,
}

impl LabMotion {
    /// Every option, in the order the UI lists them.
    pub const ALL: [LabMotion; 3] = [LabMotion::Static, LabMotion::Strafe, LabMotion::Dodge];

    /// Name shown in the UI.
    pub fn label(self) -> &'static str {
        match self {
            LabMotion::Static => "Static",
            LabMotion::Strafe => "Strafe",
            LabMotion::Dodge => "Dodge",
        }
    }
}

/// How evasive dodging enemies are: their juke, stop and crouch chances.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DodgePreset {
    /// Few jukes, rare crouches.
    Light,
    /// About the built-in "Juke Heads" drill.
    #[default]
    Medium,
    /// Mostly jukes, frequent crouches.
    Heavy,
}

impl DodgePreset {
    /// Every option, in the order the UI lists them.
    pub const ALL: [DodgePreset; 3] = [DodgePreset::Light, DodgePreset::Medium, DodgePreset::Heavy];

    /// Name shown in the UI.
    pub fn label(self) -> &'static str {
        match self {
            DodgePreset::Light => "Light",
            DodgePreset::Medium => "Medium",
            DodgePreset::Heavy => "Heavy",
        }
    }

    /// `(juke_chance, stop_chance, crouch_chance)` of this preset.
    pub fn chances(self) -> (f32, f32, f32) {
        match self {
            DodgePreset::Light => (0.2, 0.15, 0.05),
            DodgePreset::Medium => (0.5, 0.25, 0.1),
            DodgePreset::Heavy => (0.7, 0.25, 0.25),
        }
    }
}

/// What the player chose in the Movement Lab. Missing keys take their defaults and
/// out-of-range values are clamped when the scenario is built, so a hand-edited settings
/// file can never produce an invalid scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LabSettings {
    /// How enemies move.
    pub motion: LabMotion,
    /// Enemy speed, percent of the weapon's run speed (strafe and dodge).
    pub speed_pct: u32,
    /// Mean time between an enemy's direction decisions, seconds (strafe and dodge); each
    /// decision waits a random 60–140% of it.
    pub change_secs: f32,
    /// How evasive dodging enemies are.
    pub dodge: DodgePreset,
    /// Enemies alive at once.
    pub max_alive: u32,
    /// Only head hits count (otherwise the whole body is a target).
    pub head_only: bool,
    /// Spawn distance in front of the player's start, meters.
    pub distance_m: Range,
    /// Spawns land up to this far left or right of the center line, meters.
    pub spread_m: f32,
    /// Each enemy appears after a random delay between half of this and this, seconds.
    pub appear_delay_secs: f32,
    /// Enemies stay until killed instead of leaving after `lifetime_secs`.
    pub until_killed: bool,
    /// How long an enemy stays before it leaves, seconds (unless `until_killed`).
    pub lifetime_secs: f32,
    /// The player can move with WASD.
    pub player_movement: bool,
    /// The player may move this far left or right of the start, meters.
    pub area_half_width_m: f32,
    /// Depth of the player's movement area, centered on the start, meters.
    pub area_depth_m: f32,
    /// Weapon: run speed, fire rate, spread and damage zones.
    pub weapon: Weapon,
    /// Apply first-shot spread and movement inaccuracy to shots.
    pub simulate_spread: bool,
    /// Run length, seconds.
    pub duration_secs: f32,
}

impl Default for LabSettings {
    fn default() -> Self {
        Self {
            motion: LabMotion::Strafe,
            speed_pct: 100,
            change_secs: 0.6,
            dodge: DodgePreset::Medium,
            max_alive: 2,
            head_only: true,
            distance_m: Range::new(12.0, 22.0),
            spread_m: 9.0,
            appear_delay_secs: 0.4,
            until_killed: false,
            lifetime_secs: 3.0,
            player_movement: true,
            area_half_width_m: 16.0,
            area_depth_m: 8.0,
            weapon: Weapon::Vandal,
            simulate_spread: true,
            duration_secs: 60.0,
        }
    }
}

/// `v` limited to `[lo, hi]`; a non-finite value becomes `fallback`.
fn clamp_f(v: f32, (lo, hi): (f32, f32), fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        fallback
    }
}

impl LabSettings {
    /// These settings with every value inside its allowed range: numbers are clamped,
    /// non-finite numbers take their default, and a reversed distance range is reordered.
    pub fn clamped(&self) -> Self {
        let d = Self::default();
        let (a, b) = (
            clamp_f(self.distance_m.min, DISTANCE_M, d.distance_m.min),
            clamp_f(self.distance_m.max, DISTANCE_M, d.distance_m.max),
        );
        Self {
            speed_pct: self.speed_pct.clamp(SPEED_PCT.0, SPEED_PCT.1),
            change_secs: clamp_f(self.change_secs, CHANGE_SECS, d.change_secs),
            max_alive: self.max_alive.clamp(MAX_ALIVE.0, MAX_ALIVE.1),
            distance_m: Range::new(a.min(b), a.max(b)),
            spread_m: clamp_f(self.spread_m, SPREAD_M, d.spread_m),
            appear_delay_secs: clamp_f(
                self.appear_delay_secs,
                APPEAR_DELAY_SECS,
                d.appear_delay_secs,
            ),
            lifetime_secs: clamp_f(self.lifetime_secs, LIFETIME_SECS, d.lifetime_secs),
            area_half_width_m: clamp_f(
                self.area_half_width_m,
                AREA_HALF_WIDTH_M,
                d.area_half_width_m,
            ),
            area_depth_m: clamp_f(self.area_depth_m, AREA_DEPTH_M, d.area_depth_m),
            duration_secs: clamp_f(self.duration_secs, DURATION_SECS, d.duration_secs),
            ..self.clone()
        }
    }

    /// The scenario these settings describe (clamped first). Its id is `lab-<hash>` of its
    /// content: the same settings always give the same id, and different settings a
    /// different one.
    pub fn to_def(&self) -> ScenarioDef {
        let s = self.clamped();
        let mut def = ScenarioDef {
            id: LAB_PREFIX.to_owned(),
            name: LAB_NAME.to_owned(),
            category: Category::Movement,
            game: Some(crate::games::Game::Valorant),
            description: s.summary(),
            duration_secs: s.duration_secs,
            weapon: WeaponDef {
                kind: s.weapon,
                fire: FireMode::Click,
                simulate_spread: s.simulate_spread,
                ammo: None,
                recoil: false,
            },
            player: s.player(),
            targets: s.targets(),
            scoring: ScoringDef {
                miss_penalty: MISS_PENALTY,
                moving_shot_penalty: if s.player_movement {
                    MOVING_SHOT_PENALTY
                } else {
                    0.0
                },
                expire_penalty: if s.until_killed { 0.0 } else { EXPIRE_PENALTY },
                ..ScoringDef::default()
            },
            difficulty: DifficultyDef::default(),
            end: RunEnd::default(),
            fail: FailDef::default(),
            phases: None,
            view: Default::default(),
            feedback: Default::default(),
        };
        def.id = format!("{LAB_PREFIX}{:016x}", def.content_hash());
        def
    }

    /// One line describing the drill, e.g. "Strafe 100% · 2 heads · 12–22 m ±9 m · …".
    pub fn summary(&self) -> String {
        let s = self.clamped();
        let motion = match s.motion {
            LabMotion::Static => "Static enemies".to_owned(),
            LabMotion::Strafe => format!("Strafe {}% every ~{:.1} s", s.speed_pct, s.change_secs),
            LabMotion::Dodge => format!(
                "Dodge ({}) {}% every ~{:.1} s",
                s.dodge.label().to_lowercase(),
                s.speed_pct,
                s.change_secs
            ),
        };
        let count = match (s.max_alive, s.head_only) {
            (1, true) => "1 head".to_owned(),
            (1, false) => "1 body".to_owned(),
            (n, true) => format!("{n} heads"),
            (n, false) => format!("{n} bodies"),
        };
        let life = if s.until_killed {
            "until killed".to_owned()
        } else {
            format!("{:.1} s life", s.lifetime_secs)
        };
        let player = if s.player_movement {
            format!(
                "you move ±{:.0} m × {:.0} m",
                s.area_half_width_m, s.area_depth_m
            )
        } else {
            "you stand still".to_owned()
        };
        let spread = if s.simulate_spread { "on" } else { "off" };
        format!(
            "{motion} · {count} · {:.0}–{:.0} m ±{:.0} m · appear ≤{:.1} s, {life} · {player} · {} spread {spread} · {:.0} s",
            s.distance_m.min,
            s.distance_m.max,
            s.spread_m,
            s.appear_delay_secs,
            s.weapon.stats().display_name,
            s.duration_secs
        )
    }

    /// The player block of already clamped settings.
    fn player(&self) -> PlayerDef {
        if !self.player_movement {
            return PlayerDef::default();
        }
        let (w, d) = (self.area_half_width_m, self.area_depth_m / 2.0);
        PlayerDef {
            movement: true,
            bounds_x: Range::new(-w, w),
            bounds_z: Range::new(-d, d),
            elevation: 0.0,
            start: None,
            start_velocity: None,
        }
    }

    /// The targets block of already clamped settings.
    fn targets(&self) -> TargetsDef {
        let change_secs = Range::new(self.change_secs * 0.6, self.change_secs * 1.4);
        let motion = match self.motion {
            LabMotion::Static => Motion::Static,
            LabMotion::Strafe => Motion::Strafe {
                weapon: self.weapon,
                change_secs,
                stop_chance: 0.2,
                max_offset: STRAFE_WIDTH_M,
                depth: None,
            },
            LabMotion::Dodge => {
                let (juke_chance, stop_chance, crouch_chance) = self.dodge.chances();
                Motion::Dodge {
                    weapon: self.weapon,
                    change_secs,
                    juke_secs: Range::new(0.1, 0.22),
                    juke_chance,
                    stop_chance,
                    crouch_chance,
                    jump_chance: 0.0,
                    max_offset: STRAFE_WIDTH_M,
                    depth: None,
                }
            }
        };
        let moving = self.motion != LabMotion::Static;
        TargetsDef {
            shape: Shape::Humanoid {
                head_only: self.head_only,
            },
            spawn: SpawnArea::Floor {
                x: Range::new(-self.spread_m, self.spread_m),
                z: Range::new(-self.distance_m.max, -self.distance_m.min),
                height: Range::fixed(1.6),
            },
            motion,
            max_alive: self.max_alive,
            health: 1.0,
            lifetime_secs: (!self.until_killed).then_some(self.lifetime_secs),
            appear_delay_secs: Range::new(self.appear_delay_secs / 2.0, self.appear_delay_secs),
            min_separation_deg: if self.max_alive > 1 {
                SEPARATION_DEG
            } else {
                0.0
            },
            speed_scale: if moving {
                self.speed_pct as f32 / 100.0
            } else {
                1.0
            },
            decoy_chance: 0.0,
            spawn_rules: None,
            regen: None,
            reactions: None,
            behaviors: Vec::new(),
            perception: None,
            shrink_per_kill: None,
            motion_profiles: Vec::new(),
            spider: None,
        }
    }
}

/// Whether `id` (a scenario id or run key) names a generated lab scenario.
pub fn is_lab_id(id: &str) -> bool {
    id.starts_with(LAB_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::{FrameInput, Session};

    fn every_variant() -> Vec<LabSettings> {
        let mut out = Vec::new();
        for motion in LabMotion::ALL {
            for dodge in DodgePreset::ALL {
                for (head_only, until_killed, player_movement) in
                    [(true, false, true), (false, true, false)]
                {
                    for weapon in LAB_WEAPONS {
                        out.push(LabSettings {
                            motion,
                            dodge,
                            head_only,
                            until_killed,
                            player_movement,
                            weapon,
                            ..LabSettings::default()
                        });
                    }
                }
            }
        }
        out
    }

    fn extremes() -> Vec<LabSettings> {
        let low = LabSettings {
            speed_pct: SPEED_PCT.0,
            change_secs: CHANGE_SECS.0,
            max_alive: MAX_ALIVE.0,
            distance_m: Range::fixed(DISTANCE_M.0),
            spread_m: SPREAD_M.0,
            appear_delay_secs: APPEAR_DELAY_SECS.0,
            lifetime_secs: LIFETIME_SECS.0,
            area_half_width_m: AREA_HALF_WIDTH_M.0,
            area_depth_m: AREA_DEPTH_M.0,
            duration_secs: DURATION_SECS.0,
            ..LabSettings::default()
        };
        let high = LabSettings {
            speed_pct: SPEED_PCT.1,
            change_secs: CHANGE_SECS.1,
            max_alive: MAX_ALIVE.1,
            distance_m: Range::fixed(DISTANCE_M.1),
            spread_m: SPREAD_M.1,
            appear_delay_secs: APPEAR_DELAY_SECS.1,
            lifetime_secs: LIFETIME_SECS.1,
            area_half_width_m: AREA_HALF_WIDTH_M.1,
            area_depth_m: AREA_DEPTH_M.1,
            duration_secs: DURATION_SECS.1,
            motion: LabMotion::Dodge,
            dodge: DodgePreset::Heavy,
            ..LabSettings::default()
        };
        vec![low, high]
    }

    #[test]
    fn every_generated_scenario_validates() {
        for s in every_variant().into_iter().chain(extremes()) {
            let def = s.to_def();
            assert_eq!(def.validate(), Vec::new(), "{s:?}");
        }
    }

    #[test]
    fn the_defaults_are_a_wide_strafing_movement_drill() {
        let def = LabSettings::default().to_def();
        assert_eq!(def.name, LAB_NAME);
        assert_eq!(def.category, Category::Movement);
        assert!(def.player.movement);
        assert_eq!(def.player.bounds_x, Range::new(-16.0, 16.0));
        assert!(matches!(def.targets.motion, Motion::Strafe { .. }));
    }

    #[test]
    fn enemy_motion_speed_and_turns_map_to_the_motion_block() {
        let s = LabSettings {
            motion: LabMotion::Strafe,
            speed_pct: 130,
            change_secs: 1.0,
            weapon: Weapon::Phantom,
            ..LabSettings::default()
        };
        let t = s.to_def().targets;
        assert_eq!(t.speed_scale, 1.3);
        match t.motion {
            Motion::Strafe {
                weapon,
                change_secs,
                ..
            } => {
                assert_eq!(weapon, Weapon::Phantom, "enemies run with the lab weapon");
                assert!((change_secs.min - 0.6).abs() < 1e-6);
                assert!((change_secs.max - 1.4).abs() < 1e-6);
            }
            m => panic!("expected strafe, got {m:?}"),
        }

        let still = LabSettings {
            motion: LabMotion::Static,
            speed_pct: 130,
            ..LabSettings::default()
        };
        let t = still.to_def().targets;
        assert_eq!(t.motion, Motion::Static);
        assert_eq!(
            t.speed_scale, 1.0,
            "speed has no meaning for static enemies"
        );
    }

    #[test]
    fn dodge_presets_set_the_juke_stop_and_crouch_chances() {
        for preset in DodgePreset::ALL {
            let s = LabSettings {
                motion: LabMotion::Dodge,
                dodge: preset,
                ..LabSettings::default()
            };
            match s.to_def().targets.motion {
                Motion::Dodge {
                    juke_chance,
                    stop_chance,
                    crouch_chance,
                    jump_chance,
                    ..
                } => {
                    assert_eq!((juke_chance, stop_chance, crouch_chance), preset.chances());
                    assert_eq!(jump_chance, 0.0);
                }
                m => panic!("expected dodge, got {m:?}"),
            }
        }
        let (light, heavy) = (DodgePreset::Light.chances(), DodgePreset::Heavy.chances());
        assert!(
            light.0 < heavy.0 && light.2 < heavy.2,
            "heavy jukes and crouches more"
        );
    }

    #[test]
    fn count_shape_and_spawn_settings_map_to_the_targets_block() {
        let s = LabSettings {
            max_alive: 4,
            head_only: false,
            distance_m: Range::new(10.0, 30.0),
            spread_m: 7.0,
            appear_delay_secs: 1.0,
            ..LabSettings::default()
        };
        let t = s.to_def().targets;
        assert_eq!(t.max_alive, 4);
        assert_eq!(t.shape, Shape::Humanoid { head_only: false });
        assert_eq!(
            t.spawn,
            SpawnArea::Floor {
                x: Range::new(-7.0, 7.0),
                z: Range::new(-30.0, -10.0),
                height: Range::fixed(1.6),
            }
        );
        assert_eq!(t.appear_delay_secs, Range::new(0.5, 1.0));
        assert_eq!(t.min_separation_deg, SEPARATION_DEG);
        let one = LabSettings {
            max_alive: 1,
            ..LabSettings::default()
        };
        assert_eq!(one.to_def().targets.min_separation_deg, 0.0);
    }

    #[test]
    fn lifetime_or_until_killed_sets_lifetime_and_the_expire_penalty() {
        let timed = LabSettings {
            lifetime_secs: 2.5,
            until_killed: false,
            ..LabSettings::default()
        }
        .to_def();
        assert_eq!(timed.targets.lifetime_secs, Some(2.5));
        assert_eq!(timed.scoring.expire_penalty, EXPIRE_PENALTY);
        let endless = LabSettings {
            lifetime_secs: 2.5,
            until_killed: true,
            ..LabSettings::default()
        }
        .to_def();
        assert_eq!(endless.targets.lifetime_secs, None);
        assert_eq!(endless.scoring.expire_penalty, 0.0, "nothing can expire");
    }

    #[test]
    fn player_movement_and_area_map_to_the_player_block() {
        let moving = LabSettings {
            player_movement: true,
            area_half_width_m: 10.0,
            area_depth_m: 6.0,
            ..LabSettings::default()
        }
        .to_def();
        assert!(moving.player.movement);
        assert_eq!(moving.player.bounds_x, Range::new(-10.0, 10.0));
        assert_eq!(moving.player.bounds_z, Range::new(-3.0, 3.0));
        assert_eq!(moving.scoring.moving_shot_penalty, MOVING_SHOT_PENALTY);

        let still = LabSettings {
            player_movement: false,
            ..LabSettings::default()
        }
        .to_def();
        assert_eq!(still.player, PlayerDef::default());
        assert_eq!(still.scoring.moving_shot_penalty, 0.0);
    }

    #[test]
    fn weapon_spread_and_length_map_to_the_weapon_block_and_duration() {
        let def = LabSettings {
            weapon: Weapon::Guardian,
            simulate_spread: false,
            duration_secs: 90.0,
            ..LabSettings::default()
        }
        .to_def();
        assert_eq!(def.weapon.kind, Weapon::Guardian);
        assert!(!def.weapon.simulate_spread);
        assert_eq!(def.weapon.fire, FireMode::Click);
        assert_eq!(def.duration_secs, 90.0);
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let wild = LabSettings {
            speed_pct: 900,
            change_secs: 0.0,
            max_alive: 40,
            distance_m: Range::new(80.0, 1.0),
            spread_m: -3.0,
            appear_delay_secs: 9.0,
            lifetime_secs: f32::NAN,
            area_half_width_m: 100.0,
            area_depth_m: -1.0,
            duration_secs: 5.0,
            ..LabSettings::default()
        };
        let c = wild.clamped();
        assert_eq!(c.speed_pct, SPEED_PCT.1);
        assert_eq!(c.change_secs, CHANGE_SECS.0);
        assert_eq!(c.max_alive, MAX_ALIVE.1);
        assert_eq!(
            c.distance_m,
            Range::new(DISTANCE_M.0, DISTANCE_M.1),
            "reordered"
        );
        assert_eq!(c.spread_m, SPREAD_M.0);
        assert_eq!(c.appear_delay_secs, APPEAR_DELAY_SECS.1);
        assert_eq!(c.lifetime_secs, LabSettings::default().lifetime_secs, "NaN");
        assert_eq!(c.area_half_width_m, AREA_HALF_WIDTH_M.1);
        assert_eq!(c.area_depth_m, AREA_DEPTH_M.0);
        assert_eq!(c.duration_secs, DURATION_SECS.0);
        assert_eq!(MAX_ALIVE, (1, 6));
        assert_eq!(
            wild.to_def(),
            c.to_def(),
            "the def is built from clamped values"
        );
        assert_eq!(wild.to_def().validate(), Vec::new());
        let none = LabSettings {
            max_alive: 0,
            ..LabSettings::default()
        };
        assert_eq!(none.clamped().max_alive, 1);
    }

    #[test]
    fn ids_are_deterministic_distinct_and_never_collide_with_built_ins() {
        let a = LabSettings::default().to_def();
        assert_eq!(
            a.id,
            LabSettings::default().to_def().id,
            "same settings, same id"
        );
        assert!(is_lab_id(&a.id), "{}", a.id);
        let b = LabSettings {
            max_alive: 3,
            ..LabSettings::default()
        }
        .to_def();
        assert_ne!(a.id, b.id, "different settings, different id");
        for def in crate::scenario::builtin::all() {
            assert!(!is_lab_id(&def.id), "{} looks like a lab id", def.id);
        }
        let key = crate::scenario::RunKey::parse(&a.id);
        assert_eq!(key.base, a.id, "a lab id is a plain run key");
    }

    #[test]
    fn settings_round_trip_and_missing_keys_take_defaults() {
        let s = LabSettings {
            motion: LabMotion::Dodge,
            dodge: DodgePreset::Heavy,
            until_killed: true,
            weapon: Weapon::Operator,
            distance_m: Range::new(8.0, 14.0),
            ..LabSettings::default()
        };
        let text = toml::to_string(&s).unwrap();
        assert_eq!(toml::from_str::<LabSettings>(&text).unwrap(), s);
        let partial: LabSettings = toml::from_str("max_alive = 5\n").unwrap();
        assert_eq!(
            partial,
            LabSettings {
                max_alive: 5,
                ..LabSettings::default()
            }
        );
    }

    #[test]
    fn the_summary_names_every_choice() {
        let s = LabSettings::default().summary();
        assert_eq!(
            s,
            "Strafe 100% every ~0.6 s · 2 heads · 12–22 m ±9 m · appear ≤0.4 s, 3.0 s life · \
             you move ±16 m × 8 m · Vandal spread on · 60 s"
        );
        let other = LabSettings {
            motion: LabMotion::Dodge,
            dodge: DodgePreset::Light,
            max_alive: 1,
            head_only: false,
            until_killed: true,
            player_movement: false,
            simulate_spread: false,
            ..LabSettings::default()
        }
        .summary();
        assert!(other.starts_with("Dodge (light) 100%"), "{other}");
        for part in ["1 body", "until killed", "you stand still", "spread off"] {
            assert!(other.contains(part), "{other}");
        }
        assert_eq!(LabSettings::default().to_def().description, s);
    }

    #[test]
    fn a_generated_drill_plays_deterministically() {
        let def = LabSettings {
            motion: LabMotion::Dodge,
            max_alive: 3,
            ..LabSettings::default()
        }
        .to_def();
        let run = |seed| {
            let mut session = Session::new(def.clone(), seed);
            for _ in 0..120 {
                session.update(FrameInput {
                    dt: 1.0 / 60.0,
                    ..FrameInput::default()
                });
            }
            session.targets().map(|t| t.position).collect::<Vec<_>>()
        };
        let first = run(7);
        assert!(!first.is_empty(), "enemies spawn");
        assert_eq!(first, run(7));
    }
}
