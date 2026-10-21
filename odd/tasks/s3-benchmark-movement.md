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
| T2 | S1 | inline | done | (this commit) |
| T3 | S3 | inline | done | (this commit) |
| T4 | S4 | inline | done | (this commit) |

## Log

- L1 (2026-10-21, verbatim) "Sí, por favor. Y oye, comienza a implementar las correcciones que hoy nos hacen falta. Implementa los benchmarks, al playtest, pues ya lo he estado probando y al menos un momento todo bien. Implementé una vez el precisión del simulador, las pruebas del módulo. Bueno, no, esas creo que van de mi parte, así que yo las hago. Y hoy otra cosa, por favor, implemento un escenario que sea de tipo movement, que sea, pero por kills, en este caso 100 bajas, o bueno, más bien, quisiera que pudiera ser configurable, de que si es por tiempo o por número de objetivos alcanzados, y también en el movement agregar el agacharse."
- L2 `feat/dynamic-targets` merged into `main` (fast-forward to d41c5de) at the user's request.
- L3 T1: calibrate contender accuracy now `vertical-rail` 0.61 (path speeds lowered to 1.05–2.2 m/s across its three profiles) and `control-duelist` 0.46 (stop_chance 0.3, max_offset 2.5, change_secs 0.7–1.5); order recruit < contender < veteran < elite holds. The sim is seed-noisy: narrowing control-duelist to 2.2 m dropped it to 0.30.
- L4 T2: benchmark gains Linear clicking (crossing-lanes, metronome-ring), Control tracking (control-weave, control-duelist), Stability switching (stability-flyers, steady-agents-switch), thresholds from `calibrate -- --level` (new option) rounded to 50; weekly plan focus Monday/Wednesday/Friday; test pins the nine S3 names and that each is in a focus (fails without the plan change). Consequence noted to the user: overall energy reads empty until the three new subcategories are played (energy is recomputed from history; no stored data breaks). Risk item 3 (contract others consume: overall energy semantics) is user-requested.
- L5 T3: `scoring.finish_bonus_per_sec` (kills-ended runs, points per second left; validated >= 0); `difficulty::EndChoice { Kills(n), Time(s) }` with `with_kill_goal` (cap n x 4 s, bonus 100/s, `#100k`), `with_time_limit` (`~Ns` when already timed, else `#Ns` and end = time) and `RunKey.end`; proof `variant_for_key` and app `variant` rebuild it (consistency test extended). Details panel "Run ends: own / by time / by kills", saved per scenario in `Settings.end_choices` (additive optional settings field). New scenario `move-century` (kills 100, 400 s cap, bonus 100/s): calibrate contender 0.66, 100 kills in ~115 s, order holds. RED: compile failures and the app/core consistency test failing on `#100k` keys before `variant` learned them. Mid-task correction: the first UI only offered own end vs kills, so a kills-authored scenario could not be played by time; generalized to EndChoice.
- L6 T4: player crouch (left Ctrl, VALORANT default bind) in movement scenarios: `MoveInput.crouch` (serde default), speed cap `CROUCH_SPEED_FRACTION` 0.4 (estimate, no published value), eye drops `CROUCH_HEIGHT_DROP_M` over `CROUCH_TRANSITION_SECS`, crouched (>= half way) spread x `CROUCH_SPREAD_MULT` 0.85 (approximate, community measurement), applied to first shots and recoil sprays; arenas pass standing. Replay codec flag bit 4 (unused before), version kept at 1: older replays read as standing; an older build would ignore the flag. RED: compile failures; tests for speed, eye transition and stand-up, no crouch without player movement, narrower spread, key mapping, codec round trip with crouch frames. Pinned sim results and the arena golden unchanged. Needs playtest: camera drop feel, Ctrl conflicts with OS shortcuts.
