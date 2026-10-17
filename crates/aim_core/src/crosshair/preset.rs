//! Built-in crosshair presets, stored as VALORANT profile codes so they can be pasted
//! straight into the game.

use super::{Crosshair, code};

/// A named built-in crosshair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    /// Stable kebab-case ID.
    pub id: &'static str,
    pub name: &'static str,
    /// Canonical VALORANT profile code (what [`code::encode`] produces).
    pub code: &'static str,
}

impl Preset {
    pub fn crosshair(&self) -> Crosshair {
        code::parse(self.code).expect("built-in preset codes are valid (see tests)")
    }
}

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "valorant-default",
        name: "VALORANT default",
        code: "0",
    },
    Preset {
        id: "small-dot",
        name: "Small dot",
        code: "0;P;d;1;0b;0;1b;0",
    },
    Preset {
        id: "classic-plus",
        name: "Classic cyan plus",
        code: "0;P;c;5;0l;4;0o;2;0a;1;0f;0;1b;0",
    },
    Preset {
        id: "tiny-plus",
        name: "Tiny plus, no outlines",
        code: "0;P;h;0;0t;1;0l;2;0o;1;0a;1;1b;0",
    },
];

/// Looks a preset up by ID.
pub fn find(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn presets_are_valid_canonical_and_unique() {
        let mut ids = HashSet::new();
        for p in PRESETS {
            assert!(ids.insert(p.id), "duplicate id {}", p.id);
            assert_eq!(code::encode(&p.crosshair()), p.code, "{}", p.id);
        }
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
