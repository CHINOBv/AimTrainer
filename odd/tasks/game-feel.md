# Game feel and UI/UX
Objective: make deathmatch and scenarios feel less "dry" through audio, indicators and arena feedback, and improve the menus. Every new effect is configurable (on/off, plus intensity where it applies).
Branch: `feat/game-feel` (worktree `../AimTrainer-worktrees/game-feel`) · Delivery: local merge to `main`; push at the end of the batch (authorized) · Test runner: `cargo test --workspace`

## Specs

- **S1 Arena event data.** Arena events say where a hit came from and when a bot fires:
  - `PlayerHit` carries the shooter position and the zone;
  - a new `BotFired { bot_id, origin }` event.

  This lets the app build positional audio and direction indicators (map item #1).
- **S2 Audio.**
  - Positional bot gunfire and footsteps in arenas.
  - Damage-taken and death sounds.
  - Multi-kill and streak sounds.
  - Countdown beeps, round start/end stings, and low-time ticks.
  - Optional UI click sounds.
  - A volume per category (shots, world, UI, cues) on top of the master volume.

  All sounds are synthesized (no Riot assets).
- **S3 Arena HUD indicators.**
  - A damage direction indicator, sized by damage.
  - A damage vignette scaled by damage and configurable.
  - A kill feed with a streak/multi-kill banner.
  - A health bar with a damage ghost and a low-HP pulse.
  - A death screen showing the direction of the shot and the respawn timer.
  - An animated countdown, a "GO" pop and an end-of-round banner.
  - An optional screen punch on damage, as an overlay only (never moving the camera; default off).
- **S4 Crosshair and markers.**
  - Distinct head / body / kill hit markers.
  - A kill flash in arenas.
  - An optional crosshair bloom from recoil or movement inaccuracy.
  - Off-screen arrows for visible enemies in arenas.
- **S5 Menus (UI/UX).**
  - Settings split into sub-tabs (Sensitivity, Crosshair, Feedback, Audio, Overlay, Keys, Training), with search and a reset per section.
  - Settings saved off the frame with a "Saved" message (fixes a blocking `fs::write` every frame).
  - Scenario search, filters, favourites and recent runs.
  - Quick Play / continue last on Today, and number keys to switch tabs.
  - A first-run wizard (game, sensitivity, DPI, feedback preset Minimal / Standard / Juicy).
  - Scroll fixes: pinned footers and an `id_salt` on every `ScrollArea`; shared colour tokens.
- **S6 Configurability.**
  - Every effect above has a settings key in a `[feel]` table or `[sound]`, with serde defaults, so old settings files load.
  - The defaults keep today's behaviour unless the effect is clearly an improvement; anything intrusive (screen punch, UI clicks) is off by default.
  - The Settings UI exposes every key.

Rules: game rules stay in `aim_core`/`aim_world`; the app draws and plays only. Latency rules apply (nothing blocks the frame). Drawing, audio and input have no automated tests and need a playtest.

## Tasks

| ID | Specs | Route | Status | Commit |
|---|---|---|---|---|
| T1 | plan | inline | done | cda749a |
| T2 | S1, S6 foundation (`FeelSettings`, sound category volumes, arena events, `ArenaFeedback` message) | delegated (worktree `feel-0`) | done | 8cb71d7, 5c5c98f, eae9d00, 29349a1, merge a44d49c |
| T3 | S2 | delegated, after T2 (worktree `feel-audio`) | done (playtest pending) | 0cb7014, merge 651e28d |
| T4 | S3 | delegated, after T2 (worktree `feel-hud`) | done (playtest pending) | cf8e366, merge f4f6bf0 |
| T5 | S4 | delegated, after T2 (worktree `feel-markers`) | done (playtest pending) | 1299a2b, 41c6451, aaa853f, 21ae1c7, merge fee13e4 |
| T6 | S5 (+ settings UI for S6 keys) | delegated, after T2 (worktree `feel-menus`) | done (playtest pending) | e533aef..f6a7795, merge, 2206cce |
| T7 | seam: merge, full suite, verifier, review, merge to `main`, build and launch | inline | pending | |

## Log

- L1 (2026-10-18) User: "Una vez que termines con todo esto, por favor, continúa haciendo mejoras, pero ya en este caso de UI y de UX. Y también en cuanto a gameplay, siento que se siente como muy seco el jugar Deathmatch. O algunos escenarios, es como que creo que es más tema de audio, indicadores o algo así. No sé, igual deja todo configurable para que cada quien pueda deshabilitar lo que no le guste y tal."
- L2 (2026-10-18) Map: the arena session already emits damage taken, death, kills, bot spawns and end of round, but the app only plays the player's own hit/kill sounds. `PlayerHit` has no direction and bot fire emits no event. Settings are written synchronously every changed frame (`menu.rs` → `fs::write`).

  The split follows the map's suggestion: a unit 0 foundation, then four disjoint units (audio / HUD / markers / menus).
- L3 (2026-10-18) All units merged.
  - Seams: hud.rs/arena.rs between HUD and markers. The HUD unit removed `damage_flash` in favour of its scaled vignette; the markers unit's arrows, `hit_frame` and kill flash were kept; the stale `arena_status(damage_flash)` call was dropped.
  - The parent wrapped the play buttons with `ui_click::clicked` and documented per-play positional rendering in `docs/decisions.md`.
  - Checks: tests all pass (app 286, core 411, world 102 + 1 ignored); clippy clean; arena golden passes.
  - Decisions left as the writers chose them:
    - damage/death sounds sit in the cues category (as the settings docs say);
    - the scenario kill flash stays undrawn, as before;
    - the animated countdown also plays before scenarios;
    - the end-of-round hold is 0.8 s in the app run state;
    - arena bloom is movement-only.
- L4 (2026-10-18) Independent verifier: S1–S4 and S6 pass; S5 was partial on one blocker, now fixed.
  - **B1:** number keys switched tabs while the restart-key row was recording from a Settings search. Fix: `bare_key_rows_shown` disables number keys on the Keys sub-tab or during any search. RED was observed first (missing function).
  - The README intensity wording was corrected.
  - Invariants:
    - arena stats are byte-identical to baseline over 8 arenas × 3 seeds × 2 play styles;
    - settings files written by baseline load with identical values;
    - hit/kill/miss sounds and markers are unchanged at the defaults with `distinct_markers` off;
    - no effect moves the camera.
  - Advisories kept:
    - `distinct_markers` defaults on (a visible change, documented);
    - round stings are arena-only;
    - the multi-kill window is duplicated in HUD and audio;
    - the death screen may flash for one frame in a zero-protection arena.
