# Writing scenarios and routines

Scenarios, routines and benchmarks are plain TOML files. To add one, drop the file into your user folder. No rebuild is needed.

| OS | Folder |
|---|---|
| Linux | `~/.config/aimtrainer/{scenarios,routines}/` |
| Windows | `%APPDATA%\CustomAimTrainer\aimtrainer\config\{scenarios,routines}\` |

**Overrides:** a user file with the same `id` as a built-in replaces it, which is the way to tweak a built-in.

**Examples:** the built-in files are in `content/scenarios/` and `content/routines/`; copy one and edit it. `library.md` lists the drill library and the idea behind each scenario.

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
shape = { kind = "sphere", radius = 0.15 }        # or { kind = "humanoid", head_only = true }, or a capsule (see below)
spawn = { kind = "around_crosshair", offset_deg = [5.0, 30.0], distance = [10.0, 20.0] }
motion = { kind = "static" }
max_alive = 1                # simultaneous targets
health = 1                   # click: hits to kill | hold: seconds on target (inf = never dies)
lifetime_secs = 3.0          # optional; the target leaves if not killed
appear_delay_secs = [0.0, 0.0]
min_separation_deg = 0
decoy_chance = 0             # go/no-go: share of spawns that are decoys, in [0, 1); needs lifetime_secs and fire = "click"
regen = { interval_secs = 0.5, amount = 1, reset_on_hit = false }   # optional health regeneration
spawn_rules = { avoid_last_deg = 10, min_flick_deg = 15 }          # optional spawn-pattern controls
perception = { kind = "sound_cue", sound = "footsteps", reveal_after_secs = 0.8 }   # optional perception drill
shrink_per_kill = { factor = 0.97, floor = 0.5 }                  # optional: every kill shrinks spheres

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
decoy_penalty = 0            # per shot on a go/no-go decoy
decay = { window_secs = 2.0, floor = 0.3 }   # optional: a target's value fades after it appears
accuracy_mult = "none"       # "linear" or "sqrt": the final score × accuracy (or its square root)

[view]                       # optional
fov_lock = [60, 80]          # vertical FOV in degrees: a fixed value (60) or a [min, max] clamp

[feedback]                   # optional; see "Per-scenario feedback"
hit_markers = true           # false hides hit markers in this scenario
hit_sounds = true            # false mutes hit and kill sounds in this scenario
damage_numbers = false       # true: floating damage numbers at each hit (click only)
health_bars = false          # true: a small bar over targets whose health is above 1
```

### Go/no-go decoys

With `decoy_chance` above 0, each spawned target is a "non-match" decoy with that chance. Decoys are drawn as a faint, see-through grey, so they stand out by brightness and transparency rather than by colour. The four outcomes are:

| Outcome | What happens |
|---|---|
| Match hit | A normal target is shot: normal points and kill |
| Match wait | A normal target expires unshot: counts as expired, `expire_penalty` applies |
| Non-match hit | A decoy is shot: it vanishes, the shot counts as a shot that hit nothing, `decoy_penalty` applies |
| Non-match wait | A decoy expires unshot: the correct choice, with no penalty and no expiry |

Decoys need `lifetime_secs`, so that they leave, and `fire = "click"`. Adaptive difficulty counts a decoy left alone as a success. The results screen shows the four counts, decision accuracy, the median reaction to match targets and two explained verdicts (see `training-science.md`). The built-in `go-no-go` uses decoys.

### Score-zone rings

`rings` makes a hit worth more the closer it lands to the center of a sphere or a humanoid head. The distance is the shot's perpendicular distance from the zone center, as a fraction of the zone radius (the sphere radius, or half the head size). A hit takes the multiplier of the innermost ring it falls in, so with the example above a dead-center sphere hit scores `sphere × 3`. A hit outside every ring, and every body or leg hit, scores the zone's flat points. In hold mode the multiplier scales the points per second.

`radius` must be ascending fractions in (0, 1], with one `multiplier` (> 0) per radius. The built-in `precision-rings` uses them.

### Health regeneration and score decay

`targets.regen` heals damaged targets; `scoring.decay` makes a target worth less the longer it lives. Both are optional and off by default.

```toml
[targets]
health = 1.5
regen = { interval_secs = 0.3, amount = 0.2, reset_on_hit = true }

[scoring]
decay = { window_secs = 3.0, floor = 0.4 }
```

| Field | Meaning |
|---|---|
| `regen.interval_secs` | Seconds between heals, > 0. The first heal comes one interval after the target appears. |
| `regen.amount` | Health restored per heal, > 0: hits in click mode, seconds in hold mode. A target never heals above its starting `health`, which must be finite. |
| `regen.reset_on_hit` | Default `false`. When `true`, every bit of damage restarts the interval, so the target only heals after going `interval_secs` without being hit. In hold mode that means after the crosshair has been off it (or fire released) for that long. |
| `decay.window_secs` | Seconds from the target appearing until its value reaches the floor, > 0. |
| `decay.floor` | Fraction of the value left after the window, in [0, 1]. |

