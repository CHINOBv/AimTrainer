# Aim Lab-inspired insights and drills
Objective: add ten training and analytics features inspired by a static analysis of Aim Lab. Only the ideas are reused, never code or assets.
Branch: `feat/aimlab-insights` (worktree `../AimTrainer-worktrees/aimlab-insights`) · Delivery: ask-on-risk, local merge to `main` on request; push is the user's decision · Test runner: `cargo test --workspace` + `calibrate` + Windows clippy check

## Specs

The user's own words are quoted in `## Log` (L1, L2). The ideas come from the feature table agreed in the conversation on 2026-10-18. Aim Lab evidence is in Engram (`Aim Lab-inspired feature candidates`, `Aim Lab SPAR 8-bucket meaning`).

- **S1 Per-direction flicks.** Each flick is classified by the screen-space vector from the previous aim point to the target:
  - quadrant: up-right (0°, 90°], up-left (90°, 180°], down-left (−180°, −90°], down-right (−90°, 0°];
  - distance class: near or far, where far means the distance is above `(min + max) / 1.9` of the scenario's possible target distances. Aim Lab uses this rule; we treat it as a tunable estimate, not a VALORANT constant.
  
  Report hit rate and median reaction time per bucket (4 × 2), plus the weak axis (horizontal vs vertical) from over/undershoot by axis. These feed the rules engine.
- **S2 Pace and stamina within a run.** Kill or score rate per time window, the best and worst window, and flags for a slow or strong start and a stamina drop (late windows below early ones). These feed the rules engine.
- **S3 Tracking smoothness and edge tracking.** Speed match: the share of time the player's angular speed is within a tolerance of the target's angular speed. Edge tracking: the share of on-target time spent near the target's edge rather than its center. Both are added to the tracking analysis and rules.
- **S4 Click timing.** Per shot on a target, the delay from the crosshair first entering the target to the shot. Report the median and a "hesitation" verdict through the rules.
- **S5 Target prioritization.** In multi-target scenarios: the share of kills where the player shot the nearest available target first, and the count of missed opportunities (a target was under the crosshair but the player did not fire).
- **S6 Passive sensitivity suggestion.** From the directional over/undershoot bias across recent runs, suggest X and Y sensitivity multipliers. Show the evidence (sample count, bias). Nothing is applied automatically; it complements the F6 sensitivity finder.
- **S7 Replay extras.** In the replay viewer:
  - a crosshair trail coloured by speed;
  - a hit/miss heatmap around the target;
  - a side-by-side comparison with an earlier run of the same scenario.
- **S8 Firing while moving and peek efficiency.**
  - Firing while moving: the player's movement speed at each shot, and accuracy split into stopped vs moving shots (a counter-strafe check), with a rule.
  - Peek efficiency, for peek drills: the time from exposure to the first shot, and the time the player stayed exposed.
- **S9 Score-zone rings.** A new scoring building block: points scale with the hit's distance from the target center, in configurable rings. It is serde with `deny_unknown_fields`, validated, and used by at least one scenario.
- **S10 Go/no-go task.** A new target kind or flag: "match" targets must be shot and "non-match" decoys must be left alone, with a penalty. Metrics: matchHit, matchWait, nonmatchHit, nonmatchWait. At least one built-in scenario, and it must pass calibrate and the sim tests.
- **S11 Record the plan.** This document lists every feature, and the project docs (`docs/feature-analysis.md`, README or the help text where relevant) describe what shipped.

