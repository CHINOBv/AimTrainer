# Scenario library

The built-in scenarios that recreate well-known drill ideas from other aim trainers. Every file is our own: own names, own parameters, written from public descriptions of how a drill plays. Each file credits the idea in a TOML comment at its top. No other trainer's name is used as a scenario name. See the IP policy in `coding-standards.md`.

The "Trains" column is our inference from the drill's mechanic, not a measured training effect (see `training-science.md`). Scores and accuracies come from the simulated aimer; run the `calibrate` example to see them.

## Library: clicking and switching

### Clicking

| Scenario | Idea credit | Mechanic | Trains |
|---|---|---|---|
| `six-pin` Six Pin | Aim Lab Sixshot | Six small static spheres, always six alive; `accuracy_mult = "linear"` | Clean, confirmed clicks on stationary targets |
| `angle-flyers` Angle Flyers | Aim Lab Angleshot | Five spheres with `drift` plus a 10–18 m `depth` band on a wide wall | Reading a moving target's path and clicking without chasing |
| `diag-drifters` Diagonal Drifters | Voltaic dynamic clicking slot | Three spheres with `linear_bounce` in a box; `accuracy_mult = "sqrt"` | Timing a click on a steady, readable velocity |
| `hop-field` Hop Field | Voltaic dynamic clicking slot | Small spheres thrown by `ballistic` with four floor bounces | Leading a target whose vertical speed keeps changing |
| `diamond-pair` Diamond Pair | Voltaic static clicking slot | Two small spheres on a 5 × 5 spawn `grid`; `accuracy_mult = "sqrt"` | Short, precise flicks between nearby static targets |
| `wide-wall-five` Wide Wall Five | Voltaic static clicking slot | Five medium spheres over a 100° wall, with a `weapon.ammo` economy | Long flicks that land on the first click |
| `ricochet-ring` Ricochet Ring | Voltaic linear clicking slot | Two spheres with `linear_bounce` in an octagon (`sides = 8`) | Clicking a constant-velocity target and re-reading it after each ricochet |
| `drifting-heads` Drifting Heads | Voltaic linear clicking slot | Head-sized spheres with `linear_bounce` in a wide, 2°-tall band | Clicking a small target on a long, mostly horizontal path |
| `reflex-flick` Reflex Flick | KovaaK's reflex-flick drills | One sphere with a 1.2 s `lifetime_secs`; the `difficulty.lifetime` knob gives the easy and hard variants | Fast reaction flicks |
| `reactive-flick` Reactive Flick | KovaaK's reactive-flick drills | A static sphere with `health = 2` and a behaviour rule (`health_below = 1.0`) that makes it strafe once hit | A clean first flick and an immediate correction onto a target that just started moving |
| `skeet-arcs` Skeet Arcs | KovaaK's skeet drills | Spheres launched by `ballistic` on one parabola | Leading a target on a curved path |
| `popcorn-burst` Popcorn Burst | KovaaK's popcorn drills | `ballistic` spheres with launch speeds from 4 to 10 m/s and three bounces | Adapting click timing to arcs of different heights |
| `pressure-balloons` Pressure Balloons | KovaaK's pressure drills | Large spheres that `drift` in from 16–20 m to a 10–14 m band, 3.5 s lifetime, `[fail] max_expired = 5` | Staying fast and accurate while targets pile up |
| `cockroach-hop` Cockroach Hop | KovaaK's tiny-jumper drills | Tiny spheres in quick, low `ballistic` hops; `accuracy_mult = "sqrt"`; `[end] kills = 15` | Precise clicks on small, erratic targets |

### Switching

Every switching scenario uses `fire = "hold"`: a target dies after `health` seconds of firing with the crosshair on it.

| Scenario | Idea credit | Mechanic | Trains |
|---|---|---|---|
| `dot-sweep` Dot Sweep | Voltaic switching speed slot | Three small static dots, `health = 0.35` | Settling on a target at once and leaving it the moment it dies |
| `tempo-movers` Tempo Movers | Voltaic switching speed slot | Three spheres with `linear_bounce` at one fixed speed, `health = 0.4` | Matching a known velocity at once and switching on a steady rhythm |
| `jitter-switch` Jitter Switch | Voltaic switching evasive slot | Three spheres with short `strafe` runs (0.15–0.35 s) with acceleration and stops | Staying on a target through quick reversals |
| `fly-switch` Fly Switch | Voltaic switching evasive slot | Three spheres with `drift` plus `depth`, `health = 0.6` | Holding a target whose size and path both change |
| `hop-switch` Hop Switch | Voltaic switching slots | Spheres hopping under low gravity (`ballistic`, `gravity = 2.0`, six bounces) | Following a smooth vertical arc and switching before the next one lands |
| `prism-bounce` Prism Bounce | Voltaic switching slots | Three spheres with a slow `linear_bounce` in a pentagon (`sides = 5`) | Re-acquiring a target right after a ricochet |
| `mini-regen-switch` Mini Regen Switch | Voltaic switching slots | Two small strafing spheres with `regen` (`reset_on_hit = true`) | Committing to a kill before switching |

### Approximations

These ideas cannot be built exactly with the current blocks, so the files approximate them. Each file says so in its header comment.

| Scenario | Original idea | Approximation |
|---|---|---|
| `angle-flyers` | Flyers on a mostly flat wall with some depth | `drift` inside a depth band, not flight on a fixed plane |
| `diag-drifters`, `tempo-movers` | Diagonal movers | `linear_bounce` picks a random direction, so paths are straight but not strictly diagonal |
| `diamond-pair` | A diamond-shaped grid | The spawn grid is rectangular, so the "diamond" is a square lattice |
| `drifting-heads` | Long strafes with a smooth vertical bob | The bob is a zig-zag between the edges of a 2°-tall `linear_bounce` band |
| `prism-bounce` | Low-gravity spheres in a pentagon | Straight `linear_bounce` paths with no curve |
| `pressure-balloons` | Targets drifting in | A `drift` whose depth band is nearer than the spawn distance |
