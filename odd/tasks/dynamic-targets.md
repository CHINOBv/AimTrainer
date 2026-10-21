# Dynamic targets: KovaaK's/Aim Lab movement ideas and a game tag
Objective: add the moving-target mechanics found missing in the KovaaK's/Aim Lab comparison (2026-10-21), scenarios that use them, and a scenario game tag the picker can filter by.
Branch: `feat/dynamic-targets` (worktree `../CustomAimTrainer-worktrees/dynamic-targets`). Delivery: work-unit commits on the branch; merge/push are the user's decision. Test runner: `cargo test --workspace`, `cargo clippy --workspace --all-targets`, `calibrate` example.

## Specs

Source of the gap list (assistant comparison, 2026-10-21, accepted by the user): KovaaK's `.sce` dodge/scoring fields and Aim Lab IL2CPP identifiers; only ideas are reused, no files, numbers or assets.

- S1 Pause at every strafe direction swap (KovaaK's `StrafeSwapMinPause/MaxPause`) for `strafe` and `dodge` motions.
- S2 Asymmetric left/right strafe durations (KovaaK's `LeftStrafeTimeMult/RightStrafeTimeMult`) for `strafe` and `dodge`.
- S3 Reaction to accumulated damage (KovaaK's `DamageReaction*`): after a damage threshold the target reverses, with a chance to ignore, a delay, a cooldown and a reset timer for the accumulated damage.
- S4 Damage-based tracking score (KovaaK's `ScorePerDamage` + `ScoreMultDamageEfficiency`).
- S5 Balanced "shuffle bag" randomness for strafe decisions (Aim Lab `GenerateAndShuffleStrafeTrackTarget*`), deterministic with `aim_core::rng::Rng`.
- S6 Distance conditions for behaviour rules (Aim Lab `BotDistanceFromPlayerCondition`, `BotDistanceFromSpawnCondition`).
- S7 Waypoint path motion (Aim Lab `BotWaypoints`/`WaypointPatrol`). Assumption: path recording from player input (Aim Lab `BotMovementRecorder`) is out of scope for this feature; the path is written in TOML.
- S8 (verbatim, 2026-10-21): "implementa el resto. O sea, el resto de ejercicios que salgan de los parámetros de Valorant. No pasa nada, solamente tenerlos identificados y como un tipo tag o algo así para filtrar. Por ejemplo, si en un futuro metemos escenarios específicos para Counter-Strike, que pueda filtrarlos."
  - A scenario declares the game whose parameters it follows (reusing `aim_core::games::Game`); scenarios outside VALORANT parameters are tagged as such; the scenario picker filters by it.
- S9 New built-in exercises that use S1–S7, each tagged per S8, passing `calibrate` ordering and the sim playability tests.
- S10 (verbatim, 2026-10-21): "un archivo que es de más que nada ejercicios de estiramiento de manos, de la muñeca y eso para prevenir lesiones y tal. Creo que estaría bien implementarlo, o sea, tenerlo dentro de, tenerlo como una sección para que pues las personas lo puedan ir siguiendo o algo así."
  - Source: Kastor, "Wrist stretching and posture guide" (2020-01-16, PDF shared by the user). A hand-care section the player can follow step by step: mobility before playing, stretches after playing, optional posture routine and tips. Instructions in our own words, credited; keep the existing "not medical advice / no trial shows prevention" disclaimer.
- S11 (verbatim, 2026-10-21): "te comparto el archivo de las VDM, más que nada para por si hay escenarios que puedas sacar de ahí y pasarlos acá, estaría bastante bien. Y también te comparto un archivo que es sobre los benchmarks."
  - Sources: VDIM S3 for Aim Lab and "Benchmark Focused Routines" (Lowgravity56, 4rK, Voltaic) and "Voltaic x Aimlabs - Weakness-specific aim training routines". They hold no scenario parameters (playlists are Aim Lab links); reuse structure and ideas only: the three S3 subcategories we lack routines for (linear clicking, control tracking, stability switching), original scenarios for gaps, and per-subcategory technique tips in our own words.

## Tasks

| ID | Specs | Route | Status | Commit |
|----|-------|-------|--------|--------|
| T1 | S8 | inline | pending | |
| T2 | S1, S2, S5 | inline | pending | |
| T3 | S3 | inline | pending | |
| T4 | S4 | inline | pending | |
| T5 | S6 | inline | pending | |
| T6 | S7 | inline | pending | |
| T7 | S9 | inline | pending | |
| T8 | S10 | inline | pending | |
| T9 | S11 | inline (research delegated to one read-only worker) | pending | |

## Log

- L1 (2026-10-21, verbatim) "Hola, por favor usa REA y mira namelabs y callbacks que podemos tomar de la parte de escenarios de tracking o clicking dinámico, sea enemigos en movimiento, que nos pueda servir por acá."
- L2 (2026-10-21, verbatim) "Va, me parece bien. Igual implementa el resto. O sea, el resto de ejercicios que salgan de los parámetros de Valorant. No pasa nada, solamente tenerlos identificados y como un tipo tag o algo así para filtrar. Por ejemplo, si en un futuro metemos escenarios específicos para Counter-Strike, que pueda filtrarlos. O algo así."
- L4 (2026-10-21, verbatim) "Otra cosa, mira, por acá te comparto el documento de las VDM de Voltaic para que le eches un ojillo. Y también te comparto un archivo que es de más que nada ejercicios de estiramiento de manos, de la muñeca y eso para prevenir lesiones y tal. Creo que estaría bien implementarlo, o sea, tenerlo dentro de, tenerlo como una sección para que pues las personas lo puedan ir siguiendo o algo así. Igual te comparto el archivo de las VDM, más que nada para por si hay escenarios que puedas sacar de ahí y pasarlos acá, estaría bastante bien. Y también te comparto un archivo que es sobre los benchmarks." Four Google Drive files read: VDIM S3, Benchmark Focused Routines, Weakness-specific routines, Kastor's wrist guide.
- L5 Explorer map: side choice only in `MotionState::reversed()` (motion.rs:387); lane-edge flip in `strafe()` (motion.rs:450); reactions hold via `MotionState::react()`; all damage through `Session::on_damage(idx)` (session.rs:737, 1.0 per click hit, `dt` per hold tick); `sim.rs` PINNED bit-exact results, so new options must draw nothing from the RNG when off; picker filter `ScenarioFilter` (ui/organisms/scenario_browser.rs:20) is session-only.
- L3 Route: all inline (units share `def.rs`/`motion.rs`/`session.rs`, so no disjoint parallel writers). One read-only explorer mapped the motion, reaction, scoring, behaviour and picker code.
