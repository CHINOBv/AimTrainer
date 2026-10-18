# CustomAimTrainer

A VALORANT-focused aim trainer for **Arch Linux (Wayland/Hyprland, X11)** and **Windows**. It runs as a transparent overlay that you show or hide with a hotkey.

- **Exact sensitivity**
  - Uses VALORANT's 0.07°/count for yaw and pitch, read from raw, unaccelerated mouse counts.
  - Shows cm/360 and eDPI.
  - Includes a converter with monitor-distance matching for CS2, Apex, Overwatch 2, Fortnite, Call of Duty, Destiny 2, Rainbow Six Siege and Deadlock.
  - Includes a DPI check and a guided **sensitivity finder**.
- **VALORANT FOV and movement**
  - 70.53° vertical (Hor+, 103° horizontal at 16:9), with ultrawide pillarboxed like the game. ADS zoom follows the weapon.
  - Rifle and knife speeds and the stopping curves Riot measured.
  - Shots above 27.5% of run speed are inaccurate.
- **Training content**
  - **23 exercises as TOML files**, with difficulty levels generated from one definition.
  - **Spray and burst drills** with a recoil model built on Riot's published parameters.
  - **Ammo-economy clicking.**
  - **Arenas:** deathmatch, a site hold (stop bots from capturing a site), angle clearing, jiggle-peek and pre-aim drills against bots that peek, strafe and hold like VALORANT players, on original maps (TOML or glTF).
- **How to train**
  - A **weekly method** in the style of VDIM: one skill per day, drills → variants → benchmark, with tiers.
  - Interleaved routines and a **personalized playlist**.
  - **Adaptive difficulty** that holds you near 80% success.
  - A Voltaic-style **benchmark** with novice, intermediate and advanced tiers.
- **Ranked days**
  - A **queue warm-up** sized to your queue.
  - **20-second buy-phase drills** that hide themselves when done.
  - A **"ready for ranked?" check.**
  - **Fatigue warnings.**
  - **Daily goals and streaks.**
  - A **daily challenge** that everyone plays with the same seed.
- **Feedback**
  - Flick and tracking analysis with explained recommendations.
  - A **replay viewer** of your crosshair path.
  - Benchmark trends and personal bests.
  - Offline **post-match analysis** of official match files.
- **Your setup**
  - VALORANT **crosshair codes** (import and export).
  - **Sound effects** for hits, headshots, kills and (optionally) misses, synthesized in code: several presets, a volume slider and previews.
  - Subtle **hit feedback**: a short, semi-transparent hit marker (crosshair flash, dot, cross, ring or brackets; color and duration) and a target effect (flash, pop, fade or ring burst). Each can be turned off.
  - A shareable **profile code**.
  - A **system check**: FPS, 1% lows, vsync, acceleration.

The research behind the training is in [`docs/training-science.md`](docs/training-science.md), and the features are justified in [`docs/feature-analysis.md`](docs/feature-analysis.md).

Every value has a source in [`docs/valorant-mechanics.md`](docs/valorant-mechanics.md). Design decisions and known limitations: [`docs/decisions.md`](docs/decisions.md).

## Build

You need Rust ≥ 1.97.1 (Bevy 0.20 requires it).

```sh
# Arch
sudo pacman -S --needed rust vulkan-icd-loader wayland libxkbcommon alsa-lib
cargo build --release
./target/release/aimtrainer
```

On Windows, install Rust with rustup and run `cargo build --release`. The binary is `target\release\aimtrainer.exe`.

## Controls

| Action | Input |
|---|---|
| Shoot | Left mouse button (hold for tracking scenarios) |
| ADS zoom (weapons that have a scope) | Right mouse button |
| Move (movement scenarios) | WASD, Shift to walk |
| Pause / resume | Esc |
| Change your floor height (scenario pause menu) | Floor height −/+ (0.5 m steps), then Apply and restart |
| Retry or next routine run (on the results screen) | Space |

The menu tabs:

| Tab | What it holds |
|---|---|
| Today | The day's method session, the daily challenge |
| Progress | Goal, readiness, fatigue, trend, per-skill strengths and trends, bests, replays |
| Scenarios | Play, or play adaptively |
| Routines | The routines (resume a split one), your personalized playlist |
| Arenas | Deathmatch, site hold, angle clearing, jiggle-peek and pre-aim drills; a Practice button plays any arena with bots that never shoot |
| Benchmark | Your benchmark energy and ranks, at novice, intermediate or advanced tier |
| Tools | Sensitivity converter (360°, monitor distance 0/75/100%), sensitivity finder, system check |
| Settings | Sensitivity, DPI check, crosshair, sound, hit feedback, profile code, hotkeys |

## Command line

```sh
aimtrainer                                        # start the trainer
aimtrainer play <scenario|arena>                  # start straight into a scenario or arena
aimtrainer toggle | show | hide                   # control a running trainer (Linux)
aimtrainer warmup [minutes]                       # show the trainer and start a queue warm-up
aimtrainer micro                                  # one 20 s buy-phase drill, then hide again
aimtrainer analyze-match <match.json> --player Name#TAG   # report + a drill of your duels
```

## Overlay: show and hide

