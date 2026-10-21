# Architecture

**This document is binding.** Gameplay, training and analysis rules live in a pure, deterministic domain core. Engines, windows, files and the network are adapters that sit around it. Folders are named after what the product does (screaming architecture), never after technical layers. When code and this document disagree, fix the code, or change the document in the same PR.

- Principles and day-to-day rules: [`coding-standards.md`](coding-standards.md).
- Feature plan: [`roadmap.md`](roadmap.md).

## Quick path

1. **Gameplay rule** (a target behavior, scoring, movement, difficulty, an analysis): put it in `aim_core`, as a pure function or a type, with tests.
2. **New exercise, routine or benchmark:** a TOML file under `content/`. No Rust needed.
3. **Something the player sees or touches** (rendering, input, menus, overlay, files): put it in `aim_app`. It translates input into core calls and draws core state; it decides nothing.
4. **Talking to the outside world** (disk, OS hotkeys, IPC, later the Riot API): an adapter in `aim_app`, or a dedicated integration crate, behind a port (trait) defined by the core.

## Layers and the dependency rule

```
          ┌────────────────────────────── aim_app (Bevy) ─────────────────────────────┐
          │ ui/        game/       overlay/      platform/      persistence/          │
          │ (egui)     (render,    (show/hide)   (IPC, hotkey)  (settings, history)   │
          │            input)                                                         │
          └──────────────────┬────────────────────────────────────────────────────────┘
                             │ depends on ↓ (never the reverse)
          ┌──────────────────▼──────────── aim_core (pure, deterministic) ────────────┐
          │ scenario/   training/   analytics/*   sensitivity/   games/   movement/   │
          │ sim/        crosshair/*                          shared kernel: math, rng │
          └───────────────────────────────────────────────────────────────────────────┘
          * = planned (see roadmap)                   content/  ← data loaded by both
```

| Rule | Why |
|---|---|
| `aim_core` never depends on Bevy, egui, winit, the filesystem, the clock, threads or the network | Every rule stays testable without a GPU, and the engine can be replaced |
| Dependencies point inward, from adapters to the core | Changing the UI never forces a change in the domain |
| The core receives time as `dt` and randomness as a seed | Simulations are reproducible, which powers tests, replays and analytics |
| Side effects cross a **port**: a trait defined in the core and implemented in the app | Dependency inversion; tests use in-memory fakes |
| Content (`content/`) is data, validated by the core | Contributors add exercises without touching Rust |

## Repository layout

### Target layout

