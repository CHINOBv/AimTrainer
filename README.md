# CustomAimTrainer

A VALORANT-focused aim trainer for **Arch Linux (Wayland/Hyprland, X11)** and **Windows**. It runs as a transparent overlay that you show or hide with a hotkey.

- **Exact sensitivity**
  - Uses VALORANT's 0.07°/count for yaw and pitch, read from raw, unaccelerated mouse counts.
  - Shows cm/360 and eDPI.
  - Includes a converter with monitor-distance matching for CS2, Apex, Overwatch 2, Fortnite, Call of Duty, Destiny 2, Rainbow Six Siege and Deadlock.
  - Includes a **DPI deviation test** (move a measured distance along a ruler) and a guided **sensitivity finder**.
- **VALORANT FOV and movement**
  - 70.53° vertical (Hor+, 103° horizontal at 16:9), with ultrawide pillarboxed like the game. ADS zoom follows the weapon.
  - Rifle and knife speeds and the stopping curves Riot measured.
  - Shots above 27.5% of run speed are inaccurate.
- **Training content**
  - **38 exercises as TOML files**, with difficulty levels generated from one definition. They include **precision rings** (more points near the center), a **go/no-go** drill (leave the decoys alone), **reactive targets** that mirror your strafe, dodge your crosshair and change behaviour when hit, **multi-phase** drills with fail conditions and kill goals, **sound localisation**, **peripheral detection** and **radar recall**.
  - **Spray and burst drills** with a recoil model built on Riot's published parameters.
  - **Ammo-economy clicking.**
  - **Arenas:** deathmatch, a site hold (stop bots from capturing a site), angle clearing, jiggle-peek and pre-aim drills against bots that peek, strafe and hold like VALORANT players, on original maps (TOML or glTF). Bots can react to your strafe and shots, follow behaviour rules, play mixed squad personalities (pusher, angle holder, jiggle peeker, roamer), adapt their skill to your streaks and shoot back when a kill takes too long. After each death a **recap card** shows why you lost the fight (crosshair placement, first-shot time, moving shots, who saw whom first). An optional **live coaching overlay** shows crosshair height at the nearest corner and movement accuracy.
- **How to train**
  - A **weekly method** in the style of VDIM: one skill per day, drills → variants → benchmark, with tiers.
  - Interleaved routines and a **personalized playlist**.
  - **Adaptive difficulty** that holds you near 80% success.
  - A Voltaic-style **benchmark** with novice, intermediate and advanced tiers.
- **Ranked days**
  - A **queue warm-up** sized to your queue, or to your **personal warm-up dose** once your own sessions show where your scores level off (2–15 min; shown next to the setting in Settings → Training, where **Use my personal dose** can turn it off).
  - **20-second buy-phase drills** that hide themselves when done.
  - A **"ready for ranked?" check.**
  - **Fatigue warnings.**
  - **Daily goals and streaks.**
  - A **ranked journal**: log each match in one tap (win/loss/draw, RR, a 1–5 rating and a tag), see results split by readiness and fatigue, and get a 3-minute **tilt reset** (paced breathing, then easy tracking) after two losses in a row.
  - **Break reminders** after continuous play, a weekly volume cap and a short stretch list. All of it can be changed or turned off.
  - A **daily challenge** that everyone plays with the same seed.
  - **Compound daily missions** built from your own recent bests (for example "accuracy ≥ 75% AND kills ≥ 62"), with mastery missions and a low/medium/high commitment level.
