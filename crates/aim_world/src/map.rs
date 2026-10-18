//! Map definition: the TOML format, loading and semantic validation.

use aim_core::games::valorant::HumanoidDims;
use aim_core::math::{Range, Vec3};
use aim_core::scenario::ValidationError;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Horizontal half-width of the player's collision box, in meters. An estimate (no
/// published VALORANT value); the body is 0.5 m wide in [`HumanoidDims`], plus a margin so
/// the player cannot stand flush against a wall.
pub const DEFAULT_PLAYER_RADIUS: f32 = 0.35;

/// What a solid box represents. Geometry alone decides blocking; the kind tells drills and
/// renderers how the box is meant to be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoxKind {
    /// Full-height wall that blocks sight and movement.
    Wall,
    /// Head-high cover (stacked crates, pillars) that hides a standing player.
    Cover,
    /// Waist-high cover (a single crate) that leaves a standing player's head visible.
    LowCover,
}

/// An axis-aligned solid box, `min` and `max` corners in meters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SolidBox {
    pub kind: BoxKind,
    #[serde(with = "vec3_array")]
    pub min: Vec3,
    #[serde(with = "vec3_array")]
    pub max: Vec3,
}

impl SolidBox {
    pub fn is_well_formed(&self) -> bool {
        let finite = |v: Vec3| v.x.is_finite() && v.y.is_finite() && v.z.is_finite();
        finite(self.min)
            && finite(self.max)
            && self.min.x < self.max.x
            && self.min.y < self.max.y
            && self.min.z < self.max.z
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpawnKind {
    Player,
    Bot,
}

/// A spawn point: feet position on the floor and the yaw it faces (core convention: 0 looks
/// down -Z, positive turns right).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spawn {
    pub id: String,
    pub kind: SpawnKind,
    #[serde(with = "vec3_array")]
    pub position: Vec3,
    #[serde(default)]
    pub yaw_deg: f32,
}

/// A place where an enemy appears in an angle drill. The enemy waits at the hidden spot,
/// `position - peek_dir * peek_distance`, and steps out along `peek_dir` to `position`,
/// where its head is visible from at least one player spawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AngleMarker {
    pub id: String,
    pub label: String,
    /// Feet position of the enemy once it has peeked.
    #[serde(with = "vec3_array")]
    pub position: Vec3,
    /// Horizontal direction `[x, z]` the enemy moves when it peeks out (any length).
    pub peek_dir: [f32; 2],
    /// Distance in meters between the hidden spot and the peeked position.
    pub peek_distance: f32,
}

impl AngleMarker {
    /// Unit horizontal peek direction (zero if `peek_dir` is zero).
    pub fn peek_unit(&self) -> Vec3 {
        Vec3::new(self.peek_dir[0], 0.0, self.peek_dir[1]).normalize_or_zero()
    }

    /// Feet position where the enemy waits, out of sight, before it peeks.
    pub fn hidden_position(&self) -> Vec3 {
        self.position - self.peek_unit() * self.peek_distance
    }
}

/// A bot navigation node. `links` are undirected edges to other waypoint ids.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Waypoint {
    pub id: String,
    #[serde(with = "vec3_array")]
    pub position: Vec3,
    #[serde(default)]
    pub links: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Walkable floor extent on X, `[min, max]` meters. Its edges behave as walls.
    pub floor_x: Range,
    /// Walkable floor extent on Z, `[min, max]` meters.
    pub floor_z: Range,
    #[serde(default)]
    pub boxes: Vec<SolidBox>,
    #[serde(default)]
    pub spawns: Vec<Spawn>,
    #[serde(default)]
    pub angles: Vec<AngleMarker>,
    #[serde(default)]
    pub waypoints: Vec<Waypoint>,
}

#[derive(Debug, thiserror::Error)]
pub enum MapError {
    #[error("invalid map TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("invalid map: {}", join(.0))]
    Invalid(Vec<ValidationError>),
}

