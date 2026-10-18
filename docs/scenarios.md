# Writing scenarios and routines

Scenarios, routines and benchmarks are plain TOML files. To add one, drop the file into your user folder. No rebuild is needed.

| OS | Folder |
|---|---|
| Linux | `~/.config/aimtrainer/{scenarios,routines}/` |
| Windows | `%APPDATA%\CustomAimTrainer\aimtrainer\config\{scenarios,routines}\` |

**Overrides:** a user file with the same `id` as a built-in replaces it, which is the way to tweak a built-in.

**Examples:** the built-in files are in `content/scenarios/` and `content/routines/`; copy one and edit it.

**Typos fail loudly:** unknown keys are rejected. The menu lists files that failed to load, with the field that is wrong.

## Scenario

```toml
id = "my-drill"              # a-z 0-9 - _ ; used for score history
name = "My Drill"
category = "flicking"        # flicking | microcorrection | switching | tracking | movement | reaction | precision | custom
description = "What it trains."
duration_secs = 60           # default 60

[weapon]
kind = "vandal"              # knife | classic | vandal | phantom | guardian | operator
fire = "click"               # click (one shot per click, fire-rate capped) | hold (damage while held on target)
simulate_spread = false      # first-shot spread + running inaccuracy (for movement drills)
recoil = false               # click + vandal/phantom only: full-auto while held, bullets climb, sway and spread

[player]
movement = false             # WASD with VALORANT movement for the weapon
bounds_x = [-5.0, 5.0]       # play area for the player's feet (meters); start at origin facing -Z
bounds_z = [-1.0, 1.0]       # +Z is behind the player, so a larger max leaves room to step back
elevation = 0.0              # player's floor above the targets' floor (meters), within [-3, 6]

[targets]
shape = { kind = "sphere", radius = 0.15 }        # or { kind = "humanoid", head_only = true }
spawn = { kind = "around_crosshair", offset_deg = [5.0, 30.0], distance = [10.0, 20.0] }
motion = { kind = "static" }
max_alive = 1                # simultaneous targets
health = 1                   # click: hits to kill | hold: seconds on target (inf = never dies)
lifetime_secs = 3.0          # optional; the target leaves if not killed
appear_delay_secs = [0.0, 0.0]
min_separation_deg = 0

