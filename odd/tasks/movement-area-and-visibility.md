# Movement area, floor height and off-screen targets
Objective: make the movement scenarios' play area wider and visible, allow more room backwards and a chosen floor height, and stop wide-flick targets from being lost off-screen.
Branch: `feat/movement-area-and-visibility` (worktree `../CustomAimTrainer-worktrees/movement-visibility`) · Delivery: one PR · Test runner: `cargo test --workspace` + calibrate

## Specs

- S1 (verbatim): "El tema son las paredes invisibles, hazlas un poco mas anchas y visuales pero a nivel del suelo no tan altas para que no interfieran porfa"
  - The play area of the movement scenarios is wider, and its edges are drawn as low strips at floor level.
- S2 (verbatim): "tambien haz que pueda moverme mas hacia atras"
  - The play area extends further towards +Z (behind the start).
- S3 (verbatim): "y pueda escojer cierto desnivel del suelo si es posible ya dentro del escenario"
  - From inside a run (pause menu), the player picks a floor height relative to the targets; the run restarts at that height, and its scores are kept apart from other heights.
- S4 (verbatim): "en los wide flicks, es complicado saber a donde se movio el objetivo cuando sale de mi campo de vision, investiga como podemos suplir o arreglar esto sin afectar el proposito del escenario"
  - Finding: `flick-wide` spawns 20–70° from the crosshair, but VALORANT shows ±51.5° horizontally and ±35.3° vertically (103° × 70.53°), so many targets spawn off-screen.
  - Fix: crosshair-relative spawns can require the target to appear inside VALORANT's view (with a margin); `flick-wide` and `flick-headshot` use it with offsets up to the screen edge, so they stay wide flicks.
  - Backup: an optional arrow at the screen edge points to targets outside the view (setting, on by default).
- S5 (verbatim): "En los escenarios de arenas, al entrar a una arena me teletransporta jaja, yo esperaria poder moverme al rededor del mapa"
  - Entering an arena must not teleport the player again and again; the player moves freely around the map within its walls.
- S6 (verbatim): "y al terminar y darle retry desaparecen todas las texturas de las paredes, incluso si me salgo y entro a otra arena diferente"
  - Cause (found): `spawn_map` runs only on `resource_added::<ArenaRun>`, `clear_map` runs on leaving Playing, and a retry or a different arena replaces the existing `ArenaRun` instead of adding it, so the map is never drawn again.
- S7 (verbatim, 2026-10-17): "Porfa todo lo que puedas hacer en paralelo hazlo en paralelo"

## Tasks

| ID | Specs | Route | Status | Commit |
|----|-------|-------|--------|--------|
| T1 | S4 | delegated (parallel, isolated worktree B) | done | 5c62916, 22004d3 |
| T2 | S1, S2, S3 | delegated (parallel, isolated worktree C) | done | d9eab06, 0e13a96 |
| T3 | S5, S6 | delegated (parallel, isolated worktree A) | done | fd1fc29, 866ceee |

## Log

- L1 (2026-10-17) User request (verbatim): "El tema son las paredes invisibles, hazlas un poco mas anchas y visuales pero a nivel del suelo no tan altas para que no interfieran porfa, tambien haz que pueda moverme mas hacia atras y pueda escojer cierto desnivel del suelo si es posible ya dentro del escenario, otra cosa, en los wide flicks, es complicado saber a donde se movio el objetivo cuando sale de mi campo de vision, investiga como podemos suplir o arreglar esto sin afectar el proposito del escenario"
- L2 Investigation: core movement has no stalls away from the play-area edges (probe over ADAD at yaw 0–90°); the reported blocking is the invisible edges (counter-strafe z ±1 m, strafe-shot z ±2 m). The user confirmed (option a).
- L3 Route: three parallel writers in isolated worktrees (S7; shared files such as `scenario/def.rs` and the HUD rule out a shared tree); the parent merges, resolves seams and runs the full suite once.
- L4 S5 root cause (writer A, with evidence): bots one-tap the 100-health player (Vandal headshot 160) 1.3–2.1 s after the countdown and the session respawns silently, which reads as a teleport. Fix: entry spawn protection (`RESPAWN_GRACE_SECS`) and HUD cues. Open balance question for the user: bots still kill within ~2 s of seeing you.
- L5 Integration: B and C conflicted only in `difficulty.rs` (two new tests) and `pages/hud.rs` (two new system params); both sides kept. Seam check: `cargo test --workspace --all-features` 358 passed, 1 ignored (arena golden, passes with `--ignored`); clippy clean with default and all features; fmt clean; calibrate order holds (flick-wide 0.66, flick-headshot 0.70, counter-strafe 0.70, strafe-shot 0.69 contender accuracy). Needs a playtest: edge strips, floor slab and pause picker, off-screen arrows, spawn-protection HUD, arena walls after retry.
