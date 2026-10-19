# Scenario library

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