[scoring]                    # all optional
head = 100                   # per hit (click) or per second (hold)
body = 40
legs = 25
sphere = 100
miss_penalty = 0
moving_shot_penalty = 0      # per shot fired above 27.5% of run speed
expire_penalty = 0
burst_bonus = 0              # per hit within the weapon's protected bullets; needs weapon.recoil
rings = { radius = [0.25, 0.5, 1.0], multiplier = [3.0, 2.0, 1.0] }   # optional score-zone rings
```

### Score-zone rings

`rings` makes a hit worth more the closer it lands to the center of a sphere or a humanoid head. The distance is the shot's perpendicular distance from the zone center, as a fraction of the zone radius (the sphere radius, or half the head size). A hit takes the multiplier of the innermost ring it falls in, so with the example above a dead-center sphere hit scores `sphere × 3`. A hit outside every ring, and every body or leg hit, scores the zone's flat points. In hold mode the multiplier scales the points per second.

`radius` must be ascending fractions in (0, 1], with one `multiplier` (> 0) per radius. The built-in `precision-rings` uses them.

### Play area and floor height

With `movement = true`, the player's feet stay inside `bounds_x` and `bounds_z`; pushing into an edge stops only that direction, so you slide along it. The edges are drawn as low, see-through strips on the floor, a few centimeters high, so they never block the view. The built-in movement drills use `counter-strafe` x [−8, 8], z [−2, 5] and `strafe-shot` x [−7, 7], z [−3, 5].

`elevation` raises (or lowers, when negative) the player's floor; targets keep theirs at 0. The trainer draws a translucent slab at that height under the play area. You can also change it during a run: press Esc, set **Floor height** in 0.5 m steps, and choose **Apply and restart**. The run restarts at the same difficulty and duration, and a routine stays at the same run.

### Recoil

With `recoil = true`, holding the fire button fires full-auto at the weapon's fire rate, and every bullet of a burst lands further from the crosshair:

- **Climb:** each of the weapon's protected bullets (Vandal 6, Phantom 8) climbs a little higher.
- **Sway:** later bullets drift horizontally to a random side, which can switch on any bullet.
- **Spread:** spread grows from first-shot spread to the weapon's maximum over 10 bullets.
- **Reset:** pausing for the recovery time (Vandal 0.375 s, Phantom 0.35 s) resets everything, so the next shot is a fresh first bullet.

Recoil moves the bullets, not the camera. `simulate_spread` still adds running inaccuracy on top. The run then also records bursts, average burst length, the share of first bullets, and hits by bullet index (1, 2–3, 4–6, 7+). The per-bullet angles are estimates; see `valorant-mechanics.md`.

`recoil` requires `fire = "click"` and `kind = "vandal"` or `"phantom"`, and `burst_bonus` above 0 requires `recoil`.

### Spawn areas

| `kind` | Fields | Meaning |
|---|---|---|
| `angular` | `yaw_deg`, `pitch_deg`, `distance` | A direction relative to the starting view. Humanoids ignore pitch and stand on the floor. |
| `around_crosshair` | `offset_deg`, `distance`, `pitch_limit_deg` (default `[-15, 30]`), `on_screen` (default `false`) | Spawns relative to where you aim right now. Use it for flicks (large offsets) and microcorrections (1–6°). `pitch_limit_deg` keeps every spawn inside a vertical band around the horizon, so chained flicks cannot drift into the sky or the floor. `on_screen = true` keeps every spawn inside VALORANT's view from the crosshair at spawn time, with a 4° margin: at most 47.5° sideways and ≈31.3° up or down (half of 103° × 70.53°, minus the margin). `offset_deg.max` must then be ≤ 47.5, and difficulty scaling clamps the offset to that limit. Humanoids only use the horizontal offset. |
| `floor` | `x`, `z`, `height` | A world-space box. Use it for agents at realistic positions, or spheres at head height (`height = [1.8, 1.95]`). |

### Motions

| `kind` | Fields | Meaning |
|---|---|---|
| `static` | none | The target does not move. |
| `strafe` | `weapon`, `change_secs`, `stop_chance`, `max_offset` | A/D strafing with real VALORANT acceleration and stopping. |
| `dodge` | `weapon`, `change_secs`, `juke_secs` (default `[0.1, 0.25]`), `juke_chance`, `stop_chance`, `crouch_chance`, `jump_chance` (chances default `0`), `max_offset` | An evasive duelist: `strafe` physics plus jukes, stop-and-go, crouches and jumps. At each decision the target jumps, crouches in place, stops, or moves (reversing if it was moving); `stop_chance + crouch_chance + jump_chance` must be ≤ 1. After a decision, `juke_chance` picks the short `juke_secs` wait instead of `change_secs`, which produces quick A-D jukes. A crouch lowers the head 0.5 m; a jump rises 0.9 m and lands after 0.85 s with no air control (estimates, see `valorant-mechanics.md`). Spheres drop by the crouch height. |
| `drift` | `speed`, `change_secs`, `max_offset` | Smooth wandering, for tracking and evasive switching. |

The built-in `dodge-*` scenarios are dynamic clicking drills against evasive enemies:

| Scenario | Motion focus | Trains |
|---|---|---|
| `dodge-juke-heads` | jukes and stops | Holding the head line through A-D taps and clicking on the stop or reversal |
| `dodge-crouch-spam` | crouches | Following a head that drops 0.5 m and comes back up |
| `dodge-jump-spot` | jumps | Clicking an airborne head or pre-aiming the landing spot |
| `dodge-peek` | jiggle peek, appears after a delay | Reacting to a peek without firing while the agent crosses at full speed |
| `dodge-micro-spheres` | jukes on a head-sized sphere | Small corrections that do not overshoot into a reversal |

Clicking on the stop or reversal works because the target's lateral speed passes through zero there (inference from the movement model, not a measured training effect). The simulated aimer re-aims at the target every frame, so calibrate scores understate how much harder the motion makes these drills for a human.

### Difficulty

Training methods and adaptive mode play a scenario at a level from −1 (easiest) to +1 (hardest); 0 is the file as written. Each knob is the fraction a parameter changes at the ends of that range. A plain number applies both ways; `{ easy, hard }` sets each end separately. Fractions must be in `[0, 0.95)`, and `0` turns a knob off.

```toml
[difficulty]
size = { easy = 0.5, hard = 0.1 }   # spheres: 50% bigger at −1, 10% smaller at +1
speed = 0.3                         # drift speed and how soon targets change direction or juke (default)
lifetime = 0.25                     # target lifetime (default)
distance = 0.25                     # humanoid spawn distance (default)
offset = 0.2                        # crosshair-relative flick offset (default)
count = 0.0                         # targets alive at once; off by default
```

Results at a level other than 0 are stored as `id@+0.50`, so difficulties never mix in your history.

The full run key is `id@+0.50~20s%s80d120v150^+1.50m`: the difficulty level, a shortened duration, the in-run tuning, and a floor height that differs from the file's `elevation`. Each part is optional, and they always appear in that order. Retry on the results screen and the restart key replay the same key.

### In-run tuning

During a library scenario run, press Esc to tune its targets, from 50% to 200% in 10% steps, and choose **Apply and restart**. **Reset to scenario defaults** goes back to 100%.

| Knob | Changes |
|---|---|
| Target size | Sphere radius. Agents (humanoids) keep their VALORANT size; use Distance to make their heads smaller or larger on screen. |
| Distance | Spawn distance: `angular` and `around_crosshair` `distance`, and `floor` `z`. |
| Target speed | Motion speed: drift `speed`, and strafe run speed and acceleration together, so the strafe keeps its timing. |

A tuned run is stored under its own key (`%s80d120v150`: size, distance and speed percentages); the default 100/100/100 adds no suffix. Tuned runs never move the adaptive level, even when started adaptively, and never count for the benchmark, which reads only exact untuned keys. The results screen lists the tuning, and the HUD shows a "tuned" tag.

Target speed is stored in the definition as `targets.speed_scale` (default 1, allowed 0.25–4), which every motion reads. Scenario files normally leave it out.

### Sizing targets realistically

A VALORANT head is about 0.25 m (see `valorant-mechanics.md`). Its angular size depends on distance:

| Distance | Angular size |
|---|---|
| 10 m | 1.43° |
| 20 m | 0.72° |
| 40 m | 0.36° |

`aim_core::fov::angular_size_deg` computes this.

## Routine

```toml
id = "my-routine"
name = "My Routine"
description = "..."
interleave_from = 1          # blocks from index 1 on are shuffled run by run (omit to keep the order)

