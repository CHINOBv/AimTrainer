//! Original training maps: box geometry, line of sight, collision and spawn markers
//! (engine-agnostic).
//!
//! A map is plain data ([`MapDef`], loaded from TOML) made of axis-aligned solid boxes on a
//! flat floor, plus spawn points, angle markers for tactical drills and an optional waypoint
//! graph for bots. All queries are pure and deterministic, in the core's coordinate system:
//! right-handed, +Y up, -Z forward, meters, and the floor is the plane `y = 0`.
//!
//! Maps must be original geometry. See `docs/maps.md` for the format and the integrity rule.

pub mod builtin;
pub mod map;
mod nav;
mod query;

pub use map::{
    AngleMarker, BoxKind, DEFAULT_PLAYER_RADIUS, MapDef, MapError, SolidBox, Spawn, SpawnKind,
    Waypoint,
};