fn join(errs: &[ValidationError]) -> String {
    errs.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

impl MapDef {
    pub fn from_toml(src: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(src)
    }

    /// Parse and validate in one step.
    pub fn load(src: &str) -> Result<Self, MapError> {
        let def = Self::from_toml(src)?;
        let errs = def.validate();
        if errs.is_empty() {
            Ok(def)
        } else {
            Err(MapError::Invalid(errs))
        }
    }

    pub fn spawns_of(&self, kind: SpawnKind) -> impl Iterator<Item = &Spawn> {
        self.spawns.iter().filter(move |s| s.kind == kind)
    }

    /// Eye position of a standing player whose feet are at `feet`.
    pub fn eye(feet: Vec3) -> Vec3 {
        feet + Vec3::new(0.0, HumanoidDims::default().eye_height, 0.0)
    }

    /// Semantic checks that serde cannot express. Returns every problem found.
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut errs = Vec::new();
        let mut push = |field: &'static str, message: String| {
            errs.push(ValidationError { field, message });
        };

        if !valid_id(&self.id) {
            push(
                "id",
                "must be non-empty and only contain a-z, 0-9, '-' or '_'".into(),
            );
        }
        if self.name.trim().is_empty() {
            push("name", "must not be empty".into());
        }
        let floor_ok = [self.floor_x, self.floor_z]
            .iter()
            .all(|r| r.min.is_finite() && r.max.is_finite() && r.min < r.max);
        if !floor_ok {
            push(
                "floor",
                "floor_x and floor_z must be [min, max] with min < max".into(),
            );
        }
        for (i, b) in self.boxes.iter().enumerate() {
            if !b.is_well_formed() {
                push(
                    "boxes",
                    format!("box {i}: min must be below max on every axis"),
                );
            }
            if b.kind == BoxKind::LowCover && b.max.y >= HumanoidDims::default().eye_height {
                push(
                    "boxes",
                    format!("box {i}: low_cover must be lower than eye height"),
                );
            }
        }

        let ids = |items: &mut dyn Iterator<Item = &str>| -> Vec<String> {
            let mut seen = HashSet::new();
            let mut bad = Vec::new();
            for id in items {
                if !valid_id(id) {
                    bad.push(format!("'{id}' is not a valid id"));
                } else if !seen.insert(id.to_owned()) {
                    bad.push(format!("duplicate id '{id}'"));
                }
            }
            bad
        };
        for m in ids(&mut self.spawns.iter().map(|s| s.id.as_str())) {
            push("spawns", m);
        }
        for m in ids(&mut self.angles.iter().map(|a| a.id.as_str())) {
            push("angles", m);
        }
        for m in ids(&mut self.waypoints.iter().map(|w| w.id.as_str())) {
            push("waypoints", m);
        }

        if self.spawns_of(SpawnKind::Player).next().is_none() {
            push("spawns", "needs at least one player spawn".into());
        }
        for s in &self.spawns {
            if let Some(problem) = self.standing_problem(s.position) {
                push("spawns", format!("spawn '{}' {problem}", s.id));
            }
        }

        for a in &self.angles {
            if !(a.peek_distance.is_finite() && a.peek_distance > 0.0)
                || a.peek_unit() == Vec3::ZERO
            {
                push(
                    "angles",
                    format!(
                        "angle '{}': peek_dir must be non-zero and peek_distance > 0",
                        a.id
                    ),
                );
                continue;
            }
            for (spot, at) in [
                ("peek position", a.position),
                ("hidden spot", a.hidden_position()),
            ] {
                if let Some(problem) = self.standing_problem(at) {
                    push("angles", format!("angle '{}': {spot} {problem}", a.id));
                }
            }
            let is_real_angle = self.spawns_of(SpawnKind::Player).any(|s| {
                let eye = Self::eye(s.position);
                self.visible_head(eye, a.position) && !self.visible_head(eye, a.hidden_position())
            });
            if !is_real_angle {
                push(
                    "angles",
                    format!(
                        "angle '{}': no player spawn sees the head at the peek position \
                         while the hidden spot stays out of sight",
                        a.id
                    ),
                );
            }
        }

        let known: HashSet<&str> = self.waypoints.iter().map(|w| w.id.as_str()).collect();
        for w in &self.waypoints {
            if let Some(problem) = self.standing_problem(w.position) {
                push("waypoints", format!("waypoint '{}' {problem}", w.id));
            }
            for l in &w.links {
                if !known.contains(l.as_str()) {
                    push(
                        "waypoints",
                        format!("waypoint '{}' links to unknown '{l}'", w.id),
                    );
                } else if let Some(other) = self.waypoints.iter().find(|o| &o.id == l)
                    && !self.walkable(w.position, other.position, DEFAULT_PLAYER_RADIUS)
                {
                    push(
                        "waypoints",
                        format!("link '{}' -> '{l}' is blocked by a box", w.id),
                    );
                }
            }
        }
        if !self.waypoints.is_empty() {
            if !self.nav_connected() {
                push("waypoints", "the waypoint graph must be connected".into());
            }
            for s in &self.spawns {
                if self.nearest_waypoint(s.position).is_none() {
                    push(
                        "waypoints",
                        format!("spawn '{}' has no waypoint in walking line", s.id),
                    );
                }
            }
        }
        errs
    }

    /// Whether a player can stand with feet at `feet`: on the floor and in no box.
    pub(crate) fn can_stand(&self, feet: Vec3) -> bool {
        self.standing_problem(feet).is_none()
    }

    /// Why a player could not stand with feet at `feet`, if anything.
    fn standing_problem(&self, feet: Vec3) -> Option<&'static str> {
        let r = DEFAULT_PLAYER_RADIUS;
        if feet.x - r < self.floor_x.min
            || feet.x + r > self.floor_x.max
            || feet.z - r < self.floor_z.min
            || feet.z + r > self.floor_z.max
        {
            Some("is outside the floor")
        } else if self.body_overlaps(feet, r).is_some() {
            Some("is inside a solid box")
        } else {
            None
        }
    }
}

