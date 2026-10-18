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

## Honest limits

- **Heuristic thresholds:** the readiness, fatigue and sensitivity-finder thresholds are documented constants built on the cited research, not validated per player. Every message shows the numbers behind it.
- **Insight thresholds (F13–F22):** these were tuned on the simulated aimer, which never moves, always takes the nearest target and never tires. Prioritization, stamina and moving-shot weaknesses are therefore only checked with synthetic tests until real runs exist. Speed match stays low (12–33%) on strafe scenarios even for strong simulated players.
- **Sensitivity suggestion (F18):** replays do not record the sensitivity they were played at, so runs at an older sensitivity or from the finder mix into the evidence.
- **Recoil:** Riot does not publish per-bullet recoil in degrees, so the climb is an estimate (see `docs/research/recoil.md`).
- **Transfer to the game:** as `docs/training-science.md` states, no controlled study yet shows aim-trainer transfer to in-game performance. The ranked-day features are designed to make practice *consistent and well-timed*, which the evidence does support.
