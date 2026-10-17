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
| `duel-corridor` | A 4 m wide, 32 m long corridor with a side pocket at each end. | Peek duels: `north-corner` and `south-corner`, at about 20–23 m. Jiggle peeks: `north-jiggle` and `south-jiggle`, where the enemy only shoulders 1 m out of the pocket. |
| `signal-outpost` | A 30 × 40 m outpost: a site with two crates and a pillar, a 10 m divider that splits the attackers' approach into a west and an east lane, and a defender area behind the site. Three defender (player) spawns, three attacker (bot) spawns and a waypoint `site`. | Site holds. |

## glTF maps

With the `gltf` feature (on by default; it adds the `gltf` 1.4 crate with default features off, only JSON and GLB parsing), `MapDef::from_gltf(bytes, id, name)` builds a map from a glTF 2.0 file, `.gltf` or `.glb`, and validates it like a TOML map. It returns `GltfMapError::Parse`, `Convention` (every naming problem found) or `Invalid` (the usual validation errors). Only the scene graph is read: node names, node transforms (parents included) and the `min`/`max` bounds of each mesh's `POSITION` accessor, which glTF requires. Vertex data, images and external buffers are never loaded. glTF axes match ours: right-handed, +Y up, meters.

Naming convention for the nodes of the default scene (or the first scene):

| Node | Name | Becomes |
|---|---|---|
| Mesh | `floor` (also `floor.001`, …) | The walkable floor extent on X and Z (union of all floor meshes). Required. |
| Mesh | `wall`, `cover` or `low_cover`, optionally followed by `.` or `:` and anything (`wall.001`, `cover:pillar`) | A solid box of that kind: the world-space axis-aligned bounding box of the mesh. Rotated meshes are enclosed by their bounding box. |
| Mesh | anything else | An error. |
| Empty | `player:<id>`, `bot:<id>` | A spawn. It faces the node's local −Z axis. |
| Empty | `angle:<id>` and `angle_hidden:<id>` | An angle marker: the peek position and the hidden spot. `peek_dir` and `peek_distance` come from the vector between them; the label is the id. Each needs its pair. |
| Empty | without `:` (groups, cameras, lights) | Ignored. |
| Empty | `<unknown>:<id>` | An error. |

Marker heights are ignored: feet stand on the floor (`y = 0`). Nodes nested deeper than `MAX_NODE_DEPTH` (64) levels are a `Convention` error, so a malformed file cannot overflow the stack. glTF maps have no waypoint graph yet, so roaming bots walk straight lines between spawns. Loading a glTF map in the app is not wired yet; the test asset `crates/aim_world/tests/data/box-room.gltf` is an example. The integrity rule applies to glTF maps too: never export or rebuild a game's map.

## Adding a map

1. Sketch the situation you want to train, at real scale, and check it is original (see the integrity rule above).
2. Write `content/maps/<id>.toml`. The file name must equal the `id`.
3. Register it in `SOURCES` in `crates/aim_world/src/builtin.rs` (keep the list sorted).
4. Run `cargo test -p aim_world`. The built-in tests validate every map and check that a walkable path joins every player spawn to every bot spawn.

## Arenas

An arena is an activity played on a map against bots. It lives in `aim_world::arena`, is pure and deterministic (seeded with `aim_core::rng::Rng`), and is driven by the same `aim_core::scenario::FrameInput` the scenario session uses: view angles, `fire_pressed`, WASD movement and walk.

### Practice mode

Every arena has a **Practice** button next to **Play**. In practice the bots move, peek, strafe and push exactly as in the real run but never shoot (`ArenaSession::practice`, which turns off `bots_shoot_back`), so you can walk the map, learn its angles and pre-aim spots, and train first-shot placement without dying. Practice results are stored under `<arena id>+practice`, so they never mix with normal scores, and Retry stays in practice. The HUD shows "PRACTICE · bots don't shoot".

### Format

Arenas are TOML files in `content/arenas/<id>.toml`, registered in `SOURCES` in `crates/aim_world/src/arena/builtin.rs`. Unknown fields are rejected, also inside `[mode]`.

```toml
id = "yard-deathmatch"        # a-z, 0-9, '-' or '_'; must equal the file name
name = "Yard Deathmatch"
description = "What the arena trains."
map = "training-yard"         # id of the map it is played on
duration_secs = 120.0
weapon = "vandal"             # the player's weapon: fire rate, spread, damage, movement

[mode]
kind = "deathmatch"
bots = 4                      # 1..=16
bot_skill = "contender"       # recruit | contender | veteran | elite (sim::AimerProfile)
respawn_secs = 3.0
bots_shoot_back = true
```

