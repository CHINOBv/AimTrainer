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
| T1 | S15 (plan) | inline | done | c60ad3d |
| T2 | S1, S2 (scenario targets), S5, S14 | delegated, wave 1 (worktree `cand-targets`) | done (playtest pending) | 97fb221, e8daca6, 162d520, merge bb438cd |
| T3 | S6, S8 | delegated, wave 1 (worktree `cand-analytics`) | done (playtest pending) | c022ad6, a67133e, merge a5eda13 |
| T4 | S9, S12, S13 | delegated, wave 1 (worktree `cand-player`) | done (playtest pending) | b6bdc75, 512d1a5, d536110, merge 183c35f |
| T5 | S1, S2 (arena bots), S7, S10 | delegated, wave 1 (worktree `cand-arena`) | done (playtest pending) | 69d1ea3, 6447b42, d3ab727, dcb29c6, merge 6640289, f877a73 |
| T6 | S3 | delegated, wave 2, after T2 merges (worktree `cand-phases`) | done (playtest pending) | 2109708, ac6da2a, 26e9958, 16701b5, merge 9d9ce59, 7752512 | |
| T7 | S4, S11 | delegated, wave 2, after T2 merges (worktree `cand-perception`) | done (playtest pending) | 78794bf, d6d846b, merge b92e640 | |
| T8 | S1–S15 seam: merge, docs, full suite, calibrate, verifier, review, merge to main, build and launch | inline | pending | |

## Log

- L1 (2026-10-18) User, after the 14 candidates were listed: "Apuntalas, luego te aviso cuando implementarlas porfa"
- L2 (2026-10-18) User: "E implementa las feats de aimlabs que dejamos pendientes"
- L3 (2026-10-18) Routes. Writer trigger (a): independent units with disjoint surfaces, launched together.
  - The scenario core (`def.rs`, `session.rs`, `sim.rs`) is shared by S1/S2/S5/S14, S3, S4 and S11. To avoid three writers editing the same session logic in parallel, it is split into two waves: T2 first, then T6 and T7 on top of it.
  - Each worktree uses its own private cargo target (`cp -al` from main's target, then `cargo clean -p` for the workspace crates), never a shared one (lesson from the previous feature). The disk has 199 GB free.
  - RDD consent is granted automatically, per the user's standing preference.
- L4 (2026-10-18) Wave 1 merged into integration. Order: T3, T4, T2, T5. The only conflicts were append collisions in `training-science.md`, `hud.rs` and `menu.rs`; both sides were kept.
  - Checks with a private integration target: all tests pass (app 188, core 356, world 100 + 1 ignored); clippy clean; arena golden passes.
  - T2 calibrate: the 30 pre-existing rows are identical. New rows:
    - spread-flicks 2612/4404/6488/8540, contender accuracy 0.66;
    - regen-track 1469/2089/2936/4715, 0.58;
    - react-duel 5670/8424/12494/17290, 0.79 (the simulated aimer understates reactive difficulty).
  - T5 left the coaching opt-in unsaved, because the settings file was outside its surface. The parent moved it into `Settings.arena_coaching` (f877a73, test first).
  - Writer decisions:
    - T4 offers one yellow plus custom RGB (the exact VALORANT shades are unpublished) and an outline instead of a glow (targets are unlit);
    - T2 implements "adjacent spawn points" as an angle around recent spawns (spawns are positions, not cells);
    - T5 rule "speed" means the player's speed.
  - Wave 2 (T6 phases, T7 perception) started from bb438cd.
- L5 (2026-10-18) Wave 2 merged. T6 merged cleanly. T7 conflicted with T6 in `session.rs`, and seven hunks were resolved by hand:
  - imports unioned;
  - both `SessionEvent` variants kept;
  - both state field sets kept;
  - the update tail now applies run rules, refills when not finished, then plays cues;
  - `refill` runs the radar round first, then the wave-limited loop with `spawn_target(None)`;
  - `wave_has_room` was kept before T7's helpers.

  One semantic fix: `validate()` checked perception before `errs.extend(run_rule_problems())`, because the closure still borrowed `errs`.
  - Checks after merge: tests all pass (app 202, core 411, world 100 + 1 ignored); clippy clean; arena golden OK.
  - Calibrate against main's baseline: all 30 earlier rows identical, plus 8 new rows, each recruit < contender < veteran < elite with contender accuracy 0.58–0.79.
  - T6 left the HUD without a phase display, so the parent added `progress_label` (phase N/M · name, kills x/goal) test-first (7752512).
  - Docs: `feature-analysis.md` marks C1–C14 shipped; the README exercise count is now 38, with new drills, arena options and insights. Each writer documented its own fields and estimates.
  - Writer notes:
    - T6: kills and endless runs still stop at `duration_secs` as a cap; `phase-peek-ladder` fails at 5 misses (3 failed the simulated contender too early).
    - T7: Bevy spatial audio cannot tell front from back, so cues are panned in stereo with a muffled rear; a detection click counts anywhere on screen.
