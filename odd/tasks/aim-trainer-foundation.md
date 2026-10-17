# Aim trainer foundation
Objective: cross-platform (Arch Linux/Hyprland + Windows) Valorant-focused aim trainer with a toggleable overlay and an extensible exercise system.
Branch: `feat/aim-trainer-foundation` (worktree `../CustomAimTrainer-worktrees/foundation`) · Delivery: ask-on-risk · Test runner: `cargo test --workspace`

## Specs

- S1 Stack: Rust + Bevy 0.20 + bevy_egui 0.43. "quiero algo que tenga buen rendimiento y chance y pueda crecer". Pure domain crate (`aim_core`) without engine dependencies; Bevy app crate (`aim_app`) as adapter.
- S2 Platforms: "que funcione bien en arch linux y windows".
- S3 Overlay (option B chosen): "quiero que tenga un overlay que pueda ocultar o aparecer con un atajo de teclas" — a transparent always-on-top window over other apps/the game. Windows/X11: global hotkey. Wayland/Hyprland: compositor keybind calling `aimtrainer toggle` (IPC).
- S4 Sensitivity: "procura tener configs de sens precisas" — Valorant yaw/pitch per count, cm/360, eDPI, conversions from other games, Valorant FOV.
- S5 Extensibility: "quisiera que lo dejes preparado para agregar mas ejercicios, no solo algunos fijos" — data-driven scenarios (TOML files in a user directory) built from composable spawn/motion/scoring building blocks.
- S6 Initial exercises: "flicking, microcorrections, switching, tracking, algun ejercicio que incluya practicar el movement, aqui tienes que investigar para que sea exacto, y mas" — movement values must come from researched Valorant data (sources recorded in `docs/valorant-mechanics.md`).

## Tasks

| ID | Specs | Route | Status | Commit |
|----|-------|-------|--------|--------|
| T1 | S4, S6 | inline | pending | |
| T2 | S5, S6 | inline | pending | |
| T3 | S1, S2, S4 | inline | pending | |
| T4 | S3 | inline | pending | |
| T5 | S6 | inline | pending | |

- T1 `aim_core`: sensitivity/FOV math + Valorant constants + movement model, with tests.
- T2 `aim_core`: scenario definition model (TOML), validation, built-in exercises, scoring/metrics.
- T3 `aim_app`: Bevy runner — raw mouse look, targets, shooting, HUD, settings menu.
- T4 Overlay: transparent window, show/hide toggle, global hotkey (Windows/X11), IPC `aimtrainer toggle` (Linux), Hyprland rules docs.
- T5 Movement exercises in the app (WASD with Valorant accel/decel, first-shot accuracy gate) + docs.

Route evidence: all inline — units depend on each other (app depends on core API), no writer trigger.

## Log

- L1 (2026-10-16) User request: "Hola, quiero que me ayudes a crear un aim trainer que funcione bien en arch linux y windows, quiero que tenga un overlay que pueda ocultar o aparecer con un atajo de teclas, me interesa principalmente para mejorar el aim en valorant asi que procura tener configs de sens precisas, primero establezcamos el stack para esto, quiero algo que tenga buen rendimiento y chance y pueda crecer porque no?"
- L2 (2026-10-16) Stack proposed: Rust + Bevy + bevy_egui, hexagonal core.
- L3 (2026-10-16) User: "B / Y porfa quisiera que lo dejes preparado para agregar mas ejercicios, no solo algunos fijos, de entrada quiero algo para lo principal, flicking, microcorrections, switching, tracking, algun ejercicio que incluya practicar el movement, aqui tienes que investigar para que sea exacto, y mas"
- L4 (2026-10-16) User: "Inicia el repo en git si hace falta" — repo initialized on `main` (empty initial commit), feature work in sibling worktree.
- L5 Environment: Arch (CachyOS kernel), Hyprland 0.56.2 Wayland, rustc 1.97.1, bevy 0.20.0 (needs rustc 1.97.1), bevy_egui 0.43.1 (bevy 0.20).
