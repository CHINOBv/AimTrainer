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
| T1 | S11 | inline | done | this commit |
| T1b | foundation for S1–S10 | inline (`MetricSources` refactor) | done | 8afb9f7 |
| T2 | S1, S4, S6 | delegated (parallel writer, worktree `ai-flicks`, branch `feat/ai-flicks`) | in progress | |
| T3 | S2, S5 | delegated (parallel writer, worktree `ai-pace`) | in progress | |
| T4 | S3 | delegated (parallel writer, worktree `ai-tracking`) | in progress | |
| T5 | S7 | delegated (parallel writer, worktree `ai-replay`) | in progress | |
| T6 | S8 | delegated (parallel writer, worktree `ai-moving`) | in progress | |
| T7 | S9, S10 | delegated (parallel writer, worktree `ai-drills`) | in progress | |
| T8 | S1–S10 seam: merge, docs, full suite, calibrate, build and launch | inline | pending | |

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
- L5 S6 decision: the X/Y suggestion is shown as information; only the overall multiplier is applied, through the existing F6 path (`Sensitivity::from_cm_per_360`). No vertical-multiplier setting is added unless a sourced VALORANT setting exists in `docs/valorant-mechanics.md`.
