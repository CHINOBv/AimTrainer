//! Built-in crosshair presets: one TOML file per preset in `content/crosshairs/`, each a
//! VALORANT profile code so it can be pasted straight into the game.

use super::{Crosshair, code};
use serde::Deserialize;
use std::sync::LazyLock;

/// A named crosshair preset.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    /// Stable kebab-case ID (equal to the file name).
    pub id: String,
    pub name: String,
    /// Canonical VALORANT profile code (what [`code::encode`] produces).
    pub code: String,
}

impl Preset {
    pub fn from_toml(src: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(src)
    }

    pub fn crosshair(&self) -> Crosshair {
        code::parse(&self.code).expect("built-in preset codes are valid (see tests)")
    }
}

pub const SOURCES: &[(&str, &str)] = &[
    (
        "valorant-default.toml",
        include_str!("../../../../content/crosshairs/valorant-default.toml"),
    ),
    (
        "small-dot.toml",
        include_str!("../../../../content/crosshairs/small-dot.toml"),
    ),
    (
        "classic-plus.toml",
        include_str!("../../../../content/crosshairs/classic-plus.toml"),
    ),
    (
        "tiny-plus.toml",
        include_str!("../../../../content/crosshairs/tiny-plus.toml"),
    ),
];

static PRESETS: LazyLock<Vec<Preset>> = LazyLock::new(|| {
    SOURCES
        .iter()
        .map(|(file, src)| {
            Preset::from_toml(src).unwrap_or_else(|e| panic!("built-in {file}: {e}"))
        })
        .collect()
});

/// Every built-in preset, in `SOURCES` order (parsed once).
pub fn all() -> &'static [Preset] {
    &PRESETS
}

/// Looks a preset up by ID.
pub fn find(id: &str) -> Option<&'static Preset> {
    all().iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn presets_are_valid_canonical_unique_and_named_after_their_file() {
        let mut ids = HashSet::new();
        for (file, _) in SOURCES {
            let p = all().iter().find(|p| format!("{}.toml", p.id) == *file);
            assert!(p.is_some(), "{file}: file name must equal its id");
        }
        for p in all() {
            assert!(ids.insert(p.id.clone()), "duplicate id {}", p.id);
            assert_eq!(code::encode(&p.crosshair()), p.code, "{}", p.id);
        }
        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn valorant_default_preset_is_the_factory_crosshair() {
        assert_eq!(
            find("valorant-default").map(Preset::crosshair),
            Some(Crosshair::default())
        );
        assert!(find("missing").is_none());
    }

    #[test]
    fn small_dot_shows_only_the_dot() {
        let ch = find("small-dot").unwrap().crosshair();
        assert!(ch.center_dot && !ch.inner.show && !ch.outer.show);
    }
}
