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

The built-in scenarios in `content/scenarios/`, grouped by the skill they train. Every scenario is original: the ideas come from public training playlists (Voltaic, KovaaK's), credited in a comment at the top of each file, but no file, name or value was copied. See `scenarios.md` for the format and `training-science.md` for the evidence behind the drill types.

## Library: tracking and VALORANT

All of them pass validation, the simulated-player gate and the calibrate check (recruit < contender < veteran < elite, contender accuracy 45–85% where accuracy applies). The simulated tracker is a first-order lag, so constant-speed tracking scores near 0% or 100%; these scenarios vary their speeds to separate the skill levels.

### Tracking

| Scenario | Idea credit | Mechanic | Trains |
|---|---|---|---|
| `arc-follow` | Low-arc bouncing tracking (Voltaic precise tracking) | Hold fire on a small sphere thrown in low `ballistic` arcs with 3 bounces, seen from a raised floor | Smooth tracking through gravity arcs and bounce reversals |
| `thin-glide` | Thin-target smooth tracking (Voltaic precise tracking) | Hold fire on a thin capsule that `drift`s on long paths at changing speeds | Precise, steady speed matching on a narrow target |
| `sky-reactive` | Airborne reactive tracking (Voltaic reactive tracking) | Hold fire on an airborne sphere with fast `drift` changes; four 15 s `[[phases]]` alternate far and close | Reacting to direction changes at small and large angular speeds |
| `ground-reactive` | Ground strafe reactive tracking (Voltaic reactive tracking) | Hold fire on a capsule that `dodge`s with jukes, stops and `depth` movement | Reactive tracking of side-to-side and forward/back changes |
| `smooth-xyz` | 3D smooth control tracking (Voltaic control tracking) | Hold fire on a sphere that `drift`s on all three axes (`depth`) | Control: smooth corrections while the target changes size |
| `orbit-track` | Around-the-player tracking (KovaaK's and Voltaic tracking) | Hold fire on a sphere in `orbit` around you that turns back sometimes; a new one at a new speed every 5 s | Large, steady arm turns matched to the target's pace |
| `fade-track` | "Gauntlet" style fading-target tracking | Hold fire; each sphere lives 3 s and its value `decay`s to 10% (approximation, see below) | Fast acquisition and staying on target under a clock |

### VALORANT flick-tech

| Scenario | Idea credit | Mechanic | Trains |
|---|---|---|---|
| `snap-reflex` | VALORANT reflex flicks (Voltaic VALORANT playlists) | One head-sized sphere at a random height and depth, `min_flick_deg` 10, value `decay`s over 1.2 s | Fast, clean reflex flicks |
| `mini-phase` | Multi-phase flick drills (Voltaic VALORANT playlists) | Four 15 s `[[phases]]` with a `time` advance: wide statics, small statics, strafers, small strafers | Adapting flicks to target size and motion within a run |
| `four-shrink` | Shrinking multi-target flicks (Voltaic VALORANT playlists) | Four spheres on a wide wall, `shrink_per_kill` 0.97 down to 40% | Keeping accuracy as targets get smaller |

### VALORANT micros

| Scenario | Idea credit | Mechanic | Trains |
|---|---|---|---|
| `micro-cluster` | Tight-cluster micros (Voltaic VALORANT playlists) | Three tiny spheres on a tight 3 × 4 spawn `grid` | Small, exact corrections between close targets |
| `angle-hold-peek` | Angle-holding micros (Voltaic VALORANT playlists) | A head-sized sphere appears on the left or right side (`exclude_center`), strafes and leaves after 1.6 s; value `decay`s | Holding the head line and reacting to a peek |
| `micro-pace` | Accuracy-weighted micros (Voltaic VALORANT playlists) | A tiny sphere 1–4° away; `accuracy_mult = "linear"` | Micro corrections that favor accuracy over speed |

### VALORANT stability

| Scenario | Idea credit | Mechanic | Trains |
|---|---|---|---|
| `head-strafe-track` | Thin humanoid strafe tracking (Voltaic VALORANT playlists) | Hold fire on a very thin capsule with a head doing short `strafe`s; head scores 2× body | Staying on the head through fast A/D reversals |
| `peek-track` | Peek tracking (Voltaic VALORANT playlists) | Hold fire on a capsule that appears, `strafe`s and vanishes after 3 s | Acquiring a peeker and tracking it until it leaves |
| `adjust-track` | Small-correction stability (Voltaic VALORANT playlists) | Hold fire on a sphere whose `motion_profiles` switch between a slow drift and short fast darts | Smooth tracking with single clean corrections after sudden offsets |

### Approximations

The engine cannot model these ideas exactly; each file explains its approximation in a comment.

| Scenario | Wanted | Approximation |
|---|---|---|
| `arc-follow` | A slanted floor | A flat floor seen from a raised player floor (`elevation = 1.5`), with varied spawn depth |
| `thin-glide` | Purely horizontal glides | `drift` also moves vertically; the tall capsule absorbs it. `orbit` and `linear_bounce` keep one constant speed per target, which the varied-speed goal rules out |
| `fade-track` | Health that drains on its own | No negative regeneration exists: a 3 s `lifetime_secs` plus score `decay` to 10% |
| `angle-hold-peek`, `peek-track` | Peeking from behind cover | No cover in scenarios: the target appears after a wait, strafes and vanishes on `lifetime_secs` |

## Routines

Eight built-in routines in `content/routines/` use the library. Durations come from `RoutineDef::minutes` (scenario `duration_secs` × runs) and leave out the time between runs. A test keeps each one within ±20% of its target length. Run counts and block order are our design choices, not measured training effects (estimate).

| Routine | Purpose | Structure | Minutes |
|---|---|---|---|
| `valorant-flick-tech` | VALORANT flicks | Warm-up (`track-precise`, `flick-wide`), then 2 blocked runs each of `six-pin`, `angle-flyers`, `snap-reflex`, `four-shrink`, `mini-phase` and `reflex-flick`, then the same six interleaved | ≈28 |
| `valorant-micros` | Small, exact corrections | Warm-up (`track-precise`, `micro-adjust`), then 2 blocked runs each of `micro-cluster`, `micro-pace`, `angle-hold-peek` and `diamond-pair`, then those four interleaved with `track-reactive-micro` | ≈23 |
| `valorant-stability` | Head-level tracking and movement | Warm-up (`track-precise`, `counter-strafe`), then 2 blocked runs each of `head-strafe-track`, `peek-track`, `adjust-track`, `ground-reactive` and `strafe-shot`, then those interleaved with `counter-strafe` | ≈28 |
| `tracking-builder` | Smooth before reactive tracking | Blocked, in order: `track-precise`, then `thin-glide`, `smooth-xyz`, `orbit-track`, then `sky-reactive`, `ground-reactive`, `fade-track` | ≈27 |
| `switching-builder` | Target switching | Blocked, in order: warm-up, then `dot-sweep`, `tempo-movers`, `jitter-switch`, `fly-switch`, `hop-switch`, `prism-bounce`, `mini-regen-switch` | ≈27 |
| `dynamic-clicking` | Clicking moving targets | Warm-up, then 2 blocked runs each of `diag-drifters`, `hop-field`, `ricochet-ring`, `drifting-heads`, `skeet-arcs` and `popcorn-burst`, then a shorter interleaved pass | ≈25 |
| `reaction-and-decision` | Reaction and decisions under time pressure | Warm-up, then `reflex-flick`, `reactive-flick`, `pressure-balloons`, `go-no-go`, `peripheral-detect`, `radar-recall` and `sound-turn` interleaved | ≈19 |
| `ranked-prep-15` | Warm-up before ranked | In order, easiest first: `track-precise` and `flick-wide` at −0.25 difficulty, then `dot-sweep`, `micro-pace`, `head-strafe-track`, `snap-reflex`, `counter-strafe` | ≈13 |

### Evidence notes

- **Warm-up first.** Every routine opens with ordered, easy blocks: performance drops after a rest until you warm up (warm-up decrement, Lee & Genovese 1988; see `training-science.md`, "Routine design").
- **Length.** The routines last 19–28 min, so one or two of them fit the 30–50 min window where Listman et al. (2021) saw about 90% of daily gains. `ranked-prep-15` is shorter on purpose: it is for warming up, not training. Split longer days into separate sessions.
- **Blocked, then interleaved.** Random-order practice performs worse while you learn but retains better (Shea & Morgan 1979), and the effect is smaller in applied settings (Brady 2004). So the builders stay blocked while the drills are new, and the VALORANT and dynamic-clicking routines give each drill one blocked pass before the interleaved half (`interleave_from`). Splitting the two halves this way is our inference, not a tested design.
- **External-focus cues.** Block notes describe the effect on the screen (the crosshair, the target, the head line) rather than the hand or arm. Motor-learning studies find an external focus of attention helps performance and learning more than an internal one (Wulf, Höß & Prinz 1998, *Journal of Motor Behavior* 30(2); review in Wulf 2013, *International Review of Sport and Exercise Psychology* 6(1)). These are general findings, not tested for aim training.
- **Order inside the builders.** `tracking-builder` goes from smooth to reactive, and `switching-builder` from static to evasive targets, so each block adds one difficulty to the last. This easy-to-hard order is our design choice (estimate).
