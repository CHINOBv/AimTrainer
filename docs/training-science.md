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

The profile parameters are assumptions, which is why the benchmark is marked `calibrated = false`. To recalibrate from real runs, place the thresholds at percentiles of real player scores, spaced about 8–15% apart, which matches the step between Voltaic ranks (approximate).

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

## Caveat

No controlled study yet shows that aim-trainer practice transfers to in-game performance (verified as an absence of evidence). What the evidence does show is in-trainer learning and reliable measurement (Rogers et al. 2024: https://pmc.ncbi.nlm.nih.gov/articles/PMC10925653/). Use the trainer alongside real games, such as Deathmatch with the crosshair kept at head level.