- **Feedback**
  - A **visual results screen**: your score with its change against your personal best, a row of gauges (accuracy, plus reaction, overshoot and click delay for flick runs, or time on target, speed match and lag for tracking runs) coloured green, yellow or red by the recommendation thresholds, **one main fix** with a **Watch it** button that opens the mistake replays for that run (or the full replay when there is no flick mistake), and the miss compass, flick directions and pace drawn as charts. Hover any gauge, chart or the fix for the numbers and thresholds behind it; the full stats and every analysis panel sit under a collapsed **Details** section.
  - Flick and tracking analysis with explained recommendations.
  - **Run insights:** flicks by screen direction and distance, click timing, pace and stamina, target priority, tracking speed match and edge time, and firing while moving. In arenas: peek timing. A **miss compass** shows whether you miss above or below the head, and a **potential score** names the one fix worth the most points.
  - A **sensitivity suggestion** from your recent flicks' over/undershoot, with the evidence shown. It is applied only when you choose to.
  - A **replay viewer** of your crosshair path: the trail is coloured by speed, and it has a hit/miss heatmap and a side-by-side comparison with an earlier run.
  - **Mistake replays**, one mistake at a time: your whole flick, then the exact moment it went wrong in slow motion (for example, where an overflick passed the target), a tip for that flick, and a realistic corrected example drawn over your path. Open them with **Watch it** on the results screen, or from a saved replay on the Progress page.
  - Benchmark trends and personal bests, and a **plateau detector** on the Progress page: a benchmark subcategory you keep playing with no new personal best in 3 weeks gets suggestions for a change of stimulus (a variant, a sensitivity variation, a switch of emphasis or a lighter week).
  - Offline **post-match analysis** of official match files.
  - **Verifiable score proofs**: copy a run's proof code from the results screen; a friend pastes it on the Progress page and their trainer replays the run to confirm the score, with no server.
