# Phase 1: open-source readiness
Objective: make the repository ready for outside contributors: conformance with `docs/architecture.md`, CI on Linux and Windows, issue templates and a license.
Branch: `feat/phase-1-open-source` (worktree `../CustomAimTrainer-worktrees/phase-1`) · Delivery: ask-on-risk · Test runner: `cargo test --workspace`

## Specs

- S1 Roadmap Phase 1 (`docs/roadmap.md`): "License", "CI on Linux and Windows: `fmt --check`, `clippy`, `test`", "Conformance backlog C1–C4", "Manual Windows verification", "Issue templates".
- S2 No behavior change: every refactor keeps existing behavior. Tests stay green, `calibrate` output is unchanged, and the app starts the same way.
- S3 C1 — move content TOML to `content/{scenarios,routines,benchmarks}`.
- S4 C3 + C10 — app folders `game/`, `overlay/`, `platform/`, `persistence/`; Bevy and bevy_egui versions in `[workspace.dependencies]`.
- S5 C2 + C8 — `ui/` split into pages, organisms, molecules and atoms with container/presentational structure; `SystemParam` groups instead of `too_many_arguments`.
- S6 C4 — split `scenario/session.rs` into `spawn`, `motion`, `weapon` and `scoring` modules.
- S7 The license is the owner's decision (MIT vs MIT OR Apache-2.0). Not written until decided.

## Tasks

| ID | Specs | Route | Status | Commit |
|----|-------|-------|--------|--------|
| T1 | S3, S2 | inline | pending | |
| T2 | S4, S2 | inline | pending | |
| T3 | S5, S2 | inline | pending | |
| T4 | S6, S2 | delegated (parallel with T3, disjoint surface `crates/aim_core/src/scenario/`) | pending | |
| T5 | S1 | inline | pending | |
| T6 | S7 | blocked on owner decision | pending | |

- T5: CI workflow (`.github/workflows/ci.yml`), issue templates, local Windows `cargo check`.

## Log

- L1 (2026-10-17) User: "Ahora si, continua con la implementacion porfa, comencemos a desarrollar el roadmap"
- L2 Baseline: `main` = `54de7dc` (user merged it). Baseline calibrate output is recorded before refactors to prove S2.
