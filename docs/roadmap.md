# Roadmap

This is where the project is going, and which part of the architecture owns each piece. Phases are ordered by dependency: each one builds on data or modules from the previous ones. The roadmap guides placement; it is **not** permission to pre-build (YAGNI, see [`coding-standards.md`](coding-standards.md)).

| Phase | Theme | Main owner | Depends on |
|---|---|---|---|
| 0 ✅ | Foundation: exact sensitivity, VALORANT movement, 14 scenarios, routines, benchmark, overlay | `aim_core`, `aim_app` | — |
| 1 ✅ | Open-source readiness | repository | 0 |
| 2 ✅ ⭐ | **Primary training method:** a weekly skill-focus plan in the style of Voltaic's VDIM | `aim_core::training` + `content/` | 0, 1 (C1, C4) |
| 3 ✅ | Player tools: custom crosshairs, sensitivity converter | `aim_core::crosshair`, `aim_core::sensitivity` + `games` | 1 |
| 4 ✅ | Analytics: flicks, corrections, strengths and weaknesses, recommendations | `aim_core::analytics`, `training::recommend` | 1 |
| 5 ✅ | Adaptive training: dynamic difficulty, personalized playlists | `aim_core::training` | 2, 4 |
| 6 ✅ | Game-mixed training: activities, deathmatch with bots | `aim_core` (`Activity`, bots), new app plugin | 5 |
| 7 ✅ | Tactical drills: original maps for angles, peeks and clearing | `aim_world` (new crate) | 6 |
| 8 ✅ | Match integration: read rounds, then suggest or rebuild drills | `aim_integrations` (new crate) | 4, 7 |

## Status (2026-10-17)

Phases 1–8 are implemented on `feat/roadmap-completion`. The feature analysis that followed added 12 features for ranked days; see [`feature-analysis.md`](feature-analysis.md).

| Phase | What shipped | Deviations from the plan, and why |
|---|---|---|
| 1 | CI on Linux and Windows; issue and PR templates; MIT license; conformance backlog C1–C4, C8, C10, C11 | — |
| 2 | Generic difficulty scaling (`scenario::difficulty`); six VDIM-style day routines; `training::method`; Today page with tiers chosen by benchmark energy and resumable split days | Difficulty variants are *generated* from one definition (`at_difficulty`) instead of hand-written files. This is DRY, and Phase 5 needed it anyway |
| 3 | `aim_core::crosshair` with VALORANT code import/export; crosshair editor and renderer; data-driven sensitivity converter with monitor-distance matching | — |
| 4 | Deterministic `Recording` + replay codec; flick/tracking analysis; explainable recommendations; history chart; replay viewer | — |
| 5 | Adaptive staircase (~80% success); personalized playlist; ammo economy (reload mechanic) | — |
| 6 | `aim_world::arena` deathmatch with bots using VALORANT movement and a Fitts-law aim model | No shared `Activity` trait: arenas run as their own resource next to scenario runs. A trait would have added no behavior (YAGNI); revisit if a third kind of activity appears |
| 7 | `aim_world` maps (original geometry), line of sight, collision, navigation; angle-clearing arena with a crosshair-placement metric | — |
| 8 | `aim_integrations` offline analysis of official match JSON; `aimtrainer analyze-match` | No online fetch: it needs a Riot production key and RSO, which a personal or open-source build cannot ship. See `match-analysis.md` |

## Phase 1: open-source readiness

- [x] **License.** MIT (`LICENSE`), matching `Cargo.toml`. Switching to MIT OR Apache-2.0 remains possible while all contributions are from the owner.
- [x] **CI on Linux and Windows:** `fmt --check`, `clippy`, `test` (`.github/workflows/ci.yml`). The Windows build was also cross-checked locally with `cargo check --target x86_64-pc-windows-msvc`.
- [x] **Conformance backlog** C1–C4 from [`architecture.md`](architecture.md): content folder, UI split, app folders, session split. C8 and C10 are done too.
- [ ] **Manual Windows verification:** transparency, hotkey, Raw Input. It compiles for Windows; running it needs a Windows machine.
- [x] **Issue templates:** bug report (OS, GPU, compositor) and scenario proposal, plus a PR template.

## Phase 2: primary training method (VDIM-style weekly plan) ⭐

This is the project's main training method. It is modelled on **VDIM, the Voltaic Daily Improvement Method**, created by Lowgravity56 (2022). The S5 version (2025) is co-credited to 4rK and published as "by Lowgravity56 & Voltaic". The full research, with sources, is in [`training-science.md`](training-science.md#vdim-voltaic-daily-improvement-method).

**Attribution and IP.** We reproduce the *method*, meaning the structure and the run counts, with **our own scenarios**, and we credit VDIM in the UI and docs. We do not copy KovaaK's or Aimlabs scenario files, and we do not use the "Voltaic"/"VDIM" names as our own branding. Before we name anything after VDIM, we ask Voltaic.

**How VDIM is structured (what we implement):**

