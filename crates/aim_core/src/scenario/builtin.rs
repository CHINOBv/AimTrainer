//! Built-in scenario library, embedded at compile time. The same TOML files double as
//! examples for writing custom scenarios.

use super::def::ScenarioDef;

/// Embedded built-in scenario files as `(file name, TOML source)`.
pub const SOURCES: &[(&str, &str)] = &[
    (
        "ammo-discipline.toml",
        include_str!("../../../../content/scenarios/ammo-discipline.toml"),
    ),
    (
        "angle-flyers.toml",
        include_str!("../../../../content/scenarios/angle-flyers.toml"),
    ),
    (
        "burst-control.toml",
        include_str!("../../../../content/scenarios/burst-control.toml"),
    ),
    (
        "counter-strafe.toml",
        include_str!("../../../../content/scenarios/counter-strafe.toml"),
    ),
    (
        "demo-ballistic.toml",
        include_str!("../../../../content/scenarios/demo-ballistic.toml"),
    ),
    (
        "demo-bounce.toml",
        include_str!("../../../../content/scenarios/demo-bounce.toml"),
    ),
    (
        "demo-capsule.toml",
        include_str!("../../../../content/scenarios/demo-capsule.toml"),
    ),
    (
        "demo-depth.toml",
        include_str!("../../../../content/scenarios/demo-depth.toml"),
    ),
    (
        "demo-grid.toml",
        include_str!("../../../../content/scenarios/demo-grid.toml"),
    ),
    (
        "demo-orbit.toml",
        include_str!("../../../../content/scenarios/demo-orbit.toml"),
    ),
    (
        "demo-profiles.toml",
        include_str!("../../../../content/scenarios/demo-profiles.toml"),
    ),
    (
        "demo-shrink.toml",
        include_str!("../../../../content/scenarios/demo-shrink.toml"),
    ),
    (
        "diag-drifters.toml",
        include_str!("../../../../content/scenarios/diag-drifters.toml"),
    ),
    (
        "diamond-pair.toml",
        include_str!("../../../../content/scenarios/diamond-pair.toml"),
    ),
    (
        "dodge-crouch-spam.toml",
        include_str!("../../../../content/scenarios/dodge-crouch-spam.toml"),
    ),
    (
        "dodge-juke-heads.toml",
        include_str!("../../../../content/scenarios/dodge-juke-heads.toml"),
    ),
    (
        "dodge-jump-spot.toml",
        include_str!("../../../../content/scenarios/dodge-jump-spot.toml"),
    ),
    (
        "dodge-micro-spheres.toml",
        include_str!("../../../../content/scenarios/dodge-micro-spheres.toml"),
    ),
    (
        "dodge-peek.toml",
        include_str!("../../../../content/scenarios/dodge-peek.toml"),
    ),
    (
        "drifting-heads.toml",
        include_str!("../../../../content/scenarios/drifting-heads.toml"),
    ),
    (
        "dynamic-strafe-heads.toml",
        include_str!("../../../../content/scenarios/dynamic-strafe-heads.toml"),
    ),
    (
        "flick-headshot.toml",
        include_str!("../../../../content/scenarios/flick-headshot.toml"),
    ),
    (
        "flick-wide.toml",
        include_str!("../../../../content/scenarios/flick-wide.toml"),
    ),
    (
        "go-no-go.toml",
        include_str!("../../../../content/scenarios/go-no-go.toml"),
    ),
    (
        "hop-field.toml",
        include_str!("../../../../content/scenarios/hop-field.toml"),
    ),
    (
        "long-range-heads.toml",
        include_str!("../../../../content/scenarios/long-range-heads.toml"),
    ),
    (
        "micro-adjust.toml",
        include_str!("../../../../content/scenarios/micro-adjust.toml"),
    ),
    (
        "peek-reaction.toml",
        include_str!("../../../../content/scenarios/peek-reaction.toml"),
    ),
    (
        "phase-kill-race.toml",
        include_str!("../../../../content/scenarios/phase-kill-race.toml"),
    ),
    (
        "phase-peek-ladder.toml",
        include_str!("../../../../content/scenarios/phase-peek-ladder.toml"),
    ),
    (
        "peripheral-detect.toml",
        include_str!("../../../../content/scenarios/peripheral-detect.toml"),
    ),
    (
        "precision-rings.toml",
        include_str!("../../../../content/scenarios/precision-rings.toml"),
    ),
    (
        "radar-recall.toml",
        include_str!("../../../../content/scenarios/radar-recall.toml"),
    ),
    (
        "react-duel.toml",
        include_str!("../../../../content/scenarios/react-duel.toml"),
    ),
    (
        "regen-track.toml",
        include_str!("../../../../content/scenarios/regen-track.toml"),
    ),
    (
        "ricochet-ring.toml",
        include_str!("../../../../content/scenarios/ricochet-ring.toml"),
    ),
    (
        "six-pin.toml",
        include_str!("../../../../content/scenarios/six-pin.toml"),
    ),
    (
        "sound-turn.toml",
        include_str!("../../../../content/scenarios/sound-turn.toml"),
    ),
    (
        "spray-transfer.toml",
        include_str!("../../../../content/scenarios/spray-transfer.toml"),
    ),
    (
        "spread-flicks.toml",
        include_str!("../../../../content/scenarios/spread-flicks.toml"),
    ),
    (
        "static-headline.toml",
        include_str!("../../../../content/scenarios/static-headline.toml"),
    ),
    (
        "strafe-shot.toml",
        include_str!("../../../../content/scenarios/strafe-shot.toml"),
    ),
    (
        "switch-evasive-small.toml",
        include_str!("../../../../content/scenarios/switch-evasive-small.toml"),
    ),
    (
        "switch-evasive.toml",
        include_str!("../../../../content/scenarios/switch-evasive.toml"),
    ),
    (
        "switch-heads.toml",
        include_str!("../../../../content/scenarios/switch-heads.toml"),
    ),
    (
        "switch-speed.toml",
        include_str!("../../../../content/scenarios/switch-speed.toml"),
    ),
    (
        "switch-wide.toml",
        include_str!("../../../../content/scenarios/switch-wide.toml"),
    ),
    (
        "tap-discipline.toml",
        include_str!("../../../../content/scenarios/tap-discipline.toml"),
    ),
    (
        "track-close-strafe.toml",
        include_str!("../../../../content/scenarios/track-close-strafe.toml"),
    ),
    (
        "track-orbit.toml",
        include_str!("../../../../content/scenarios/track-orbit.toml"),
    ),
    (
        "track-precise.toml",
        include_str!("../../../../content/scenarios/track-precise.toml"),
    ),
    (
        "track-reactive-micro.toml",
        include_str!("../../../../content/scenarios/track-reactive-micro.toml"),
    ),
    (
        "track-reactive-strafe.toml",
        include_str!("../../../../content/scenarios/track-reactive-strafe.toml"),
    ),
    (
        "wide-wall-five.toml",
        include_str!("../../../../content/scenarios/wide-wall-five.toml"),
    ),
];

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_builtin_parses_validates_and_has_a_unique_id() {
        let all = all();
        assert_eq!(all.len(), SOURCES.len());
        let mut ids = HashSet::new();
        for (def, (file, _)) in all.iter().zip(SOURCES) {
            assert!(def.validate().is_empty(), "{file}: {:?}", def.validate());
            assert_eq!(format!("{}.toml", def.id), *file, "file name must match id");
            assert!(ids.insert(def.id.clone()), "duplicate id {}", def.id);
        }
    }
}