```toml
[mode]
kind = "angle_clear"
angles = ["doorway", "close-corner"]   # angle marker ids of the map, cleared in order
hold_secs = 1.5                        # how long a bot holds the angle before hiding
bot_reaction_secs = [0.35, 0.6]        # bot reaction time, sampled per sighting
bots_shoot_back = true
peek_delay_secs = [0.3, 0.9]           # optional, default [0.4, 1.2] (PEEK_DELAY_SECS)
pre_aim_tolerance_deg = 5.0            # optional: count and score pre-aimed sightings
```

Deathmatch options, both optional and off by default so existing arenas behave as before:

```toml
[mode]
kind = "deathmatch"
# ...the fields above, plus:
bot_strafe = true             # bots strafe (ADAD) while they fight and fire only when slow

[mode.objective]              # makes the deathmatch a site hold
site = "site"                 # waypoint id of the map: the site's center
site_radius = 2.5             # meters
capture_secs = 3.0            # an attacker on the site this long, uninterrupted, captures it
attackers = 6                 # attackers sent in the run (1..=64), at most `bots` alive at once
```

`ArenaDef::validate` checks the definition alone; `ArenaDef::validate_with_map` also checks that the map id matches, that a deathmatch map has bot spawns and that every angle id exists. `ArenaSession::new(def, map, seed)` runs both and returns `ArenaError::Invalid` on failure. `arena::builtin::load(id)` returns a built-in arena together with its built-in map.

### Modes

- **`deathmatch`:** `bots` bots spawn at bot spawns the player cannot see. They roam between the map's waypoints and bot spawns along `MapDef::path`, pause at the end of each route, and stop to fight when they notice the player. A killed bot respawns after `respawn_secs`. When the player dies, they respawn at the spawn (player or bot) seen by the fewest living bots, and bots ignore them for `RESPAWN_GRACE_SECS` (1.5 s, an arena rule against spawn kills, not a VALORANT value).
- **`angle_clear`:** one bot per listed angle, in order. Each angle starts with the player at the player spawn that sees the marker's peek position but not its hidden spot. The bot waits at the hidden spot; once the player can see the angle it peeks after 0.4–1.2 s (`PEEK_DELAY_SECS`), holds for `hold_secs`, hides again and repeats until it dies. Killing it starts the next angle; clearing every angle ends the run early. An angle id may repeat in the list. `peek_delay_secs` replaces the peek delay; with a short `hold_secs` the bot jiggle-peeks: it shows for a fraction of a second, hides and peeks again.
- **Site hold** (a `deathmatch` with `[mode.objective]`, the hybrid activity of roadmap Phase 6): `attackers` bots are sent in, at most `bots` alive at once; a killed attacker is replaced after `respawn_secs` until all have been sent. Attackers walk toward the site along `MapDef::path`. When they see the player they stop to fight for at most `PUSH_DUEL_SECS` (2.5 s), then walk on for `PUSH_ADVANCE_SECS` (1.5 s) before they stop to fight again, so they keep pressure on the site. A living attacker within `site_radius` of the site advances the capture; if no attacker is on the site the capture resets to 0. When it reaches `capture_secs` the site is lost and the run ends. Killing every attacker ends the run early with the site held; running out the clock also holds it. The defender respawns at the player spawn seen by the fewest attackers. `ArenaSession::objective()` reports attackers left, capture progress and whether the site was lost. All these durations and the bonus below are design choices, not VALORANT values.

### Player

- Movement uses `movement::step` with the weapon's movement profile and `MapDef::move_with_collision`; velocity pushing into a wall is lost.
- Shooting is one shot per click, capped by the weapon's fire rate. Every shot has the weapon's first-shot spread, plus run spread when the speed is above the accuracy limit (27.5% of run speed for rifles), the same rule as the scenario session. Such shots count as moving shots.
- A shot hits the nearest living bot whose hitbox (`ray_humanoid`) it crosses, unless a map box is hit first. Damage comes from `Weapon::stats()` per zone.
- The player has 100 health and respawns at once when it reaches 0; each death is counted. A bot's Vandal headshot (160) kills in one shot, as in VALORANT.
- Every spawn is protected: entering the arena and every respawn start `RESPAWN_GRACE_SECS` (1.5 s, an arena rule, not a VALORANT value) during which bots ignore the player, so the player can move off the spawn before the first fight. `ArenaSession::spawn_protection_secs()` reports the time left; the HUD shows it, and after a death it says "You died · respawned", so a respawn is never mistaken for a teleport. Outside a death the player is only moved by the session when an `angle_clear` starts its next angle.

### Bot model