| Element | VDIM S5 | Our version |
|---|---|---|
| Week | Mon Clicking I, Tue Clicking II, Wed Tracking I, Thu Tracking II, Fri Switching I, Sat Switching II | Same skill split, mapped to our benchmark subcategories |
| Sunday | Benchmark focus routine, replay a weak day, or rest | Same three options; the "weak day" comes from the skill profile once Phase 4 exists |
| Day = 3 blocks | One block per benchmark scenario of that day | Same |
| Block order | Related drills (2–3 runs each) → harder benchmark variants, "Hard" last (2–3 runs) → benchmark (4 runs) | Same; variants come before the benchmark so it "feels easier" |
| Tiers | Entry, Novice, Adept, Intermediate, Advanced, Elite, chosen from the benchmark rank | Tiers chosen from our benchmark rank; the player can move up or down |
| Progression | No thresholds of its own; the day's benchmark runs track progress | Same; best benchmark scores feed the benchmark energy |
| Length | About 1.5 h a day; can be split into 2–3 sessions, or runs cut from 3 to 2 | Split and resume supported; a "short" variant with fewer runs |
| VALORANT | No VALORANT-specific VDIM exists | Our adaptation adds VALORANT-relevant drills (head height, counter-strafe) inside the matching skill days |

**Pieces to build, in this order:**

1. **Benchmark tiers:** the same scenario families at three difficulties (novice, intermediate, advanced), each tier with its own thresholds. This extends `benchmark` and adds content.
2. **Difficulty variants as content:** for each benchmark scenario, authored "drill", "variant" and "hard" TOML files. YAGNI: write them by hand now; generate them from difficulty knobs only once Phase 5 exists.
3. **`training::method`:** a weekly method definition (TOML in `content/methods/`). It maps each tier and weekday to a routine, defines the Sunday options, and maps rank bands to tiers. It is pure and validated against the routine and benchmark libraries.
4. **Routine progress persistence:** "today's" plan, its position in it, and resuming a split session. A small port in the core, stored by the app.
5. **App "Today" page:** today's skill, tier, remaining runs, a resume button, and a credit line for VDIM.

**Research tension to surface to players.** VDIM blocks one skill per day for about 90 minutes. Listman et al. (2021) saw daily gains peak at about 1 h, and contextual interference (Shea & Morgan 1979) favors mixing exercises within a session. Our version keeps VDIM's structure as designed, recommends splitting long days into 2–3 sessions, and keeps `valorant-daily` (interleaved) as the alternative. The docs explain the tradeoff instead of picking a winner without evidence.

## Phase 3: player tools

**Custom crosshairs**
- `aim_core::crosshair`: one crosshair model (lines, outline, dot, opacity, gap), presets in `content/crosshairs/`.
- Import and export VALORANT crosshair profile codes. *The format must be researched and documented with sources before implementation.*
- egui painting stays in `ui/atoms`.

**Sensitivity converter**
- Data-driven game profiles: yaw constant, FOV model (Hor+ / Vert−, base FOV), zoom model. Each value needs a documented source.
- Matching methods: 360° distance (today), plus monitor-distance matching (0%, 75%, 100%) for zoomed and different-FOV cases.
- A standalone converter page that does not require a run.

## Phase 4: analytics (Aimlabs-style feedback)

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

## Phase 5: adaptive training

- **Difficulty knobs:** scenarios declare which parameters scale (target size, speed, lifetime, count) and their easy/hard bounds.
- **Controller:** a staircase that keeps success around **80%** (challenge-point framework; Wilson et al. 2019; see `training-science.md`).
- **Personalized playlists:** built from the skill profile, with weak skills weighted up and interleaved (Shea & Morgan 1979).
- **Benchmark recalibration:** set thresholds from real-player score percentiles, then flip `calibrated = true`.
- **Reload mechanic** for clicking scenarios: misses cost ammo, as in Voltaic's S5 clicking scenarios (https://app.voltaic.gg/blog/announcing-the-voltaic-season-5-aiming-benchmarks-beta-for-kovaaks/). It is a new scoring building block in `scenario/`.

## Phase 6: game-mixed training and bots

- **`Activity` trait:** introduce the trait sketched in the architecture; the scenario session is its first implementation. Routines then sequence activities.
- **Hybrid activities:** game-like objectives combined with aim drills (similar in spirit to Aimstars on Roblox, which mixes ranked scenarios with game modes and human-like AI).
- **Deathmatch bots:**
  - Movement with `movement::step` and per-game movement profiles. VALORANT comes first; other games only with sourced values.
  - Aim with `sim::SimAimer`: the Fitts-law model already provides reaction time, movement time and endpoint error, so difficulty is a profile.
  - Behavior (peek, strafe, hold) as a separate, testable state machine.

## Phase 7: tactical drills on original maps

- **New crate `aim_world`:** map loading (glTF), collision, line-of-sight and occlusion for hit tests, spawn and angle markers.
- **Maps are original:** common tactical-shooter situations (doorways, boxes, long angles, off-angles, multi-angle clears) built at real-world scale. **No replicas of VALORANT maps** (Riot's fan policy forbids using their IP in games and apps; see the integrity section of the coding standards).
- **Drills:** angle clearing, jiggle-peeks, crosshair placement at head level, pre-aim, with analytics hooks such as crosshair-placement error at first sight.

## Phase 8: match integration (later)

- **New crate `aim_integrations`,** feature-gated.
- **Data sources:** only official, documented sources, such as the Riot API with a valid key under its terms, or files the player exports. **Never memory reading or screen capture of the game** (account-ban risk and against the integrity rules).
- **Data to verify:** what round and kill data the official API exposes must be researched and documented before design.
- **Use:** map round events to the skill profile, recommend drills, and rebuild the *situation* (distances, angles, timings) in original geometry to show the error and train it.

## Not planned

- Anything that gives an in-game advantage or touches the game process.
- Monetization, which Riot's policy restricts for fan projects using its IP.
