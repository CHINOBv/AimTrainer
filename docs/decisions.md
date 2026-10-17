# Project decisions

This page records what the project is, what was decided and why, what is known to be limited, and what comes next. Read it before changing direction on any of these points. The full log of the first build session is in `odd/tasks/aim-trainer-foundation.md`.

## What we are building

A VALORANT-focused aim trainer for **Arch Linux (Hyprland/Wayland, X11)** and **Windows**, with these properties:

- **Overlay:** a transparent overlay that a hotkey shows and hides.
- **Exact sensitivity:** degrees per count, cm/360 and FOV all match VALORANT.
- **Extensible exercises:** new exercises are data files, not code.
- **Grounded routines:** routines and a benchmark based on real training methods (Voltaic) and published studies.
- **Room to grow:** good performance and an architecture that can expand.

## Decisions at a glance

| # | Topic | Decision | Status |
|---|-------|----------|--------|
| D1 | Stack | Rust + Bevy 0.20 + bevy_egui 0.43 | done |
| D2 | Architecture | Engine-agnostic `aim_core` plus a Bevy adapter `aim_app` | done |
| D3 | Overlay type | **Option B:** a transparent window over other apps or the game, not an in-app HUD | done |
| D4 | Overlay toggle | Global hotkey on Windows/X11; `aimtrainer toggle` over IPC on Linux/Wayland | done |
| D5 | Sensitivity | Exact °/count from raw, unaccelerated mouse counts | done |
| D6 | FOV | Fixed vertical 70.53° (Hor+), which gives 103° horizontal at 16:9 | done |
| D7 | Movement | Replays the stopping curves Riot measured; no fitted friction formula | done |
| D8 | Exercises | Data-driven TOML built from composable building blocks | done |
| D9 | Routines | Voltaic structure plus motor-learning research | done |
| D10 | Benchmark | Voltaic-style energy and harmonic mean, with **provisional** thresholds | needs real data |
| D11 | Play-area bounds | Bounds act like walls: the player slides along them | done (bug fix) |
| D12 | Code standard | Binding architecture and coding standards for open-source growth | done |
| D13 | Maps and IP | **Original** tactical maps, not VALORANT replicas; no Riot assets; never touch the game process | policy |
| D14 | Primary training method | A VDIM-style weekly plan (one skill per day, drills → variants → benchmark, tiers by rank), built with our own scenarios and credited | planned (roadmap Phase 2) |

## Details

### D1 Stack: Rust + Bevy + egui

- **Why:** the user asked for "buen rendimiento" and something that "pueda crecer". Rust is as fast as C++ without memory bugs. Bevy uses Vulkan on Arch and DX12/Vulkan on Windows, and its ECS lets new modes and systems be added without rewrites. egui is fast for menus and settings.
- **Rejected:**
  - Godot: faster to prototype, but less control over input latency.
  - C++ with SDL3: full control, but slower to develop and more error-prone.
- **Latency choices:**
  - `PipelinedRenderingPlugin` is disabled because it adds a frame of latency.
  - `PresentMode::AutoNoVsync` is used.
  - The Hyprland rule sets `immediate = true` to allow tearing.

### D2 Engine-agnostic core

- **What lives in `aim_core`:** every gameplay rule. That covers sensitivity, FOV, movement, VALORANT data, the scenario simulation, scoring, routines, the benchmark and the simulated aimer.
- **What `aim_app` does:** it turns input into a `FrameInput` and draws whatever the session reports.
- **Why:** each exercise can be unit-tested without a window or GPU, and the engine can be swapped later.

### D3–D4 Overlay

| Platform | How it toggles | How it hides |
|---|---|---|
| Windows | Global hotkey (`global-hotkey` crate), default **Ctrl+Shift+F10** | The window is hidden |
| X11 | Global hotkey | The window is hidden |
| Wayland/Hyprland | A compositor bind runs `aimtrainer toggle` over a Unix socket | The window stays mapped but transparent and click-through, because Wayland clients cannot hide or raise themselves |

**What "hidden" means everywhere:** nothing is drawn, mouse input passes through, the cursor is released and the run pauses.

**Requirements and limits:**

- **VALORANT must be in Windowed Fullscreen.** Exclusive fullscreen covers every other window.
- **Valorant does not run on Linux** (the Vanguard kernel anti-cheat), so on Linux the overlay sits over the desktop or other apps.
- **The trainer is a separate process:** no memory reads, no input injection. It is still not endorsed by Riot.

