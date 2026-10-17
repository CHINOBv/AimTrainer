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

## Honest limits

- **Heuristic thresholds:** the readiness, fatigue and sensitivity-finder thresholds are documented constants built on the cited research, not validated per player. Every message shows the numbers behind it.
- **Recoil:** Riot does not publish per-bullet recoil in degrees, so the climb is an estimate (see `docs/research/recoil.md`).
- **Transfer to the game:** as `docs/training-science.md` states, no controlled study yet shows aim-trainer transfer to in-game performance. The ranked-day features are designed to make practice *consistent and well-timed*, which the evidence does support.
