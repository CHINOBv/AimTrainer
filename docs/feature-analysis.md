# Feature analysis: training aim around ranked play

After roadmap Phases 1–8 landed, the project was reviewed against its goal: **improve aim for VALORANT, and be the trainer you open while playing ranked, to warm up and to stay warm**. This page records that analysis and the features it produced, each with the gap it closes and the evidence behind it.

## What the project already did well

- **Training content:** exact transfer of VALORANT's sensitivity, FOV and movement; data-driven exercises; and a method (the VDIM-style week) with tiers and benchmarks.
- **Feedback:** explainable analysis after every run, plus adaptive difficulty.
- **Game-like practice:** arenas with bots and maps.

## Gaps found

| # | Gap | Why it matters for ranked days |
|---|---|---|
| G1 | No short session that fits a matchmaking queue | Players warm up in the 2–5 minutes before a match, not in a 40-minute routine |
| G2 | Nothing to do *between* rounds | Buy phase is about 30 s; hands go cold between rounds and between matches |
| G3 | No answer to "am I warm yet?" | Queuing cold costs the first rounds (warm-up decrement, Lee & Genovese 1988) |
| G4 | No signal to stop training | Gains peak around 1 h a day and drop with fatigue (Listman et al. 2021) |
| G5 | No reason to come back daily | Distributed practice beats cramming; streaks and goals support consistency |
| G6 | Sensitivity is chosen by guesswork | A bad sensitivity caps every other gain |
| G7 | Spraying and bursting are not trained | Vandal/Phantom bursts and taps win most rifle duels |
| G8 | Progress is invisible beyond one scenario | Players quit when they cannot see improvement |
| G9 | Analysis tells you *what* went wrong but not *how* | Seeing the crosshair path makes overshoot and corrections obvious |
| G10 | No check that the PC itself is not the problem | Low 1% lows, vsync or acceleration silently ruin training |
| G11 | A setup cannot be moved between PCs or shared | Players train on several machines and copy settings from pros or friends |
| G12 | No shared, fair comparison | Friendly competition motivates; random seeds make scores incomparable |

## Features