The value falls linearly: a hit `age` seconds after the target appeared scores its points × `1 − (1 − floor) × min(age / window_secs, 1)`. It multiplies the score-ring multiplier, and in hold mode it scales the points per second. The built-in `regen-track` combines both: breaking off lets the sphere heal, and a slow kill earns less, so steady tracking pays twice.

### Capsule targets

`shape = { kind = "capsule", radius, height, head }` is an upright capsule: a cylinder with rounded ends, centered on the target's position, with an optional round head above it. Body hits score `scoring.body` and head hits `scoring.head`; `rings` apply to the head. The built-in `demo-capsule`:

```toml
[targets]
shape = { kind = "capsule", radius = 0.25, height = 1.4, head = { radius = 0.14, offset = 0.85 } }
```

| Field | Meaning |
|---|---|
| `radius` | Radius of the cylinder and its end caps, meters, > 0. |
| `height` | Total height including both caps, meters, ≥ 2 × `radius`. |
| `head` | Optional `{ radius, offset }`: a head sphere of `radius` (> 0) whose center is `offset` meters (≥ 0) above the capsule's center. When the ray passes through both, the nearer one counts. |

Capsules spawn like spheres, with their center at least `height / 2` above the floor so they never sink into it, and the aim point (for spacing, spawn rules and the simulated aimer) is the head's center, or the capsule's center without a head. Ballistic capsules touch the floor with their bottom. The difficulty size knob and size tuning scale every length of a capsule; `shrink_per_kill` needs a sphere.

### Accuracy multiplier and shrinking targets

`scoring.accuracy_mult` makes accuracy part of the score, and `targets.shrink_per_kill` makes every kill harder than the last. Both are optional and off by default.

```toml
[targets]
shrink_per_kill = { factor = 0.97, floor = 0.5 }

[scoring]
accuracy_mult = "linear"
```

| Field | Meaning |
|---|---|
| `accuracy_mult` | `"none"` (default), `"linear"` or `"sqrt"`. The final score (on the results screen and in history) is multiplied by the run's accuracy, or by its square root, a gentler penalty. Accuracy is hits ÷ shots in click mode and time on target ÷ time firing in hold mode. The live score during the run is not multiplied. |
| `shrink_per_kill.factor` | Every kill multiplies the sphere radius by this, in (0, 1). Live targets shrink too. |
| `shrink_per_kill.floor` | The smallest radius, as a fraction of the starting one, in (0, 1]. |

Shrinking needs a sphere. With phases, each phase starts again at its own radius. The built-in `demo-shrink` uses both.

### Motion profiles

`[[targets.motion_profiles]]` (optional, at most 8) gives each target several motions to switch between. A target picks a profile by weight when it spawns, and again every `switch_secs`; picking the same profile keeps it moving without interruption. When profiles are set they replace `targets.motion`, which must be left out. The built-in `demo-profiles`:

```toml
[[targets.motion_profiles]]
weight = 2
switch_secs = [1.0, 2.0]
motion = { kind = "drift", speed = [1.5, 2.5], change_secs = [0.8, 1.5], max_offset = 4.0 }

[[targets.motion_profiles]]
weight = 1
switch_secs = [0.5, 1.0]
motion = { kind = "static" }
```

| Field | Meaning |
|---|---|
| `weight` | Relative chance of picking the profile, ≥ 0 (default 1); the weights must add up to more than 0. A zero weight is never picked. |
| `switch_secs` | Time until the target picks again, seconds (a positive range). Without it the first pick lasts the target's life. |
| `motion` | Any motion except `ballistic`, written like `targets.motion`. When a pick changes the motion, the new motion decides at once and movement it has no part in stops, as with behaviour rules. |

Behaviour rules still override the profile in force. `reactions` need every profile's motion to be `strafe` or `dodge`. The difficulty speed knob and speed tuning scale every profile's motion.

### Play area and floor height

With `movement = true`, the player's feet stay inside `bounds_x` and `bounds_z`; pushing into an edge stops only that direction, so you slide along it. The edges are drawn as low, see-through strips on the floor, a few centimeters high, so they never block the view. The built-in movement drills use `counter-strafe` x [−16, 16], z [−3, 8] and `strafe-shot` x [−16, 16], z [−4, 8] (widened from about ±8 m so a full ADAD rhythm fits without hitting an edge).

`elevation` raises (or lowers, when negative) the player's floor; targets keep theirs at 0. The trainer draws a translucent slab at that height under the play area. You can also change it during a run: press Esc, set **Floor height** in 0.5 m steps, and choose **Apply and restart**. The run restarts at the same difficulty and duration, and a routine stays at the same run.

### FOV lock

By default every scenario uses VALORANT's camera: a vertical FOV of ≈70.53° (103° horizontal at 16:9, see `valorant-mechanics.md`). `[view] fov_lock` changes it for this scenario only:

- `fov_lock = 60` always renders at 60° vertical.
- `fov_lock = [min, max]` clamps VALORANT's FOV into the range, so `[60, 80]` keeps 70.53° while `[40, 55]` renders at 55°.

