# Contributing

Thanks for helping. This page gets you from zero to a mergeable PR. The rules behind it are in [`docs/architecture.md`](docs/architecture.md) and [`docs/coding-standards.md`](docs/coding-standards.md).

## Quick path

1. **Set up:** install Rust ≥ 1.97.1. On Arch: `sudo pacman -S --needed rust vulkan-icd-loader wayland libxkbcommon`.
2. **Build and check:**
   ```sh
   cargo fmt --all --check
   cargo clippy --workspace --all-targets   # must print no warnings
   cargo test --workspace
   ```
3. **Pick the right place** for your change (details in the architecture's Quick path):

   | Change | Where |
   |---|---|
   | Exercise, routine or benchmark | TOML only, see [`docs/scenarios.md`](docs/scenarios.md) |
   | Gameplay, training or analysis rule | `crates/aim_core`, with tests |
   | UI, rendering, input, overlay, files | `crates/aim_app` |

4. **Write the test first,** then the code. Bug fixes include the test that reproduces the bug.
5. **Open a PR** with the checklist below.

## Contributing an exercise (no Rust needed)

1. Copy a similar file from the built-in content and give it a new kebab-case `id`; the file name must equal the `id`.
2. Describe what it trains in the `description`.
3. Use real-world target sizes and distances, and follow the sources rule in the coding standards.
4. Run `cargo test -p aim_core` and `cargo run -p aim_core --release --example calibrate`. Check that the scores keep the order recruit < contender < veteran < elite, and that contender accuracy lands at about 45–85%.
5. In the PR, say what skill it trains and include the calibrate output for your scenario.

## PR checklist

- [ ] One logical change; about 400 changed lines or fewer (split larger work into chained PRs).
- [ ] `fmt`, `clippy` and `test` pass locally.
- [ ] Tests cover the new rule or the bug.
- [ ] Code lives in the right layer: no gameplay rules in `aim_app`, no engine or I/O in `aim_core`.
- [ ] Any new game constant has a source and confidence tag in `docs/valorant-mechanics.md`; any training claim has a citation in `docs/training-science.md`.
- [ ] User-facing docs are updated: README, `docs/scenarios.md`, in-app text.
- [ ] Nothing touches the game process, ships Riot assets or replicates Riot maps (integrity rules in the coding standards).
- [ ] Manual testing stated: what you tried and on which OS and compositor.

## Commits

Use Conventional Commits with a scope:

- `feat(core): add drift motion easing`
- `fix(app): release cursor when overlay hides`
- `feat(content): add long-angle hold scenario`
- `docs: explain monitor-distance matching`

Write commits, code, comments and issues in English.

## Reporting bugs

Include:

- your OS and version;
- your GPU and driver;
- your compositor or desktop (for example Hyprland 0.56, Windows 11);
- the scenario `id`;
- what you expected and what happened.

For input or feel issues, also include your DPI and sensitivity settings and the result of the DPI check in Settings.
