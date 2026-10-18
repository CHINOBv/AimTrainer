//! Built-in arena library, embedded at compile time (see `docs/maps.md`).

use super::def::{ArenaDef, ArenaError};
use crate::map::MapDef;

pub const SOURCES: &[(&str, &str)] = &[
    (
        "angles-lab-clear.toml",
        include_str!("../../../../content/arenas/angles-lab-clear.toml"),
    ),
    (
        "angles-lab-prefire.toml",
        include_str!("../../../../content/arenas/angles-lab-prefire.toml"),
    ),
    (
        "corridor-duels.toml",
        include_str!("../../../../content/arenas/corridor-duels.toml"),
    ),
    (
        "corridor-jiggle.toml",
        include_str!("../../../../content/arenas/corridor-jiggle.toml"),
    ),
    (
        "lanes-deathmatch.toml",
        include_str!("../../../../content/arenas/lanes-deathmatch.toml"),
    ),
    (
        "outpost-site-hold.toml",
        include_str!("../../../../content/arenas/outpost-site-hold.toml"),
    ),
    (
        "yard-deathmatch.toml",
        include_str!("../../../../content/arenas/yard-deathmatch.toml"),
    ),
];

/// Parse every built-in arena. Panics only if a shipped file is broken, which the test
/// suite guards against.
pub fn all() -> Vec<ArenaDef> {
    SOURCES
        .iter()
        .map(|(file, src)| {
            ArenaDef::from_toml(src).unwrap_or_else(|e| panic!("built-in {file}: {e}"))
        })
        .collect()
}

/// The built-in arena with this id.
pub fn get(id: &str) -> Option<ArenaDef> {
    all().into_iter().find(|a| a.id == id)
}

/// The built-in arena with this id and its built-in map, validated together.
pub fn load(id: &str) -> Option<Result<(ArenaDef, MapDef), ArenaError>> {
    let def = get(id)?;
    let map = crate::builtin::get(&def.map)?;
    let errs = def.validate_with_map(&map);
    Some(if errs.is_empty() {
        Ok((def, map))
    } else {
        Err(ArenaError::Invalid(errs))
    })
}
