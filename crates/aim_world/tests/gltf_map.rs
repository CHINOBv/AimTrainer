//! Loading an original map from glTF through the public API. The asset is a 20 x 20 m room
//! built from one unit cube: a wall, a crate, a player and a bot spawn, and one angle whose
//! markers sit under a group node.
#![cfg(feature = "gltf")]

use aim_world::arena::{ArenaDef, ArenaSession};
use aim_world::gltf::GltfMapError;
use aim_world::{BoxKind, MapDef, SpawnKind};

const ROOM: &str = include_str!("data/box-room.gltf");

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

#[test]
fn a_gltf_room_becomes_a_valid_map() {
    let map = MapDef::from_gltf(ROOM.as_bytes(), "box-room", "Box Room").unwrap();
    assert_eq!(
        (map.id.as_str(), map.name.as_str()),
        ("box-room", "Box Room")
    );
    assert_eq!((map.floor_x.min, map.floor_x.max), (-10.0, 10.0));
    assert_eq!((map.floor_z.min, map.floor_z.max), (-10.0, 10.0));

    assert_eq!(
        map.boxes.len(),
        2,
        "the floor is not a box: {:?}",
        map.boxes
    );
    let wall = map.boxes[0];
    assert_eq!(wall.kind, BoxKind::Wall);
    assert!(
        close(wall.min.x, 0.5) && close(wall.max.x, 10.0),
        "{wall:?}"
    );
    assert!(close(wall.min.y, 0.0) && close(wall.max.y, 3.0), "{wall:?}");
    assert!(
        close(wall.min.z, -5.0) && close(wall.max.z, -4.0),
        "{wall:?}"
    );
    assert_eq!(map.boxes[1].kind, BoxKind::LowCover);

    let player = map.spawns_of(SpawnKind::Player).next().unwrap();
    assert_eq!(player.id, "start");
    assert!(close(player.position.z, 6.0) && close(player.yaw_deg, 0.0));
    let bot = map.spawns_of(SpawnKind::Bot).next().unwrap();
    assert_eq!(bot.id, "far");
    assert!(
        close(bot.yaw_deg.abs(), 180.0),
        "rotated to face +Z: {}",
        bot.yaw_deg
    );

    assert_eq!(map.angles.len(), 1);
    let a = &map.angles[0];
    assert_eq!(a.id, "wall-edge");
    assert_eq!(a.position.y, 0.0, "markers stand on the floor");
    assert!(
        close(a.position.x, -0.5) && close(a.position.z, -7.0),
        "{a:?}"
    );
    assert!(
        close(a.peek_distance, 3.5) && close(a.peek_unit().x, -1.0),
        "{a:?}"
    );
    assert!(map.validate().is_empty());
}

#[test]
fn a_gltf_map_plays_an_angle_clear() {
    let map = MapDef::from_gltf(ROOM.as_bytes(), "box-room", "Box Room").unwrap();
    let def = ArenaDef::load(
        r#"
id = "box-room-clear"
name = "Box Room Clear"
map = "box-room"
duration_secs = 10.0
weapon = "vandal"

[mode]
kind = "angle_clear"
angles = ["wall-edge"]
hold_secs = 1.0
bot_reaction_secs = [0.3, 0.5]
bots_shoot_back = false
"#,
    )
    .unwrap();
    assert!(ArenaSession::new(def, map, 1).is_ok());
}

/// Wraps the JSON in a GLB container (header and one JSON chunk).
fn glb(json: &str) -> Vec<u8> {
    let mut chunk = json.as_bytes().to_vec();
    while !chunk.len().is_multiple_of(4) {
        chunk.push(b' ');
    }
    let total = 12 + 8 + chunk.len() as u32;
    let mut out = Vec::new();
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&chunk);
    out
}

#[test]
fn a_glb_file_loads_the_same_map() {
    let from_json = MapDef::from_gltf(ROOM.as_bytes(), "box-room", "Box Room").unwrap();
    let from_glb = MapDef::from_gltf(&glb(ROOM), "box-room", "Box Room").unwrap();
    assert_eq!(from_json, from_glb);
}

#[test]
fn names_outside_the_convention_are_reported() {
    let broken = ROOM
        .replace("\"low_cover:crate\"", "\"crate\"")
        .replace("\"angle_hidden:wall-edge\"", "\"angle_hiden:wall-edge\"");
    let Err(GltfMapError::Convention(problems)) =
        MapDef::from_gltf(broken.as_bytes(), "box-room", "Box Room")
    else {
        panic!("expected convention errors");
    };
    let all = problems.join("\n");
    assert!(all.contains("mesh node 'crate'"), "{all}");
    assert!(all.contains("unknown kind 'angle_hiden'"), "{all}");
    assert!(
        all.contains("angle 'wall-edge' has no 'angle_hidden:wall-edge'"),
        "{all}"
    );

    let no_floor = ROOM.replace("\"floor\"", "\"wall:floor-slab\"");
    assert!(matches!(
        MapDef::from_gltf(no_floor.as_bytes(), "box-room", "Box Room"),
        Err(GltfMapError::Convention(p)) if p.iter().any(|m| m.contains("'floor'"))
    ));
}

#[test]
fn geometry_that_breaks_map_rules_is_invalid() {
    // Moving the player spawn inside the wall.
    let inside = ROOM.replace(
        "[\n    0,\n    0,\n    6\n   ]",
        "[\n    5,\n    0,\n    -4.5\n   ]",
    );
    assert_ne!(inside, ROOM, "the fixture layout changed");
    assert!(matches!(
        MapDef::from_gltf(inside.as_bytes(), "box-room", "Box Room"),
        Err(GltfMapError::Invalid(_))
    ));
    assert!(matches!(
        MapDef::from_gltf(b"not gltf", "x", "X"),
        Err(GltfMapError::Parse(_))
    ));
}
