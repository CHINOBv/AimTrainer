## What and why

<!-- One logical change. Link the issue or roadmap phase. -->

## How it was tested

<!-- Commands run and manual checks, including the OS and compositor. -->

## Checklist

The full checklist, with explanations, is in [CONTRIBUTING.md](../CONTRIBUTING.md#pr-checklist).

- [ ] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets` and `cargo test --workspace` pass
- [ ] Tests cover the new rule or bug
- [ ] Code is in the right layer: no rules in `aim_app`, no engine or I/O in `aim_core`
- [ ] New game constants and training claims have sources
- [ ] User-facing docs are updated
- [ ] Nothing touches the game process, ships game assets or replicates game maps