Zoom narrows the locked FOV the way VALORANT zooms: the horizontal FOV is divided by the weapon's zoom. Every bound must lie within 30–120° and `min <= max`. The lock changes only what you see: hit tests and spawns still use world angles, so an `on_screen` spawn tuned for VALORANT's FOV can land off screen under a much narrower lock.

### Per-scenario feedback

`[feedback]` adjusts hit feedback for one scenario. It never turns on what the player switched off in Settings:

| Field | Default | Effect |
|---|---|---|
| `hit_markers` | `true` | `false` hides hit markers, whatever the player's marker style. |
| `hit_sounds` | `true` | `false` mutes hit, headshot and kill sounds. |
| `damage_numbers` | `false` | `true` floats the weapon's VALORANT damage for the zone hit (e.g. Vandal 160 head, 40 body; a sphere counts as body) above each hit. Needs `fire = "click"`. |
| `health_bars` | `false` | `true` draws a small bar over every target whose starting health is above 1 (click: hits, hold: seconds); it shows the share of health left. |

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
| `angular` | `yaw_deg`, `pitch_deg`, `distance`, `grid` (optional) | A direction relative to the starting view. Humanoids ignore pitch and stand on the floor. `grid = { rows, cols }` (each 1–16) snaps spawns to a grid of points: `cols` yaws from `yaw_deg.min` to `yaw_deg.max` and `rows` pitches from `pitch_deg.min` to `pitch_deg.max`, evenly spaced (a single row or column sits in the middle); each spawn picks a random point, so use `min_separation_deg` to keep live targets on different points. Humanoids use the columns only. The built-in `demo-grid` uses a 3 × 5 grid. |
| `around_crosshair` | `offset_deg`, `distance`, `pitch_limit_deg` (default `[-15, 30]`), `on_screen` (default `false`) | Spawns relative to where you aim right now. Use it for flicks (large offsets) and microcorrections (1–6°). `pitch_limit_deg` keeps every spawn inside a vertical band around the horizon, so chained flicks cannot drift into the sky or the floor. `on_screen = true` keeps every spawn inside VALORANT's view from the crosshair at spawn time, with a 4° margin: at most 47.5° sideways and ≈31.3° up or down (half of 103° × 70.53°, minus the margin). `offset_deg.max` must then be ≤ 47.5, and difficulty scaling clamps the offset to that limit. Humanoids only use the horizontal offset. |
| `floor` | `x`, `z`, `height` | A world-space box. Use it for agents at realistic positions, or spheres at head height (`height = [1.8, 1.95]`). |

### Spawn patterns

`[targets.spawn_rules]` (optional) stops targets from appearing where the player expects them. All four controls default to off and can be combined:

```toml
[targets]
spawn_rules = { avoid_last_deg = 15, avoid_recent = 2, min_flick_deg = 12, exclude_center = 0.25 }
```

| Field | Default | Meaning |
|---|---|---|
| `avoid_last_deg` | `0` (off) | Blocks the last spawn point and every position within this angle of it, in [0, 180). A small angle only stops a target reappearing in the same place; a wider one also blocks the positions next to it. |
| `avoid_recent` | `1` | How many of the latest spawn points `avoid_last_deg` blocks, 1–16. |
| `min_flick_deg` | `0` (off) | Minimum angle between the crosshair at spawn time and the new target, in [0, 180): a minimum flick distance. |
| `exclude_center` | `0` (off) | Keeps targets out of the middle of the area: a fraction in [0, 1) of each axis' half-width around its center. `angular` uses yaw and pitch (humanoids yaw only); `floor` uses x and z. With `around_crosshair` it is rejected, because `offset_deg.min` already keeps targets out of the middle. |

Angles are measured from the eye between aim points (a humanoid's head, a sphere's center). A spawn that breaks a rule is redrawn together with `min_separation_deg`, up to 24 times; if every attempt breaks a rule, the last one is used, so a very strict combination is not guaranteed. The trainer uses positions, not a grid of spawn points, so "the last spawn point" and "the positions adjacent to it" are both an angle around the last position. The built-in `spread-flicks` uses all four controls.

### Motions

