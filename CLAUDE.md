# CLAUDE.md

A VALORANT-focused aim trainer (Rust + Bevy 0.20 + bevy_egui 0.43) for Arch Linux (Hyprland/Wayland) and Windows. It runs as a hotkey-toggled transparent overlay.

**Read first (binding):**
- `docs/architecture.md` — layers, dependency rule, folder layout, extension points, conformance backlog.
- `docs/coding-standards.md` — SOLID/YAGNI/DRY as applied here, naming and units, errors, testing, latency rules, game-integrity and IP policy.
- `docs/roadmap.md` — the planned features and which module owns each one. Use it to decide placement; do not build ahead of a feature (YAGNI).
- `docs/decisions.md` — what was decided and why.
- `CONTRIBUTING.md` — the PR checklist.

This file only lists what an agent needs on top of those documents.

## Commands

```sh
cargo build                                            # debug (deps are optimized via profile.dev.package."*")
cargo run -p aim_app                                   # run the trainer (binary: aimtrainer)
cargo test --workspace                                 # all tests (core + app)
cargo clippy --workspace --all-targets                 # must stay warning-free
cargo fmt --all
cargo run -p aim_core --release --example calibrate    # simulated-player scores per scenario
./target/debug/aimtrainer toggle                       # show/hide a running instance (Linux IPC)
# Windows check from Linux (blake3 needs PURE when cross-compiling); run before pushing:
CARGO_FEATURE_PURE=1 cargo clippy --workspace --all-targets --target x86_64-pc-windows-msvc -- -D warnings
```

The toolchain must be rustc ≥ 1.97.1, which Bevy 0.20 requires.

## Architecture essentials

Full rules are in `docs/architecture.md`.

| Crate | Contains | Must not |
|---|---|---|
| `crates/aim_core` | Every gameplay, training and analysis rule. Pure and deterministic. | Depend on Bevy, egui, winit, the filesystem, the clock or the network |
| `crates/aim_app` | Bevy adapter: input, rendering, egui, overlay, IPC, persistence | Implement gameplay rules; it only builds a `FrameInput` and renders core state |

- **New mechanic:** add a building block to `scenario/def.rs` (serde with `deny_unknown_fields`), handle it in `session.rs`, extend `validate()`, and add tests.
- **New exercise, routine or benchmark:** a TOML file in `content/{scenarios,routines,benchmarks}/`, registered in the matching `builtin` `SOURCES` (`scenario/builtin.rs`, `routine.rs`, `benchmark.rs`). The file name must equal its `id`.
- **Coordinates:** right-handed, +Y up, −Z forward, meters. Core yaw is positive turning right; Bevy needs `-yaw` (see `update_camera`).
- **Randomness:** `aim_core::rng::Rng`, seeded and deterministic. Do not add `rand`. `Session` must stay deterministic for a given definition, seed and input sequence.
- **Conformance backlog:** when you touch an area listed in the architecture's conformance backlog (C1–C10), fix that gap in a separate refactor commit, never mixed into an unrelated feature.
- **Integrity and IP:** never touch the game process, ship Riot assets or replicate VALORANT maps. See `docs/coding-standards.md`.

## Accuracy rules

- Every VALORANT constant must have a source and a confidence tag in `docs/valorant-mechanics.md`. Training claims go in `docs/training-science.md`. Do not invent numbers; mark anything uncertain as approximate or estimate.
- After changing any scenario, run the `calibrate` example. Check that recruit < contender < veteran < elite, and that contender accuracy lands at roughly 45–85% (challenge point). The sim tests enforce playability.
- The benchmark keeps `calibrated = false` until its thresholds come from real player data.

## Testing

- Write the failing test first (RED), then the fix. Tests go through public APIs; `Session` covers most behavior.
- Mouse input, the camera and the cursor in `aim_app` have no automated tests. Say so whenever you change them, and ask the user to playtest.
- Never launch the app window on the user's desktop without saying so: it is a fullscreen overlay. For a smoke run, use a scratch `XDG_CONFIG_HOME` containing a `settings.toml` with `fullscreen = false`.

## API gotchas (verified against the crate sources)

- **egui 0.36:**
  - Panels are `egui::Panel::{top,bottom,left,right}` and `CentralPanel`, and they `.show(&mut Ui, …)` inside a root `Ui::new(ctx.clone(), id, UiBuilder::new().layer_id(LayerId::background()).max_rect(ctx.viewport_rect()))`.
  - Use `default_size`, not `default_width`.
  - Use `ctx.content_rect()`, not `screen_rect`.
- **bevy_egui 0.43:** UI systems run in the `EguiPrimaryContextPass` schedule. `contexts.ctx_mut()?` returns a `Result`.
- **Bevy 0.20:**
  - `CursorOptions` is its own component, with `grab_mode`, `visible` and `hit_test`.
  - `App::insert_non_send` replaces `insert_non_send_resource`.
  - Mouse deltas come from `AccumulatedMouseMotion`; they are raw counts, with +y pointing down.
  - `CursorGrabMode::Locked` falls back to `Confined` automatically.
  - Exit with `commands.write_message(AppExit::Success)`.
- **Wayland:** clients cannot hide or raise themselves (`window.visible` is skipped). Hiding means a transparent window plus `hit_test = false`.

## User environment

- **Hyprland 0.56 with a Lua config.** Overrides belong in `~/.config/hypr/user.lua` (`hl.bind`, `hl.window_rule`). **Do not edit the user's Hyprland config;** document snippets instead.
- **Paths:** settings and custom content go in `~/.config/aimtrainer/`, and run history in `~/.local/share/aimtrainer/history.csv`.

## Conventions

- **Language:** the user writes in Spanish. Code, comments, docs and commits are in English.
- **Commits:** Conventional Commits, with **no** `Co-Authored-By` or other AI attribution.
- **Branches:** the default branch is `main`, with remote `origin` = github.com/CHINOBv/AimTrainer. Do feature work on a branch, in a sibling worktree `../CustomAimTrainer-worktrees/<name>`.
- **Delivery:** pushing, merging to `main` and opening PRs are the user's decisions; ask first unless they asked.
- **Tracking:** large work is tracked in `odd/tasks/<feature>.md`, with specs, tasks and a log.
