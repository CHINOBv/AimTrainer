//! Built-in scenario library, embedded at compile time. The same TOML files double as
//! examples for writing custom scenarios.

use super::def::ScenarioDef;

pub const SOURCES: &[(&str, &str)] = &[];

/// Parse every built-in scenario. Panics only if a shipped file is broken, which the
/// test suite guards against.
pub fn all() -> Vec<ScenarioDef> {
    SOURCES
        .iter()
        .map(|(file, src)| {
            ScenarioDef::from_toml(src).unwrap_or_else(|e| panic!("built-in {file}: {e}"))
        })
        .collect()
}