| `kind` | Fields | Meaning |
|---|---|---|
| `static` | none | The target does not move. |
| `strafe` | `weapon`, `change_secs`, `stop_chance`, `max_offset` | A/D strafing with real VALORANT acceleration and stopping. |
| `dodge` | `weapon`, `change_secs`, `juke_secs` (default `[0.1, 0.25]`), `juke_chance`, `stop_chance`, `crouch_chance`, `jump_chance` (chances default `0`), `max_offset` | An evasive duelist: `strafe` physics plus jukes, stop-and-go, crouches and jumps. At each decision the target jumps, crouches in place, stops, or moves (reversing if it was moving); `stop_chance + crouch_chance + jump_chance` must be ≤ 1. After a decision, `juke_chance` picks the short `juke_secs` wait instead of `change_secs`, which produces quick A-D jukes. A crouch lowers the head 0.5 m; a jump rises 0.9 m and lands after 0.85 s with no air control (estimates, see `valorant-mechanics.md`). Spheres drop by the crouch height. |
| `drift` | `speed`, `change_secs`, `max_offset` | Smooth wandering, for tracking and evasive switching. |
| `ballistic` | `speed`, `angle_deg`, `gravity` (default 9.81), `bounces` (default 0), `restitution` (default 0.5), `on_floor` (default `"expire"`) | Thrown once under gravity, then lands. See "Ballistic targets". |
| `linear_bounce` | `speed_deg`, `arena = { yaw_deg, pitch_deg, sides }` | Straight lines at a constant angular speed, reflecting off the walls of a box or polygon. See "Bouncing and orbiting targets". |
| `orbit` | `speed_deg`, `radius` (optional), `change_secs` (optional), `reverse_chance` (default 0.5) | Circles around the player. See "Bouncing and orbiting targets". |
| `wave` | `speed`, `amplitude`, `frequency_hz`, `form` (default `"sine"`), `max_offset` (default 3) | Glides sideways while weaving up and down on a sine or zigzag path. See "Wave targets". |

`strafe`, `dodge` and `drift` also take an optional `depth` block, which adds forward/back movement (see "Depth movement").

The built-in `dodge-*` scenarios are dynamic clicking drills against evasive enemies:

| Scenario | Motion focus | Trains |
|---|---|---|
| `dodge-juke-heads` | jukes and stops | Holding the head line through A-D taps and clicking on the stop or reversal |
| `dodge-crouch-spam` | crouches | Following a head that drops 0.5 m and comes back up |
| `dodge-jump-spot` | jumps | Clicking an airborne head or pre-aiming the landing spot |
| `dodge-peek` | jiggle peek, appears after a delay | Reacting to a peek without firing while the agent crosses at full speed |
| `dodge-micro-spheres` | jukes on a head-sized sphere | Small corrections that do not overshoot into a reversal |

#### Ballistic targets

