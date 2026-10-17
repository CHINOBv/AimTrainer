//! VALORANT reference data used by scenarios. Every value and its source is documented in
//! `docs/valorant-mechanics.md`; values tagged "approximate" there are best estimates.

use crate::movement::MovementProfile;
use serde::{Deserialize, Serialize};

/// Weapons a scenario can use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Weapon {
    /// Melee; fastest movement.
    Knife,
    /// Starting sidearm.
    Classic,
    /// Full-auto rifle.
    Vandal,
    /// Full-auto rifle.
    Phantom,
    /// Semi-auto rifle with optional scope.
    Guardian,
    /// Scoped sniper rifle.
    Operator,
}

/// Reference stats of a weapon.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeaponStats {
    /// Name shown in the UI.
    pub display_name: &'static str,
    /// Run speed while holding the weapon, m/s.
    pub run_speed: f32,
    /// Rounds per second (hip-fire).
    pub fire_rate: f32,
    /// First-shot spread standing still, degrees.
    pub first_shot_spread_deg: f32,
    /// Extra spread while running, degrees.
    pub run_spread_deg: f32,
    /// Fraction of run speed under which shots are fully accurate.
    pub accuracy_threshold: f32,
    /// ADS/scope zoom factor (1.0 = no zoom).
    pub zoom: f32,
    /// Damage per head hit.
    pub damage_head: f32,
    /// Damage per body hit.
    pub damage_body: f32,
    /// Damage per leg hit.
    pub damage_legs: f32,
}

impl Weapon {
    /// Reference stats of this weapon.
    pub fn stats(self) -> WeaponStats {
        match self {
            Weapon::Knife => WeaponStats {
                display_name: "Knife",
                run_speed: 6.75,
                fire_rate: 2.0,
                first_shot_spread_deg: 0.0,
                run_spread_deg: 0.0,
                accuracy_threshold: 1.0,
                zoom: 1.0,
                damage_head: 50.0,
                damage_body: 50.0,
                damage_legs: 50.0,
            },
            Weapon::Classic => WeaponStats {
                display_name: "Classic",
                run_speed: 5.73,
                fire_rate: 6.75,
                first_shot_spread_deg: 0.4,
                run_spread_deg: 1.8,
                accuracy_threshold: 0.275,
                zoom: 1.0,
                damage_head: 78.0,
                damage_body: 26.0,
                damage_legs: 22.0,
            },
            Weapon::Vandal => WeaponStats {
                display_name: "Vandal",
                run_speed: 5.4,
                fire_rate: 9.75,
                first_shot_spread_deg: 0.25,
                run_spread_deg: 6.0,
                accuracy_threshold: 0.275,
                zoom: 1.25,
                damage_head: 160.0,
                damage_body: 40.0,
                damage_legs: 34.0,
            },
            Weapon::Phantom => WeaponStats {
                display_name: "Phantom",
                run_speed: 5.4,
                fire_rate: 11.0,
                first_shot_spread_deg: 0.2,
                run_spread_deg: 6.0,
                accuracy_threshold: 0.275,
                zoom: 1.25,
                damage_head: 156.0,
                damage_body: 39.0,
                damage_legs: 33.0,
            },
            Weapon::Guardian => WeaponStats {
                display_name: "Guardian",
                run_speed: 5.4,
                fire_rate: 5.25,
                first_shot_spread_deg: 0.1,
                run_spread_deg: 6.0,
                accuracy_threshold: 0.275,
                zoom: 1.5,
                damage_head: 195.0,
                damage_body: 65.0,
                damage_legs: 48.75,
            },
            Weapon::Operator => WeaponStats {
                display_name: "Operator",
                run_speed: 5.13,
                fire_rate: 0.75,
                first_shot_spread_deg: 5.0,
                run_spread_deg: 6.0,
                accuracy_threshold: 0.15,
                zoom: 2.5,
                damage_head: 255.0,
                damage_body: 150.0,
                damage_legs: 127.0,
            },
        }
    }

