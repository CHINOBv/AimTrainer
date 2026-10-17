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

- S12 Roadmap gaps found by audit (2026-10-17), quoted from `docs/roadmap.md`:
  - S12a Phase 2: "**Benchmark tiers:** the same scenario families at three difficulties (novice, intermediate, advanced), each tier with its own thresholds."
  - S12b Phase 2: "**Routine progress persistence:** "today's" plan, its position in it, and resuming a split session. A small port in the core, stored by the app."
  - S12c Phase 3: "presets in `content/crosshairs/`"; "monitor-distance matching (0%, 75%, 100%)"; "A standalone converter page that does not require a run."
  - S12d Phase 4: "Per-subcategory strengths and weaknesses from the benchmark and trends over time"; "Rules (data) that map weaknesses to scenarios, routines or tips"; "Per-scenario history, per-skill trends"; "a `RunStore` port moves history behind a trait (backlog C7)".
  - S12e Phase 5: "scenarios declare which parameters scale (target size, speed, lifetime, count) and their easy/hard bounds."
  - S12f Phase 6: "**Hybrid activities:** game-like objectives combined with aim drills"; "Behavior (peek, strafe, hold) as a separate, testable state machine."
  - S12g Phase 7: "map loading (glTF)"; "Drills: angle clearing, jiggle-peeks, crosshair placement at head level, pre-aim".
  - S12h Architecture backlog C5, C6, C9.

## Tasks

| ID | Specs | Route | Status | Commit |
|----|-------|-------|--------|--------|
| T1 | S3 | inline | done | e25f274, 058b816 |
| T2 | S4 | inline | done | 18c2b65, 47c901e, 936b63b |
| T3 | S5 | inline/delegated after research | done | 7385776, 3fbb77b |
| T4 | S6 | inline | done | 1a8f1e0, aa7ae89, 3d2a700, a83c32a |
| T5 | S7 | inline | done | d7c9482, 0982b7c, 0343a5e, e87144f |
| T6 | S8 | inline | done | 48d668e, 091b54a, Activity: see L5 |
| T7 | S9 | inline | done | c0a8d9c, 48d668e |
| T8 | S10 | inline | done (offline only) | 59f6fff, 424f185 |
| T9 | S11 | inline + 2 delegated (F7, F9) | done: F1–F12 in docs/feature-analysis.md | e483507, b7f0457, 24ba88f, e5f2239, cef2223, 803fcd6, 53dc3bd, a83c32a, 65c65ad |
| T10 | S12a | delegated (parallel, benchmark unit) | done | 72af253, 1f36e78 |
| T11 | S12f, S12g | delegated (parallel, world unit) | done (site hold is a deathmatch objective, not a new mode) | 1295e51, f7e2b45 |
| T12 | S12d (profile, rules, trends, charts) | delegated (parallel, analytics unit) | done | 5b8f333, 1f36e78 |
| T13 | S12b, S12d (RunStore) | inline | done | 70b3d9b, 1f36e78 |
| T14 | S12c, S12e | inline | done | 4634f43, 1f36e78 |
| T15 | S12h | inline (C9 docs delegated to 3 parallel writers) | done: C5, C6, C7, C9 | 70b3d9b, 2d8e3e5, 98378cc |

## Log

- L1 (2026-10-17) Goal set by the user (S1). PR #1 was merged by the user; Windows CI failed on `Command::as_str` dead code under `-D warnings` → fixed (e25f274). License MIT chosen by default (owner can switch).
- L2 Research launched: VALORANT crosshair codes, sensitivity/FOV data and monitor-distance math, Riot VAL-MATCH-V1 schema/policy, Vandal/Phantom recoil.
- L3 Phases 2–8 implemented and committed (see Tasks). Deviations recorded in `docs/roadmap.md` status table.
- L4 Feature analysis written (`docs/feature-analysis.md`): 12 gaps → F1–F12. F7 (agent) and F9 (agent) integrated; sim made recoil-aware so burst drills calibrate (contender 0.62–0.80). Verification: `cargo test --workspace` green, `cargo clippy --workspace --all-targets -D warnings` clean on Linux and the Windows target. App-side mouse/camera/HUD changes need a playtest.
- L5 Stop-hook review: S8's `Activity` abstraction had been skipped unilaterally. Implemented it (`aim_core::activity`, `ArenaSession` impl, arena routine blocks, `aim-and-duel` routine, results/next-run naming). Verification: 280 tests, clippy clean.
- L6 Roadmap Phase 8 also asks to rebuild match situations as drills: added `aim_integrations::rebuild` (distance band of deaths → `match-rebuild` scenario saved by `analyze-match`). Angles omitted: `viewRadians` has no documented convention. Online fetch stays out (production key + RSO; see docs/research/riot-match-api.md). Verification: 283 tests with all features, clippy clean, CLI smoke run in scratch XDG.
- L7 Audit (explorer) found S12a–h partial or missing. Route: three parallel writers (disjoint surfaces: benchmark, world, analytics; writer trigger (a)), the rest inline. Parent owns git and `menu.rs`.
- L8 T10, T12, T13, T14 committed; C9 done (missing_docs enforced). Waiting for T11 (world) before C5 and the full verification.
- L9 All S12 tasks done. Verification: `cargo test --workspace --all-features` 334 passed; clippy `-D warnings` clean with default and all features; `cargo fmt --check` clean; calibrate order recruit<contender<veteran<elite holds everywhere (advisory: track-close-strafe 0.43 and track-reactive-strafe 0.41 contender accuracy, slightly under the 45% band, level-0 behavior unchanged by S12). Needs playtest: Tools tab, resume button, tier selector, skills section, site-hold HUD, new arenas.
- L10 RDD: the full batch exceeded the reviewer budget, so it was reviewed as two slices with consent granted (user S2): `1295e51~1..966fdf0` (risk+reliability, approved, acknowledged `review-a991cfbb798c1ba5`) and `70b3d9b~1..72af253` in a temporary worktree (approved, acknowledged `review-c587d08829f21077`). Advisories fixed in c5b81bf (glTF depth limit) and the next commit (unique tier ids, History without Default, resume index guard, advance_day test, resumes doc). Third slice `72af253..1f36e78` reviewed too (approved, acknowledged `review-9cd7306513ce3712`); its advisories are fixed in the following commit (strict knob tables, count knob guard, trend week span, MIN/MAX_LEVEL docs). Only 2d8e3e5 (docs-only C9 pass) had no native review. 336 tests, clippy clean in both feature sets.
