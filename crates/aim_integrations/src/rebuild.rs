//! Rebuild the match's duels as a drill: moving heads at the distances where the player
//! died. Distances are the only per-duel geometry the match data documents (angles are
//! not: `viewRadians` has no published convention), so the drill reproduces range, not
//! angle. See docs/match-analysis.md.

use crate::analysis::Analysis;
use crate::insight::MIN_DEATHS;
use aim_core::math::Range;
use aim_core::scenario::{ScenarioDef, SpawnArea, builtin};

/// Id (and file name) of the rebuilt drill in the user's scenarios folder.
pub const REBUILD_ID: &str = "match-rebuild";
/// The drill the rebuild starts from: strafing humanoid heads, the closest built-in match
/// for a duel against a moving player.
pub const REBUILD_BASE: &str = "dynamic-strafe-heads";
/// Distances are kept inside what the scenario floor supports and the built-ins use.
pub const MIN_DISTANCE_M: f64 = 3.0;
pub const MAX_DISTANCE_M: f64 = 50.0;
/// Horizontal spread of spawns, in degrees either side of forward at the nearest distance.
pub const SPREAD_DEG: f64 = 25.0;

/// The middle half of the player's known death distances, in meters (25th–75th
/// percentile, clamped and at least 2 m wide). `None` with fewer than [`MIN_DEATHS`].
pub fn death_band_m(analysis: &Analysis) -> Option<(f64, f64)> {
    let mut d: Vec<f64> = analysis
        .death_distances_m
        .iter()
        .flatten()
        .copied()
        .collect();
    if d.len() < MIN_DEATHS as usize {
        return None;
    }
    d.sort_by(f64::total_cmp);
    let pct = |p: f64| {
        let pos = p * (d.len() - 1) as f64;
        let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
        d[lo] + (d[hi] - d[lo]) * (pos - lo as f64)
    };
    let clamp = |v: f64| v.clamp(MIN_DISTANCE_M, MAX_DISTANCE_M);
    let (mut near, mut far) = (clamp(pct(0.25)), clamp(pct(0.75)));
    if far - near < 2.0 {
        // Widen around the middle, sliding back inside the limits at either end.
        let mid = (near + far) / 2.0;
        near = (mid - 1.0).clamp(MIN_DISTANCE_M, MAX_DISTANCE_M - 2.0);
        far = near + 2.0;
    }
    Some((near, far))
}

/// The rebuilt drill, or `None` when the match has too few deaths with a known distance.
pub fn rebuild_duels(analysis: &Analysis, match_id: &str) -> Option<ScenarioDef> {
    let (near, far) = death_band_m(analysis)?;
    let mut def = builtin::all().into_iter().find(|d| d.id == REBUILD_BASE)?;
    let half_width = (near * SPREAD_DEG.to_radians().tan()) as f32;
    def.targets.spawn = SpawnArea::Floor {
        x: Range::new(-half_width, half_width),
        z: Range::new(-far as f32, -near as f32),
        height: Range::fixed(1.6),
    };
    def.id = REBUILD_ID.to_owned();
    def.name = "Your Match Duels".to_owned();
    def.description = format!(
        "Strafing heads at {near:.0}–{far:.0} m, the middle half of the distances you died at in match {match_id}. Replace it by analyzing another match."
    );
    Some(def)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deaths(d: &[f64]) -> Analysis {
        Analysis {
            death_distances_m: d.iter().copied().map(Some).chain([None]).collect(),
            ..Analysis::default()
        }
    }

    #[test]
    fn the_band_is_the_middle_half_of_known_deaths() {
        assert_eq!(
            death_band_m(&deaths(&[50.0, 5.0, 20.0])),
            Some((12.5, 35.0))
        );
        assert_eq!(death_band_m(&deaths(&[5.0, 20.0])), None, "too few deaths");
        assert_eq!(
            death_band_m(&deaths(&[1.0, 1.0, 1.0])),
            Some((3.0, 5.0)),
            "clamped and widened"
        );
        assert_eq!(
            death_band_m(&deaths(&[90.0, 99.0, 80.0])),
            Some((48.0, 50.0))
        );
    }

    #[test]
    fn the_rebuilt_drill_is_a_valid_scenario_at_those_distances() {
        let def = rebuild_duels(&deaths(&[50.0, 5.0, 20.0]), "m1").unwrap();
        assert!(def.validate().is_empty(), "{:?}", def.validate());
        assert_eq!(def.id, REBUILD_ID);
        assert!(
            def.description.contains("35 m") && def.description.contains("m1"),
            "{}",
            def.description
        );
        let SpawnArea::Floor { z, .. } = def.targets.spawn else {
            panic!("floor spawn")
        };
        assert_eq!((z.min, z.max), (-35.0, -12.5));
        let toml = toml::to_string(&def).unwrap();
        assert_eq!(
            ScenarioDef::from_toml(&toml).unwrap(),
            def,
            "saved file loads back"
        );
    }
}
