# Roadmap completion (Phases 1–8) and new features
Objective: finish `docs/roadmap.md` Phases 1–8 with improvements along the way, then analyze the project and add at least 10 new high-value features for training aim and warming up while playing ranked.
Branch: `feat/roadmap-completion` (worktree `../CustomAimTrainer-worktrees/roadmap`) · Delivery: one PR updated as phases land · Test runner: `cargo test --workspace` + Windows clippy check (see CLAUDE.md)

## Specs

- S1 User goal (verbatim): "Continua hasta terminar con el roadmap, busca y aplica mejoras correspondientes, una vez termine haz un analisis del proyecto y agrega al menos 10 funcionalidades nuevas que sumen al proyecto y aporten valor importante y real, que ayuden a cumplir el objetivo de mejorar el aim y ser una app para entrenar el aim mientras se juega en rankeds o mantenerte caliente, aprueba por defecto todas las revisiones que se soliciten de RDD no preguntes"
- S2 RDD consent: grant every requested review by default without asking (from S1).
- S3 Phase 1 leftovers: CI green on Windows, license (MIT, matching Cargo.toml).
- S4 Phase 2: VDIM-style weekly method (roadmap Phase 2 table), built on a generic difficulty model.
- S5 Phase 3: crosshair model with VALORANT code import/export (researched format); data-driven sensitivity converter with monitor-distance matching (researched data).
- S6 Phase 4: run telemetry + deterministic replay, flick/tracking analysis, skill profile, explainable recommendations, history charts.
- S7 Phase 5: adaptive difficulty (staircase to ~80% success), personalized playlists, reload mechanic.
- S8 Phase 6: `Activity` abstraction, deathmatch with bots using VALORANT movement and the simulated aimer.
- S9 Phase 7: `aim_world` crate with original box-based maps, occlusion, angle-clearing drills.
- S10 Phase 8: `aim_integrations` crate, offline import of official match data, mapping to drills; online fetch only with a user key.
- S11 ≥10 new features after the analysis, each with a rationale and tests.

## Tasks

| ID | Specs | Route | Status | Commit |
|----|-------|-------|--------|--------|
| T1 | S3 | inline | done (CI rerun pending) | e25f274, 058b816 |
| T2 | S4 | inline | pending | |
| T3 | S5 | inline/delegated after research | pending | |
| T4 | S6 | inline | pending | |
| T5 | S7 | inline | pending | |
| T6 | S8 | inline | pending | |
| T7 | S9 | inline | pending | |
| T8 | S10 | inline | pending | |
| T9 | S11 | tbd | pending | |

## Log

- L1 (2026-10-17) Goal set by the user (S1). PR #1 was merged by the user; Windows CI failed on `Command::as_str` dead code under `-D warnings` → fixed (e25f274). License MIT chosen by default (owner can switch).
- L2 Research launched: VALORANT crosshair codes, sensitivity/FOV data and monitor-distance math, Riot VAL-MATCH-V1 schema/policy, Vandal/Phantom recoil.
