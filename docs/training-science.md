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
