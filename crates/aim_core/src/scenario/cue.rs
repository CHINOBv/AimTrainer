//! Perception drills (`[targets.perception]`): targets that are first heard before they are
//! seen. The session reads these settings when it spawns and reveals targets; see
//! "Perception drills" in `docs/scenarios.md`.

use super::def::ScenarioDef;
use serde::{Deserialize, Serialize};

/// Longest delay a sound cue may wait before revealing its target, seconds.
pub const MAX_REVEAL_AFTER_SECS: f32 = 10.0;

/// A perception drill that changes how targets are announced and revealed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Perception {
    /// Sound localisation: each target is first only heard, a synthesized sound played at
    /// its position, and becomes visible `reveal_after_secs` later, or as soon as the
    /// crosshair comes within `reveal_within_deg` of it.
    SoundCue {
        /// Sound the target makes when it spawns.
        sound: CueSound,
        /// Seconds from the sound to the target appearing, in (0, 10].
        reveal_after_secs: f32,
        /// Reveal the target early once the crosshair is within this angle of its aim
        /// point, degrees (0 = off).
        #[serde(default)]
        reveal_within_deg: f32,
    },
}

/// The synthesized sound a [`Perception::SoundCue`] target makes. Both are made up in the
/// app from noise and tones; no game sound is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CueSound {
    /// A few quick footsteps.
    Footsteps,
    /// A single sharp gunshot-like crack.
    Gunshot,
}

impl Perception {
    /// Problems with these settings in scenario `def`, as `(field, message)`.
    pub(super) fn problems(&self, _def: &ScenarioDef) -> Vec<(&'static str, &'static str)> {
        let mut out = Vec::new();
        match *self {
            Perception::SoundCue {
                reveal_after_secs,
                reveal_within_deg,
                ..
            } => {
                if !(reveal_after_secs > 0.0 && reveal_after_secs <= MAX_REVEAL_AFTER_SECS) {
                    out.push((
                        "targets.perception.reveal_after_secs",
                        "must be in (0, 10] seconds",
                    ));
                }
                if !(0.0..180.0).contains(&reveal_within_deg) {
                    out.push((
                        "targets.perception.reveal_within_deg",
                        "must be in [0, 180) degrees",
                    ));
                }
            }
        }
        out
    }

    /// Seconds from a sound cue to its target appearing, if this is a sound cue.
    pub(super) fn reveal_after_secs(&self) -> Option<f32> {
        match *self {
            Perception::SoundCue {
                reveal_after_secs, ..
            } => Some(reveal_after_secs),
        }
    }

    /// The early-reveal angle of a sound cue, degrees, if it is on.
    pub(super) fn reveal_within_deg(&self) -> Option<f32> {
        match *self {
            Perception::SoundCue {
                reveal_within_deg, ..
            } => (reveal_within_deg > 0.0).then_some(reveal_within_deg),
        }
    }

    /// The sound of a sound cue.
    pub fn cue_sound(&self) -> Option<CueSound> {
        match *self {
            Perception::SoundCue { sound, .. } => Some(sound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Range, ViewAngles};
    use crate::scenario::{FrameInput, ScenarioDef, Session, SessionEvent, SpawnArea};

    /// One static sphere straight behind the player, announced by footsteps.
    fn cue_def(reveal_within_deg: f32, lifetime: &str) -> ScenarioDef {
        let src = format!(
            r#"
            id = "cue"
            name = "Cue"
            category = "reaction"
            duration_secs = 30
            [targets]
            shape = {{ kind = "sphere", radius = 0.3 }}
            spawn = {{ kind = "angular", yaw_deg = [180, 180], pitch_deg = [0, 0], distance = [10, 10] }}
            {lifetime}
            [targets.perception]
            kind = "sound_cue"
            sound = "footsteps"
            reveal_after_secs = 1.0
            reveal_within_deg = {reveal_within_deg}
            "#
        );
        let def = ScenarioDef::from_toml(&src).expect("valid toml");
        assert!(def.validate().is_empty(), "{:?}", def.validate());
        def
    }

    const AHEAD: ViewAngles = ViewAngles {
        yaw_deg: 0.0,
        pitch_deg: 0.0,
    };

    fn frame(s: &mut Session, view: ViewAngles, fire: bool) -> Vec<SessionEvent> {
        s.update(FrameInput {
            dt: 0.05,
            view,
            fire_pressed: fire,
            fire_held: fire,
            ..Default::default()
        })
    }

    fn towards_heard(s: &Session) -> ViewAngles {
        let t = s.heard_targets().next().expect("a heard target");
        ViewAngles::from_direction(t.position - s.eye_position())
    }

    fn is_cue(e: &SessionEvent) -> bool {
        matches!(e, SessionEvent::Cue { .. })
    }

    #[test]
    fn a_cued_target_is_heard_first_and_appears_after_the_delay() {
        let mut s = Session::new(cue_def(0.0, ""), 1);
        assert_eq!(s.targets().count(), 0, "hidden until its sound has played");
        let events = frame(&mut s, AHEAD, false);
        let cues: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                SessionEvent::Cue {
                    id,
                    position,
                    sound,
                } => Some((*id, *position, *sound)),
                _ => None,
            })
            .collect();
        assert_eq!(cues.len(), 1, "{events:?}");
        let (id, position, sound) = cues[0];
        assert_eq!(sound, CueSound::Footsteps);
        assert!(position.z > 9.9, "behind the player (+Z): {position:?}");
        let heard = s.heard_targets().next().expect("heard");
        assert_eq!((heard.id, heard.cue_at), (id, Some(0.0)));
        assert!(heard.is_heard());
        assert_eq!(s.targets().count(), 0, "still hidden right after the sound");
        // The sound plays once, and the target stays hidden for reveal_after_secs.
        for _ in 0..18 {
            let events = frame(&mut s, AHEAD, false);
            assert!(!events.iter().any(is_cue), "{events:?}");
            assert_eq!(s.targets().count(), 0, "t = {}", s.time());
        }
        frame(&mut s, AHEAD, false);
        assert!(s.time() >= 1.0);
        assert_eq!(
            s.targets().count(),
            1,
            "visible reveal_after_secs after the sound"
        );
    }