When hidden, the trainer draws nothing, becomes click-through, releases the mouse and pauses the run.

### Hyprland / Wayland

Wayland does not let an app grab global shortcuts, so the compositor runs `aimtrainer toggle` instead. Install the binary somewhere on your `PATH` first, for example `cargo install --path crates/aim_app`. Then add this to `~/.config/hypr/user.lua` (Lua config, Hyprland ≥ 0.53):

```lua
-- Show/hide the overlay, start a queue warm-up, play a buy-phase drill
hl.bind("CTRL + SHIFT + F10", hl.dsp.exec_cmd("aimtrainer toggle"))
hl.bind("CTRL + SHIFT + F9",  hl.dsp.exec_cmd("aimtrainer warmup"))
hl.bind("CTRL + SHIFT + F8",  hl.dsp.exec_cmd("aimtrainer micro"))

hl.window_rule({
    name      = "aimtrainer-overlay",
    match     = { class = "^aimtrainer$" },
    float     = true,
    pin       = true,   -- stay visible on every workspace
    no_blur   = true,
    no_shadow = true,
    immediate = true,   -- allow tearing for lowest input lag (needs general.allow_tearing)
})
```

If your config uses a rebind helper (`K(...)`), wrap the key string in it. On a classic `hyprland.conf`, the bind is:

```
bind = CTRL SHIFT, F10, exec, aimtrainer toggle
bind = CTRL SHIFT, F9, exec, aimtrainer warmup
bind = CTRL SHIFT, F8, exec, aimtrainer micro
```

Other commands: `aimtrainer show` and `aimtrainer hide`.

### Windows and X11

The trainer registers three global hotkeys (defaults below). To change one, open **Settings → Global hotkeys**, click **Record** and press the new combination; Esc cancels and **Clear** disables the hotkey. A key needs Ctrl, Alt or Shift unless it is an F-key, so a hotkey never fires while you type. The change applies immediately, no restart needed. Recording stops after 10 s or when the window loses focus. A combination already used by another hotkey is flagged and only the first one is registered.

| Hotkey | Action |
|---|---|
| **Ctrl+Shift+F10** | Toggle the overlay |
| **Ctrl+Shift+F9** | Start a queue warm-up |
| **Ctrl+Shift+F8** | Play a buy-phase drill |

To draw on top of VALORANT, set VALORANT to **Windowed Fullscreen**, because an exclusive-fullscreen game covers every other window.

The trainer is a separate window. It does not read game memory or inject input. Even so, nothing about it is endorsed by Riot.

## Sensitivity setup

Open Settings and enter:

- your **VALORANT sensitivity**;
- your **DPI**;
- your **ADS multiplier**, if you changed it.

Then use **DPI check**: move the mouse exactly 10 cm along a ruler. The measured DPI should be within a few percent of the DPI you configured. If it isn't, you have OS mouse acceleration on or the wrong DPI.

## Your own exercises

User files live in the folders below. A file with the same `id` as a built-in replaces it.

| OS | Folder |
|---|---|
| Linux | `~/.config/aimtrainer/{scenarios,routines,benchmarks}/` |
| Windows | `%APPDATA%\CustomAimTrainer\aimtrainer\config\…` |

See [`docs/scenarios.md`](docs/scenarios.md) for the format; the built-ins in `content/scenarios/` are working examples. Run history is saved as CSV in the data directory (`~/.local/share/aimtrainer/history.csv` on Linux).

## Architecture

```
crates/
  aim_core/          engine-agnostic domain, fully unit-tested
                     sensitivity (+ converter, finder) · fov · movement · VALORANT data
                     scenario engine (difficulty, recoil, recording) · training (routines,
                     benchmark, weekly method, adaptive, playlists, warm-ups, progress)
                     analytics · crosshair codes · profile codes · simulated aimer
  aim_world/         original maps, line of sight, collision, navigation, arenas with bots
  aim_integrations/  offline post-match analysis of official match JSON
  aim_app/           Bevy 0.20 + egui adapter: input, rendering, overlay, menus, persistence
content/             scenarios, routines, benchmarks, methods, maps, arenas (TOML)
```

All gameplay rules live in `aim_core`. The app only turns raw input into `FrameInput` and draws whatever the session reports. To add a new mechanic, add a building block in `aim_core::scenario::def` and handle it in `session.rs`; it is then available to every TOML file.

```sh
cargo test --workspace                                  # core + app tests
cargo run -p aim_core --release --example calibrate     # simulated-player scores per scenario
```

## Roadmap

Phases 1–8 of [`docs/roadmap.md`](docs/roadmap.md) are implemented; its status table lists what shipped and where the implementation deliberately differs from the plan.

## Contributing

Read [`CONTRIBUTING.md`](CONTRIBUTING.md). The binding rules are in [`docs/architecture.md`](docs/architecture.md) and [`docs/coding-standards.md`](docs/coding-standards.md). Exercises can be contributed as TOML without writing Rust.

## Legal

CustomAimTrainer is an independent fan project. It is not endorsed or sponsored by Riot Games. VALORANT is a trademark of Riot Games, Inc., and it is named here only to describe compatibility.

The project does not ship Riot assets and never interacts with the game process. Riot's fan-content policy: https://www.riotgames.com/en/legal.
