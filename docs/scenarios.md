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
bounds_z = [-1.0, 1.0]

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
```

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
| `around_crosshair` | `offset_deg`, `distance`, `pitch_limit_deg` (default `[-15, 30]`) | Spawns relative to where you aim right now. Use it for flicks (large offsets) and microcorrections (1–6°). `pitch_limit_deg` keeps every spawn inside a vertical band around the horizon, so chained flicks cannot drift into the sky or the floor. |
| `floor` | `x`, `z`, `height` | A world-space box. Use it for agents at realistic positions, or spheres at head height (`height = [1.8, 1.95]`). |

### Motions

| `kind` | Fields | Meaning |
|---|---|---|
| `static` | none | The target does not move. |
| `strafe` | `weapon`, `change_secs`, `stop_chance`, `max_offset` | A/D strafing with real VALORANT acceleration and stopping. |
| `drift` | `speed`, `change_secs`, `max_offset` | Smooth wandering, for tracking and evasive switching. |

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

## Checking your files

Run `cargo test -p aim_core` to validate every built-in file. To check the balance of a new scenario before you share it, copy it into `content/scenarios/`, add it to `SOURCES` in `crates/aim_core/src/scenario/builtin.rs`, and run the calibrate example, which reports how the simulated aimer profiles score on it:

```sh
cargo run -p aim_core --release --example calibrate
```