**Hyprland config:** the user's Hyprland is configured in Lua, with overrides in `~/.config/hypr/user.lua` (`hl.bind`, `hl.window_rule`). We documented the snippet in the README and **did not edit the user's config**.

### D5–D6 Sensitivity and FOV

- **Rotation:** 0.07°/count × sens, for yaw and pitch.
- **Raw input:**
  - Windows uses relative Raw Input.
  - Wayland uses `zwp_relative_pointer` `dx_unaccel`.
  - Both paths were verified in the winit 0.30.13 source.
- **ADS zoom:** zoomed °/count = base × multiplier / zoom. A multiplier of 1.0 is a 100% horizontal monitor match.
- **DPI check tool:** you move the mouse 10 cm and it reports the real DPI. It catches a wrong DPI setting or OS acceleration.
- Sources and confidence levels for every number are in `docs/valorant-mechanics.md`.

### D7 Movement

- **Speeds:** 5.4 m/s with rifles, 6.75 m/s with the knife.
- **Accuracy threshold:** 27.5% of max speed (15% for the Operator).
- **Deceleration:** replays a Riot designer's measurements, as fractions of run speed.

| Situation | 61% | 25% | Stopped |
|---|---|---|---|
| Key released | 0.065 s | 0.110 s | 0.170 s |
| Counter-strafe | 0.055 s | 0.104 s | 0.160 s |

- **Approximate values:** acceleration of 18.75 m/s² (community-derived), walk at 0.6× run speed, and the linear spread model.

### D8 Data-driven exercises

- A scenario is composed of these building blocks:
  - `shape`: sphere or humanoid;
  - `spawn`: angular, around the crosshair, or floor;
  - `motion`: static, strafe or drift;
  - `weapon` and fire mode;
  - `player` movement;
  - `scoring`.
- **14 built-ins**, covering flicking, microcorrections, switching, tracking, movement (counter-strafe, strafe-shot), angle holding and long-range precision.
- **User files:** in `~/.config/aimtrainer/scenarios/`. A file with the same `id` as a built-in replaces it.
- **Strict parsing:** unknown keys are rejected so typos fail loudly. The menu lists files that failed to load.

### D9 Routines (research-based)

| Principle | Source | Applied as |
|---|---|---|
| 30–60 min a day | Listman et al. 2021 (7,174 Aim Lab players) | `valorant-daily` lasts about 37 min |
| Interleave exercises | Shea & Morgan 1979 (contextual interference) | `interleave_from` shuffles runs and avoids back-to-back repeats |
| Tracking → clicking → switching | Voltaic's skill routines | `fundamentals-hour` follows this order |
| Target ~80% success | Guadagnoli & Lee 2004; Wilson et al. 2019 | Scenarios are tuned so the mid-level simulated aimer lands at roughly 45–85% accuracy |

Full citations are in `docs/training-science.md`.

### D10 Benchmark

- **Same rules as Voltaic:**
  - a subcategory's energy comes from its best scenario;
  - overall energy is the harmonic mean of the subcategories;
  - there is no overall energy until every subcategory has a score.
- **Thresholds are provisional (`calibrated = false`).** KovaaK's thresholds cannot be copied because our scenarios and scoring differ. Ours were generated by a simulated aimer (Fitts' law + reaction delay + endpoint error) at 4 skill levels. Regenerate them with `cargo run -p aim_core --release --example calibrate`.

### D11 Play-area bounds (bug fix)

- **Reported by the user:** in the strafe scenarios, movement in some directions was sometimes blocked.
- **Cause:** touching a bound zeroed the **whole** velocity. The counter-strafe area is only 2 m deep, so this happened often.
- **Fix:** only the velocity component pointing into the bound is removed, so the player slides along it. Covered by tests (commit `e6c9d8c`).

### D12 Code standard

- **Where it lives:** `docs/architecture.md` (structure, dependency rule, extension points, conformance backlog) and `docs/coding-standards.md` (principles, naming, errors, testing, latency, integrity), with the planned features in `docs/roadmap.md` and contributor entry points in `CONTRIBUTING.md`.
- **Enforced by tools:**
  - workspace lints (`unsafe_code` forbidden; `dbg!`, `todo!` and `unimplemented!` warned);
  - clippy warning-free;
  - `cargo fmt`.
- **Why:** the project is meant to become open source and keep growing (analytics, bots, maps, integrations) without decaying.
- **Existing gaps:** the code that predates the standard is tracked as conformance backlog C1–C10, to be fixed in separate refactor PRs.