A `ballistic` target is launched once, when it appears: at a random `speed` (m/s) and a random `angle_deg` above the horizon, towards a random side along the line across your view. 90° throws it straight up, 0° sideways, and negative angles throw it downwards; a target spawned high with `speed = [0, 0]` simply drops. Gravity pulls it down at `gravity` m/s² (Earth's 9.81 by default). Each of its `bounces` reflects it off the floor, keeping `restitution` (in (0, 1]) of its vertical speed. A sphere touches the floor when its center comes down to its radius, a humanoid when its feet reach the floor.

When it comes down after its last bounce, it lands:

| `on_floor` | Landing |
|---|---|
| `"expire"` | The target expires: it counts in `expired` and costs `expire_penalty`, like a target whose `lifetime_secs` ran out. |
| `"fail"` | The same, and the run fails at once ("a target hit the floor"). |

Landing ballistic targets satisfy `fail.max_expired` without a `lifetime_secs`, so `[fail] max_expired = 3` gives three drops. A ballistic target cannot be switched by `[[targets.behaviors]]` (nor be a rule's motion) or be part of `motion_profiles`, because its flight is one throw. Target speed tuning and the difficulty speed knob fly the same arc faster: launch speed × k and gravity × k². The built-in `demo-ballistic` tosses spheres up from the floor:

```toml
[targets]
shape = { kind = "sphere", radius = 0.3 }
spawn = { kind = "floor", x = [-4.0, 4.0], z = [-14.0, -10.0], height = [0.5, 0.5] }
motion = { kind = "ballistic", speed = [6.0, 8.0], angle_deg = [70.0, 85.0], bounces = 1, restitution = 0.5 }
```

| Field | Meaning |
|---|---|
| `speed` | Launch speed, m/s: a non-negative range. |
| `angle_deg` | Launch angle above the horizon, degrees within [-90, 90]. |
| `gravity` | Downward acceleration, m/s², > 0. |
| `bounces` | Floor bounces before it lands, 0–16. |
| `restitution` | Share of the vertical speed a bounce keeps, in (0, 1]. |
| `on_floor` | `"expire"` or `"fail"`. |

#### Depth movement

`depth = { change_secs, distance }` makes a `strafe`, `dodge` or `drift` target also walk towards and away from you:

```toml
motion = { kind = "strafe", change_secs = [0.4, 0.9], depth = { change_secs = [0.6, 1.4], distance = [10.0, 18.0] } }
```

| Field | Meaning |
|---|---|
| `change_secs` | Time between forward/back decisions, seconds (a positive range). Each decision reverses the direction; the first one picks a random direction. |
| `distance` | Horizontal distance from your eye (where it was when the target spawned) that the target stays within, meters, ≥ 1. It turns back at either edge. A target that spawns outside the band first walks into it. |

`strafe` and `dodge` walk with the weapon's VALORANT movement on both axes, and diagonal movement is normalized as it is for the player, so a target moving on both axes is no faster than one strafing. `dodge` targets keep their depth speed in the air. `drift` moves in depth at a speed drawn from its own `speed` at every depth decision, on top of its drift. The built-in `demo-depth` uses it.

#### Bouncing and orbiting targets

**`linear_bounce`** moves in a straight line at a constant angular speed `speed_deg` (degrees per second, seen from your eye where it was at spawn), in a random direction, and reflects off the walls of `arena`, keeping its distance from you:

```toml
motion = { kind = "linear_bounce", speed_deg = [10.5, 13.5], arena = { yaw_deg = [-20.0, 20.0], pitch_deg = [0.0, 18.0], sides = 5 } }
```

| `arena` field | Meaning |
|---|---|
| `yaw_deg` | Yaw range, degrees within [-180, 180], min < max. |
| `pitch_deg` | Pitch range, degrees within ±89. |
| `sides` | 0 (default) bounces off the box itself. 3 to 12 bounces off the regular polygon inscribed in the box, with a vertex at the top (5 makes a pentagon); its pitch range must then not be empty. |

The arena is angular so it lines up with an `angular` spawn area; spawn inside it. A target that starts outside drifts in and is held from then on. Humanoids move only horizontally and need the box.

**`orbit`** circles around you (your eye where it was at spawn) at `speed_deg` degrees per second, keeping its height:

```toml
motion = { kind = "orbit", speed_deg = [12.0, 26.0], change_secs = [1.0, 2.5], reverse_chance = 0.5 }
```

| Field | Meaning |
|---|---|
| `speed_deg` | Angular speed, degrees per second (a positive range); the direction is random. |
| `radius` | Horizontal radius, meters, ≥ 1. Default: the target's distance when it spawned. With a radius, the target moves onto that circle on its first frame. |
| `change_secs` | Time between direction decisions, seconds. Without it the target never turns back. |
| `reverse_chance` | Chance that a decision reverses the direction, in [0, 1]. |

The built-in `demo-bounce` and `demo-orbit` use them. Speed tuning and the difficulty speed knob scale `speed_deg`. Every new motion draws random numbers only when it is used, so other scenarios replay exactly as before.

#### Wave targets

A `wave` target glides sideways (across your view, like a strafe) at a constant `speed` in m/s and weaves up and down around its spawn height, turning back when it is `max_offset` meters from its spawn point:

```toml
motion = { kind = "wave", speed = [2.0, 3.0], amplitude = [0.3, 0.5], frequency_hz = [0.8, 1.2], form = "zigzag", max_offset = 4.0 }
```

| Field | Meaning |
|---|---|
| `speed` | Lateral speed, m/s, a non-negative range. A speed near 0 gives a target that mostly bobs in place. |
| `amplitude` | How far the weave rises above and falls below its center line, meters (a positive range). |
| `frequency_hz` | Full up-and-down cycles per second, a positive range of at most 10 Hz. |
| `form` | `"sine"` (default) slows into each turn, like a smooth bob; `"zigzag"` climbs and falls at a constant speed and turns sharply. |
| `max_offset` | Max lateral distance from the spawn point, meters (> 0). |

Each value is drawn once, when the target appears, together with a random side and a random point of the cycle, so several targets do not weave in step. Spheres never sink below the minimum sphere height. Humanoids walk on the floor, so `wave` needs a sphere or capsule shape (also in motion profiles and behaviour rules). Speed tuning and the difficulty speed knob scale `speed` and `frequency_hz` together, so the target travels the same path faster; the amplitude stays. The built-in `wave-runners`, `zigzag-sparks`, `bobbing-heads` and `stop-go-weavers` use it. Like the other newer motions, `wave` draws random numbers only when it is used, so other scenarios replay exactly as before.

### Reactive targets

`targets.reactions` (optional) makes a `strafe` or `dodge` target answer the player. Each reaction sets the target's strafe direction and holds it for a `react_secs` draw before the motion decides again. Airborne `dodge` targets react only after they land.

```toml
[targets]
motion = { kind = "strafe", change_secs = [0.5, 1.0], stop_chance = 0.8 }
reactions = { mirror_strafe_chance = 0.3, oppose_strafe_chance = 0.3, dodge_within_deg = 2.5, shot_juke_chance = 0.35 }
```

| Field | Default | Meaning |
|---|---|---|
| `mirror_strafe_chance` | `0` | When the player starts a strafe, the chance that the target strafes the same way. A new strafe is noticed when the player's speed along the target's side-to-side axis reaches 1 m/s in a new direction (an estimate, see `training-science.md`), so it needs `player.movement = true`. |
| `oppose_strafe_chance` | `0` | The chance that it strafes the other way instead. Both share one roll, so their sum must be ≤ 1. |
| `dodge_within_deg` | `0` (off) | When the crosshair comes within this angle of the target's aim point (head or sphere center), in [0, 90), the target moves away from the crosshair. It triggers again only after the crosshair has left that circle. |
| `shot_juke_chance` | `0` | Whenever the player fires (click mode), the chance that every visible target jukes: it reverses, or, when standing still, moves away from the crosshair. |
| `react_secs` | `[0.2, 0.4]` | How long a reaction holds, seconds (a positive range). |

### Behaviour rules

`[[targets.behaviors]]` (optional, at most 8) switches a target's behaviour on conditions. Every frame the first rule whose conditions all hold is in force; with none, the target uses `targets.motion`. The built-in `react-duel` "holds the angle, strafes when hit, retreats under 50% health":

```toml
[[targets.behaviors]]
when = { health_below = 0.5 }
retreat_m = 4

[[targets.behaviors]]
when = { damaged_within_secs = 1.5 }
motion = { kind = "dodge", change_secs = [0.2, 0.45], juke_chance = 0.4 }
```

| Condition (`when`) | Holds when |
|---|---|
| `health_below` | Remaining health is below this fraction of the starting health, in (0, 1]. Never holds for `inf` health. |
| `damaged_within_secs` | The target took damage within the last this many seconds (> 0). |
| `alive_for_secs` | The target has been visible for at least this many seconds (≥ 0). |
| `crosshair_within_deg` | The crosshair is within this angle of the target's aim point, in (0, 180). |
| `speed_above` | The target moves faster than this many m/s (≥ 0). |

A rule needs at least one condition and at least one override:

| Override | Meaning |
|---|---|
| `motion` | A motion to use instead of `targets.motion`, written and validated like it. When a rule switch changes the motion, the new motion decides at once and movement it has no part in stops (for example a `static` override stops a strafe). |
| `speed_scale` | Multiplier on movement speed, in [0.25, 4] (default 1), on top of in-run tuning. |
| `retreat_m` | Move straight away from the player, with the motion's weapon movement physics (the Vandal's for `static` and `drift`), until the target has retreated this many meters in total, in [0, 20]. Its strafe lane moves back with it. |

There is no line-of-sight condition: scenario targets always stand in the open. Arena bots, which can hide behind walls, live in `aim_world`. An airborne `dodge` target keeps its rule until it lands. Reactions and rules draw random numbers only when they are used, so other scenarios replay exactly as before.

Clicking on the stop or reversal works because the target's lateral speed passes through zero there (inference from the movement model, not a measured training effect). The simulated aimer re-aims at the target every frame, so calibrate scores understate how much harder the motion makes these drills for a human.

### Phases, fail conditions and run end

A scenario can chain phases (waves), fail early, and end on kills or never. All three are optional; without them a run ends after `duration_secs`, as before. The built-in `phase-peek-ladder` chains close and far waves and fails at 5 misses; `phase-kill-race` ends at 30 kills after an accuracy-gated warm-up.

```toml
[end]
kind = "kills"               # time (default) | kills | endless
kills = 30                   # kills only

[fail]                       # optional; the first one reached fails the run
max_misses = 3               # click fire only; a shot on a decoy is a miss
max_expired = 2              # needs lifetime_secs on the targets or a phase, or a ballistic motion

[[phases]]
name = "Close trio"
targets = { max_alive = 3, spawn = { kind = "angular", yaw_deg = [-20, 20], pitch_deg = [0, 0], distance = [8, 12] } }
count = 3
advance = { kind = "cleared" }
time_limit_secs = 6

[[phases]]
name = "Far peeker"
targets = { max_alive = 1, lifetime_secs = 2.0 }
```

**`[end]`.** `duration_secs` stays a hard cap for every kind, so no run can go on forever.

| `kind` | The run ends |
|---|---|
| `time` | After `duration_secs` (default). Completed. |
| `kills` | At the `kills`-th kill (≥ 1): completed. Reaching `duration_secs` first fails it ("ran out of time"). |
| `endless` | When a fail condition is reached (failed), or at `duration_secs` (completed). It needs `[fail]` or a phase `time_limit_secs`. |

**`[fail]`.** Counts cover the whole run, across phases. `max_misses` counts shots that hit no target (≥ 1, click fire only). `max_expired` counts expired targets (≥ 1).

**`[[phases]]`** (1 to 16). The run starts in the first phase. Each phase has:

| Field | Meaning |
|---|---|
| `name` | Shown on the results screen; "Phase N" when empty. |
| `targets` | Overrides for the phase. Any of `shape`, `spawn`, `motion`, `max_alive`, `health`, `lifetime_secs`, `appear_delay_secs`, `reactions` and `behaviors`, written as under `[targets]`. Unset fields keep the `[targets]` value; a set `reactions` or `behaviors` replaces the scenario's block for the phase. `shape` must keep the kind of `targets.shape` (change a sphere's radius or `head_only`). The resolved targets are validated like `[targets]`. |
| `count` | Wave size: the phase spawns this many targets in total (1 to 1000), then no more. Only with `advance = { kind = "cleared" }`, which needs it. |
| `advance` | What moves the run to the next phase. Every phase but the last needs one. On the last phase it completes the run; a last phase without one runs until the run ends. |
| `time_limit_secs` | Fail the run if the phase has not advanced this many seconds after it started. Needs `advance`, and must be longer than a `time` trigger. |

