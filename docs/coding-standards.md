# Coding standards

These are the rules every change follows. They exist so that the project can grow (new scenarios, modes, analytics, bots, maps) and many contributors can work on it without the codebase decaying. Structure and dependency rules are in [`architecture.md`](architecture.md); this page covers how to write the code inside that structure.

## Enforcement at a glance

| Enforced by | What |
|---|---|
| `cargo fmt --all --check` | Formatting (default rustfmt; no custom config) |
| `cargo clippy --workspace --all-targets` | Warning-free; workspace lints in `Cargo.toml` (`unsafe_code` forbidden, no `dbg!`, `todo!`, `unimplemented!`) |
| `cargo test --workspace` | Unit, content and simulation tests |
| Review | Everything below that a tool cannot check |

A PR that fails the first three is not ready for review.

## Principles, applied to this codebase

### SOLID

| Principle | What it means here | Example in this repo |
|---|---|---|
| **S**ingle responsibility | A module has one reason to change. A Bevy system does one job. | `movement.rs` changes only when the movement model changes; `overlay.rs` only when show/hide behavior changes |
| **O**pen/closed | Extend through data and new variants or implementations, not by editing unrelated code. | New exercises are TOML files; a new `Motion` variant touches its own match arms only |
| **L**iskov substitution | Every implementation of a trait honors its documented contract. Shared contract tests run against every implementation. | Future `RunStore`: the same test suite runs against the CSV and the in-memory store |
| **I**nterface segregation | Small traits; functions take only what they use. | `fn deg_per_count(settings: &Settings, …)`, not the whole `World`; split `SystemParam`s per concern |
| **D**ependency inversion | The core defines ports (traits); adapters implement them. The core never calls the OS. | `Session` gets `dt` and a seed instead of reading a clock or a global RNG |

### YAGNI: build for today's feature, leave room for tomorrow's

- **No speculative abstractions.** A trait needs either two real implementations or a test double that is actually used. A crate needs a trigger from the architecture's split rules.
- **The roadmap tells you where things will go, not what to build now.** For example, `Activity` stays a design note until deathmatch exists.
- **No configuration "just in case".** Every setting needs a user-facing reason.
- **Delete dead code.** Do not comment it out; git remembers.

### DRY: one source of truth for each piece of *knowledge*

- **Constants:** each game constant exists once (`valorant.rs`), with its source documented once (`docs/valorant-mechanics.md`).
- **Shared rules:** reuse the existing function. Bots will move with `movement::step`, the same as the player and strafing targets do today. Never re-implement a rule in a different shape.
- **Rule of three:** duplicate twice if the cases may still diverge; extract on the third occurrence.
- **Same code is not always the same knowledge.** Do not merge code that looks alike but changes for different reasons, such as player bounds and target lanes.
- **Docs follow DRY too:** link to the canonical page instead of repeating it.

### KISS and composition