| Part | Rule |
|---|---|
| Health | 150: 100 health plus 50 armor as one pool (simplification: armor damage reduction is not modeled). |
| Weapon | Vandal (`BOT_WEAPON`) for damage, fire rate and spread. |
| Movement | `movement::step` with the rifle profile; bots walk toward their next route point and brake with the stop curve when they stop to fight. Angle bots stop exactly on the peek position (a perfect counter-strafe). |
| Strafe | Only with `bot_strafe = true`. While a roaming or attacking bot fights, it holds one strafe key for 0.2–0.45 s (`STRAFE_SECS`, a design choice), then the other (ADAD). Pressing the opposite key makes `movement::step` counter-strafe, so the bot passes through the accurate speed window at each switch. It fires only when its speed is at or below the accuracy limit (27.5% of run speed, the movement module's VALORANT value), so its shots carry first-shot spread only. |
| Behavior | The angle peek cycle (hidden, peek, hold, hide), the strafe (ADAD) and the attacker push (duel, press on) are small state machines in `arena/behavior.rs`, unit-tested on their own. They only draw random numbers on the transitions that need them, so an arena that does not use a machine keeps its random sequence. |
| Sight | Line of sight from the bot's eye to any part of the player's head (`visible_head`). Roaming bots only notice the player within 60° of where they face (`BOT_NOTICE_HALF_ANGLE_DEG`, an estimate from VALORANT's 103° horizontal field of view); once engaged they track at any angle. |
| Aim | The `sim::AimerProfile` model: a reaction delay, then a minimum-jerk flick to the player's head whose duration follows Fitts' law (`fitts_a + fitts_b · log2(d / 2r + 1)`) and whose endpoint misses by a Gaussian error of `endpoint_noise · r`, where `r` is the head's angular radius. After each shot the bot plans a corrective flick after half its reaction time. Deathmatch bots use the `bot_skill` preset; angle bots use the contender flick constants with `bot_reaction_secs` as their reaction time. |
| Fire | When the flick has landed, at the Vandal fire rate, only if `bots_shoot_back`. Bot shots are occluded by the map like the player's. |

### Crosshair placement metric

At the first frame each bot life becomes visible to the player, the arena records the angle in degrees between the player's view direction and the bot's head (`ArenaStats::crosshair_errors_deg`); `ArenaStats::median_crosshair_error_deg` summarizes it. A player who pre-aims where the head appears scores close to 0°; one who looks elsewhere scores tens of degrees. It trains pre-aim and head-level placement (roadmap Phase 7). The vertical part of the same error, how far the crosshair was above or below head level in degrees of pitch, is recorded in `ArenaStats::head_level_errors_deg` (`median_head_level_error_deg`). In an angle clear with `pre_aim_tolerance_deg`, each bot life first seen within that many degrees counts in `ArenaStats::pre_aimed`. A perfectly pre-aimed angle still measures a few degrees, because the head is first seen while only partly out of cover (about 3° on the `angles-lab` doorway), so 5° is a sensible tolerance.

### Result

`ArenaSession::result()` returns an `aim_core::scenario::RunResult` so arenas share history and the results UI with scenarios: `scenario_id` is the arena id, `score = kills × 100 − deaths × 50 + headshot hits × 25 + pre-aimed sightings × 50 (PRE_AIM_POINTS) + 500 if the site was held (SITE_HELD_BONUS)` (the last two only in arenas that use them), `accuracy = hits / shots`, `expired` is the number of player deaths, and kills per minute, average time to kill (from first sight) and headshot rate (headshot hits / hits) are as in scenarios.

### Simplifications

- Bots have 150 HP in one pool; the player has 100 health and no armor.
- The world is "2D-ish": a flat floor of boxes, no jumping, crouching, stairs or height differences.
- No abilities, utility, recoil patterns beyond first-shot and run spread, reloads or economy.
- Bots do not collide with each other or with the player.

### Built-in arenas

| Id | Map | Mode |
|---|---|---|
| `yard-deathmatch` | `training-yard` | Deathmatch, 4 contender bots that shoot back, 120 s. |
| `angles-lab-clear` | `angles-lab` | Angle clear of all five markers in order, reaction 0.35–0.6 s, bots shoot back, 90 s. |
| `corridor-duels` | `duel-corridor` | Deathmatch, 2 veteran bots that shoot back, 90 s. |
| `angles-lab-prefire` | `angles-lab` | Pre-aim drill: the five angles against fast bots (reaction 0.2–0.3 s) that shoot back, pre-aim tolerance 5°, 90 s. |
| `corridor-jiggle` | `duel-corridor` | Jiggle-peek drill: eight peeks alternating `north-jiggle` and `south-jiggle`, hold 0.15 s, peek delay 0.3–0.9 s, bots do not shoot back, 60 s. |
| `outpost-site-hold` | `signal-outpost` | Site hold: 6 contender attackers, 2 at a time, strafing and shooting back; capture 3 s within 2.5 m; 90 s. |
