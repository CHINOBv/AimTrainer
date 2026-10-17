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
| T1 | S4 | delegated (parallel, isolated worktree B) | in progress | |
| T2 | S1, S2, S3 | delegated (parallel, isolated worktree C) | in progress | |
| T3 | S5, S6 | delegated (parallel, isolated worktree A) | in progress | |

## Log

- L1 (2026-10-17) User request (verbatim): "El tema son las paredes invisibles, hazlas un poco mas anchas y visuales pero a nivel del suelo no tan altas para que no interfieran porfa, tambien haz que pueda moverme mas hacia atras y pueda escojer cierto desnivel del suelo si es posible ya dentro del escenario, otra cosa, en los wide flicks, es complicado saber a donde se movio el objetivo cuando sale de mi campo de vision, investiga como podemos suplir o arreglar esto sin afectar el proposito del escenario"
- L2 Investigation: core movement has no stalls away from the play-area edges (probe over ADAD at yaw 0–90°); the reported blocking is the invisible edges (counter-strafe z ±1 m, strafe-shot z ±2 m). The user confirmed (option a).
- L3 Route: three parallel writers in isolated worktrees (S7; shared files such as `scenario/def.rs` and the HUD rule out a shared tree); the parent merges, resolves seams and runs the full suite once.