- Prefer plain data plus functions over clever generics. Choose an `enum` over a trait object when the set is closed (see the architecture's "Enum or trait?").
- Compose small pieces: scenarios are built from shape + spawn + motion + weapon + scoring blocks, not from a class hierarchy.

## Naming

| Item | Convention | Example |
|---|---|---|
| Crates | `aim_<area>` | `aim_core`, `aim_app` |
| Modules and folders | Domain nouns (screaming architecture); never `utils`, `helpers`, `common`, `misc` | `scenario`, `training`, `analytics` |
| Types | Nouns, `UpperCamelCase` | `ScenarioDef`, `RunResult` |
| Functions | Verbs, `snake_case` | `cm_per_360`, `spawn_target` |
| Units | Suffix in the name for every physical quantity | `yaw_deg`, `duration_secs`, `run_speed` (m/s, documented on the field), `radius` (m) |
| Content IDs | `kebab-case`, stable forever (history keys) | `counter-strafe` |

**Units at the boundary:** meters, seconds and degrees in data and public APIs. Radians appear only at the render boundary (`to_radians()` next to Bevy calls).

## Errors and panics

| Situation | Rule |
|---|---|
| User data (TOML, settings, history) | Never panic. Validate, collect every problem, and report it with the file and field (`ValidationError`, the menu error panel) |
| Fallible core APIs | Return `Result` with a typed error (`thiserror`) |
| App adapters (I/O, OS) | Log with `warn!` or `error!` and degrade gracefully, e.g. default settings if the file is broken |
| Programmer errors | `expect("why this cannot fail")`; bare `unwrap()` is for tests only |
| Built-in content | May `panic!` at load, because the test suite guarantees it is valid |

## Determinism, randomness and time

- Core simulations are a pure function of (definition, seed, input sequence). Tests assert it (`simulation_is_deterministic_per_seed`).
- Use `aim_core::rng::Rng` (seeded). Do not use the `rand` crate, a thread-local RNG or `HashMap` iteration order in simulation logic.
- Time enters only as `dt` inside `FrameInput`.

## Performance and latency

Aim training is latency-sensitive. These rules are part of correctness, not optimizations.

| Rule | Why |
|---|---|
| Mouse deltas are applied raw, in the frame they arrive: no smoothing, filtering, acceleration or interpolation | Any filter changes the effective sensitivity and adds lag |
| Rotation math uses the exact degrees per count from `aim_core::sensitivity` | One source for the sensitivity math |
| Keep `PipelinedRenderingPlugin` disabled and use an uncapped, no-vsync present mode | Each removes a frame or more of input-to-photon latency |
| Measure before optimizing (Tracy or `bevy/trace`, plus benchmarks) | Clarity beats speculative micro-optimization |
| Hot paths (per-frame core update, analytics over long recordings) avoid needless allocation once profiling shows they matter | Stable frame times |

## Testing

**Test-first:** write a failing test that reproduces the rule or bug (RED), make it pass (GREEN), then refactor. Bug fixes always come with the test that would have caught them. The bounds-sliding fix `e6c9d8c` is an example.

| Level | Where | What |
|---|---|---|
| Unit | `#[cfg(test)] mod tests` next to the code | Math, movement curves, spawn rules, scoring, validation |
| Content | `scenario::builtin`, `routine`, `benchmark` tests | Every shipped file parses and validates, and every ID is unique |
| Simulation | `sim` tests and the `calibrate` example | Every scenario is playable; better simulated aimers score higher |
| Contract | One shared suite per port trait | Every storage or service implementation behaves the same |
| App | Pure helpers, plus headless Bevy (`MinimalPlugins`) for system logic | Overlay state transitions, cursor rules, IPC round trips |
| Manual | Checklist in the PR | Feel, rendering, platform behavior; state what you tested and on which OS |

Further rules:

- **Test through public interfaces** (`Session::update`, not private helpers). Assert every observable effect: return value, events, stats and persisted data.
- **Scenario balance band:** after a change to any scenario, run `cargo run -p aim_core --release --example calibrate`. The order must be recruit < contender < veteran < elite, and contender accuracy should land at about 45–85% (challenge-point framework, `docs/training-science.md`).
- **Scratch state** (temp dirs, `XDG_*` overrides) lives outside the repository and is removed after the test.

## Accuracy and research

- **Every game constant has a source and a confidence tag** in `docs/valorant-mechanics.md`. Every training claim has a citation in `docs/training-science.md`. No numbers without sources; mark uncertain values as *approximate* or *estimate*.
- **Provisional data stays labelled until real data replaces it,** such as `calibrated = false` on benchmarks.
- **Analytics and recommendations must explain themselves.** Show the metric and the threshold behind every "weakness" or recommendation; never show a black-box verdict.

## Game integrity and intellectual property (non-negotiable)

This project must never put a player's account at risk or infringe Riot's IP.

| Allowed | Not allowed |
|---|---|
| A separate process with its own window and overlay | Reading or writing game memory, injecting DLLs or hooking the game process |
| Official APIs with a valid key, under their terms (Riot API for match and round data) | Screen-reading or automating the game, synthetic input into the game, anything that gives an in-game advantage |
| Game *facts* with sources: speeds, yaw constant, FOV, accuracy thresholds | Shipping Riot assets: models, textures, sounds, maps extracted or replicated from the game, logos |
| Naming a game to describe compatibility ("VALORANT sensitivity") | Using Riot names or logos as the project's name or branding, in domains or as search keywords |
| **Original** training maps that exercise the same skills (angles, peeks, clearing) | Real-scale replicas of VALORANT maps |

Riot's fan policy ("Legal Jibber Jabber", https://www.riotgames.com/en/legal) is noncommercial only and says "We prohibit the use of our IP in games and apps", which is why map replicas are excluded. This is a summary for contributors, not legal advice. When in doubt, ask in an issue before you build.

## Dependencies

- **Add a crate only when it removes real work or risk.** Say why in the PR.
- **Licenses must be compatible** with the project license (currently MIT): prefer MIT, Apache-2.0 or similar permissive licenses; no copyleft in the shipped binary without a maintainer decision.
- **Shared versions go in `[workspace.dependencies]`.**
- **Never add a dependency to `aim_core`** that pulls in an engine, I/O or platform code.

## Comments and documentation

- **Comments explain *why*, not *what*.** Good examples: the reason for a constant, a platform quirk, a link to a source.
- **Every public item in `aim_core` gets a doc comment.** Module docs (`//!`) state the module's responsibility in one or two sentences.
- **Update user-facing docs in the same PR** when behavior changes: README, `docs/scenarios.md`, in-app help text.
- **Language:** code, comments, docs, commits and issues are in **English**, so the project is open to everyone.

## Content files (TOML)

- **File name and ID:** the file name equals `id`; IDs never change after release, because history and benchmarks reference them.
- **Description:** says what the exercise trains and how to play it, in at most four lines.
- **Values:** game-specific values follow the accuracy rules above. Target sizes should be real-world sizes at real distances, not arbitrary angles.
- **Validation:** every new building-block field is validated in `validate()`, with a message that names the field.
- **Compatibility:** never break existing user files silently. A breaking format change needs a migration or a clear load error, plus a changelog note.

## UI (egui) structure

- **Container/presentational:**
  - A **container** is a Bevy system: it reads resources and builds a plain view model.
  - A **view** is a function `fn(ui, &ViewModel) -> Option<Action>`: it draws and returns what the user did.
  - The container then applies the action, e.g. starting a run or saving settings. Views never touch ECS state.
- **Atomic design folders in `aim_app/src/ui/`:**

| Folder | Holds | Examples |
|---|---|---|
| `atoms/` | Painters and labels | Crosshair, stat value |
| `molecules/` | Small composed widgets | Sensitivity row, scenario list item |
| `organisms/` | Panels | Settings panel, results card, benchmark table |
| `pages/` | Full screens | Menu, HUD, results, pause |

- **Every string shown to the user is in one place per page,** so internationalization can be added later without hunting through drawing code.

## Git and review

- **Commits:** Conventional Commits with a scope from the architecture: `feat(core): …`, `fix(app): …`, `feat(content): …`, `docs: …`. One logical change per commit, with its tests and docs. No AI attribution trailers.
- **PR size:** about 400 changed lines or fewer (excluding generated files and lockfiles). Larger work becomes chained PRs.
- **Before opening a PR, use the checklist in [`CONTRIBUTING.md`](../CONTRIBUTING.md).**