### D13 Maps, assets and the game process

- **The request:** recreate VALORANT maps at real scale to practise clearing angles and peeks.
- **The conflict:** Riot's fan policy (https://www.riotgames.com/en/legal) says "We prohibit the use of our IP in games and apps" and "Please do not take any part of our IP". Map layouts are Riot IP.
- **Decision:** build **original** maps at real-world scale that train the same skills, using common situations such as doorways, long angles, off-angles and boxes. No replicas and no extracted assets.
- **Reading rounds:** only official APIs with a valid key under their terms (Riot's policy names Riot API keys as the sanctioned route), or files the player exports. Never memory or screen reading.
- **Caveat:** this is a conservative reading of the policy, not legal advice.

### D14 Primary training method: VDIM-style

- **The request:** the user named VDIM (Voltaic Daily Improvement Method) as the main method or playlist to add.
- **Decision:** make it the primary method, as roadmap Phase 2.
  - Same week: Clicking I/II, Tracking I/II, Switching I/II, and a flexible Sunday.
  - Same day structure: 3 blocks of drills → variants (Hard last) → benchmark ×4.
  - Tiers are chosen from the benchmark rank.
- **Fits the current model:** a VDIM day is an ordered routine.
- **New pieces needed:**
  - benchmark tiers;
  - authored difficulty variants;
  - a weekly method definition (`training::method`, `content/methods/`);
  - routine progress that persists, so a split day can be resumed;
  - a "Today" page.
- **Attribution:** we credit VDIM (Lowgravity56, 4rK, Voltaic) and use original scenarios. Ask Voltaic before naming anything after VDIM.
- **Tradeoff recorded in `training-science.md`:** VDIM's ~90 min days versus the ~1 h daily-gain peak in Listman 2021, plus blocking versus interleaving.

### D15 Flick targets stay on screen

- **The problem:** wide flicks spawned 20–70° from the crosshair (headshot flicks 10–60°). VALORANT shows 103° × 70.53°, which is ±51.5° sideways and ±35.27° vertically from the crosshair, so many targets spawned off-screen. The player had to search for the target instead of flicking to it.
- **Why it matters:** a flick is a ballistic move to a target you can already see. Searching first adds a visual-search reaction to every rep and trains something else.
- **Decision:** `around_crosshair` gets `on_screen = true`, which keeps every spawn inside VALORANT's view from the crosshair at spawn time.
  - The margin is 4°, a design choice so the target is not clipped by the screen edge. The limits are 47.5° sideways and ≈31.3° vertically (`fov::valorant_on_screen_half_h_deg`, `valorant_on_screen_half_v_deg`).
  - `flick-wide` now uses 20–47° and `flick-headshot` 10–47°. They stay wide flicks: up to about 90 % of the half-screen.
  - Difficulty scaling clamps the offset to the on-screen limit.
- **Backup:** a semi-transparent chevron at the screen edge points to every target outside the current view (including the ADS zoom), for scenarios that can still leave the screen. It is on by default (`offscreen_arrows`) and does not show in arenas.

## Known limitations

| Limitation | Impact |
|---|---|
| **Not tested on Windows** | The build and window transparency need verification, which may depend on the GPU driver |
| **Input/camera glue in `aim_app` has no automated tests** | Regressions there only show up when playing |
| Approximate hitbox dimensions | Riot does not publish them; values are in `HumanoidDims` |
| Provisional benchmark thresholds | Ranks are guidance, not a real comparison with other players |
| No controlled study shows aim training transferring to in-game skill | Use the trainer alongside real games |
| Determinism is per platform | `sin`, `cos` and `atan2` come from each OS's math library, so a replay recorded on one OS can drift when replayed on another, and simulated scores differ slightly between machines. Tests compare results computed in the same process, never pinned simulated numbers |

## Next steps

- [ ] Playtest on Hyprland and Windows, and record feel issues.
- [ ] Recalibrate the benchmark from real runs (`history.csv`).
- [ ] Adaptive difficulty: a staircase that holds about 80% success.
- [ ] Voltaic-style reload mechanic (misses cost ammo).
- [ ] History charts and per-skill trends.

## Workflow and repository

- **Remote:** `origin` = https://github.com/CHINOBv/AimTrainer.
- **Branches:** the default branch is `main`. Feature work happens on branches, in sibling worktrees under `../CustomAimTrainer-worktrees/`.
- **Commits:** Conventional Commits, with no AI attribution lines.