[[blocks]]
scenario = "track-precise"
minutes = 2                  # converted to runs using the scenario's duration
note = "Warm-up"

[[blocks]]
scenario = "micro-adjust"
runs = 3                     # an exact run count wins over minutes

[[blocks]]
arena = "yard-deathmatch"    # an arena instead of a scenario (see maps.md)
runs = 2                     # arena blocks need `runs`
duration_secs = 90           # optional: shorter arena runs, stored as `yard-deathmatch~90s`
```

A block names a `scenario` or an `arena`, never both. Routines are sequences of *activities*: the scenario session and the arena session both implement `aim_core::activity::Activity`. `content/routines/aim-and-duel.toml` mixes drills with deathmatch.

## Benchmark

Custom benchmarks go in the `benchmarks` folder next to `scenarios` and `routines`.

```toml
id = "my-benchmark"
name = "My Benchmark"
calibrated = false           # true only once thresholds come from real player data
ranks = ["Recruit", "Contender", "Veteran", "Elite"]
energy_per_rank = 100        # optional, default 100

# Optional tiers, easiest first. Exactly one tier has level 0 and uses `thresholds`.
[[tiers]]
id = "novice"
name = "Novice"
level = -0.5                 # every scenario of the tier is played at this difficulty level

[[tiers]]
id = "intermediate"
name = "Intermediate"
level = 0.0

[[subcategories]]
skill = "clicking"
name = "Static clicking"
scenarios = [
  # one ascending threshold per rank; `tiers.<id>` for every tier whose level is not 0
  { id = "micro-adjust", thresholds = [3300, 6000, 9400, 12600], tiers = { novice = [3400, 6200, 9950, 13300] } },
]
```

A tier's scores are stored under the run key (`micro-adjust@-0.50`), so they never mix with scores at other levels. Without `[[tiers]]` the benchmark has one tier: the scenarios as authored. The built-in benchmark is `content/benchmarks/valorant-benchmark.toml`.

## Checking your files

Run `cargo test -p aim_core` to validate every built-in file. To check the balance of a new scenario before you share it, copy it into `content/scenarios/`, add it to `SOURCES` in `crates/aim_core/src/scenario/builtin.rs`, and run the calibrate example, which reports how the simulated aimer profiles score on it:

```sh
cargo run -p aim_core --release --example calibrate
```