    /// Full-auto recoil and spread model, for the rifles whose recoil is documented
    /// (Vandal and Phantom). `None` for every other weapon.
    pub fn recoil(self) -> Option<RecoilProfile> {
        match self {
            Weapon::Vandal => Some(RecoilProfile {
                protected_bullets: 6,
                max_spread_deg: 1.0,
                spread_ramp_bullets: 10,
                recovery_secs: 0.375,
                yaw_switch_chance: 0.1,
                yaw_switch_secs: 0.6,
                climb_deg_per_bullet: 0.35,
                sway_deg_per_bullet: 0.3,
            }),
            Weapon::Phantom => Some(RecoilProfile {
                protected_bullets: 8,
                max_spread_deg: 0.9,
                spread_ramp_bullets: 10,
                recovery_secs: 0.35,
                yaw_switch_chance: 0.1,
                yaw_switch_secs: 0.6,
                climb_deg_per_bullet: 0.3,
                sway_deg_per_bullet: 0.25,
            }),
            _ => None,
        }
    }

    /// Movement profile while holding this weapon.
    pub fn movement(self) -> MovementProfile {
        let s = self.stats();
        MovementProfile {
            accuracy_threshold: s.accuracy_threshold,
            ..MovementProfile::valorant_with_speed(s.run_speed)
        }
    }
}

/// Recoil and spread of a full-auto spray (see `docs/valorant-mechanics.md`, "Recoil").
/// Riot publishes the bullet counts and timings; the per-bullet angles are estimates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecoilProfile {
    /// Bullets that only climb vertically before the horizontal sway starts (patch 11.08).
    pub protected_bullets: u32,
    /// Spread a long spray reaches, standing, degrees.
    pub max_spread_deg: f32,
    /// Bullets over which spread grows linearly from first-shot spread to the maximum
    /// (trainer model, estimate).
    pub spread_ramp_bullets: u32,
    /// Pause without firing after which recoil and spread fully reset (patch 0.50).
    pub recovery_secs: f32,
    /// Chance per bullet, after the protected ones, that the sway switches side.
    pub yaw_switch_chance: f32,
    /// Time the sway takes to swing to the other side after a switch.
    pub yaw_switch_secs: f32,
    /// Vertical climb added by each protected bullet, degrees (estimate).
    pub climb_deg_per_bullet: f32,
    /// Horizontal sway added by each bullet after the protected ones, degrees (estimate).
    pub sway_deg_per_bullet: f32,
}

/// Agent hitbox dimensions in meters. All agents share one hitbox in VALORANT; exact
/// dimensions are not published, so these are documented estimates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HumanoidDims {
    /// Total height in meters.
    pub height: f32,
    /// Head diameter in meters.
    pub head_size: f32,
    /// Body width in meters.
    pub body_width: f32,
    /// Body depth in meters.
    pub body_depth: f32,
    /// Height where legs end and torso starts.
    pub waist_height: f32,
    /// Camera (eye) height of a standing player. Sits below the head center, so a level
    /// crosshair lines up with the mouth of an equal-height enemy.
    pub eye_height: f32,
}

impl Default for HumanoidDims {
    fn default() -> Self {
        Self {
            height: 2.0,
            head_size: 0.25,
            body_width: 0.5,
            body_depth: 0.3,
            waist_height: 1.0,
            eye_height: 1.82,
        }
    }
}

impl HumanoidDims {
    /// Height in meters of the head center above the floor.
    pub fn head_center_height(&self) -> f32 {
        self.height - self.head_size / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rifles_use_rifle_run_speed_and_operator_has_strict_threshold() {
        assert_eq!(Weapon::Vandal.movement().run_speed, 5.4);
        assert_eq!(Weapon::Knife.movement().run_speed, 6.75);
        let op = Weapon::Operator.movement();
        assert!((op.accuracy_speed_limit() - 5.13 * 0.15).abs() < 1e-6);
    }

    #[test]
    fn only_vandal_and_phantom_have_a_recoil_model_with_patch_values() {
        let v = Weapon::Vandal.recoil().unwrap();
        let p = Weapon::Phantom.recoil().unwrap();
        assert_eq!((v.protected_bullets, p.protected_bullets), (6, 8));
        assert_eq!((v.recovery_secs, p.recovery_secs), (0.375, 0.35));
        assert!(Weapon::Guardian.recoil().is_none());
        assert!(Weapon::Classic.recoil().is_none());
    }

    #[test]
    fn eye_sits_below_head_center() {
        let d = HumanoidDims::default();
        assert!(d.eye_height < d.head_center_height());
        assert!(d.eye_height > d.height - d.head_size);
    }
}