| `advance` | Fires when |
|---|---|
| `{ kind = "kills", kills = 3 }` | The phase has this many kills (≥ 1). |
| `{ kind = "accuracy", accuracy = 0.8, min_shots = 5 }` | Hits ÷ shots within the phase reach `accuracy` (in (0, 1]) after at least `min_shots` (≥ 1) phase shots. Click fire only. |
| `{ kind = "cleared" }` | All `count` targets of the wave are gone, killed or expired. Expiries still count for `max_expired` and `expire_penalty`. |
| `{ kind = "time", secs = 10 }` | The phase has lasted this many seconds (> 0). |

Each frame the session checks, in order: the phase trigger, the phase time limit, the fail conditions, and a kills goal. So a trigger reached on the frame its phase times out still advances. When a phase starts, the previous phase's targets disappear (they do not count as expired) and the new phase's targets spawn at once. Difficulty levels and in-run tuning scale each phase's overrides exactly as they scale `[targets]`.

Score, accuracy and kills per minute use the time actually played. Pace and stamina are not analysed for phased runs, because the phases change the rate on purpose. For runs that do not end on time, the potential-score tip converts no saved seconds into points, because finishing faster does not add kills there; only miss penalties count.

### Perception drills

`[targets.perception]` (optional) changes how targets are announced and revealed. It has a `kind` (`sound_cue`, `detection` or `radar`); only one perception drill fits in a scenario.