    #[test]
    fn a_hidden_target_cannot_be_shot_until_it_appears() {
        let mut s = Session::new(cue_def(0.0, ""), 1);
        frame(&mut s, AHEAD, false);
        let aim = towards_heard(&s);
        frame(&mut s, aim, false);
        frame(&mut s, aim, true);
        assert_eq!((s.stats().shots, s.stats().hits), (1, 0));
    }

    #[test]
    fn the_crosshair_near_a_heard_target_reveals_it_early_and_its_lifetime_starts_then() {
        let mut s = Session::new(cue_def(20.0, "lifetime_secs = 0.5"), 1);
        frame(&mut s, AHEAD, false);
        assert_eq!(s.targets().count(), 0, "the crosshair is 180° away");
        let near = towards_heard(&s).rotated(15.0, 0.0);
        frame(&mut s, near, false);
        let t = s.targets().next().expect("revealed by the crosshair");
        assert!((t.visible_at - 0.1).abs() < 1e-5, "{}", t.visible_at);
        // Lifetime counts from the reveal: gone 0.5 s later, not 1.5 s after the sound.
        for _ in 0..9 {
            frame(&mut s, near, false);
        }
        assert_eq!(s.stats().expired, 0, "t = {}", s.time());
        frame(&mut s, near, false);
        assert_eq!(s.stats().expired, 1, "t = {}", s.time());
    }

    #[test]
    fn a_revealed_target_can_be_killed_and_the_next_one_is_heard_at_once() {
        let mut s = Session::new(cue_def(20.0, ""), 1);
        frame(&mut s, AHEAD, false);
        let aim = towards_heard(&s);
        frame(&mut s, aim, false);
        let events = frame(&mut s, aim, true);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SessionEvent::Killed { .. }))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SessionEvent::Cue { id: 2, .. })),
            "the respawn makes its sound in the same frame: {events:?}"
        );
    }

    #[test]
    fn sound_cues_are_deterministic_per_seed() {
        let run = |seed| {
            let mut def = cue_def(20.0, "lifetime_secs = 1.0");
            def.targets.spawn = SpawnArea::Angular {
                yaw_deg: Range::new(-180.0, 180.0),
                pitch_deg: Range::fixed(0.0),
                distance: Range::new(6.0, 14.0),
            };
            let mut s = Session::new(def, seed);
            let mut log = Vec::new();
            for i in 0..400 {
                let view = AHEAD.rotated(i as f32 * 3.0, 0.0);
                log.extend(frame(&mut s, view, i % 7 == 0));
            }
            log
        };
        assert_eq!(run(5), run(5));
        assert!(run(5).iter().filter(|e| is_cue(e)).count() > 3);
    }

    #[test]
    fn perception_defaults_off_stays_out_of_saved_files_and_is_validated() {
        let plain = cue_def(0.0, "");
        let mut off = plain.clone();
        off.targets.perception = None;
        assert!(!toml::to_string(&off).unwrap().contains("perception"));
        let saved = toml::to_string(&plain).unwrap();
        assert_eq!(ScenarioDef::from_toml(&saved).unwrap(), plain);

        let fields = |p: Perception| {
            let mut d = plain.clone();
            d.targets.perception = Some(p);
            d.validate()
                .into_iter()
                .map(|e| e.field)
                .collect::<Vec<_>>()
        };
        let cue = |reveal_after_secs, reveal_within_deg| Perception::SoundCue {
            sound: CueSound::Gunshot,
            reveal_after_secs,
            reveal_within_deg,
        };
        for bad in [0.0, -1.0, 10.5, f32::NAN] {
            assert_eq!(
                fields(cue(bad, 0.0)),
                ["targets.perception.reveal_after_secs"],
                "{bad}"
            );
        }
        for bad in [-1.0, 180.0, f32::NAN] {
            assert_eq!(
                fields(cue(1.0, bad)),
                ["targets.perception.reveal_within_deg"],
                "{bad}"
            );
        }
        assert!(fields(cue(10.0, 179.0)).is_empty());
        let typo = saved.replace("reveal_after_secs", "reveal_afterr_secs");
        assert!(ScenarioDef::from_toml(&typo).is_err());
    }

    #[test]
    fn scenarios_without_a_perception_drill_never_cue() {
        for def in crate::scenario::builtin::all()
            .into_iter()
            .filter(|d| d.targets.perception.is_none())
        {
            let mut s = Session::new(def.clone(), 3);
            for _ in 0..40 {
                let events = frame(&mut s, AHEAD, true);
                assert!(!events.iter().any(is_cue), "{}", def.id);
            }
            assert!(s.targets().all(|t| t.cue_at.is_none()), "{}", def.id);
            assert_eq!(s.heard_targets().count(), 0, "{}", def.id);
        }
    }
}
