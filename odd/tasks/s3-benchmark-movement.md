# S3 benchmark, kill-count runs and player crouch
Objective: finish the follow-ups of `dynamic-targets`: the three Voltaic S3 subcategories in the benchmark, the two scenarios below the 45% simulated-accuracy target, a run end the player picks (time or kills) with a 100-kill movement scenario, and crouching in movement scenarios.
Branch: `feat/s3-benchmark-movement` (worktree `../CustomAimTrainer-worktrees/s3-benchmark-movement`). Delivery: work-unit commits, merge to `main` when asked. Test runner: `cargo test --workspace`, clippy, `calibrate`.

## Specs

- S1 (verbatim, 2026-10-21): "Implementa los benchmarks" — add linear clicking, control tracking and stability switching to the benchmark (the follow-up left open in `dynamic-targets` L13).
- S2 (verbatim): "Implementé una vez el precisión del simulador" — bring `vertical-rail` and `control-duelist` (contender 0.43) into the 45–85% simulated contender accuracy band.
- S3 (verbatim): "implemento un escenario que sea de tipo movement, que sea, pero por kills, en este caso 100 bajas, o bueno, más bien, quisiera que pudiera ser configurable, de que si es por tiempo o por número de objetivos alcanzados"
- S4 (verbatim): "y también en el movement agregar el agacharse."
- Out of scope (verbatim): "las pruebas del módulo. Bueno, no, esas creo que van de mi parte, así que yo las hago." Playtest: "ya lo he estado probando y al menos un momento todo bien."

## Tasks

| ID | Specs | Route | Status | Commit |
|----|-------|-------|--------|--------|
| T1 | S2 | inline | done | (this commit) |
| T2 | S1 | inline | pending | |
| T3 | S3 | inline | pending | |
| T4 | S4 | inline | pending | |

## Log

- L1 (2026-10-21, verbatim) "Sí, por favor. Y oye, comienza a implementar las correcciones que hoy nos hacen falta. Implementa los benchmarks, al playtest, pues ya lo he estado probando y al menos un momento todo bien. Implementé una vez el precisión del simulador, las pruebas del módulo. Bueno, no, esas creo que van de mi parte, así que yo las hago. Y hoy otra cosa, por favor, implemento un escenario que sea de tipo movement, que sea, pero por kills, en este caso 100 bajas, o bueno, más bien, quisiera que pudiera ser configurable, de que si es por tiempo o por número de objetivos alcanzados, y también en el movement agregar el agacharse."
- L2 `feat/dynamic-targets` merged into `main` (fast-forward to d41c5de) at the user's request.
- L3 T1: calibrate contender accuracy now `vertical-rail` 0.61 (path speeds lowered to 1.05–2.2 m/s across its three profiles) and `control-duelist` 0.46 (stop_chance 0.3, max_offset 2.5, change_secs 0.7–1.5); order recruit < contender < veteran < elite holds. The sim is seed-noisy: narrowing control-duelist to 2.2 m dropped it to 0.30.