| Feature | Closes | How | Where |
|---|---|---|---|
| **F1 Queue warm-up** | G1 | A routine sized to the queue (default 5 min). It uses 30 s runs in warm-up order (smooth tracking → flicks → precision → switching → movement), easier first. Started with `aimtrainer warmup [min]`, a Windows/X11 hotkey, or a compositor bind. | `training::warmup`, `ui::launcher` |
| **F2 Buy-phase micro drill** | G2 | A 20 s drill that rotates through four drills. It shows the overlay, plays, and **hides itself** when done so the mouse goes straight back to the game. Started with `aimtrainer micro` or a hotkey. | `training::warmup`, `UiAction::StartTimed` |
| **F3 Readiness check** | G3 | Compares today's best with the median of your daily bests over 30 days. Labels: READY ≥ 95 %, WARMING ≥ 85 %, otherwise COLD. Shown with the numbers. | `training::progress::readiness`, Progress page |
| **F4 Fatigue detector** | G4 | Flags a session over 60 min, or a ≥ 10 % drop from the session's peak on a scenario. Shown on Progress and after every run. | `training::progress::fatigue` |
| **F5 Daily goal and streaks** | G5 | Minutes trained today against a goal (default 30 min, the low end of Listman's 30–50 min window), plus the streak of days that met it. | `training::progress::daily_goal` |
| **F6 Sensitivity finder** | G6 | A bracketing search in cm/360 on short wide-switching tests, with the order alternated each round. The bracket narrows from ×1.5 to about ×1.05 in up to 6 rounds. You then apply the result. | `sensitivity::finder`, Settings |
| **F7 Spray and burst trainer** | G7 | Recoil from Riot's published parameters: protected bullets, side switch, recovery time. Burst metrics and three drills (`burst-control`, `spray-transfer`, `tap-discipline`). The climb per bullet is an estimate. | `scenario::recoil`, content |
| **F8 Progress dashboard** | G8 | Weekly overall-energy trend for the benchmark and personal bests per subcategory. | `training::progress::benchmark_trend`, Progress page |
| **F9 Replay viewer** | G9 | Replays saved runs and plots the crosshair path around each target, with hit and miss markers and a per-engagement summary. | `analytics::path`, Progress page |
| **F10 System check** | G10 | FPS average, 1 % low and frame time; vsync and fullscreen state; raw-input and OS acceleration notes. | `platform::frame_stats`, Settings |
| **F11 Profile code** | G11 | `CAT1\|game\|sens\|dpi\|ads\|<VALORANT crosshair code>`: one line to copy, share and import. | `aim_core::profile`, Settings |
| **F12 Daily challenge** | G12 | One scenario and one seed per UTC day, the same for every player, so scores are directly comparable. | `training::challenge`, Today page |

Two smaller additions came out of the same review:

- **`aimtrainer play <scenario|arena>`:** bind your favourite drill to a key.
- **Ultrawide pillarboxing:** the trainer matches VALORANT's 16:9 view on wide monitors.

## Insights and drills inspired by Aim Lab (2026-10-18)

A static analysis of an installed Aim Lab build listed its analytics and training features. Only the *ideas* were taken: no code, assets or data. One example: Aim Lab splits accuracy and reaction time by the screen quadrant of each flick and by near/far distance. The ten features below were missing here. The plan and evidence are in `odd/tasks/aimlab-insights.md`.

| Feature | What it shows | Where |
|---|---|---|
| **F13 Flicks by direction** | Hit rate and median reaction time per screen quadrant (up-right, up-left, down-left, down-right) and per near/far distance. Far means above `(min + max) / 1.9` of the scenario's target distances. It also gives the weaker axis and a weakest-direction rule. | `analytics::direction`, results card |
| **F14 Pace and stamina** | The kill or score rate in six windows of the run, the best and worst window, and slow-start and stamina-drop rules | `analytics::pace`, results card |
| **F15 Speed match and edge tracking** | The share of tracking time with the crosshair moving at the target's speed (too slow / too fast split), and the share of time spent on the target's edge | `analytics::tracking`, results card |
| **F16 Click timing** | The median delay from the crosshair entering the target to the shot, with a hesitation rule | `analytics::flick`, results card |
| **F17 Target prioritization** | In multi-target scenarios, the share of kills that took the nearest target first, and missed opportunities (the crosshair rested on a target without firing) | `analytics::priority`, results card |
| **F18 Passive sensitivity suggestion** | A multiplier built from the over/undershoot bias of your recent flicks, with the evidence behind it. Only the overall multiplier can be applied, through the same path as F6; the X/Y split is information only. | `analytics::sens_suggest`, Progress page |
| **F19 Replay extras** | A crosshair trail coloured by speed, a hit/miss heatmap, and a side-by-side comparison with an earlier run of the same scenario | `analytics::path`, replay viewer |
| **F20 Firing while moving and peek efficiency** | Speed at the shot, accuracy split into stopped and moving shots, and a counter-strafe check, for movement scenarios. In arenas: the time from first sight to the first shot, and the exposure time. | `analytics::moving`, `aim_world::arena`, results |
| **F21 Score rings** | A scoring block that pays more for hits near the center, used by `precision-rings` | `scenario` scoring, `content/scenarios/precision-rings.toml` |
| **F22 Go/no-go** | Decoys that must be left alone, with match/non-match hit and wait counts. `go-no-go` trains holding fire on the wrong target. | `scenario` targets, `analytics::decision`, `content/scenarios/go-no-go.toml` |

Every new threshold is provisional, set against the simulated aimer, and documented with its rationale under "Analytics metrics" in `training-science.md`.

## Second Aim Lab pass: C1–C14 (shipped 2026-10-18)

A second static pass (2026-10-18) looked at what F1–F22 and the roadmap do not cover. The richest source was Aim Lab's Creator Studio, a modular system of bot behaviours, conditions and triggers. Its VALORANT-specific code is thin (crosshair placement, movement accuracy, peek efficiency, a callout-based task generator), and we already cover it.

These were recorded first and then implemented at the owner's request (`odd/tasks/aimlab-candidates.md`). Size: S small, M medium. The evidence names are Aim Lab identifiers from the analysis; only the ideas were reused.

What shipped:
- **Scenario targets:** reactions (C1), behaviour rules (C2), spawn rules (C5), regeneration and score decay (C14). Built-ins: `react-duel`, `spread-flicks`, `regen-track`.
- **Phases, fail conditions and run end (C3):** `phase-peek-ladder`, `phase-kill-race`. The HUD shows the phase and the kills goal.
- **Perception (C4, C11):** `sound-turn`, `peripheral-detect`, `radar-recall`.
- **Analytics:** miss compass (C6), potential score with one tip (C8).
- **Training and settings:** compound daily missions with commitment levels (C9), enemy highlight colours with outline and tint (C12), sensitivity randomiser (C13).
- **Arenas:** reactive bots and behaviour rules (C1, C2), a shoot-back deadline (C10), live coaching overlay with automatic corner detection (C7). The arena is `corridor-reactive`; the overlay toggle is saved in settings.

Every threshold and constant without a source is marked as an estimate in `training-science.md`, `scenarios.md` or `maps.md`. Calibrate keeps every earlier scenario identical.

| # | Candidate | What it adds | Size | Likely owner | Evidence |
|---|---|---|---|---|---|
| C1 | **Reactive bots** | Bots copy or oppose the player's strafe (ADAD), dodge when the crosshair gets close, and react to shots. Our jukes run on fixed timers. | M | `scenario` motion, `aim_world` bots | `mimicPlayerDirectionChance`, `crosshairMaxDist`, `onShotFrequency` |
| C2 | **Conditional bot behaviour** | Switch behaviour on conditions such as health, recent damage, time alive, line of sight, distance or speed. Example: "holds the angle, strafes when hit, retreats under 50% health". | M | `scenario` def/session, `aim_world` | `ConditionalOverrideData`, `BotSwitchCondition` |
| C3 | **Multi-phase scenarios** | Chained waves, triggers on kills, accuracy or wave clear, fail conditions, and end types (time, kills, endless) | M | `scenario` def/session, content | `TriggerSpawnersOnKill`, `Wave`, `LEVEL_END_TYPES` |
| C4 | **Sound localisation drill** | Turn to footsteps or gunshots; angular error and reaction time in 8 directions | M | `scenario`, `analytics`, app audio | `AudioSpatialData` (`accB`/`rtB`/`errB` per 8 bins) |
| C5 | **Spawn-pattern controls** | Block the last or adjacent spawn points, enforce a minimum flick distance, exclude the middle | S | `scenario::def` spawn areas | `blockLastSpawn`, `blockAdjacentSpawnPoints`, `ignoreMiddlePercentage` |
| C6 | **Miss compass relative to the head** | Misses in 8 directions: above or below the head shows a crosshair-height problem | S | `analytics` | `missUp` … `missDownRight` |
| C7 | **Live coaching overlays in arenas** | Crosshair placement and movement accuracy shown while playing, with automatic corner detection | M | `aim_world`, app UI | `VTCSettingsData.show*InPratice`, `CornerFinder` |
| C8 | **Potential score with one tip** | "Fix overshoot and you would score about X", instead of a list of warnings | M | `analytics`, app UI | `PotentialScoreHandler`, `*ResultMetricTip` |
| C9 | **Compound missions** | Missions with AND conditions (for example accuracy ≥ 75% and 62 kills), mastery missions, and a low/medium/high commitment level | S | `training`, app UI | `ChallengeType`, `IsMastery`, `ProgressionCommitmentLevel` |
| C10 | **Bots that shoot back on a deadline** | Points are lost if a kill takes too long, giving duel pressure | M | `aim_world`, `scenario` scoring | `FireballShooter.scorePenalty`, `LifeTimeDespawn.scoreOnDespawn` |
| C11 | **Peripheral perception** | Detection in 8 screen areas with false alarms, and a minimap-glance recall drill | M | content, `analytics` | `DetectionData` (`rtB0..7`, `FATotal`), `RadarCapacityTaskManager` |
| C12 | **Colour-blind palettes and target visibility** | VALORANT's three enemy highlight colours, plus target outline/glow and environment colour | S | app settings and rendering | `TargetColorType`, `targetOutlineEnabled`, `GrayboxSettingsData` |
| C13 | **Sensitivity randomiser** | A robustness drill with bounded random sensitivity changes | S | `sensitivity`, app input | `sensitivityRandomizer*` |
| C14 | **Health regeneration and score decay** | Tracking targets regenerate and target value decays over time, rewarding staying on target and fast kills | S | `scenario` scoring | `HealthRegeneration`, `ScoreOverrides.interpolateTime*` |

**Suggested first batch:** C1, C5, C6, C9 and C12, which offer the most VALORANT value for a contained size. C1 with C2 would make bots believable opponents.

**Rejected:** sniper ballistics, bunny-hop or jetpack movement, controller aim assist, the server-side AI coach, leaderboards and percentiles (they need a player population), rhythm with licensed music, cosmetics and economy.

## Honest limits

- **Heuristic thresholds:** the readiness, fatigue and sensitivity-finder thresholds are documented constants built on the cited research, not validated per player. Every message shows the numbers behind it.
- **Insight thresholds (F13–F22):** these were tuned on the simulated aimer, which never moves, always takes the nearest target and never tires. Prioritization, stamina and moving-shot weaknesses are therefore only checked with synthetic tests until real runs exist. Speed match stays low (12–33%) on strafe scenarios even for strong simulated players.
- **Sensitivity suggestion (F18):** replays do not record the sensitivity they were played at, so runs at an older sensitivity or from the finder mix into the evidence.
- **Recoil:** Riot does not publish per-bullet recoil in degrees, so the climb is an estimate (see `docs/research/recoil.md`).
- **Transfer to the game:** as `docs/training-science.md` states, no controlled study yet shows aim-trainer transfer to in-game performance. The ranked-day features are designed to make practice *consistent and well-timed*, which the evidence does support.
