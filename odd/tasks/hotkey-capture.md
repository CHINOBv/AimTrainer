# Hotkey capture
Objective: make changing the global hotkeys intuitive: record a combination by pressing it, see validation and conflicts, and apply it without restarting.
Branch: `feat/hotkey-capture` (worktree `../AimTrainer-worktrees/hotkey-capture`) · Delivery: merge to `main` (user-requested) · Test runner: `cargo test --workspace`

## Specs

- S1 (verbatim, 2026-10-18): "necesito que agregues una forma más intuitiva de cambiar el, los atajos de teclado. Haz el cambio y haz el merge a main y también compílalo para probarlo."
  - Today the three hotkeys (overlay toggle, queue warm-up, buy-phase micro drill) are free-text fields (`ctrl+shift+F10`) and need a restart.
  - Each hotkey gets a "Record" button: click it, press the combination, and it is stored in the `global-hotkey` spec format. Esc cancels; a "Clear" button disables the hotkey.
  - The UI shows a readable label (`Ctrl + Shift + F10`), flags invalid combinations (no modifier on a non-function key, unparsable) and duplicates between the three hotkeys.
  - Changed hotkeys are re-registered live (Windows/X11); no restart. Wayland keeps the compositor path.

## Tasks

| ID | Specs | Route | Status | Commit |
|----|-------|-------|--------|--------|
| T1 | S1 | delegated (writer: 3+ non-trivial files in `aim_app`) | done (uncommitted) | |

## Log

- L1 (2026-10-18) Exploration: hotkeys live in `persistence/settings.rs`, are registered once in `platform/hotkey.rs::install` from `main.rs`, and edited as text in `ui/organisms/settings_panels.rs`.
- L2 (2026-10-18) T1 implemented. Pure spec logic in `platform/hotkey_spec.rs` (parse, readable label, `problems`: invalid/unknown key, bare non-function key, duplicates with `KeyK`/`k` aliasing); egui key → spec mapping (`capture`, physical key preferred) and the Record/Cancel/Clear rows in `ui/organisms/hotkey_settings.rs`. `platform/hotkey.rs` now keeps the `GlobalHotKeyManager` alive and a `sync` system re-registers whenever the wanted set changes; `HotkeyCapture` unregisters every hotkey while recording (so the OS delivers the combo to the window) and `poll` ignores events meanwhile; the menu stops recording when leaving the Settings tab, hiding the overlay or leaving the menu. `OverlayPlugin` no longer carries specs. README updated. RED: 9 new tests failed against stubs; GREEN: `cargo test -p aim_app hotkey` 9 passed; workspace 370 passed, 1 ignored. Recording/UI and live re-registration have no automated tests: needs a playtest on Windows/X11.
- L3 (2026-10-18) Review follow-ups on 610f869 (uncommitted). R3-001: `capture` ignores a bare non-function key (hint asks for Ctrl/Alt/Shift or an F-key); `sync` registers only `hotkey_spec::registrable` (drops empty, invalid/unknown, bare non-function and later duplicates). R3-003: `HotkeyCapture::tick` ends recording after 10 s or when the primary window loses focus (`expire_recording`, PreUpdate, before `sync`). R3-004: wanted-set computation extracted to `registrable` with unit tests. R3-002 skipped by request. RED: 4 new tests failed against stubs; GREEN: `cargo test -p aim_app hotkey` 15 passed. Focus/timeout behaviour in the running app still needs a playtest.