**`kind = "sound_cue"`: sound localisation.** Each target is first only heard: a synthesized sound plays from its position when it would have appeared, and the target itself appears `reveal_after_secs` later, or as soon as the crosshair comes within `reveal_within_deg` of it. A hidden target cannot be shot. When the crosshair reveals it early, its `lifetime_secs` starts then. The built-in `sound-turn` spawns agents 60–180° away from the crosshair, so every sound comes from the side or from behind:

```toml
[targets]
shape = { kind = "humanoid" }
spawn = { kind = "around_crosshair", offset_deg = [60.0, 180.0], distance = [8.0, 16.0] }
lifetime_secs = 3.0

[targets.perception]
kind = "sound_cue"
sound = "footsteps"
reveal_after_secs = 0.8
reveal_within_deg = 25
```

| Field | Meaning |
|---|---|
| `sound` | `"footsteps"` (three quick steps) or `"gunshot"` (one crack). Both are made up from tones and noise; no game sound is used. |
| `reveal_after_secs` | Seconds from the sound to the target appearing, in (0, 10]. |
| `reveal_within_deg` | Default 0 (off). Reveal the target early once the crosshair is within this angle of its aim point, in [0, 180). |

The sound is panned to where it comes from when it plays: left/right by its angle from your view, duller and quieter when it is behind you, quieter when it is far. Bevy's spatial audio is not used because it cannot tell front from back. Cue sounds play even with shot sound effects off; the volume slider still applies. Use headphones. The results card shows, for each of eight directions around you, the median angular error 1 s after each sound (or at your first shot) and the median turn reaction time (see "Sound localisation" in `training-science.md`).

**`kind = "detection"`: peripheral detection.** Each trial is a target slot: after `appear_delay_secs` (the wait before a flash), a target flashes for `lifetime_secs` in one of eight screen areas around the crosshair (up, up-right, right and so on), `spawn.offset_deg` away from where the crosshair points when the trial starts and `spawn.distance` from the eye. With `blank_chance` a trial stays blank: nothing shows. A click anywhere while a flash shows detects it (it scores like a hit on it, so `scoring.decay` rewards fast clicks); a click with nothing showing is a false alarm (a missed shot, `miss_penalty`); a flash nobody clicks expires (`expire_penalty`). The built-in `peripheral-detect`:

```toml
[player]
elevation = 2.0   # raised floor, so flashes below the crosshair stay above the ground

[targets]
shape = { kind = "sphere", radius = 0.06 }
spawn = { kind = "around_crosshair", offset_deg = [6.0, 18.0], distance = [6.0, 6.0] }
lifetime_secs = 0.5
appear_delay_secs = [0.8, 2.2]

[targets.perception]
kind = "detection"
blank_chance = 0.25
```

| Field | Meaning |
|---|---|
| `blank_chance` | Default 0. Share of trials with no flash, in [0, 0.9]. |

Detection needs a sphere, `spawn.kind = "around_crosshair"` with `offset_deg.max` inside the screen's height (so flashes above and below stay on screen), `max_alive = 1`, `lifetime_secs`, click fire, no `decoy_chance` and no `scoring.rings`. The results card shows flashes seen, false alarms, blank trials and, for each screen area, the hit rate and median reaction time.

**`kind = "radar"`: minimap-glance recall.** Each round, a radar in the top-left corner shows for `show_secs` where `dots` enemies stand (top-down, turning with your view, forward up). Then all `max_alive` targets of the round appear together, `appear_delay_secs` after the radar is gone, and they look alike. Shoot the ones the radar showed; the others are decoys to leave alone until they expire, scored like go/no-go decoys (`decoy_penalty`). The next round starts when every target of this one is gone. The built-in `radar-recall`:

