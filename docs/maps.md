# Maps

Maps live in the `aim_world` crate. A map is a TOML file of axis-aligned solid boxes on a flat floor, plus spawn points, angle markers for tactical drills and an optional waypoint graph for bots. The crate is engine-agnostic and deterministic: the app renders the boxes, and every rule (line of sight, collision, head visibility, bot routes) is decided in `aim_world`.

## Integrity rule: original geometry only

Every map must be **original**. Build the common tactical-shooter *situations* (doorways, close corners, off-angles, long angles, boxes, multi-angle clears) at real-world scale, but never replicate the layout of a VALORANT map or any other game's map, and never import extracted game geometry.

Why: Riot's fan policy ("Legal Jibber Jabber", https://www.riotgames.com/en/legal) is noncommercial only and says "We prohibit the use of our IP in games and apps". A map replica would use that IP. See "Game integrity and intellectual property" in `coding-standards.md`. When in doubt, ask in an issue before you build.

## Coordinates and scale

- Right-handed, +Y up, −Z forward, meters. The floor is the plane `y = 0`.
- Yaw follows the core: 0 looks down −Z, positive yaw turns right (90 looks down +X, 180 looks down +Z).
- The player body is a vertical box as tall as `HumanoidDims::height` (2.0 m) and `2 × DEFAULT_PLAYER_RADIUS` wide. The 0.35 m radius is an **estimate**, not a published game value.
- The eye sits at `HumanoidDims::eye_height` (1.82 m) above the feet.
- Real-world scale guide (approximate): a doorway is 1.5–2 m wide, a crate is about 1 m, a corridor is 3–6 m wide and a long angle is 30–40 m.

## Format

```toml
id = "my-map"                 # a-z, 0-9, '-' or '_'; must equal the file name without .toml
name = "My Map"
description = "What the map is for."
floor_x = [-10.0, 10.0]       # walkable extent [min, max] in meters; the edges act as walls
floor_z = [-30.0, 5.0]

[[boxes]]
kind = "wall"                 # wall | cover | low_cover
min = [-10.0, 0.0, -6.4]      # [x, y, z] corner, every axis below max
max = [-0.9, 3.5, -6.0]

[[spawns]]
id = "start"
kind = "player"               # player | bot
position = [0.0, 0.0, 2.0]    # feet
yaw_deg = 0.0                 # optional, default 0

[[angles]]
id = "doorway"
label = "Doorway"
position = [0.6, 0.0, -9.0]   # enemy feet once it has peeked
peek_dir = [-1.0, 0.0]        # horizontal [x, z] direction of the peek (any length)
peek_distance = 1.5           # the enemy waits at position - peek_dir * peek_distance

[[waypoints]]                 # optional bot navigation graph
id = "room"
position = [0.0, 0.0, 2.0]
links = ["door"]              # undirected edges to other waypoint ids
```

Unknown fields are rejected.

### Box kinds

| Kind | Meaning |
|---|---|
| `wall` | Full-height wall: blocks sight and movement. |
| `cover` | Head-high cover, such as stacked crates or a pillar, that hides a standing player. |
| `low_cover` | Waist-high cover, such as a single crate, that leaves a standing player's head visible. Its top must be below eye height. |

Geometry alone decides blocking; the kind tells drills and renderers how the box is meant to be used.

## Validation

`MapDef::validate` returns every problem, not just the first one:

- `id` uses only `a-z`, `0-9`, `-` and `_`; `name` is not empty; the floor ranges have `min < max`.
- Every box has `min < max` on all three axes; `low_cover` stays below eye height.
- Spawn, angle and waypoint ids are valid and unique within their list.
- There is at least one player spawn. No spawn stands outside the floor or inside a box.
- Every angle marker has a non-zero `peek_dir` and a positive `peek_distance`, and both its peek position and its hidden spot are standable. At least one player spawn must see the enemy's head at the peek position **and not** at the hidden spot; otherwise it is not an angle.
- Waypoints are standable, their links point to known ids and are walkable in a straight line for the player body, the graph is connected, and every spawn can walk straight to some waypoint.

`MapDef::load` parses and validates in one step and returns `MapError::Parse` or `MapError::Invalid`.

## Queries

| Method | Use |
|---|---|
| `raycast(ray, max_t) -> Option<(f32, usize)>` | Nearest box hit and its index (reuses `aim_core::scenario::hit::ray_aabb`). |
| `line_of_sight(a, b) -> bool` | Segment against all boxes. |
| `visible_head(eye, target_feet) -> bool` | Whether any part of a standing enemy's head is in sight. |
| `move_with_collision(feet, delta, radius) -> Vec3` | Horizontal move that slides along faces (X then Z) and cannot tunnel through a box. |
| `walkable(a, b, radius) -> bool` | Straight walk for the player body. |
| `nearest_waypoint(p)`, `path(from, to)` | Bot routes: Dijkstra over the waypoint graph, or the direct segment when it is walkable. |

## Built-in maps

| Id | Layout | Use |
|---|---|---|
| `training-yard` | Open 40 × 40 m yard: a 4 m center wall, two side walls, four 2.5 m pillars and four 1 m crates. Two player spawns, six bot spawns and a two-ring waypoint graph. | Deathmatch against bots. |
| `angles-lab` | A start room, a 1.8 m doorway, a 6 m wide corridor with a wall stub and a long hall with a tall box, a crate and a far block. Player spawns at the start, the door and the hall. | Angle drills. Markers: `doorway`, `close-corner`, `off-angle`, `box` and `long-angle` (about 37 m from the hall spawn). |
| `duel-corridor` | A 4 m wide, 32 m long corridor with a side pocket at each end. | Peek duels: `north-corner` and `south-corner`, at about 20–23 m. |

## Adding a map

1. Sketch the situation you want to train, at real scale, and check it is original (see the integrity rule above).
2. Write `content/maps/<id>.toml`. The file name must equal the `id`.
3. Register it in `SOURCES` in `crates/aim_world/src/builtin.rs` (keep the list sorted).
4. Run `cargo test -p aim_world`. The built-in tests validate every map and check that a walkable path joins every player spawn to every bot spawn.
