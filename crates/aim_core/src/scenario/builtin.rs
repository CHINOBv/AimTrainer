//! Built-in scenario library, embedded at compile time. The same TOML files double as
//! examples for writing custom scenarios.

use super::def::ScenarioDef;

/// Embedded built-in scenario files as `(file name, TOML source)`.
pub const SOURCES: &[(&str, &str)] = &[
    (
        "adjust-track.toml",
        include_str!("../../../../content/scenarios/adjust-track.toml"),
    ),
    (
        "ammo-discipline.toml",
        include_str!("../../../../content/scenarios/ammo-discipline.toml"),
    ),
    (
        "angle-flyers.toml",
        include_str!("../../../../content/scenarios/angle-flyers.toml"),
    ),
    (
        "angle-hold-peek.toml",
        include_str!("../../../../content/scenarios/angle-hold-peek.toml"),
    ),
    (
        "arc-follow.toml",
        include_str!("../../../../content/scenarios/arc-follow.toml"),
    ),
    (
        "bobbing-heads.toml",
        include_str!("../../../../content/scenarios/bobbing-heads.toml"),
    ),
    (
        "burst-control.toml",
        include_str!("../../../../content/scenarios/burst-control.toml"),
    ),
    (
        "burst-strafe.toml",
        include_str!("../../../../content/scenarios/burst-strafe.toml"),
    ),
    (
        "close-strafe-heads.toml",
        include_str!("../../../../content/scenarios/close-strafe-heads.toml"),
    ),
    (
        "cockroach-hop.toml",
        include_str!("../../../../content/scenarios/cockroach-hop.toml"),
    ),
    (
        "counter-strafe.toml",
        include_str!("../../../../content/scenarios/counter-strafe.toml"),
    ),
    (
        "crouch-duel.toml",
        include_str!("../../../../content/scenarios/crouch-duel.toml"),
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
        "depth-brake.toml",
        include_str!("../../../../content/scenarios/depth-brake.toml"),
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
        "dot-sweep.toml",
        include_str!("../../../../content/scenarios/dot-sweep.toml"),
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
        "fade-track.toml",
        include_str!("../../../../content/scenarios/fade-track.toml"),
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
        "flinch-stop.toml",
        include_str!("../../../../content/scenarios/flinch-stop.toml"),
    ),
    (
        "fly-switch.toml",
        include_str!("../../../../content/scenarios/fly-switch.toml"),
    ),
    (
        "four-shrink.toml",
        include_str!("../../../../content/scenarios/four-shrink.toml"),
    ),
    (
        "go-no-go.toml",
        include_str!("../../../../content/scenarios/go-no-go.toml"),
    ),
    (
        "head-rail-run.toml",
        include_str!("../../../../content/scenarios/head-rail-run.toml"),
    ),
    (
        "hop-field.toml",
        include_str!("../../../../content/scenarios/hop-field.toml"),
    ),
    (
        "hop-switch.toml",
        include_str!("../../../../content/scenarios/hop-switch.toml"),
    ),
    (
        "jitter-switch.toml",
        include_str!("../../../../content/scenarios/jitter-switch.toml"),
    ),
    (
        "ground-reactive.toml",
        include_str!("../../../../content/scenarios/ground-reactive.toml"),
    ),
    (
        "head-strafe-track.toml",
        include_str!("../../../../content/scenarios/head-strafe-track.toml"),
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
        "mini-regen-switch.toml",
        include_str!("../../../../content/scenarios/mini-regen-switch.toml"),
    ),
    (
        "micro-cluster.toml",
        include_str!("../../../../content/scenarios/micro-cluster.toml"),
    ),
    (
        "micro-pace.toml",
        include_str!("../../../../content/scenarios/micro-pace.toml"),
    ),
    (
        "mini-phase.toml",
        include_str!("../../../../content/scenarios/mini-phase.toml"),
    ),
    (
        "mirror-duel.toml",
        include_str!("../../../../content/scenarios/mirror-duel.toml"),
    ),
    (
        "orbit-track.toml",
        include_str!("../../../../content/scenarios/orbit-track.toml"),
    ),
    (
        "peek-reaction.toml",
        include_str!("../../../../content/scenarios/peek-reaction.toml"),
    ),
    (
        "peek-track.toml",
        include_str!("../../../../content/scenarios/peek-track.toml"),
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
        "pill-crossfire.toml",
        include_str!("../../../../content/scenarios/pill-crossfire.toml"),
    ),
    (
        "popcorn-burst.toml",
        include_str!("../../../../content/scenarios/popcorn-burst.toml"),
    ),
    (
        "precision-rings.toml",
        include_str!("../../../../content/scenarios/precision-rings.toml"),
    ),
    (
        "pressure-balloons.toml",
        include_str!("../../../../content/scenarios/pressure-balloons.toml"),
    ),
    (
        "prism-bounce.toml",
        include_str!("../../../../content/scenarios/prism-bounce.toml"),
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
        "reactive-flick.toml",
        include_str!("../../../../content/scenarios/reactive-flick.toml"),
    ),
    (
        "reflex-flick.toml",
        include_str!("../../../../content/scenarios/reflex-flick.toml"),
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
        "shrinking-movers.toml",
        include_str!("../../../../content/scenarios/shrinking-movers.toml"),
    ),
    (
        "six-pin.toml",
        include_str!("../../../../content/scenarios/six-pin.toml"),
    ),
    (
        "skeet-arcs.toml",
        include_str!("../../../../content/scenarios/skeet-arcs.toml"),
    ),
    (
        "sky-reactive.toml",
        include_str!("../../../../content/scenarios/sky-reactive.toml"),
    ),
    (
        "smooth-xyz.toml",
        include_str!("../../../../content/scenarios/smooth-xyz.toml"),
    ),
    (
        "snap-reflex.toml",
        include_str!("../../../../content/scenarios/snap-reflex.toml"),
    ),
    (
        "sound-turn.toml",
        include_str!("../../../../content/scenarios/sound-turn.toml"),
    ),
    (
        "spider-strafe.toml",
        include_str!("../../../../content/scenarios/spider-strafe.toml"),
    ),
    (
        "spider-web.toml",
        include_str!("../../../../content/scenarios/spider-web.toml"),
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
        "spread-gate.toml",
        include_str!("../../../../content/scenarios/spread-gate.toml"),
    ),
    (
        "squad-strafe.toml",
        include_str!("../../../../content/scenarios/squad-strafe.toml"),
    ),
    (
        "static-headline.toml",
        include_str!("../../../../content/scenarios/static-headline.toml"),
    ),
    (
        "stop-go-weavers.toml",
        include_str!("../../../../content/scenarios/stop-go-weavers.toml"),
    ),
    (
        "strafe-shot.toml",
        include_str!("../../../../content/scenarios/strafe-shot.toml"),
    ),
    (
        "stutter-ladder.toml",
        include_str!("../../../../content/scenarios/stutter-ladder.toml"),
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
        "tempo-movers.toml",
        include_str!("../../../../content/scenarios/tempo-movers.toml"),
    ),
    (
        "thin-glide.toml",
        include_str!("../../../../content/scenarios/thin-glide.toml"),
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
        "wave-runners.toml",
        include_str!("../../../../content/scenarios/wave-runners.toml"),
    ),
    (
        "wide-wall-five.toml",
        include_str!("../../../../content/scenarios/wide-wall-five.toml"),
    ),
    (
        "zigzag-sparks.toml",
        include_str!("../../../../content/scenarios/zigzag-sparks.toml"),
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

    #[test]
    fn builtins_modelled_on_valorant_are_tagged_valorant() {
        use crate::games::Game;
        use crate::scenario::def::{Motion, Shape};
        let valorant_mechanic = |d: &ScenarioDef| {
            let human_motion =
                |m: &Motion| matches!(m, Motion::Strafe { .. } | Motion::Dodge { .. });
            matches!(d.targets.shape, Shape::Humanoid { .. })
                || human_motion(&d.targets.motion)
                || d.targets
                    .motion_profiles
                    .iter()
                    .any(|p| human_motion(&p.motion))
                || d.player.movement
                || d.weapon.recoil
                || d.weapon.simulate_spread
        };
        let all = all();
        for d in &all {
            if valorant_mechanic(d) {
                assert_eq!(
                    d.game,
                    Some(Game::Valorant),
                    "{} uses VALORANT mechanics",
                    d.id
                );
            }
            assert!(
                matches!(d.game, None | Some(Game::Valorant)),
                "{}: only VALORANT content ships today",
                d.id
            );
        }
        assert!(all.iter().any(|d| d.game.is_none()), "generic drills exist");
    }

    /// Every piece of built-in content text the UI shows as-is: names, descriptions,
    /// routine notes, benchmark ranks and tiers, method credit, arena and map text and
    /// angle labels. Arenas and maps belong to `aim_world`, so their files are read here.
    fn shown_content_text() -> Vec<(String, String)> {
        use crate::crosshair::preset;
        use crate::training::{benchmark, method, routine};
        let mut out: Vec<(String, String)> = Vec::new();
        let mut push = |at: &str, text: &str| {
            if !text.trim().is_empty() {
                // A CRLF checkout gives CRLF line breaks; catalog keys use LF (see `tr_in`).
                out.push((at.to_owned(), text.replace("\r\n", "\n")));
            }
        };
        for d in all() {
            push(&d.id, &d.name);
            push(&d.id, &d.description);
        }
        for r in routine::builtin::all() {
            push(&r.id, &r.name);
            push(&r.id, &r.description);
            for b in &r.blocks {
                push(&r.id, &b.note);
            }
        }
        for b in benchmark::builtin::all() {
            push(&b.id, &b.name);
            push(&b.id, &b.description);
            b.ranks.iter().for_each(|x| push(&b.id, x));
            b.tiers.iter().for_each(|t| push(&b.id, &t.name));
            b.subcategories.iter().for_each(|s| push(&b.id, &s.name));
        }
        for m in method::builtin::all() {
            push(&m.id, &m.name);
            push(&m.id, &m.description);
            push(&m.id, &m.credit);
            m.tiers.iter().for_each(|t| push(&m.id, &t.name));
        }
        for p in preset::all() {
            push(&p.id, &p.name);
        }
        for dir in ["arenas", "maps"] {
            let path = format!("{}/../../content/{dir}", env!("CARGO_MANIFEST_DIR"));
            let mut files: Vec<_> = std::fs::read_dir(&path)
                .unwrap_or_else(|e| panic!("{path}: {e}"))
                .map(|e| e.unwrap().path())
                .collect();
            files.sort();
            for file in files {
                let at = file.display().to_string();
                let src = std::fs::read_to_string(&file).unwrap();
                let table: toml::Table = toml::from_str(&src).unwrap();
                let text = |t: &toml::Table, key: &str| {
                    t.get(key).and_then(|v| v.as_str()).unwrap_or("").to_owned()
                };
                push(&at, &text(&table, "name"));
                push(&at, &text(&table, "description"));
                if let Some(angles) = table.get("angles").and_then(|a| a.as_array()) {
                    for angle in angles.iter().filter_map(|a| a.as_table()) {
                        push(&at, &text(angle, "label"));
                    }
                }
            }
        }
        out
    }

    /// `text` as a TOML basic string, for the missing-entry report.
    fn quoted(text: &str) -> String {
        let escaped = text
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n");
        format!("\"{escaped}\"")
    }

    #[test]
    fn every_shown_content_text_has_a_spanish_entry() {
        let catalog: std::collections::BTreeMap<String, String> =
            toml::from_str(include_str!("../../../../locales/es/content.toml")).unwrap();
        let shown = shown_content_text();
        let texts: HashSet<&str> = shown.iter().map(|(_, t)| t.as_str()).collect();
        let stale: Vec<&String> = catalog
            .keys()
            .filter(|k| !texts.contains(k.as_str()))
            .collect();
        assert!(
            stale.is_empty(),
            "entries for text no content shows: {stale:?}"
        );
        let mut seen = HashSet::new();
        let missing: Vec<String> = shown
            .into_iter()
            .filter(|(_, text)| !catalog.contains_key(text) && seen.insert(text.clone()))
            .map(|(at, text)| format!("# {at}\n{} = \"\"", quoted(&text)))
            .collect();
        assert!(
            missing.is_empty(),
            "{} content texts have no entry in locales/es/content.toml:\n{}",
            missing.len(),
            missing.join("\n")
        );
    }

    #[test]
    fn content_text_is_shown_in_spanish_and_english_is_unchanged() {
        use crate::i18n::{Lang, tr_in};
        let track = all().into_iter().find(|d| d.id == "adjust-track").unwrap();
        assert_eq!(tr_in(Lang::En, &track.description), track.description);
        assert_ne!(tr_in(Lang::Es, &track.description), track.description);
        assert_eq!(tr_in(Lang::Es, "Recruit"), "Recluta");
    }
}