```toml
[targets]
shape = { kind = "humanoid" }
spawn = { kind = "angular", yaw_deg = [-70.0, 70.0], pitch_deg = [0.0, 0.0], distance = [8.0, 22.0] }
max_alive = 4
min_separation_deg = 15
lifetime_secs = 3.0
appear_delay_secs = [0.4, 0.8]

[targets.perception]
kind = "radar"
show_secs = 1.5
dots = 2
```

| Field | Meaning |
|---|---|
| `show_secs` | Seconds the radar stays on screen, in (0, 10]. It is hidden during the countdown and the pause. |
| `dots` | Enemies per round the radar shows, from 1 to `max_alive - 1`; the rest are decoys. |

Radar needs `lifetime_secs`, click fire and no `decoy_chance`. Keep `min_separation_deg` large enough that the radar's dots and the decoys are easy to tell apart. The results card shows correct picks (radar enemies shot), incorrect picks (others shot), radar enemies missed and others left alone, next to the go/no-go section.

Perception drills draw random numbers only when they are used, so other scenarios replay exactly as before.

### Difficulty

Training methods and adaptive mode play a scenario at a level from −1 (easiest) to +1 (hardest); 0 is the file as written. Each knob is the fraction a parameter changes at the ends of that range. A plain number applies both ways; `{ easy, hard }` sets each end separately. Fractions must be in `[0, 0.95)`, and `0` turns a knob off.

```toml
[difficulty]
size = { easy = 0.5, hard = 0.1 }   # spheres: 50% bigger at −1, 10% smaller at +1
speed = 0.3                         # motion speeds and how soon targets change direction or juke (default)
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
| Target size | Sphere radius, and every length of a capsule. Agents (humanoids) keep their VALORANT size; use Distance to make their heads smaller or larger on screen. |
| Distance | Spawn distance: `angular` and `around_crosshair` `distance`, and `floor` `z`. |
| Target speed | Motion speed: drift `speed`; strafe run speed and acceleration together, so the strafe keeps its timing; `linear_bounce` and `orbit` `speed_deg`; `wave` `speed` and `frequency_hz` together (the same path, faster); and a `ballistic` launch speed (× k) with its gravity (× k²), so it flies the same arc faster. |

A tuned run is stored under its own key (`%s80d120v150`: size, distance and speed percentages); the default 100/100/100 adds no suffix. Tuned runs never move the adaptive level, even when started adaptively, and never count for the benchmark, which reads only exact untuned keys. The results screen lists the tuning, and the HUD shows a "tuned" tag.

Target speed is stored in the definition as `targets.speed_scale` (default 1, allowed 0.25–4), which every motion reads. Scenario files normally leave it out.

### Movement Lab

The **Movement Lab** (Scenarios tab, below the list) builds a movement drill from a few settings instead of a file. `aim_core::scenario::lab::LabSettings::to_def` turns them into an ordinary `ScenarioDef` made of the blocks on this page, so it validates and plays like any scenario.

| Setting | Allowed | Becomes |
|---|---|---|
| Enemy motion | static, strafe, dodge | `targets.motion` (`strafe` or `dodge` with the lab weapon's run speed, `max_offset = 3`) |
| Speed | 50–150% | `targets.speed_scale` (strafe and dodge only) |
| Direction changes | every 0.2–2 s | `change_secs`, a random 60–140% of the value |
| Dodge style | light, medium, heavy | `juke_chance` / `stop_chance` / `crouch_chance`: 0.2/0.15/0.05, 0.5/0.25/0.1, 0.7/0.25/0.25 |
| Enemies at once | 1–6 | `targets.max_alive` (6° apart when more than one) |
| Shape | head only, full body | `humanoid` with `head_only` |
| Spawn distance | 5–50 m | `floor` spawn `z = [-max, -min]` |
| Lateral spread | ±1–20 m | `floor` spawn `x` |
| Appear delay | 0–3 s | `appear_delay_secs = [value / 2, value]` |
| Lifetime | 0.5–10 s, or until killed | `lifetime_secs` (and a 50-point `expire_penalty`), or none |
| Player movement and area | on/off, ±2–20 m wide, 0–12 m deep | `player.movement`, `bounds_x`, `bounds_z` centered on the start (default ±16 m × 8 m) |
| Weapon, spread, length | Classic, Vandal, Phantom, Guardian, Operator; 15–180 s | `weapon.kind`, `weapon.simulate_spread`, `duration_secs` |

Out-of-range values (for example from a hand-edited `settings.toml`) are clamped. Every miss costs 10 points and, when you can move, every shot fired while moving costs 50.

The generated id is `lab-` plus the content hash of the definition, so the same settings always share one history key and personal best, and lab results never mix with the built-in scenarios. The last settings are saved as `[movement_lab]` in `settings.toml`, and Retry on the results screen replays the same drill.

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
