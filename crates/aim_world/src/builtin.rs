//! Built-in map library, embedded at compile time. The same TOML files double as examples
//! for writing custom maps (see `docs/maps.md`).

use crate::map::MapDef;

pub const SOURCES: &[(&str, &str)] = &[
    (
        "angles-lab.toml",
        include_str!("../../../content/maps/angles-lab.toml"),
    ),
    (
        "duel-corridor.toml",
        include_str!("../../../content/maps/duel-corridor.toml"),
    ),
    (
        "training-yard.toml",
        include_str!("../../../content/maps/training-yard.toml"),
    ),
];

/// Parse every built-in map. Panics only if a shipped file is broken, which the test suite
/// guards against.
pub fn all() -> Vec<MapDef> {
    SOURCES
        .iter()
        .map(|(file, src)| {
            MapDef::from_toml(src).unwrap_or_else(|e| panic!("built-in {file}: {e}"))
        })
        .collect()
}

/// The built-in map with this id.
pub fn get(id: &str) -> Option<MapDef> {
    all().into_iter().find(|m| m.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DEFAULT_PLAYER_RADIUS;
    use crate::map::SpawnKind;
    use std::collections::HashSet;

    #[test]
    fn every_builtin_parses_validates_and_has_a_unique_id() {
        let all = all();
        assert_eq!(all.len(), SOURCES.len());
        let mut ids = HashSet::new();
        for (def, (file, _)) in all.iter().zip(SOURCES) {
            assert!(def.validate().is_empty(), "{file}: {:?}", def.validate());
            assert_eq!(format!("{}.toml", def.id), *file, "file name must match id");
            assert!(ids.insert(def.id.clone()), "duplicate id {}", def.id);
        }
    }

    #[test]
    fn a_walkable_path_joins_every_player_spawn_to_every_bot_spawn() {
        for map in all() {
            for p in map.spawns_of(SpawnKind::Player) {
                for b in map.spawns_of(SpawnKind::Bot) {
                    let route = map
                        .path(p.position, b.position)
                        .unwrap_or_else(|| panic!("{}: no path {} -> {}", map.id, p.id, b.id));
                    for leg in route.windows(2) {
                        assert!(
                            map.walkable(leg[0], leg[1], DEFAULT_PLAYER_RADIUS),
                            "{}: {} -> {} leg {leg:?}",
                            map.id,
                            p.id,
                            b.id
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn angles_lab_has_the_five_drill_angles_and_a_real_long_angle() {
        let lab = get("angles-lab").unwrap();
        let ids: Vec<&str> = lab.angles.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(
            ids,
            ["doorway", "close-corner", "off-angle", "box", "long-angle"]
        );
        let hall = lab.spawns.iter().find(|s| s.id == "hall").unwrap();
        let long = lab.angles.iter().find(|a| a.id == "long-angle").unwrap();
        let d = (long.position - hall.position).horizontal().length();
        assert!((30.0..=40.0).contains(&d), "long angle is {d} m");
    }
}