Each analytics item explains itself: it shows the metric and the threshold behind each verdict (Phase 4 rule). Mouse, camera and rendering changes in `aim_app` have no automated tests and need a playtest.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S11 | inline | done | 516f9a8, fde78d9 |
| T1b | foundation for S1–S10 | inline (`MetricSources` refactor) | done | 8afb9f7 |
| T2 | S1, S4, S6 | delegated (parallel writer, worktree `ai-flicks`, branch `feat/ai-flicks`) | done (playtest pending) | 81424bd, 22561d6, 1aaebe8, merge 0b3b974 |
| T3 | S2, S5 | delegated (parallel writer, worktree `ai-pace`) | done (playtest pending) | 34fbb19, 947cd5c, merge 18d5f83 |
| T4 | S3 | delegated (parallel writer, worktree `ai-tracking`) | done (playtest pending) | 3d082d6, merge 6733cd8 |
| T5 | S7 | delegated (parallel writer, worktree `ai-replay`) | done (playtest pending) | f0f2d30, 27ce3d6, a0278b7, merge 3f31fb0 |
| T6 | S8 | delegated (parallel writer, worktree `ai-moving`) | done (playtest pending) | 54a98a3, 8e8ab96, beb1382, merge adf3fde |
| T7 | S9, S10 | delegated (parallel writer, worktree `ai-drills`) | done (playtest pending) | ed84148, 042faf3, 150b73c, merge 9724533 |
| T8 | S1–S10 seam: merge, docs, full suite, calibrate, build and launch | inline | done (playtest pending) | merges 6733cd8..0b3b974, fde78d9 |

## Log

- L1 (2026-10-18) User, after the Aim Lab analysis: "Hagamoslo, pero pimero quiero que busques mas features a implementar"
- L2 (2026-10-18) User, after the 10-feature table: "Apuntalos en algun lugar, implementemos todos"
- L3 (2026-10-18) Baseline `main` at 629895e. An explorer is mapping the code to choose disjoint edit surfaces. The routes are decided after that map.
- L4 (2026-10-18) Map done. Facts from the map:
  - `Recording::replay` drops `SessionEvent`s, so analyses read only `trace::Sample`. Player speed, score, kills and target kind must be added there.
  - The rules TOML is pinned by `builtin_rules_equal_the_former_rust_table`.
  - `SimAimer` always shoots the nearest target, so go/no-go needs a decoy-aware sim.
  - Mouse look uses one `deg_per_count` for both axes.
  - Arena runs keep no `Recording`.

  Writer trigger (a): six independent units, each several files of work, launched together in isolated sibling worktrees branched from 8afb9f7. They share hot spots: `trace.rs`, `RunAnalysis`, `rules.rs` with `recommendations.toml` and `FORMER_RULES`, `analysis_panel.rs`, `results_card.rs` and `docs/training-science.md`. Writers append there; the parent resolves those seams at merge. All units share `CARGO_TARGET_DIR=C:\Users\David\projects\AimTrainer\target`, because the disk has 51 GB free and one target is 18 GB.
- L6 (2026-10-18) The T5 writer found that worktrees sharing one `CARGO_TARGET_DIR` overwrite each other's workspace-crate artifacts, so a test run could execute another worktree's code. All writers now use private hard-linked targets (`cp -al`). The integration worktree uses `.target-integration`.
  - T4 speed match: weakness below 35% of firing time with the target moving ≥ 5°/s, within 25% speed with a 2°/s floor. Edge share: above 60% of on-target time farther than 70% of the radius. Strafe scenarios score 12–33% speed match even for the simulated elite; tune later.
  - T5 speed bands (slow < 30°/s, fast ≥ 300°/s) are display estimates. Heatmap grid: 15×15 over ±3 radii.
  - Integration verify at 3f31fb0 with a private target: fmt OK, clippy -D warnings clean, `cargo test --workspace` all passed (app 154, core 226, world 63 + 1 ignored, others).
