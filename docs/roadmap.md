# Roadmap

This is where the project is going, and which part of the architecture owns each piece. Phases are ordered by dependency: each one builds on data or modules from the previous ones. The roadmap guides placement; it is **not** permission to pre-build (YAGNI, see [`coding-standards.md`](coding-standards.md)).

| Phase | Theme | Main owner | Depends on |
|---|---|---|---|
| 0 ✅ | Foundation: exact sensitivity, VALORANT movement, 14 scenarios, routines, benchmark, overlay | `aim_core`, `aim_app` | — |
| 1 | Open-source readiness | repository | 0 |
| 2 | Player tools: custom crosshairs, sensitivity converter | `aim_core::crosshair`, `aim_core::sensitivity` + `games` | 1 |
| 3 | Analytics: flicks, corrections, strengths and weaknesses, recommendations | `aim_core::analytics`, `training::recommend` | 1 |
| 4 | Adaptive training: dynamic difficulty, personalized playlists | `aim_core::training` | 3 |
| 5 | Game-mixed training: activities, deathmatch with bots | `aim_core` (`Activity`, bots), new app plugin | 4 |
| 6 | Tactical drills: original maps for angles, peeks and clearing | `aim_world` (new crate) | 5 |
| 7 | Match integration: read rounds, then suggest or rebuild drills | `aim_integrations` (new crate) | 3, 6 |

## Phase 1: open-source readiness

- [ ] **License.** `Cargo.toml` declares MIT, but no `LICENSE` file exists yet. The project owner decides between MIT and the Rust-ecosystem norm, MIT OR Apache-2.0.
- [ ] **CI on Linux and Windows:** `fmt --check`, `clippy`, `test`. This also gives Windows its first build check.
- [ ] **Conformance backlog** C1–C4 from [`architecture.md`](architecture.md): content folder, UI split, app folders, session split.
- [ ] **Manual Windows verification:** transparency, hotkey, Raw Input.
- [ ] **Issue templates:** bug report (OS, GPU, compositor) and scenario proposal.

## Phase 2: player tools

**Custom crosshairs**
- `aim_core::crosshair`: one crosshair model (lines, outline, dot, opacity, gap), presets in `content/crosshairs/`.
- Import and export VALORANT crosshair profile codes. *The format must be researched and documented with sources before implementation.*
- egui painting stays in `ui/atoms`.

**Sensitivity converter**
- Data-driven game profiles: yaw constant, FOV model (Hor+ / Vert−, base FOV), zoom model. Each value needs a documented source.
- Matching methods: 360° distance (today), plus monitor-distance matching (0%, 75%, 100%) for zoomed and different-FOV cases.
- A standalone converter page that does not require a run.

## Phase 3: analytics (Aimlabs-style feedback)

Analytics are pure functions over recorded runs. Each one explains itself: it shows the metric and the threshold behind the verdict.

| Step | What | Notes |
|---|---|---|
| Telemetry | Store each run's `FrameInput` log plus seed; re-simulate it with the deterministic `Session` to recover everything | Small storage footprint; requires the determinism rule |
| Flick analysis | Segment movements by angular speed. Per flick: reaction time, movement time, peak speed, overshoot or undershoot, number of corrective sub-movements, settle time, path efficiency | Fitts' law: compute throughput per ISO 9241-9 for an objective speed–accuracy metric |
| Tracking analysis | Time on target, lag versus lead error, reaction to direction changes | Uses target velocity from the session |
| Skill profile | Per-subcategory strengths and weaknesses from the benchmark and trends over time | Needs real data (benchmark recalibration) |
| Recommendations | Rules (data) that map weaknesses to scenarios, routines or tips | Example: "overshoots 60% of flicks → slow-flick drills, check sensitivity" |
| Charts | Per-scenario history, per-skill trends | `ui/organisms` |

Before this phase starts, a `RunStore` port moves history behind a trait (backlog C7).

## Phase 4: adaptive training

- **Difficulty knobs:** scenarios declare which parameters scale (target size, speed, lifetime, count) and their easy/hard bounds.
- **Controller:** a staircase that keeps success around **80%** (challenge-point framework; Wilson et al. 2019; see `training-science.md`).
- **Personalized playlists:** built from the skill profile, with weak skills weighted up and interleaved (Shea & Morgan 1979).
- **Benchmark recalibration:** set thresholds from real-player score percentiles, then flip `calibrated = true`.
- **Reload mechanic** for clicking scenarios: misses cost ammo, as in Voltaic's S5 clicking scenarios (https://app.voltaic.gg/blog/announcing-the-voltaic-season-5-aiming-benchmarks-beta-for-kovaaks/). It is a new scoring building block in `scenario/`.

## Phase 5: game-mixed training and bots

- **`Activity` trait:** introduce the trait sketched in the architecture; the scenario session is its first implementation. Routines then sequence activities.
- **Hybrid activities:** game-like objectives combined with aim drills (similar in spirit to Aimstars on Roblox, which mixes ranked scenarios with game modes and human-like AI).
- **Deathmatch bots:**
  - Movement with `movement::step` and per-game movement profiles. VALORANT comes first; other games only with sourced values.
  - Aim with `sim::SimAimer`: the Fitts-law model already provides reaction time, movement time and endpoint error, so difficulty is a profile.
  - Behavior (peek, strafe, hold) as a separate, testable state machine.

## Phase 6: tactical drills on original maps

- **New crate `aim_world`:** map loading (glTF), collision, line-of-sight and occlusion for hit tests, spawn and angle markers.
- **Maps are original:** common tactical-shooter situations (doorways, boxes, long angles, off-angles, multi-angle clears) built at real-world scale. **No replicas of VALORANT maps** (Riot's fan policy forbids using their IP in games and apps; see the integrity section of the coding standards).
- **Drills:** angle clearing, jiggle-peeks, crosshair placement at head level, pre-aim, with analytics hooks such as crosshair-placement error at first sight.

## Phase 7: match integration (later)

- **New crate `aim_integrations`,** feature-gated.
- **Data sources:** only official, documented sources, such as the Riot API with a valid key under its terms, or files the player exports. **Never memory reading or screen capture of the game** (account-ban risk and against the integrity rules).
- **Data to verify:** what round and kill data the official API exposes must be researched and documented before design.
- **Use:** map round events to the skill profile, recommend drills, and rebuild the *situation* (distances, angles, timings) in original geometry to show the error and train it.

## Not planned

- Anything that gives an in-game advantage or touches the game process.
- Monetization, which Riot's policy restricts for fan projects using its IP.
