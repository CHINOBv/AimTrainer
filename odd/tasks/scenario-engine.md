# Scenario engine blocks and new scenario library
Objective: add the engine blocks that Voltaic-, KovaaK's- and Aim Lab-style drills need, then build an original scenario library (Six Shot, Angle Shot, equivalents of every Voltaic benchmark slot) and more routines.
Branch: `feat/scenario-engine` (worktree `../AimTrainer-worktrees/scenario-engine`) · Delivery: local merge to `main`; push at the end of the batch (authorized) · Test runner: `cargo test --workspace` + `calibrate`

## Specs

- **S1 Ballistic motion.** Targets launched with a velocity range under gravity, bouncing off the floor or walls a set number of times. A floor hit can expire the target or fail the run. Covers skeet, popcorn and falling-tile drills.
- **S2 Depth and bounce.**
  - A forward/back (depth) axis for strafe/drift/dodge, within a distance band.
  - A straight-line motion that bounces off the walls of a polygon or box arena (Frogtagon, Penta Bounce style).
  - An orbit motion around the player.
- **S3 Shapes.** A capsule/cylinder target with an optional head zone (hit test in core, rendering in the app).
- **S4 Scoring and spawn extras.**
  - An accuracy score multiplier (`linear` or `sqrt`).
  - Per-kill target shrink, with a floor.
  - A grid or point-set spawn snap.
  - Weighted motion profiles per target with switch timers.
  - A target "rotation" (several target definitions in one scenario).
- **S5 Per-scenario options.** An FOV lock (min/max). Per-scenario feedback toggles: hide hit markers or hit sounds, floating damage numbers, health bars.
- **S6 History and analytics.**
  - A settings snapshot per run (sensitivity, FOV, crosshair, FPS, pauses) and a scenario content hash.
  - TTK spread and damage efficiency.
  - CSV export of the history.
- **S7 Scenario library.** Original scenarios with our own names, crediting the idea:
  - Six Shot: six small static targets, always six.
  - Angle Shot: evasive flyers on a mostly flat wall with depth.
  - One equivalent for every Voltaic benchmark slot (clicking dynamic/static/linear, tracking precise/reactive/control, switching speed/evasive/stability), the VALORANT S1 ideas (flick-tech, micros, stability), and the useful KovaaK's mechanics (reflex flick, reactive flick, skeet, popcorn, pressure).
  - No copied files and no Voltaic/KovaaK's/Aim Lab names as our branding.
  - Every scenario passes validation, the sim gate and calibrate (contender accuracy 45–85% where accuracy applies).
- **S8 Routines.** New routines that use the library, following the training-methods research (`odd/research` notes in the Log): warm-up, skill-specific blocks, session lengths.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | plan | inline | done | 12ad695 |
| T2 | S1, S2, S3, S4 (engine) | delegated (worktree `se-engine`) | done except target sets (deferred) | 9be6e5a, ad8a306, 3e9f019, merge a196167 |
| T3 | S5, S6 | delegated, after T2 | pending | |
| T4 | S7, S8 (content) | delegated, after T2 | in progress (split: se-library clicking+switching, se-library-b tracking+VALORANT; routines after) | |
| T5 | seam: merge, calibrate, verifier, review, merge to `main` | inline | pending | |

## Log

- L1 (2026-10-18) User (verbatim excerpt): "También crea unos escenarios. Uno que se me viene a la mente es el de Six Shot. Angel Shot. Y pues nada. Si puedes investigar sobre todas las playlists de Voltaic y replica cada uno de los escenarios. [...] Crea más rutinas [...]"
- L2 (2026-10-18) User on KovaaK's: "usa REA para lo mismo y mira que cosas nos pueden ser utiles de el", then "Exploralas e implementalas".
  - Binaries were not reverse-engineered (EULA). Only plain-text `.sce`/JSON data that KovaaK's exposes to players was read.
- L3 (2026-10-18) Research (Voltaic API and docs, the KovaaK's `.sce` vocabulary, aim-training methods) ranks these engine gaps by need: ballistic/gravity motion (~12 of 28 KovaaK's drills), capsule shapes (~17), accuracy multiplier (~12), depth movement (~15), multi-profile targets (~9), FOV lock (13).
  - Training methods: click dwell and corrections are the strongest skill markers (Warburton 2023); external-focus cues; contextual interference is weak in applied settings; drill parameters are our inference.
- L4 (2026-10-18) T2 merged.
  - Blocks: ballistic (with `on_floor` expire/fail and the new `FailReason::Dropped`), depth band on strafe/dodge/drift, `linear_bounce` (box or N-gon), orbit, capsule with optional head, `accuracy_mult` none/linear/sqrt, `shrink_per_kill`, spawn `grid`, `motion_profiles`.
  - Calibrate: all pre-existing rows identical; 8 `demo-*` rows ordered, contender accuracy 0.57–0.82.
  - **Deferred:** `[[target_sets]]`. Per-target definitions would rewrite most of `Session`, plus analytics and HUD files that read the scenario-wide shape, and the interaction with phases is unspecified. Phases already vary targets within a run (YAGNI).
  - T3 (FOV lock, per-scenario feedback toggles, history snapshot/CSV) waits until game-feel reaches `main`, because both touch app settings and results.