- L5 S6 decision: the X/Y suggestion is shown as information; only the overall multiplier is applied, through the existing F6 path (`Sensitivity::from_cm_per_360`). No vertical-multiplier setting is added unless a sourced VALORANT setting exists in `docs/valorant-mechanics.md`.
- L7 (2026-10-18) Seam (T8). The six units were merged in order T4, T5, T7, T6, T3, T2.
  - Text conflicts were all append collisions; both sides were kept. In `analytics/mod.rs`, T2's `distance_split` was moved into the `RunAnalysis` constructor.
  - Semantic conflicts were test struct literals that lacked other units' fields. They were fixed with `..Default::default()` (`Sample` derives `Default`), `decoy: false`, or `None` fields.
  - Checks on the integration branch with a private target:
    - fmt OK;
    - clippy `-D warnings` clean;
    - `cargo test --workspace` all passed (app 172, core 297, world 66 + 1 ignored, integrations 3, analyze 14, gltf 6);
    - ignored arena golden passed;
    - calibrate identical to baseline for every pre-existing scenario. New: go-no-go 135/1180/2450/2880, contender accuracy 0.54; precision-rings 1715/3740/6895/11130, 0.68.
  - Docs: `docs/feature-analysis.md` (F13–F22 and honest limits), README (exercise count corrected 23→30 against main's 28 plus 2 new; insights; Progress row), `docs/roadmap.md` status line, recommendations.toml metric list.
  - Known gaps reported by writers:
    - flick segmentation sometimes treats a start blip as the primary sub-movement (pre-existing; S6 drops those flicks);
    - replays do not store their sensitivity;
    - the Progress tab re-simulates up to 20 replays in one frame when replays change;
    - a decoy shot plays miss feedback;
    - decoys are told apart by brightness, not hue.
  - Risk: item 6 (UI drawing in aim_app has no automated tests: results sections, replay viewer, decoy material, sensitivity panel). An independent verifier runs once at the feature end; playtest pending.
- L8 (2026-10-18) Independent verifier (Risk item 6) found no blockers.
  - S1–S7, S9, S10: pass. S11 was partial only because the T8 row was open (closed here). S8 was partial on scope.
  - S8 interpretation: peek efficiency is measured where the player peeks, i.e. the `aim_world` arena peek drills (jiggle-peek, angle clearing). In core scenarios such as `peek-reaction` or `dodge-peek`, the target peeks and the player never is exposed.
  - Invariants: calibrate is identical for all 28 pre-existing rows. A 9-scenario × 2-preset probe of the pre-existing Flick, FlickSummary, TrackingAnalysis, TrackingSummary, Stats and findings outputs matched the baseline (1181 lines); the only additions are the new-rule findings.
  - Follow-ups (advisories, not fixed):
    - the S1 weak axis and the S10 impulsive/hesitant verdicts are panel-only (no rules);
    - `decoy_penalty` without `decoy_chance` is accepted silently;
    - the Progress tab calls `replays::list` twice per frame;
    - the go/no-go nearest-target logic can pick a decoy;
    - L7's known gaps still apply.
- L9 (2026-10-18) Native review (RDD). The user asked for a review per unit after the whole-branch candidate hit `lens_context_budget_exceeded`.
  - Six slices were reviewed against the previous slice end (629895e..6733cd8, ..3f31fb0, ..9724533, ..adf3fde, ..18d5f83, ..69c118d), with the readability and reliability lenses. All six were approved and acknowledged (authority burned).
  - Non-blocking findings, kept as follow-up work.

  Likely user-visible:
  - (a) after Apply, the sensitivity panel recomputes from the new sensitivity and shows a never-applied "Suggested" value;
  - (b) prioritization counts a kill on the frame right after the previous kill as contested and nearest-first, which inflates the share;
  - (c) a decoy shot is attributed to the decoy in samples, so go-no-go flick metrics can count it as a missed flick;
  - (d) a correctly ignored decoy emits the same `Expired` event as a missed target;
  - (e) the replay viewer goes blank with no message when the selected entry leaves the list;
  - (f) the compare verdict uses the unrounded delta, e.g. "+0 pts ▲ better";
  - (g) the Progress tab scans the replays twice per frame and reloads all replays when the set changes.

  Maintainability:
  - labels duplicated with `Metric::label` (speed match, edge), and the `1.9` literal duplicated with `FAR_DIVISOR`;
  - `PaceAnalysis` methods panic if it is built by hand with fewer than 2 windows;
  - the weakest-direction gap can be negative;
  - the legend swatch positions are literals;
  - the duplicate expiry push;
  - the arena note wording;
  - unused fields (`accurate_share`, `MovingShot::time_secs`);
  - the private `Engagement` name clash in the arena;
  - the slow-start rule can fire on noise for weak players;
  - speed match can give a false weakness on strafe-heavy tracking;
  - a machine-specific path in this log.