- **Your setup**
  - VALORANT **crosshair codes** (import and export).
  - **Target visibility:** VALORANT's enemy highlight colours (red, purple for tritanopia, yellow for deuteranopia/protanopia) or any custom colour, an optional target outline with colour and thickness, and an environment tint for the floor grid.
  - **Sound effects**, all synthesized in code (no game audio): hits, headshots, kills and (optionally) misses, with several presets, previews and a volume slider. Arenas add positional bot gunfire and footsteps, a damage thud scaled by damage, a death sound, rising streak and multi-kill stings, countdown beeps and round start/end stings. Timed runs tick through their last 10 seconds, and menu clicks are optional. Each sound has its own toggle, and four category volumes (shots, world, UI, cues) scale the master volume.
  - Subtle **hit feedback**: a short, semi-transparent hit marker (crosshair flash, dot, cross, ring or brackets; color and duration) and a target effect (flash, pop, fade or ring burst). Each can be turned off.
  - A shareable **profile code**.
  - An opt-in **sensitivity randomiser drill**: a random multiplier within your range every few seconds, shown on the HUD. Your saved sensitivity never changes.
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
| Restart the run as it is (scenarios and arenas, also paused or in the countdown; keeps an adaptive run's level and records nothing) | R (Settings → Keys) |
| Change your floor height (scenario pause menu) | Floor height −/+ (0.5 m steps), then Apply and restart |
| Tune target size, distance and speed (scenario pause menu; tuned runs are saved apart and never move the adaptive level or the benchmark) | Size / Distance / Target speed −/+ (10% steps, 50–200%), then Apply and restart; Reset to scenario defaults |
| Retry or next routine run (on the results screen) | Space |

The menu tabs (keys 1–9 switch tabs while no text field has focus):

| Tab | What it holds |
|---|---|
| Today | Quick play (your last scenario) and Continue (a routine you started today), the day's method session, the daily challenge, daily missions |
| Progress | Goal, readiness chips, fatigue, trend, per-skill bars with weekly sparklines (explanations on hover), bests, sensitivity suggestion, replays (speed trail, heatmap, comparison) |
| Scenarios | Search and filter (category, click or hold, moving or standing, waves); ★ pins a scenario to Favourites (saved as `favorites` in `settings.toml`); Recent lists your last 5 scenarios. Play, or play adaptively |
| Routines | The routines (resume a split one), your personalized playlist |
| Arenas | Deathmatch (also with mixed bot squads or an adaptive duel ladder), site hold, angle clearing, jiggle-peek and pre-aim drills; a Practice button plays any arena with bots that never shoot |
| Benchmark | Your benchmark energy and ranks, at novice, intermediate or advanced tier |
| Tools | Sensitivity converter (360°, monitor distance 0/75/100%), sensitivity finder, system check |
| Settings | Sub-tabs: Sensitivity (DPI check, profile code), Crosshair, Feedback (hit feedback, game feel), Audio, Overlay (with the UI language: English or Español, see `docs/i18n.md`), Keys, Training. A search box filters every section; each sub-tab has a reset to defaults. Changes save automatically ("Saved ✔") |
| Coach | The optional AI coach (off until you enable it): pick a provider (OpenAI-compatible such as OpenAI, OpenCode, Ollama or LM Studio; Anthropic; or your own installed CLI), store the API key in the OS keyring, preview exactly what is sent, chat ("Analyse my stats", "Recommend a routine"), then validate, play and save the scenarios it generates (labelled uncalibrated). "Forget everything" wipes its memory. See `docs/coach.md` |

## Command line

```sh
aimtrainer                                        # start the trainer
aimtrainer play <scenario|arena>                  # start straight into a scenario or arena
aimtrainer toggle | show | hide                   # control a running trainer (Linux)
aimtrainer warmup [minutes]                       # show the trainer and start a queue warm-up (no minutes: personal dose or Settings length)
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

The trainer registers three global hotkeys (defaults below). To change one, open **Settings → Keys → Global hotkeys**, click **Record** and press the new combination; Esc cancels and **Clear** disables the hotkey. A key needs Ctrl, Alt or Shift unless it is an F-key, so a hotkey never fires while you type. The change applies immediately, no restart needed. Recording stops after 10 s or when the window loses focus. A combination already used by another hotkey is flagged and only the first one is registered.

| Hotkey | Action |
|---|---|
| **Ctrl+Shift+F10** | Toggle the overlay |
| **Ctrl+Shift+F9** | Start a queue warm-up |
| **Ctrl+Shift+F8** | Play a buy-phase drill |

To draw on top of VALORANT, set VALORANT to **Windowed Fullscreen**, because an exclusive-fullscreen game covers every other window.

The trainer is a separate window. It does not read game memory or inject input. Even so, nothing about it is endorsed by Riot.

## Sensitivity setup

On the very first start (no `settings.toml` yet) a short wizard asks for your game, sensitivity, DPI and feedback preset (Minimal / Standard / Juicy). Skip it at any step; it is not shown again, and players with an existing settings file never see it (`first_run_done` in `settings.toml`).

To change them later, open **Settings → Sensitivity** and enter:

- your **VALORANT sensitivity**;
- your **DPI**;
- your **ADS multiplier**, if you changed it.

Then use **Measure DPI**: lay a ruler next to the mouse, press **Start**, hold **Shift** and move the mouse the distance you set (10 cm by default, at least 5 cm) to the right, then release Shift. The page shows the real DPI from the raw counts and its deviation from the DPI you configured; mice often differ by a few percent. A large mismatch means OS mouse acceleration is on or the DPI value is wrong. **Apply corrected sensitivity…** stores the real DPI and scales the sensitivity so your cm/360 stays the same; it asks for confirmation first, and you then need to set the same sensitivity in VALORANT so the trainer still matches the game.

## Feel settings

Every key below has a control in **Settings → Feedback** (game feel) or **Settings → Audio** (sound): a checkbox, plus an intensity slider for toggles. The Feedback tab's preset picker (Minimal / Standard / Juicy) sets every feel key at once.

Game-feel effects live in the `[feel]` table of `settings.toml`. Missing keys take the Standard preset, which keeps the look of effects that already existed (the red damage flash stays on at its usual strength). A toggle is a table with `enabled` and `intensity_percent` (100 = designed strength); a toggle table that leaves out `enabled` turns the effect on.

| Key | Default | What it does |
|---|---|---|
| `preset` | `"standard"` | Last preset applied: `"minimal"`, `"standard"` or `"juicy"`. Changing one key does not change it |
| `damage_indicator` | on, 100 | Arena: arrow around the crosshair towards the damage source, sized by damage |
| `damage_vignette` | on, 100 | Arena: red screen-edge flash on damage |
| `kill_feed` | `true` | Arena: list of recent kills |
| `streak_banner` | `true` | Arena: multi-kill and streak banner |
| `health_bar` | `true` | Arena: health bar with a ghost of the damage just taken |
| `low_hp_pulse` | `true` | Arena: health bar pulse at low health |
| `death_screen` | `true` | Arena: killing-shot direction and respawn timer after a death |
| `round_transitions` | `true` | Animated countdown, "GO" pop and an end-of-round banner that holds the results for 0.8 s |
| `screen_punch` | off, 100 | Arena: screen-space jolt on damage; never moves the camera |
| `crosshair_bloom` | `false` | Crosshair opens with recoil and movement inaccuracy |
| `arena_arrows` | `true` | Arena: edge arrows towards enemies in sight but outside the view |
| `kill_flash_arena` | `true` | Arena: brief flash on a kill |
| `distinct_markers` | `true` | Different hit markers for head, body and kill |

Minimal keeps only the damage indicator (at 60%), health bar and death screen. Juicy adds screen punch and crosshair bloom to Standard.

Every feel key above takes effect. The arena HUD keys (`damage_indicator`, `damage_vignette`, `kill_feed`, `streak_banner`, `health_bar`, `low_hp_pulse`, `death_screen`, `round_transitions` and `screen_punch`) each turn an effect on or off, and the three toggles with an intensity (`damage_indicator`, `damage_vignette`, `screen_punch`) scale its opacity and size; `distinct_markers`, `kill_flash_arena`, `crosshair_bloom` and `arena_arrows` change the markers and crosshair. The animated countdown also plays before scenario runs.

The `[sound]` table adds per-category volumes, each a percent of `volume_percent` (a sound plays at master × category): `shots_percent`, `world_percent`, `ui_percent` and `cues_percent` (all 100).

| Category | Sounds |
|---|---|
| Shots | Your hit, kill and miss sounds |
| World | Arena bot gunfire and footsteps, panned to where they come from |
| UI | Menu clicks (main tabs and the scenario list) |
| Cues | Damage taken, death, streaks and multi-kills, countdown and round stings, low-time ticks, and the sound cues of perception drills |

Sound toggles: `bot_gunfire`, `footsteps`, `damage_taken`, `death`, `streak`, `round_cues` and `low_time_warning` are on; `ui_clicks` is off. `enabled = false` silences every effect except perception-drill cues, which are part of the drill. Footsteps are heard from running bots within about 20 m; a multi-kill is a kill within 1.5 s of the previous one (both estimates, tuned by ear).

## Your own exercises

User files live in the folders below. A file with the same `id` as a built-in replaces it.

| OS | Folder |
|---|---|
| Linux | `~/.config/aimtrainer/{scenarios,routines,benchmarks}/` |
| Windows | `%APPDATA%\CustomAimTrainer\aimtrainer\config\…` |

See [`docs/scenarios.md`](docs/scenarios.md) for the format; the built-ins in `content/scenarios/` are working examples.

## Run history

Every finished scenario run is saved as one CSV row in the data directory (`~/.local/share/aimtrainer/history.csv` on Linux). Next to the result, each row stores a **settings snapshot**, so runs played under different setups can be told apart:

| Column | Meaning |
|---|---|
| `timestamp` … `duration_secs` | The result: Unix time, scenario run key, score, accuracy, kills, kills/min, average time to kill, headshot rate, moving shots, run length |
| `sens_game`, `sens_value`, `dpi`, `cm_per_360` | The sensitivity used (a sensitivity-finder test's own value for those runs) |
| `vfov_deg` | Vertical FOV of the camera, a scenario's FOV lock included |
| `crosshair_code` | Your crosshair as a VALORANT profile code |
| `avg_fps`, `low_1pct_fps` | Average and 1% low frame rate over the run |
| `pause_count`, `pause_secs` | How often and how long the run was paused |
| `content_hash` | A hash of the scenario as played (16 hex digits). Editing a scenario changes it, so an edited file does not silently share history with the original |

Rows written before snapshots existed keep their columns and load with empty snapshot fields; the next run written upgrades the file's header line. Arena runs are stored without a snapshot.

**Export:** on the Progress page, **Export history (CSV)** writes a copy with the same columns, one row per run, with older rows padded with empty fields so every row has all 21 columns. Type a path in the field next to the button, or leave it empty to write `history-export-<unix time>.csv` next to `history.csv`. It never replaces an existing file (the history included); pick a new name instead. The export runs on a background thread, and the page shows where the file went or why it failed.

## Score proofs

Runs are deterministic: the same scenario, seed and inputs always give the same score. **Copy proof code** on the results screen of a scenario run copies a share code (base64url) that bundles the run key, a hash of the scenario as played, the seed, every input and the claimed score and accuracy.

To check a friend's score, paste the code into **Verify a proof** on the Progress page. Your trainer rebuilds the scenario from your own library, checks the hash and replays the inputs on a background thread, then shows either `Verified: <scenario> score X, accuracy Y%` or why it failed: not a proof code, an unsupported version, an unknown scenario, a scenario that differs from the one played (for example an edited custom file), or a replay that gives another score. A proof shows that the inputs produce the score; it cannot show that a human produced them. Arena runs have no proof code. Verify on the same operating system the run was played on: Windows and Linux round some math differently, so a cross-platform proof can report a slightly different score.

## Ranked journal

The **Ranked journal** section of the Progress page logs a match in one tap: pick the RR change (optional, such as `+18` or `-12`), a 1–5 self-rating (3 if you skip it) and an optional tag, then press **Win**, **Loss** or **Draw**. Each entry also stores today's warm-up readiness (the coldest of the "ready for ranked?" scenarios, or unknown) and whether a fatigue warning was showing, so the page can split your results by readiness and fatigue. A pattern needs many matches; a handful says little.

After **two losses in a row**, the page suggests a **tilt reset (3 min)**: a 60-second breathing timer at 6 breaths a minute (a circle grows as you breathe in and shrinks as you breathe out), then 2 minutes of easy smooth tracking. You can skip to the tracking or cancel. It is a pause before the next queue, not a treatment.

Entries are kept in `journal.toml` next to `history.csv`. It is written in the background through a temporary file and a rename. If the file cannot be read, it is never overwritten: the page says so, and the file is renamed to `journal.corrupt-<unix time>.toml` before the next entry is saved.

## Breaks and wrist care

Time in a run counts as play; time in the menu, on the results screen or with the overlay hidden does not. After **45 min** of continuous play, a banner in the menu and on the results screen (never during a run) suggests a **5 min** break, with a short stretch list. Time away from runs as long as a break resets the count. **Dismiss** hides the banner until another full interval of play; **Snooze** hides it for 10 min of play. A separate note appears when your aim training in the last 7 days reaches the **weekly cap** (420 min, about 1 h a day).

Every number is an estimate, and no trial has shown that breaks or stretches prevent wrist complaints in esports; the banner says so. Change them or turn them off in Settings → Training, or in `settings.toml`:

```toml
[breaks]
enabled = true
continuous_minutes = 45
break_minutes = 5
weekly_cap_minutes = 420
```

An invalid `[breaks]` table (an unknown key, a wrong type or a zero) resets only the breaks to these defaults. Background: "Breaks and wrist care" in [`docs/training-science.md`](docs/training-science.md).

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
