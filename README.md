# CustomAimTrainer

A VALORANT-focused aim trainer for **Arch Linux (Wayland/Hyprland, X11)** and **Windows**. It runs as a transparent overlay that you show or hide with a hotkey.

- **Exact sensitivity**
  - Uses VALORANT's 0.07°/count, both yaw and pitch.
  - Reads raw, unaccelerated mouse counts: Raw Input on Windows, `zwp_relative_pointer` on Wayland.
  - Shows cm/360 and eDPI, and converts from CS2, Apex and Overwatch 2.
  - Includes a DPI check that measures the real counts your mouse sends.
- **VALORANT FOV:** 70.53° vertical (Hor+), which is 103° horizontal at 16:9. ADS zoom matches each weapon's multiplier.
- **Real movement**
  - 5.4 m/s with rifles, 6.75 m/s with the knife.
  - Deceleration replays the stopping curves a Riot designer measured.
  - Shots fired above 27.5% of run speed are inaccurate.
- **Exercises are data:** each one is a TOML file. Drop new ones into a folder; no rebuild needed. Ships with 14, covering flicking, microcorrections, switching, tracking, movement (counter-strafe), angle holding and long-range precision.
- **Evidence-based routines and a Voltaic-style benchmark:** subcategory energy combined with a harmonic mean. See [`docs/training-science.md`](docs/training-science.md).

Every value has a source in [`docs/valorant-mechanics.md`](docs/valorant-mechanics.md).

## Build

You need Rust ≥ 1.97.1 (Bevy 0.20 requires it).

```sh
# Arch
sudo pacman -S --needed rust vulkan-icd-loader wayland libxkbcommon
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
| Retry or next routine run (on the results screen) | Space |

## Overlay: show and hide

When hidden, the trainer draws nothing, becomes click-through, releases the mouse and pauses the run.

### Hyprland / Wayland

Wayland does not let an app grab global shortcuts, so the compositor runs `aimtrainer toggle` instead. Install the binary somewhere on your `PATH` first, for example `cargo install --path crates/aim_app`. Then add this to `~/.config/hypr/user.lua` (Lua config, Hyprland ≥ 0.53):

```lua
-- Show/hide the aim trainer overlay
hl.bind("CTRL + SHIFT + F10", hl.dsp.exec_cmd("aimtrainer toggle"))

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
```

Other commands: `aimtrainer show` and `aimtrainer hide`.

### Windows and X11

The trainer registers a global hotkey, **Ctrl+Shift+F10** by default. To change it, set `overlay_hotkey` in Settings or in `settings.toml`, then restart.

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

See [`docs/scenarios.md`](docs/scenarios.md) for the format; the built-ins in `crates/aim_core/scenarios/` are working examples. Run history is saved as CSV in the data directory (`~/.local/share/aimtrainer/history.csv` on Linux).

## Architecture

```
crates/
  aim_core/   engine-agnostic domain, fully unit-tested
              sensitivity · fov · movement · valorant data · scenario engine
              routines · benchmark · simulated aimer (Fitts' law)
  aim_app/    Bevy 0.20 + egui adapter: input, rendering, overlay, menus, persistence
```

All gameplay rules live in `aim_core`. The app only turns raw input into `FrameInput` and draws whatever the session reports. To add a new mechanic, add a building block in `aim_core::scenario::def` and handle it in `session.rs`; it is then available to every TOML file.

```sh
cargo test --workspace                                  # core + app tests
cargo run -p aim_core --release --example calibrate     # simulated-player scores per scenario
```

## Roadmap

- **Recalibrate the benchmark** from real player data. The thresholds are provisional.
- **Adaptive difficulty:** a staircase that keeps success around 80% (challenge-point framework).
- **Voltaic-style reload mechanic** for clicking scenarios: spending ammo on misses.
- **Score history charts** and per-skill trends.
- **Windows:** verify transparency across GPU drivers. It is untested so far.
