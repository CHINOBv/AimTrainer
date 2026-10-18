# Aim Lab second-pass candidates C1–C14
Objective: implement the fourteen candidates recorded in `docs/feature-analysis.md` ("Candidates from a second Aim Lab pass"). Only ideas are reused, never Aim Lab code, assets or data.
Branch: `feat/aimlab-candidates` (worktree `../AimTrainer-worktrees/aimlab-candidates`) · Delivery: local merge to `main` (as in the previous feature) · Test runner: `cargo test --workspace` + `calibrate`

## Specs

The user's own words are quoted in `## Log` (L1, L2). The rows are copied from `docs/feature-analysis.md`.

- **S1 (C1) Reactive bots.** Bots copy or oppose the player's strafe (ADAD), dodge when the crosshair gets close, and react to shots. This applies to scenario targets and to arena bots.
- **S2 (C2) Conditional bot behaviour.** Switch behaviour on conditions such as health, recent damage, time alive, line of sight, distance or speed. Example: "holds the angle, strafes when hit, retreats under 50% health".
- **S3 (C3) Multi-phase scenarios.** Chained waves, triggers on kills, accuracy or wave clear, fail conditions, and end types (time, kills, endless).
- **S4 (C4) Sound localisation drill.** Turn to footsteps or gunshots; angular error and reaction time in 8 directions.
- **S5 (C5) Spawn-pattern controls.** Block the last or adjacent spawn points, enforce a minimum flick distance, exclude the middle.
- **S6 (C6) Miss compass relative to the head.** Misses in 8 directions: above or below the head shows a crosshair-height problem.
- **S7 (C7) Live coaching overlays in arenas.** Crosshair placement and movement accuracy shown while playing, with automatic corner detection.
- **S8 (C8) Potential score with one tip.** "Fix overshoot and you would score about X", instead of a list of warnings.
- **S9 (C9) Compound missions.** Missions with AND conditions (for example accuracy ≥ 75% and 62 kills), mastery missions, and a low/medium/high commitment level.
- **S10 (C10) Bots that shoot back on a deadline.** Points are lost if a kill takes too long, giving duel pressure.
- **S11 (C11) Peripheral perception.** Detection in 8 screen areas with false alarms, and a minimap-glance recall drill.
- **S12 (C12) Colour-blind palettes and target visibility.** VALORANT's three enemy highlight colours, plus target outline/glow and environment colour.
- **S13 (C13) Sensitivity randomiser.** A robustness drill with bounded random sensitivity changes.
- **S14 (C14) Health regeneration and score decay.** Tracking targets regenerate and target value decays over time, rewarding staying on target and fast kills.
- **S15 Docs.** Move C1–C14 from "not scheduled" to shipped in `docs/feature-analysis.md`. Update the README, `docs/scenarios.md`, `docs/training-science.md` and the help text where relevant.

Rules that hold for every spec:
- New scenario building blocks are serde with `deny_unknown_fields`, validated, and used by at least one new built-in scenario.
- Existing scenarios behave byte-identically (calibrate and sim tests); new RNG draws happen only when the new option is enabled.
- Every threshold or constant without a source is marked provisional or estimate.
- UI drawing, audio and input in `aim_app` have no automated tests and need a playtest.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | S15 (plan) | inline | done | this commit |
| T2 | S1, S2 (scenario targets), S5, S14 | delegated, wave 1 (worktree `cand-targets`) | pending | |
| T3 | S6, S8 | delegated, wave 1 (worktree `cand-analytics`) | pending | |
| T4 | S9, S12, S13 | delegated, wave 1 (worktree `cand-player`) | pending | |
| T5 | S1, S2 (arena bots), S7, S10 | delegated, wave 1 (worktree `cand-arena`) | pending | |
| T6 | S3 | delegated, wave 2, after T2 merges (worktree `cand-phases`) | pending | |
| T7 | S4, S11 | delegated, wave 2, after T2 merges (worktree `cand-perception`) | pending | |
| T8 | S1–S15 seam: merge, docs, full suite, calibrate, verifier, review, merge to main, build and launch | inline | pending | |

## Log

- L1 (2026-10-18) User, after the 14 candidates were listed: "Apuntalas, luego te aviso cuando implementarlas porfa"
- L2 (2026-10-18) User: "E implementa las feats de aimlabs que dejamos pendientes"
- L3 (2026-10-18) Routes. Writer trigger (a): independent units with disjoint surfaces, launched together.
  - The scenario core (`def.rs`, `session.rs`, `sim.rs`) is shared by S1/S2/S5/S14, S3, S4 and S11. To avoid three writers editing the same session logic in parallel, it is split into two waves: T2 first, then T6 and T7 on top of it.
  - Each worktree uses its own private cargo target (`cp -al` from main's target, then `cargo clean -p` for the workspace crates), never a shared one (lesson from the previous feature). The disk has 199 GB free.
  - RDD consent is granted automatically, per the user's standing preference.
