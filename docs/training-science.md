# Training science behind routines and benchmarks

This page explains why the built-in routines and the benchmark look the way they do. Each claim is tagged:

- **verified**: comes from a primary source.
- **approximate**: comes from a secondary source.
- **inference**: our own design decision.

Research date: 2026-10-16.

## Skill taxonomy (Voltaic)

Voltaic S5 splits aim into three skills with three subcategories each (verified, https://app.voltaic.gg/blog/announcing-the-voltaic-season-5-aiming-benchmarks-beta-for-kovaaks/):

| Skill | Subcategories |
|---|---|
| Clicking | static, dynamic, linear |
| Tracking | precise, reactive, control |
| Switching | speed, evasive, stability |

Each subcategory is measured by 2 scenarios, and tiers reuse the same scenario families with harder parameters.

**Our mapping:** we keep the clicking, tracking and switching skills and add a VALORANT-specific **movement** skill, covering counter-strafe accuracy and angle holding. VALORANT asks for movement skills that generic trainers cannot simulate (inference).

## Benchmark scoring (Voltaic energy)

These three rules are verified (https://app.voltaic.gg/leaderboards/about):

- A subcategory's energy comes from its best scenario.
- Overall energy is the **harmonic mean** of the subcategory energies.
- There is no overall energy until every subcategory has a score.

Two details are approximate (Season 4 data):

- Each rank is worth 100 energy.
- Energy is interpolated between thresholds.

**Implementation:** `aim_core::benchmark` follows these rules.

**Provisional thresholds:** we cannot reuse KovaaK's thresholds, because our scenarios and scoring are different. The current thresholds are averaged scores from 4 simulated aimer profiles (`sim.rs`, regenerate with `cargo run -p aim_core --release --example calibrate`). The aimer model uses:

- **Fitts' law movement time:** MT = a + b·log₂(D/W + 1).
- A reaction delay.
- Gaussian endpoint error.
- First-order lag for tracking.

The intermediate thresholds are those scores rounded to 50, with a few hand-rounded values. The profile parameters are assumptions, which is why the benchmark is marked `calibrated = false`.

**Tiers (estimate):** like Voltaic, the benchmark has novice, intermediate and advanced tiers that reuse the same scenario families with other parameters (inference: we change parameters through the difficulty level, not by authoring new scenarios). Each tier plays every scenario at the level of the matching weekly-plan tier (novice −0.5, intermediate 0, advanced +0.5, see `ScenarioDef::at_difficulty`) and stores scores under the run key (`micro-adjust@-0.50`), so tier scores never mix. A training tier is measured on the benchmark tier at its level (`MethodDef::benchmark_for`); the tier suggestion still reads the intermediate tier. The novice and advanced thresholds are estimates, derived the same way as the intermediate ones:

1. Average the score of each simulated profile over seeds 1–5 at the tier's level.
2. Round each score to 50; that is the threshold of the profile's rank.
3. Saturation: when two or more top profiles score within 1% of the elite score (the scenario's ceiling), space those ranks evenly between the last unsaturated threshold and 50 below the rounded elite score, so thresholds stay strictly ascending. This applies to novice `track-precise` and `track-reactive-micro`.

Thresholds are **not** ordered novice < intermediate < advanced for a rank, and should not be. An easier level usually raises every profile's score (novice thresholds are mostly higher), but not always: on `track-reactive-strafe`, `track-close-strafe` and parts of `peek-reaction` the simulated scores rise with the level, and saturated tracking scenarios cap out. The ranks of different tiers are therefore not comparable by score; each tier only ranks players against the profiles at its own level. The enforced invariants are: tiers are listed easiest first (levels strictly increase), one tier has level 0, and every tier's thresholds are positive and strictly ascending, one per rank. To recalibrate from real runs, place the thresholds at percentiles of real player scores, spaced about 8–15% apart, which matches the step between Voltaic ranks (approximate).

## Routine design

| Principle | Evidence | Where it shows up |
|---|---|---|
| Sessions of about 30–60 min a day | Listman et al. 2021 studied 7,174 Aim Lab players and 682,564 runs. Gains peaked at about 1 h/day, and about 90% of the benefit came within 30–50 min (verified, https://pmc.ncbi.nlm.nih.gov/articles/PMC8720934/) | `valorant-daily` lasts about 37 min; `fundamentals-hour` about 52 min |
| Interleave instead of blocking | Shea & Morgan 1979: random-order practice performs worse during acquisition but retains and transfers better (verified, https://www.gwern.net/doc/psychology/spaced-repetition/1979-shea.pdf). The effect is smaller in applied settings (approximate, Brady 2004) | `interleave_from` in a routine shuffles its runs and avoids repeating the same scenario back to back |
| Warm up before measuring | Warm-up decrement after rest (verified, Lee & Genovese 1988) | Routines start with ordered warm-up blocks; `quick-warmup` is for before you queue |
| Spread practice across days, and sleep | Distributed practice helps continuous tasks most (Lee & Genovese 1988). Sleep consolidates motor skills (approximate, Walker et al. 2002). Voltaic recommends 8–9 h of sleep (verified, https://app.voltaic.gg/blog/lifestyle) | Routine descriptions recommend 5–6 days a week |
| Score speed, not only accuracy | In Listman 2021, accuracy plateaued at about 85% while hits per second kept improving | Clicking scores reward kills per minute, and miss penalties keep accuracy honest |
| Benchmark rarely, train often | Voltaic: benchmarks measure, routines train (verified, https://docs.google.com/document/d/1BPiDxbaqQVxInKwFfdr_AJdJDYVGKbUBGHJDKOeUy-Q/mobilebasic) | The benchmark description says to run it once a week |
| Order: tracking → clicking → switching | Voltaic's skill routines use this order (verified, same doc) | `fundamentals-hour` follows it |

## VDIM (Voltaic Daily Improvement Method)

This is the structure behind our primary training method (roadmap Phase 2). It was researched on 2026-10-17.

**Origin.** Created by Lowgravity56 and introduced on 2022-06-16 (verified, https://www.youtube.com/watch?v=XMxIHLXJhY8). The S5 rework was published on 2025-03-06 and is credited "by Lowgravity56 & 4rK" (verified, https://www.youtube.com/watch?v=ZEH4CfytNyo and the S5 doc below).

**Goal.** In the S5 doc's own words: "isolating specific aiming subcategories each day of the week to build proper technique, and also to be able to beat your previous high scores because of the gradual increments of difficulty leading to the benchmark scenarios on each playlist" (verified, https://docs.google.com/document/d/1R4IyJqYmprRauaACt6bah7YzOOVuG6GqXlB03clNHeU).

**Week (verified, S5 doc):**

| Day | Skill |
|---|---|
| Monday | Clicking I (static + linear) |
| Tuesday | Clicking II (dynamic + linear) |
| Wednesday | Tracking I |
| Thursday | Tracking II |
| Friday | Switching I |
| Saturday | Switching II |
| Sunday | Benchmark focus routine (recommended), replay a weak day, or rest |

**Inside a day (verified, from KovaaK's public playlist data):**

- A day has 3 blocks, one per benchmark scenario.
- Each block runs in this order:
  1. 3–7 related drills, 2–3 runs each.
  2. 2–7 benchmark variants, ending with a "Hard" one, 2–3 runs each.
  3. The benchmark itself, 4 runs.
- A day totals 72–80 runs.
- The doc says the playlist is "roughly an hour and a half long" and should be played "in the order they are presented".

**Tiers (verified, S5 doc).** There are six tiers, picked from your Voltaic rank. They are recommendations; move up or down if a tier is too easy or too hard.

| Tier | Recommended rank band | Ends on |
|---|---|---|
| Entry | Unranked to Bronze | Novice benchmarks |
| Novice | Bronze Complete to Gold | Novice benchmarks |
| Adept | Gold Complete to Diamond | Intermediate benchmarks |
| Intermediate | Diamond Complete to Master | Intermediate benchmarks |
| Advanced | Master Complete to Nova | Advanced benchmarks |
| Elite | Aiming for Astra and above | Advanced benchmarks |

**Progression.** VDIM sets no pass or fail thresholds of its own. Progress is measured by the benchmark runs included each day; no separate weekly benchmark day is needed (verified, S5 doc).

**Rationale given by its authors.** Frustration signals that something must change, and noticing your errors matters. Sleep is essential to learning. One technique per day keeps a consistent visual theme and FOV (verified, S5 doc). The doc itself cites no peer-reviewed studies.

**How it relates to the rest of this page.** VDIM blocks one skill per day for about 90 minutes:

| Evidence | What it suggests | How it compares with VDIM |
|---|---|---|
| Listman et al. 2021 | Daily gains peak at about 1 h, with 90% of the benefit in 30–50 min | VDIM's ~90 min day is longer |
| Shea & Morgan 1979 | Mixing exercises within a session aids retention | VDIM uses one skill per day |

Spreading the skills across the week does give VDIM spacing between sessions. Our implementation keeps VDIM's structure, recommends splitting long days into 2–3 sessions (the S5 doc allows this), and keeps the interleaved `valorant-daily` as an alternative.

**Reuse.** No license or reuse terms were found on the VDIM docs (verified). We reproduce the method with original scenarios and credit it; we do not copy scenario files or use the names as our own branding.

## Difficulty

- **Challenge Point Framework** (Guadagnoli & Lee 2004): the best difficulty depends on the learner's skill (verified).
- **"Eighty Five Percent Rule"** (Wilson et al. 2019): for gradient learners the ideal error rate is about 15% (verified, https://pmc.ncbi.nlm.nih.gov/articles/PMC6831579). Applying it to aiming is extrapolation (inference).
- **Applied here:** we tuned the built-in scenarios so the mid-level simulated aimer ("contender") lands at roughly 45–85% accuracy. An adaptive staircase that holds about 80% success is planned (see the README roadmap).

## Sensitivity

- Voltaic says changing sensitivity is not harmful for practice, and that a change can help break a plateau (verified, https://app.voltaic.gg/blog/muscle-memory).
- No controlled evidence supports or rejects sensitivity randomization.
- **Our default:** train at your exact VALORANT sensitivity. The trainer converts it to the same cm/360.

## Analytics metrics

Post-run analysis (`aim_core::analytics`) re-simulates the recorded run and measures it. Every number below is shown with the metric that produced it. **All thresholds are provisional:** they are estimates chosen against the simulated aimer, not calibrated on player data, and will be revised once real runs exist.

**Sources.**

- **Fitts' law** (Fitts 1954, *J. Exp. Psychol.* 47(6), 381–391): movement time grows with the index of difficulty `log2(D/W + 1)` (Shannon form, MacKenzie 1992, *Human–Computer Interaction* 7, 91–139). The simulated aimer already uses it.
- **Effective throughput, ISO 9241-9** (Soukoreff & MacKenzie 2004, *Int. J. Human-Computer Studies* 61, 751–789): `We = 4.133 · SD` of the endpoint error along the movement axis, `IDe = log2(De/We + 1)` with `De` the mean effective distance, `TP = IDe / MT` in bits/s.
- **Optimized sub-movement model** (Meyer, Abrams, Kornblum, Wright & Smith 1988, *Psychological Review* 95(3), 340–370): an aimed movement is a primary sub-movement followed by zero or more corrective sub-movements; the primary endpoint error shows the speed–accuracy trade-off.
- Confidence: the citations are standard references (not re-checked online in this pass). Applying them to mouse flicks in an aim trainer, and the thresholds, are our inference.

**Flick metrics (click scenarios).** A flick runs from when its target could first be aimed at (it became visible, or the previous shot was fired, whichever is later) to the shot.

| Metric | Definition |
|---|---|
| Reaction time | Start → movement onset. Onset is the first frame whose angular speed reaches 10% of the flick's peak (floor 5°/s), a common kinematic criterion (estimate). Fresh flicks only; a retry after a miss is a correction. Onset detection adds ~45 ms to the simulated aimer's nominal delay |
| Movement time | Onset → shot |
| Sub-movements | A new sub-movement starts when speed drops to a valley ≤ 50% of the preceding peak and rises again by ≥ 20% of that peak, or after the crosshair stops (< floor) and moves again. Corrections = sub-movements − 1 |
| Overshoot / undershoot | Signed crosshair error at the end of the primary sub-movement, along the start→target direction, in target radii. > +1 radius (ended past the far edge) is an overshoot, < −1 an undershoot |
| Settle time | End of the primary sub-movement → shot |
| Path efficiency | Straight-line angle start→shot ÷ angle travelled (1 = perfectly straight) |
| Throughput | ISO 9241-9 effective throughput over fresh flicks (needs ≥ 5). Mixed distances per run make it an approximation of the per-condition ISO value |

**Tracking metrics (hold scenarios).**

| Metric | Definition |
|---|---|
| Time on target | Seconds on a counted zone (the session's own hit test) ÷ seconds firing with a target visible |
| Error from center | Angle from the crosshair to the aim point (sphere center, humanoid head center) |
| Lag vs lead | Sign of the crosshair error projected on the target's angular velocity (target moving ≥ 5°/s). Lag ratio = time behind ÷ time moving |
| Reaction to direction changes | After the target's direction of motion reverses (dot product < 0), the time until the crosshair moves the new way. Responses over 1 s are dropped as lapses |

**Rules (provisional).** A category needs ≥ 5 flicks or ≥ 3 s of firing before it is judged. The rules are data: `content/rules/recommendations.toml` (`aim_core::analytics::rules`), one `[[rules]]` entry per row with `metric`, `direction` (`above`/`below`, strict), `threshold`, `verdict` (`strength`/`weakness`), `meaning` and an optional `[rules.recommendation]` (`scenario_ids` best first, optional `routine_id`, `tip`). Validation rejects unknown fields, non-finite thresholds and scenario or routine ids that do not exist. The table below mirrors that file.

| Metric | Weakness | Strength | Recommended practice |
|---|---|---|---|
| Median reaction time | > 300 ms | < 220 ms | `peek-reaction`, `quick-warmup` routine |
| Overshoot rate | > 35% | < 10% | `micro-adjust`, `static-headline`; slow flicks slightly, check sensitivity |
| Undershoot rate | > 35% | — | `flick-wide`; commit to the full distance |
| Corrections per flick | > 1.0 | — | `micro-adjust`, `flick-headshot` |
| Path efficiency | < 85% | — | `flick-headshot` |
| Time on target | < 50% | > 75% | `track-precise` |
| Lag ratio | > 65% (trailing) or < 35% (over-leading) | — | `track-precise`, `track-reactive-micro` |
| Reaction to direction changes | > 250 ms | — | `track-reactive-strafe`, `track-reactive-micro` |
| Shots fired while moving | > 20% (≥ 10 shots) | — | `counter-strafe`, `strafe-shot` |

The 300 ms and 220 ms reaction thresholds bracket the simulated recruit (0.32 s + onset) and elite (0.20 s + onset); the rest are estimates.

**Skill profile per benchmark subcategory.** `aim_core::analytics::skills::BenchmarkProfile` takes the benchmark definition and the run history. Each subcategory's energy comes from personal bests with the benchmark's own energy rules (above). It is compared with the arithmetic mean over the subcategories played (at least 2): more than 15% below the mean is a weakness, and the profile offers that subcategory's scenarios to practice; more than 15% above is a strength. The 15% margin is an estimate and is not calibrated. The weakest subcategory matters most because overall energy is a harmonic mean.

**Trends.** `training::progress::subcategory_trend` gives each subcategory's energy at the end of each of the last weeks, using the same weekly cutoffs as the overall `benchmark_trend` (personal bests up to each cutoff). The Progress page charts one trend per subcategory once it has two played weeks.

### Speed match and edge tracking (hold scenarios)

Two tracking metrics in `aim_core::analytics::tracking` describe *how* the crosshair follows the target, not only whether it is on it. Both constants and both rule thresholds are **provisional estimates**, chosen against the simulated aimer (`track-*` scenarios, presets recruit to elite), not calibrated on player data. The idea of measuring them comes from a static analysis of Aim Lab's tracking feedback; no Aim Lab code, numbers or assets are reused.

| Metric | Definition |
|---|---|
| Speed match | While firing at a target moving ≥ 5°/s, the share of time the crosshair's angular speed is within a tolerance of the target's angular speed. Tolerance = `max(25% of the target speed, 2°/s)`: relative, so fast targets allow more slack, with an absolute floor so a slow target does not demand an exact speed. The rest of that time is split into **too slow** (crosshair slower than the band) and **too fast** (faster) |
| Edge share | Of the firing time on a counted zone with the crosshair inside the aim zone's radius (sphere, humanoid head), the share farther than 70% of the radius from the aim point. Hits on another zone (a humanoid's body) are excluded. If the crosshair were spread uniformly over the disk, about 51% of the time would be past 70% of the radius |

Both are per-frame comparisons, so mouse polling jitter lowers speed match slightly. Speeds are compared as magnitudes; direction is covered by lag/lead and reversals. Abrupt strafes (`track-reactive-strafe`, `track-close-strafe`) give low speed match even for the simulated elite (about 27–33%), because the target's speed jumps faster than any follower can match.

| Metric | Weakness | Recommended practice |
|---|---|---|
| Speed match | < 35% | `track-orbit`, `track-precise`; match the target's speed before correcting position |
| Edge share | > 60% | `track-precise`, `track-orbit`; steer back to the center as soon as you are on the target |

Simulated values (seed 13): speed match ranges from 60–87% on `track-precise` and `track-orbit` to 12–33% on strafe scenarios, rising with the aimer's skill; edge share falls from 35–71% (recruit to veteran) to under 1% for the elite on the smooth scenarios.

### Go/no-go decisions and score rings

**Go/no-go.** `go-no-go` mixes match targets, which must be shot, with faint decoys that must be left alone until they vanish. This is the shape of the go/no-go task that cognitive psychology uses to measure response inhibition; no source in this repository yet backs it, so treat it as background, not a verified claim. The in-game link is a judgement call (an estimate, not a measured transfer): a duel often rewards confirming friend or foe, or a real peek against a fake one, before clicking. `aim_core::analytics::decision` reports four counts from the traced run (matchHit, matchWait, nonmatchHit, nonmatchWait), decision accuracy, and two verdicts with their thresholds: decoys shot above 20% of all decoys reads as impulsive, and match targets missed above 20% reads as hesitant. Both thresholds are estimates.

The simulated aimer adds a recognition delay of half its reaction time before each movement, since a choice takes longer than a simple reaction (Hick, 1952, *Quarterly Journal of Experimental Psychology* 4(1); the factor of 0.5 is an estimate). It mistakes a decoy for a match with a chance of 0.25 × its endpoint noise (recruit ≈ 22%, elite ≈ 9%), also an estimate. Scenarios without decoys draw no extra random numbers, so their results are unchanged.

**Score rings.** `precision-rings` pays more for hits near the center (×3, ×2, ×1). The aim is to reward landing on the exact point aimed at rather than anywhere on the target. That this helps in-game headshot precision is an estimate with no study behind it.

### Firing while moving (movement scenarios)

`aim_core::analytics::moving` measures every shot of a scenario with `player.movement = true`. A shot is *moving* when the player was above the weapon's accuracy speed limit when it was fired (27.5% of run speed for rifles, verified in `valorant-mechanics.md`); otherwise it is *stopped*. The session already decides this; the analysis reuses its flag.

| Metric | Definition |
|---|---|
| Speed at the shot | Median player speed (m/s) over all shots |
| Stopped vs moving | Share of shots fired at or below the limit, and above it |
| Hit rate split | Hit rate of the stopped shots and of the moving shots, separately |
| Counter-strafe check | A shot *follows movement* when the player was above the limit at any frame within the last **0.5 s** (or at the shot). Counter-strafed share = shots that follow movement and were fired already accurate ÷ shots that follow movement |
| Stop-to-shot time | For counter-strafed shots: time from the first accurate frame after the last moving frame to the shot (median) |

The 0.5 s window is an estimate: it covers the measured stop (0.16 s counter-strafing, 0.17 s releasing, `valorant-mechanics.md`) plus a reaction margin, so a shot right after a stop counts and a shot after standing still does not.

**Rule (provisional).** Shots fired while moving > 20% is a weakness and recommends `counter-strafe`, then `strafe-shot`. A movement category needs ≥ 10 shots before it is judged. The 20% threshold is an estimate, not calibrated.

### Peek efficiency (arenas)

`aim_world::arena::ArenaStats` times each *engagement*: the first exposure of a bot life, from the first frame any part of its head is in the player's line of sight (the same first sight that feeds crosshair placement and time to kill).

| Metric | Definition |
|---|---|
| First shot delay | Time from first sight to the player's first shot during that exposure, at any target. An exposure without a shot adds nothing |
| Exposure time | Time from first sight to the kill, to the first frame without line of sight, or to the player's death. An exposure still open when the run ends adds nothing |

The results notes show the medians. Line of sight is checked from the player's eye to the bot's head, so the exposure approximates how long the player could be seen. Later re-peeks by the same bot life are not separate engagements. These are measurements only; no rule judges them yet.

### Pace and stamina

`aim_core::analytics::pace` splits every run into 6 equal time windows (5–10 s for 30–60 s runs) and measures the rate in each: kills per second for click scenarios, score per second for hold scenarios, where score accrues while on target. A frame's gain belongs to the window in which the frame started. The results screen shows the best and worst window with their time ranges, and the rate in each window.

| Ratio | Definition | Flag (provisional) |
|---|---|---|
| Opening pace | Rate of the first window ÷ mean rate of the other five | Slow start below 70% (weakness rule); strong start above 130% (shown only, no rule, because the rest of the run is then slower) |
| Late pace | Mean rate of the last two windows ÷ mean rate of the first two | Stamina drop below 80% (weakness rule) |

A run needs at least 12 s and, in click scenarios, at least 12 kills (two per window on average), or it has no pace section. A category's ratio is the mean over its runs. All three thresholds are estimates, not calibrated on player data: the simulated aimer does not tire, so its late pace stays at or above about 90%, but its slowest preset shows opening ratios of about 50–65% from noise alone at 3–8 kills per window. Treat one slow-start verdict as a hint and look for it across several runs. The flag thresholds are constants in `pace.rs`, and a test pins them to the rules in `content/rules/recommendations.toml`.

| Metric | Weakness | Recommended practice |
|---|---|---|
| Opening pace | < 70% | `flick-wide`, `track-precise`, `quick-warmup` routine; warm up before scored runs |
| Late pace | < 80% | `static-headline`, `track-precise`, `fundamentals-hour` routine; relaxed grip, short breaks |

### Target prioritization

`aim_core::analytics::priority` runs only when more than one target was visible at once in some frame that is not a kill frame (a kill frame keeps the dead target next to its respawn, so single-target scenarios never qualify). The results screen shows a "Target priority" section.

| Metric | Definition |
|---|---|
| Nearest target first | For each kill, the engagement starts on the frame after the previous kill, or when the killed target appeared, whichever is later. If two or more targets were visible then, the kill is *contested*, and it counts as nearest-first when the killed target was the one nearest to the crosshair by angle in that frame. Share = nearest-first kills ÷ contested kills |
| Missed opportunities | The crosshair rests inside the nearest target's aim zone (angle to its aim point ≤ its angular radius) for at least 300 ms with no shot and fire not held, and then leaves while the target is still there. A target that is killed or expires under the crosshair, or a rest that lasts to the end of the run, does not count. Rate = missed ÷ (kills + missed) |

The session's own `on_target` flag only exists while firing, so the "under the crosshair" test here uses the aim zone geometry instead. The 300 ms rest is an estimate: it is longer than a typical visual reaction time (about 200–250 ms), so the normal delay between landing on a target and clicking is not counted. A category's counts are summed over its runs. The share needs at least 5 contested kills and the rate at least 5 kills plus missed opportunities. The thresholds are provisional, not calibrated on player data. The simulated aimer always takes the nearest target (share 100%) and almost never rests without firing (rate 0–2%), so it only shows the strength.

| Metric | Weakness | Strength | Recommended practice |
|---|---|---|---|
| Nearest target first | < 50% | > 85% | `switch-wide`, `switch-speed`; clear the closest target first |
| Missed opportunities | > 15% | — | `static-headline`, `switch-heads`; fire as soon as the crosshair is on the target |

## Caveat

No controlled study yet shows that aim-trainer practice transfers to in-game performance (verified as an absence of evidence). What the evidence does show is in-trainer learning and reliable measurement (Rogers et al. 2024: https://pmc.ncbi.nlm.nih.gov/articles/PMC10925653/). Use the trainer alongside real games, such as Deathmatch with the crosshair kept at head level.
