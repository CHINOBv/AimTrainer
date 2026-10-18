# Sounds and subtle hit feedback
Objective: add configurable sound effects and subtle, configurable target-hit feedback, each with several options.
Branch: `feat/audio-hit-feedback` (worktree `../AimTrainer-worktrees/audio-hit-feedback`) · Delivery: merge to `main` when asked · Test runner: `cargo test --workspace`

## Specs

- S1 (verbatim, 2026-10-18): "Porfa agreguemos sonidos pls, y target hits discretos, que estos dos sean configurables, agrega varias opciones"
  - Sounds: the app has no audio today. Add hit, kill and optional miss sounds, synthesized in code (no shipped audio assets, never Riot sounds). Settings: enabled, master volume, hit sound style and kill sound style (several presets each), miss sound on/off.
  - Subtle hit feedback ("discretos" read as subtle/discreet): a crosshair hit marker and a target hit effect, each with several styles plus Off; subtle by default. Configurable in Settings.
  - Feedback must not add input-to-photon latency (see `docs/coding-standards.md`).

## Tasks

| ID | Specs | Route | Status | Commit |
|----|-------|-------|--------|--------|
| T1 | S1 sounds | delegated (writer: settings, audio module, UI, Cargo features) | done | (recorded in the next commit) |
| T2 | S1 hit feedback | delegated (same writer, sequential: shares settings and UI) | pending | |

## Log

- L1 (2026-10-18) Exploration: no audio code; workspace Bevy uses `default-features = false, features = ["3d"]`; no assets folder; CI installs Linux deps in `.github/workflows/ci.yml` (audio will need ALSA dev headers). No hit marker or hit effect exists.
- L2 (2026-10-18) T1: `bevy_audio` feature only (no decoder; Cargo.lock +1 line, rodio/cpal MIT/Apache-2.0). Clips synthesized once at startup (`audio::synth`), played via a custom `Decodable` on the frame's strongest `ShotFeedback` (kill > headshot > hit > miss). Settings `[sound]`: enabled, volume_percent, hit (off/click/tick/pop/ding/thump), kill (off/chime/ping/pluck/burst), miss. RED observed for synth (3 failing with stub render) and sound-settings defaults (2 failing); clip_for/strongest checked by stub mutation (3 failing). GREEN: `cargo test --workspace` 392 passed; clippy -D warnings clean. `cargo build -p aim_app` compiled but could not replace the running `aimtrainer.exe` (os error 5); the test binary links the same crate. Playback has no automated test: playtest needed.
