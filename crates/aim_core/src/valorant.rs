//! VALORANT reference data used by scenarios. Every value and its source is documented in
//! `docs/valorant-mechanics.md`; values tagged "approximate" there are best estimates.

use crate::movement::MovementProfile;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Weapon {
    Knife,
    Classic,
    Vandal,
    Phantom,
    Guardian,
    Operator,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeaponStats {
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
    pub damage_head: f32,
    pub damage_body: f32,
    pub damage_legs: f32,
}

impl Weapon {
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

    /// Movement profile while holding this weapon.
    pub fn movement(self) -> MovementProfile {
        let s = self.stats();
        MovementProfile {
            accuracy_threshold: s.accuracy_threshold,
            ..MovementProfile::valorant_with_speed(s.run_speed)
        }
    }
}

/// Agent hitbox dimensions in meters. All agents share one hitbox in VALORANT; exact
/// dimensions are not published, so these are documented estimates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct HumanoidDims {
    pub height: f32,
    pub head_size: f32,
    pub body_width: f32,
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
    fn eye_sits_below_head_center() {
        let d = HumanoidDims::default();
        assert!(d.eye_height < d.head_center_height());
        assert!(d.eye_height > d.height - d.head_size);
    }
}
