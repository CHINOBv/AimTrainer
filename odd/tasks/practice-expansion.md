# Practice expansion: dodging click targets, arena cover, restart key, in-run tuning
Objective: more click practice against moving/dodging targets, arenas with real cover, a restart key that keeps adaptive difficulty, and in-run target size/distance/speed tuning with a reset to scenario defaults.
Branches (parallel, isolated worktrees under `../AimTrainer-worktrees/`): `feat/dodge-scenarios` (T1), `feat/arena-cover` (T2), `feat/restart-tuning` (T3, T4). Parent merges into `main`, resolves seams, runs the full suite once, builds and launches. Delivery strategy: local fast-forward/merge into `main` per the user's standing request; push is the user's decision. Test runner: `cargo test --workspace` + `calibrate`.

## Specs

- S1 (verbatim, 2026-10-18): "Necesito que agregues más escenarios para practicar y mejorar lo que es el clicking. Pero con enemigos moviéndose. O esquivando."
- S2 (verbatim): "en la parte de arenas, la mayoría al siento que les falta pulir mucho. Me refiero más que nada al tema de los mapas. Siento que faltan como más lugares de cobertura y cosas de ese tipo. Porfa, échale una pensada y mira qué mejorar y cómo."
- S3 (verbatim): "agregar alguna tecla que sea un keyband para poder reiniciar el escenario. Pero sin que se reinicie la dificultad en los escenarios con dificultad adaptativa."
- S4 (verbatim): "agregar alguna opción para ajustar el tamaño de los targets o la distancia dentro de la partida, pero que se pueda resetear a de cada escenario. Así como también la velocidad de movimiento y eso."
  - Decision (parent): tuned runs are stored under their own run-key suffix, never move the adaptive staircase and never count for the benchmark.

## Tasks

| ID | Specs | Route | Status | Commit |
|----|-------|-------|--------|--------|
| T1 | S1 | delegated (parallel writer, worktree `dodge-scenarios`; core motion + content) | done | 8a540e3, aea2e79, 94dbac1 |
| T2 | S2 | delegated (parallel writer, worktree `arena-cover`; `aim_world` maps + bots) | done | c86a267, 905fc9e, f933b19 |
| T3 | S3 | delegated (parallel writer, worktree `restart-tuning`; app input + navigation) | done | 543a96a, 7be5fd4 |
| T4 | S4 | delegated (same writer as T3, sequential; core run-key transform + pause UI) | done | 6be4d6c |

## Log

- L1 (2026-10-18) Exploration (explorer handoff): motion is Static/Strafe/Drift only (`scenario/def.rs:329-358`, `motion.rs`); difficulty knobs and run-key suffixes in `difficulty.rs` (`@level ~duration ^elevation`); 4 maps / 6 arenas in `content/maps`, `content/arenas` (`aim_world`), bots never seek cover; no in-run restart key; mid-run quit/restart records nothing; adaptive staircase keyed by base id; benchmark uses exact base ids.
- L2 Route: three parallel writers in isolated sibling worktrees (disjoint areas: core motion/content, `aim_world`, app flow). T1 and T4 both touch `aim_core` scenario code; the parent resolves that seam at merge.
- L3 (2026-10-18) T1: `Motion::Dodge` (jukes, stop-and-go, crouch, jump; crouch/jump constants tagged estimate in `docs/valorant-mechanics.md`) and 5 click scenarios (`dodge-juke-heads`, `dodge-crouch-spam`, `dodge-jump-spot`, `dodge-peek`, `dodge-micro-spheres`); calibrate order holds, contender accuracy 0.66–0.78. Known gap: hit effects and off-screen arrows still use the standing head height.
- L4 T2: map audit in `docs/maps.md`; 4 maps reworked (boxes 4–11 → 20–35), new `freight-lanes` map + `lanes-deathmatch` arena; roaming bots retreat to cover when shot and re-peek; test keeps every waypoint within 5 m of cover. Arena golden re-recorded (accuracy/damage only; explained by more shots hitting boxes).
- L5 T3/T4: restart key (default R, configurable, records nothing, keeps adaptive flag/level, run key, challenge seed); tuning `Tuning { size, distance, speed }` 50–200 % with run-key suffix `%s..d..v..`, reset to defaults, tuned runs never move the staircase nor count for the benchmark.
- L6 Integration (`integrate/practice-expansion`): conflicts in `motion.rs` (shared strafe helper now takes the speed multiplier, so Dodge honors tuning) and an import in `game/mod.rs`. Seam test `the_speed_multiplier_scales_drift_strafe_and_dodge_motion`: RED with Dodge ignoring the multiplier (5.40 → 5.40), GREEN after. Reviews: one-lens reliability review approved and acknowledged for each branch; follow-up fixes f933b19 (bots give up an unreachable retreat after 2.5 s) and 7be5fd4 (restart key falls back to R; ignores presses with modifiers), assessed medium/under budget. Full suite 446 passed, golden passes, clippy clean, calibrate order holds. Needs playtest: dodge rendering (crouch/jump), denser maps and bot cover use, R restart, pause-menu tuning.