/// Serde helper so positions are written `[x, y, z]` in TOML.
mod vec3_array {
    use aim_core::math::Vec3;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(v: &Vec3, s: S) -> Result<S::Ok, S::Error> {
        [v.x, v.y, v.z].serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec3, D::Error> {
        let [x, y, z] = <[f32; 3]>::deserialize(d)?;
        Ok(Vec3::new(x, y, z))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BROKEN: &str = r#"
id = "Bad Id"
name = " "
floor_x = [-5.0, 5.0]
floor_z = [-5.0, 5.0]

[[boxes]]
kind = "wall"
min = [1.0, 0.0, 1.0]
max = [0.0, 3.0, 2.0]

[[boxes]]
kind = "cover"
min = [-1.0, 0.0, -1.0]
max = [1.0, 2.5, 1.0]

[[spawns]]
id = "p"
kind = "player"
position = [0.0, 0.0, 0.0]

[[spawns]]
id = "p"
kind = "bot"
position = [4.0, 0.0, 4.0]
"#;

    #[test]
    fn validation_reports_every_problem() {
        let def = MapDef::from_toml(BROKEN).unwrap();
        let errs = def.validate();
        let has = |field: &str, text: &str| {
            errs.iter()
                .any(|e| e.field == field && e.message.contains(text))
        };
        assert!(has("id", "a-z"), "{errs:?}");
        assert!(has("name", "empty"), "{errs:?}");
        assert!(has("boxes", "box 0"), "{errs:?}");
        assert!(has("spawns", "duplicate id 'p'"), "{errs:?}");
        assert!(has("spawns", "inside a solid box"), "{errs:?}");
        assert!(errs.len() >= 5);
        assert!(matches!(MapDef::load(BROKEN), Err(MapError::Invalid(e)) if e.len() == errs.len()));
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let src =
            "id = \"a\"\nname = \"A\"\nfloor_x = [0.0, 1.0]\nfloor_z = [0.0, 1.0]\nsize = 3\n";
        assert!(matches!(MapDef::load(src), Err(MapError::Parse(_))));
    }

    #[test]
    fn angle_without_cover_is_rejected() {
        let src = r#"
id = "open"
name = "Open"
floor_x = [-10.0, 10.0]
floor_z = [-10.0, 10.0]

[[spawns]]
id = "p"
kind = "player"
position = [0.0, 0.0, 5.0]

[[angles]]
id = "nothing-hides"
label = "No cover"
position = [0.0, 0.0, -5.0]
peek_dir = [1.0, 0.0]
peek_distance = 2.0
"#;
        let errs = MapDef::from_toml(src).unwrap().validate();
        assert!(
            errs.iter()
                .any(|e| e.field == "angles" && e.message.contains("out of sight")),
            "{errs:?}"
        );
    }
}