New code follows this layout; existing code migrates as described in [Conformance backlog](#conformance-backlog).

```
.
├── Cargo.toml                  workspace: members, shared dependency versions, lints
├── content/                    data that contributors edit (TOML, later glTF maps)
│   ├── scenarios/<id>.toml     file name == id
│   ├── routines/<id>.toml
│   ├── benchmarks/<id>.toml
│   ├── methods/<id>.toml       planned: weekly training methods (VDIM-style plan)
│   ├── games/<id>.toml         planned: sensitivity profiles for the converter
│   └── crosshairs/<id>.toml    planned: crosshair presets
├── crates/
│   ├── aim_core/               domain library (pure)
│   │   ├── src/
│   │   │   ├── lib.rs          module list + crate docs only
│   │   │   ├── math.rs rng.rs  shared kernel: tiny, stable, no domain knowledge
│   │   │   ├── sensitivity/    cm/360, eDPI, conversions, zoom
│   │   │   ├── games/          per-game constants (VALORANT today), FOV models
│   │   │   ├── movement/       ground movement model, stop curves
│   │   │   ├── scenario/       def (data), session (runtime), spawn, motion, weapon, scoring, hit
│   │   │   ├── training/       routine, benchmark, method* (weekly plans), progress*, difficulty*, recommend*
│   │   │   ├── analytics/*     telemetry, flick segmentation, skill profile
│   │   │   ├── coach/          AI coach: player context, knowledge base, drills, tools, tool loop;
│   │   │   │                   ports ChatModel and CoachMemory (providers and stores are adapters)
│   │   │   ├── crosshair/*     crosshair model, import/export codes
│   │   │   └── sim/            simulated aimer (tests, calibration, bot aim later)
│   │   ├── examples/           developer tools (calibrate)
│   │   └── tests/              cross-module tests through the public API
│   ├── aim_ai/                 AI coach adapters (see docs/coach.md): config, API keys (OS keyring,
│   │                           env fallback), chat providers, SQLite coach memory
│   ├── aim_app/                Bevy front-end (binary `aimtrainer`)
│   │   └── src/
│   │       ├── main.rs         CLI parsing + App assembly only
│   │       ├── game/           states, run lifecycle, camera, target visuals, input
│   │       ├── ui/             egui: pages/ organisms/ molecules/ atoms/ (see coding standards)
│   │       ├── overlay/        visibility, click-through, hotkey wiring
│   │       ├── platform/       OS specifics: ipc (Unix socket), hotkeys
│   │       └── persistence/    settings, history: port implementations
│   └── aim_mcp/                MCP server (binary `aimtrainer-mcp`, stdio, rmcp): serves the core
│                               coach tools and the built-in scenarios to coding agents (docs/coach.md)
├── docs/                       architecture, standards, research with sources, decisions
└── odd/tasks/                  feature work logs
```

### When to split a module into its own crate

Keep a module inside `aim_core` until **one** of these is true:

| Trigger | Example |
|---|---|
| It needs a dependency the rest of the core must not have | Maps need a glTF loader and collision: `aim_world` |
| It is a separate product surface with its own release cadence | Riot API client: `aim_integrations` |
| It is a separate process with its own protocol dependencies | MCP server for coding agents (rmcp, tokio): `aim_mcp` |
| Compile times of the core become painful (measure first) | Analytics with heavy math crates |

Splitting early is YAGNI. A crate boundary costs API design, versioning and compile coordination.

## Extension points

This is where the codebase is designed to grow. Prefer extending one of these over inventing a new mechanism.

| To add | Do this | Touches |
|---|---|---|
| An exercise | A TOML file in `content/scenarios/` | Data only |
| A routine or playlist | A TOML file in `content/routines/` | Data only |
| A weekly training method (VDIM-style) | A TOML file in `content/methods/` mapping tier × weekday to routines | Data only, once `training::method` exists |
| A target shape, spawn area, motion or scoring rule | A new variant in `scenario/def.rs`, its logic in the matching `scenario/` module, `validate()`, and tests | `aim_core` |
| A game to the sensitivity converter | A profile (yaw constant, FOV model) in `games/`; later a TOML file in `content/games/` | `aim_core` |
| An analysis metric | A pure function `fn(&Telemetry) -> Metric` in `analytics/` | `aim_core` |
| A recommendation rule | Data that maps weaknesses to scenarios or routines, evaluated in `training/recommend` | `aim_core` + content |
| A new activity type (deathmatch round, map drill) | A new `Activity` implementation and `ActivityKind` variant (see below) | `aim_core` or a domain crate + an app plugin |
| A screen or panel | A page or organism in `aim_app/src/ui/` | `aim_app` |
| A storage backend or external service | Implement the core's port trait in `aim_app/src/persistence/` or an integration crate | adapter |

### Enum or trait?

| Use | When | Today |
|---|---|---|
| `enum` + `match` | The set of variants is closed and owned by this repository, and the data comes from TOML | `Shape`, `SpawnArea`, `Motion`, `FireMode` |
| `trait` | Implementations live in different modules or crates, or tests need fakes | Ports (storage, clock-free services), future `Activity`, analyzers |

Adding an enum variant is a small, checked change, because the compiler lists every `match` to update. Do not replace enums with trait objects "for extensibility" until a real out-of-crate implementation exists.

## Core runtime model

```
FrameInput { dt, view angles, buttons, move keys }  ──►  Session::update  ──►  Vec<SessionEvent>
                                                             │
                                          targets(), stats(), result()  ──►  rendering / HUD / history
```

- **`Session` is deterministic:** the same `ScenarioDef`, seed and `FrameInput` sequence produce identical results. Never break this. The planned analytics and replays depend on it: a run can be stored as its input log and re-simulated.
- **Score proofs depend on it too:** `analytics::proof` shares a run as a code (run key, content hash of the definition, seed, inputs, claimed score) and verifies it by re-simulating against the verifier's own library. Any change to `Session` that alters results invalidates proofs made with an earlier build, as it does saved replays.
- **No hidden inputs:** the session reads nothing that is not in `FrameInput`, such as globals, wall-clock time or the OS.
- **Simulation runs at the render frame rate,** with the real `dt`. Mouse look is applied in the same frame it arrives; it is never smoothed or delayed (see the latency rules in the coding standards).

### Player-care and journal modules

These modules are pure and keep their state as plain serde types or values passed in; the app stores them and supplies the clock as Unix seconds.

| Module | Holds |
|---|---|
| `training::journal` | Ranked match entries with that day's readiness and fatigue, losing streak, tilt-reset routine |
| `training::ergonomics` | `BreakPolicy` (continuous play, weekly cap), stretch list and disclaimers |
| `training::care` | Guided hand-care routines (mobility, stretches, posture) and their step timer, `CareSession` |
| `training::warmup` (dose) | Personal warm-up length from past sessions |
| `training::progress` (plateaus) | Benchmark subcategories without a new personal best |
| `sensitivity::dpi_check` | Real DPI from a measured movement, corrected sensitivity |
| `analytics::proof` | Score proof share codes and their verification |

### Activities

A routine sequences **activities**. `aim_core::activity` defines the trait, the shared event vocabulary and `ActivityKind` (which kind a routine block names):

```rust
/// Anything the player can do for a while that produces a result.
pub trait Activity {
    fn update(&mut self, input: FrameInput) -> Vec<ActivityEvent>;
    fn is_finished(&self) -> bool;
    fn result(&self) -> RunResult;
    fn time(&self) -> f32;
    fn view(&self) -> Option<ViewAngles> { None }
}
```

- The scenario `Session` is the first implementation. `aim_world::arena::ArenaSession` is the second; the trait lives in `aim_core`, so `aim_world` depends on the core and not the other way round.
- Each session keeps its own rich event type (`SessionEvent`, `ArenaEvent`) for its own renderer. `ActivityEvent` holds only what every activity shares.
- A routine `Block` names a `scenario` or an `arena`. `PlannedRun::kind` tells the app which plugin starts the run. Arena ids are checked by `RoutineDef::validate_arenas`, because only the caller knows the arena catalogue.
- To add a third activity, implement the trait, add an `ActivityKind` variant and handle it where the navigator starts planned runs.

## Bevy adapter rules

| Rule | Detail |
|---|---|
| One `Plugin` per feature folder | `GamePlugin`, `UiPlugin`, `OverlayPlugin` and so on; `main.rs` only assembles plugins |
| Systems are thin | Read ECS state, call the core, write ECS state; no gameplay math in systems |
| Resources hold global state, components hold per-entity state | The active `Run` is a resource; target visuals are entities |
| Group related parameters | When a system needs more than 7, use a `#[derive(SystemParam)]` struct, not `#[allow(clippy::too_many_arguments)]` |
| Order with sets, not long `.chain()`s | Only when ordering grows beyond one plugin |
| UI follows container/presentational | Container systems build plain view models; view functions draw them and return actions |
| Every system is checked for conflicting access | Each module with systems has a test calling `bevy::ecs::system::assert_is_system`; a page never requests a resource that its `Navigator` already holds (Bevy error B0002) |

## Conformance backlog

These are known gaps between the current code and this document. Fix them **when you next touch the area**, or in a dedicated refactor PR. Never mix one into an unrelated feature PR.

| # | Gap | Target |
|---|---|---|
| C1 ✅ | ~~Content TOML lives in `crates/aim_core/{scenarios,routines,benchmarks}`~~ | Done: moved to `content/` |
| C2 ✅ | ~~`aim_app/src/ui.rs` mixes ECS access and drawing~~ | Done: `ui/{atoms,molecules,organisms,pages}`; pages are containers, views return `UiAction`, applied by `ui::navigation::Navigator` |
| C3 ✅ | ~~Flat files `game.rs`, `overlay.rs`, `ipc.rs`, `settings.rs`, `history.rs`~~ | Done: `game/`, `overlay/`, `platform/` (ipc, hotkey), `persistence/` (settings, history, library) |
| C4 ✅ | ~~`scenario/session.rs` does spawning, motion, weapons and scoring~~ | Done: `scenario/{spawn,motion,weapon,scoring}.rs`; `Session` orchestrates |
| C5 ✅ | ~~`valorant.rs` and `sensitivity.rs` are flat modules~~ | Done: `games/` (VALORANT mechanics and every converter game profile) and `sensitivity/` (conversion, matching, finder) |
| C6 ✅ | ~~The crosshair model lives in `aim_app/settings.rs`~~ | Done: `aim_core::crosshair` (model, VALORANT codes, presets in `content/crosshairs/`) |
| C7 ✅ | ~~History is written as CSV directly by the app; no port~~ | Done: `aim_core::training::store::RunStore` (queries live in the core); `aim_app::persistence::history::History` is the CSV adapter. Routine positions use the same pattern: `training::resume::PositionStore` |
| C8 ✅ | ~~`#[allow(clippy::too_many_arguments)]` on Bevy systems~~ | Done: `SystemParam` groups (`Navigator`, `RunFinisher`, `PlayerInput`, `SettingsAccess`) |
| C11 ✅ | ~~`routine.rs`, `benchmark.rs` at the crate root~~ | Done: `training/{routine,benchmark}` |
| C9 ✅ | ~~Public items in `aim_core` are not all documented~~ | Done: every public item is documented and `#![warn(missing_docs)]` (an error under CI's `-D warnings`) keeps it that way |
| C10 ✅ | ~~Bevy and bevy_egui versions are declared in `aim_app`~~ | Done: `[workspace.dependencies]` |
