//! Perception drills (`[targets.perception]`): targets that are heard before they are seen,
//! brief flashes in peripheral vision, and a radar glance to remember. The session reads
//! these settings when it spawns, reveals and scores targets; see "Perception drills" in
//! `docs/scenarios.md`.

use super::MIN_SPHERE_HEIGHT;
use super::def::{FireMode, ScenarioDef, Shape, SpawnArea};
use crate::fov;
use crate::math::{Range, Vec3, ViewAngles};
use crate::rng::Rng;
use serde::{Deserialize, Serialize};

/// Longest delay a sound cue may wait before revealing its target, seconds.
pub const MAX_REVEAL_AFTER_SECS: f32 = 10.0;
/// Highest share of blank trials a detection drill may have.
pub const MAX_BLANK_CHANCE: f32 = 0.9;
/// Longest a radar may stay on screen, seconds.
pub const MAX_RADAR_SECS: f32 = 10.0;

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
    /// Peripheral detection: each trial flashes a target in one of eight screen areas
    /// around the crosshair (`spawn.offset_deg` away), or stays blank. A click anywhere
    /// while a flash shows detects it; a click with nothing showing is a false alarm.
    Detection {
        /// Share of trials with no flash, in [0, 0.9].
        #[serde(default)]
        blank_chance: f32,
    },
    /// Minimap-glance recall: each round a radar shows where `dots` of the round's
    /// `max_alive` targets will be for `show_secs`; then they all appear looking alike.
    /// The ones the radar did not show are decoys to leave alone.
    Radar {
        /// Seconds the radar stays on screen, in (0, 10].
        show_secs: f32,
        /// Targets per round that the radar shows, from 1 to `max_alive - 1`.
        dots: u32,
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
    pub(super) fn problems(&self, def: &ScenarioDef) -> Vec<(&'static str, &'static str)> {
        let mut out = Vec::new();
        let mut check = |ok: bool, field, msg| {
            if !ok {
                out.push((field, msg));
            }
        };
        let t = &def.targets;
        let click = def.weapon.fire == FireMode::Click;
        match *self {
            Perception::SoundCue {
                reveal_after_secs,
                reveal_within_deg,
                ..
            } => {
                check(
                    reveal_after_secs > 0.0 && reveal_after_secs <= MAX_REVEAL_AFTER_SECS,
                    "targets.perception.reveal_after_secs",
                    "must be in (0, 10] seconds",
                );
                check(
                    (0.0..180.0).contains(&reveal_within_deg),
                    "targets.perception.reveal_within_deg",
                    "must be in [0, 180) degrees",
                );
            }
            Perception::Detection { blank_chance } => {
                check(
                    (0.0..=MAX_BLANK_CHANCE).contains(&blank_chance),
                    "targets.perception.blank_chance",
                    "must be in [0, 0.9]",
                );
                let on_screen = match t.spawn {
                    SpawnArea::AroundCrosshair { offset_deg, .. } => {
                        offset_deg.max <= fov::valorant_on_screen_half_v_deg() as f32
                    }
                    _ => false,
                };
                check(
                    on_screen,
                    "targets.perception",
                    "detection needs spawn kind = \"around_crosshair\" with offset_deg inside \
                     the screen's height",
                );
                check(
                    matches!(t.shape, Shape::Sphere { .. })
                        && t.max_alive == 1
                        && t.lifetime_secs.is_some()
                        && click
                        && t.decoy_chance == 0.0
                        && def.scoring.rings.is_none(),
                    "targets.perception",
                    "detection needs a sphere, max_alive = 1, lifetime_secs, fire = \"click\", \
                     no decoy_chance and no scoring.rings",
                );
            }
            Perception::Radar { show_secs, dots } => {
                check(
                    show_secs > 0.0 && show_secs <= MAX_RADAR_SECS,
                    "targets.perception.show_secs",
                    "must be in (0, 10] seconds",
                );
                check(
                    dots >= 1 && dots < t.max_alive,
                    "targets.perception.dots",
                    "must be between 1 and max_alive - 1",
                );
                check(
                    t.lifetime_secs.is_some() && click && t.decoy_chance == 0.0,
                    "targets.perception",
                    "radar needs lifetime_secs, fire = \"click\" and no decoy_chance",
                );
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
            _ => None,
        }
    }

    /// The early-reveal angle of a sound cue, degrees, if it is on.
    pub(super) fn reveal_within_deg(&self) -> Option<f32> {
        match *self {
            Perception::SoundCue {
                reveal_within_deg, ..
            } => (reveal_within_deg > 0.0).then_some(reveal_within_deg),
            _ => None,
        }
    }

    /// The sound of a sound cue.
    pub fn cue_sound(&self) -> Option<CueSound> {
        match *self {
            Perception::SoundCue { sound, .. } => Some(sound),
            _ => None,
        }
    }

    /// Whether this is a peripheral detection drill.
    pub fn is_detection(&self) -> bool {
        matches!(self, Perception::Detection { .. })
    }

    /// Whether this is a radar recall drill.
    pub fn is_radar(&self) -> bool {
        matches!(self, Perception::Radar { .. })
    }

    /// Whether the scenario's decoys look like the other targets. Radar decoys can only be
    /// told apart by remembering the radar, so they must not be drawn differently.
    pub fn disguises_decoys(&self) -> bool {
        self.is_radar()
    }
}

/// Where a detection flash appears: in one of eight screen areas around the crosshair (up,
/// up-right, right and so on, 45° apart), at the center of the area's direction,
/// `offset_deg` away and `distance` from the eye (both drawn from the `around_crosshair`
/// spawn).
pub(super) fn flash_point(spawn: &SpawnArea, eye: Vec3, view: ViewAngles, rng: &mut Rng) -> Vec3 {
    let (offset_deg, distance) = match *spawn {
        SpawnArea::AroundCrosshair {
            offset_deg,
            distance,
            ..
        } => (offset_deg, distance),
        // `validate()` rejects other spawns for detection; a fixed ring keeps this total.
        _ => (Range::fixed(10.0), Range::fixed(10.0)),
    };
    let area = rng.next_u64() % 8;
    let eccentricity = rng.in_range(offset_deg);
    let dist = rng.in_range(distance);
    // Clockwise from straight up, like `analytics::miss_compass::Compass::ALL`.
    let bearing = (area as f32 * 45.0).to_radians();
    let (right, up) = (eccentricity * bearing.sin(), eccentricity * bearing.cos());
    let mut p = eye + view.rotated(right, up).direction() * dist;
    p.y = p.y.max(MIN_SPHERE_HEIGHT);
    p
}

/// What a radar shows while it is on screen: where the round's matching targets stand.
#[derive(Debug, Clone, PartialEq)]
pub struct RadarView {
    /// Feet positions of the targets the radar shows, meters.
    pub dots: Vec<Vec3>,
    /// Player feet position, meters.
    pub player: Vec3,
    /// Seconds before the radar disappears.
    pub remaining_secs: f32,
}

impl RadarView {
    /// Each dot as `(right, forward)` meters from the player, for a radar that turns with a
    /// view of yaw `view_yaw_deg` (forward is up on the radar).
    pub fn offsets(&self, view_yaw_deg: f32) -> Vec<(f32, f32)> {
        let (s, c) = view_yaw_deg.to_radians().sin_cos();
        let forward = Vec3::new(s, 0.0, -c);
        let right = Vec3::new(c, 0.0, s);
        self.dots
            .iter()
            .map(|d| {
                let to = (*d - self.player).horizontal();
                (to.dot(right), to.dot(forward))
            })
            .collect()
    }

    /// Radius the radar should cover to show every dot with some margin, meters.
    pub fn range_m(&self) -> f32 {
        let far = self
            .dots
            .iter()
            .map(|d| (*d - self.player).horizontal().length())
            .fold(0.0f32, f32::max);
        (far * 1.25).max(5.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Range, Vec3, ViewAngles};
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
            assert_eq!(s.radar(), None, "{}", def.id);
            assert_eq!(
                (s.stats().false_alarms, s.stats().blank_trials),
                (0, 0),
                "{}",
                def.id
            );
        }
    }

    /// A detection drill: 0.5 s flashes 10° from the crosshair after a 0.5 s foreperiod.
    fn detection_def(blank_chance: f32) -> ScenarioDef {
        let src = format!(
            r#"
            id = "detect"
            name = "Detect"
            category = "reaction"
            duration_secs = 60
            [player]
            elevation = 2
            [targets]
            shape = {{ kind = "sphere", radius = 0.06 }}
            spawn = {{ kind = "around_crosshair", offset_deg = [10, 10], distance = [6, 6] }}
            lifetime_secs = 0.5
            appear_delay_secs = [0.5, 0.5]
            [targets.perception]
            kind = "detection"
            blank_chance = {blank_chance}
            [scoring]
            miss_penalty = 50
            "#
        );
        let def = ScenarioDef::from_toml(&src).expect("valid toml");
        assert!(def.validate().is_empty(), "{:?}", def.validate());
        def
    }

    #[test]
    fn detection_flashes_land_in_all_eight_screen_areas_at_the_set_eccentricity() {
        use crate::analytics::miss_compass::Compass;
        use crate::analytics::trace::tangent_offset_deg;
        let mut s = Session::new(detection_def(0.0), 4);
        let mut areas = Vec::new();
        let mut seen = Vec::new();
        for _ in 0..1200 {
            frame(&mut s, AHEAD, false);
            for t in s.targets() {
                if seen.contains(&t.id) {
                    continue;
                }
                seen.push(t.id);
                let to = ViewAngles::from_direction(t.position - s.eye_position());
                let offset = tangent_offset_deg(AHEAD, to);
                let eccentricity = (offset.0.powi(2) + offset.1.powi(2)).sqrt();
                assert!((eccentricity - 10.0).abs() < 0.2, "{offset:?}");
                let area = Compass::of_offset(offset).expect("off-center");
                if !areas.contains(&area) {
                    areas.push(area);
                }
            }
        }
        assert!(seen.len() > 40, "{}", seen.len());
        assert_eq!(areas.len(), 8, "{areas:?}");
    }

    #[test]
    fn a_click_anywhere_while_a_flash_shows_detects_it() {
        let mut s = Session::new(detection_def(0.0), 4);
        while s.targets().count() == 0 {
            frame(&mut s, AHEAD, false);
        }
        // The crosshair stays in the middle, 10° away from the flash.
        let events = frame(&mut s, AHEAD, true);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SessionEvent::Killed { .. })),
            "{events:?}"
        );
        let st = s.stats();
        assert_eq!((st.shots, st.hits, st.kills, st.false_alarms), (1, 1, 1, 0));
        assert_eq!(st.score, 100.0);
    }

    #[test]
    fn a_click_with_no_flash_showing_is_a_false_alarm() {
        let mut s = Session::new(detection_def(0.0), 4);
        frame(&mut s, AHEAD, false);
        assert_eq!(s.targets().count(), 0, "inside the foreperiod");
        let events = frame(&mut s, AHEAD, true);
        assert!(events.contains(&SessionEvent::Shot {
            hit: None,
            moving: false
        }));
        let st = s.stats();
        assert_eq!((st.shots, st.hits, st.false_alarms), (1, 0, 1));
        assert_eq!(st.score, -50.0);
    }

    #[test]
    fn blank_trials_never_show_and_are_counted_when_they_end() {
        let mut s = Session::new(detection_def(0.5), 9);
        let mut spawned = 0;
        let mut shown = Vec::new();
        for _ in 0..1200 {
            spawned += frame(&mut s, AHEAD, false)
                .iter()
                .filter(|e| matches!(e, SessionEvent::Spawned { .. }))
                .count() as u32;
            for t in s.targets() {
                if !shown.contains(&t.id) {
                    shown.push(t.id);
                }
            }
        }
        let st = s.stats();
        assert!(st.blank_trials > 5, "{st:?}");
        assert!(shown.len() > 5);
        // Every trial that ended was either a missed flash or a blank one (one is still open,
        // plus the one spawned in `Session::new`).
        assert_eq!(st.expired + st.blank_trials, spawned);
        assert_eq!(
            st.expired as usize,
            shown.len() - 1 + usize::from(s.targets().count() == 0)
        );
        let replay = |seed| {
            let mut s = Session::new(detection_def(0.5), seed);
            (0..600)
                .flat_map(|i| frame(&mut s, AHEAD, i % 13 == 0))
                .collect::<Vec<_>>()
        };
        assert_eq!(replay(9), replay(9));
    }

    /// A radar drill: four agents per round, two shown on the radar for 1 s; the round
    /// appears 0.5 s after the radar is gone and lives 2 s.
    fn radar_def() -> ScenarioDef {
        let def = ScenarioDef::from_toml(
            r#"
            id = "radar"
            name = "Radar"
            category = "reaction"
            duration_secs = 60
            [targets]
            shape = { kind = "humanoid" }
            spawn = { kind = "angular", yaw_deg = [-60, 60], pitch_deg = [0, 0], distance = [8, 20] }
            max_alive = 4
            min_separation_deg = 15
            lifetime_secs = 2.0
            appear_delay_secs = [0.5, 0.5]
            [targets.perception]
            kind = "radar"
            show_secs = 1.0
            dots = 2
            [scoring]
            decoy_penalty = 150
            "#,
        )
        .expect("valid toml");
        assert!(def.validate().is_empty(), "{:?}", def.validate());
        def
    }

    fn wait(s: &mut Session, secs: f32) {
        let end = s.time() + secs;
        while s.time() < end - 1e-4 {
            frame(s, AHEAD, false);
        }
    }

    #[test]
    fn a_radar_round_shows_the_matches_then_spawns_them_with_look_alike_decoys() {
        let mut s = Session::new(radar_def(), 2);
        let radar = s.radar().expect("the round starts with the radar");
        assert_eq!(radar.dots.len(), 2);
        assert!((radar.remaining_secs - 1.0).abs() < 1e-5);
        assert_eq!(
            s.targets().count(),
            0,
            "nothing to shoot while the radar shows"
        );
        wait(&mut s, 1.0);
        assert_eq!(s.radar(), None, "the radar is gone after show_secs");
        assert_eq!(
            s.targets().count(),
            0,
            "the round appears appear_delay_secs later"
        );
        wait(&mut s, 0.55);
        let shown: Vec<_> = s.targets().cloned().collect();
        assert_eq!(shown.len(), 4);
        let matches: Vec<Vec3> = shown
            .iter()
            .filter(|t| !t.decoy)
            .map(|t| t.position)
            .collect();
        assert_eq!(
            matches, radar.dots,
            "the radar showed exactly the matching targets"
        );
        assert!(shown.iter().all(|t| t.visible_at == shown[0].visible_at));
    }

    #[test]
    fn radar_picks_score_like_go_no_go_and_the_next_round_waits_for_this_one() {
        let mut s = Session::new(radar_def(), 2);
        wait(&mut s, 1.55);
        let pick = |s: &Session, decoy: bool| {
            let t = s
                .targets()
                .find(|t| t.decoy == decoy)
                .expect("target")
                .clone();
            let head = t.position + Vec3::new(0.0, s.dims().head_center_height(), 0.0);
            ViewAngles::from_direction(head - s.eye_position())
        };
        let wrong = pick(&s, true);
        frame(&mut s, wrong, false);
        frame(&mut s, wrong, true);
        assert_eq!(s.stats().decoy_hits, 1, "an incorrect pick");
        wait(&mut s, 0.2);
        let right = pick(&s, false);
        frame(&mut s, right, false);
        frame(&mut s, right, true);
        assert_eq!(s.stats().kills, 1, "a correct pick");
        assert_eq!(
            s.radar(),
            None,
            "no new round while targets of this one are alive"
        );
        wait(&mut s, 2.0);
        let st = s.stats();
        assert_eq!((st.expired, st.decoy_waits), (1, 1), "{st:?}");
        assert!(
            s.radar().is_some(),
            "the next round starts once this one is over"
        );
    }

    #[test]
    fn radar_offsets_turn_with_the_view() {
        let view = RadarView {
            dots: vec![Vec3::new(3.0, 0.0, -4.0)],
            player: Vec3::ZERO,
            remaining_secs: 1.0,
        };
        let (right, forward) = view.offsets(0.0)[0];
        assert!((right - 3.0).abs() < 1e-5 && (forward - 4.0).abs() < 1e-5);
        // Facing +X (yaw 90°), the dot is ahead-left.
        let (right, forward) = view.offsets(90.0)[0];
        assert!((right + 4.0).abs() < 1e-5 && (forward - 3.0).abs() < 1e-5);
        assert!((view.range_m() - 6.25).abs() < 1e-5);
    }

    #[test]
    fn detection_and_radar_settings_are_validated() {
        let detect = detection_def(0.0);
        let fields = |d: &ScenarioDef| {
            d.validate()
                .into_iter()
                .map(|e| e.field)
                .collect::<Vec<_>>()
        };
        let mut bad = detect.clone();
        bad.targets.perception = Some(Perception::Detection { blank_chance: 0.95 });
        assert_eq!(fields(&bad), ["targets.perception.blank_chance"]);
        let mut bad = detect.clone();
        bad.targets.max_alive = 2;
        assert_eq!(fields(&bad), ["targets.perception"]);
        let mut bad = detect.clone();
        bad.targets.spawn = SpawnArea::AroundCrosshair {
            offset_deg: Range::new(10.0, 40.0),
            distance: Range::fixed(6.0),
            pitch_limit_deg: crate::scenario::def::default_pitch_limit(),
            on_screen: false,
        };
        assert_eq!(
            fields(&bad),
            ["targets.perception"],
            "40° leaves the screen vertically"
        );
        let mut bad = detect.clone();
        bad.targets.lifetime_secs = None;
        assert_eq!(fields(&bad), ["targets.perception"]);

        let radar = radar_def();
        let with = |show_secs, dots| {
            let mut d = radar.clone();
            d.targets.perception = Some(Perception::Radar { show_secs, dots });
            fields(&d)
        };
        assert_eq!(with(0.0, 2), ["targets.perception.show_secs"]);
        assert_eq!(with(1.0, 0), ["targets.perception.dots"]);
        assert_eq!(
            with(1.0, 4),
            ["targets.perception.dots"],
            "at least one decoy"
        );
        assert!(with(10.0, 3).is_empty());
        let mut decoys = radar.clone();
        decoys.targets.decoy_chance = 0.3;
        assert_eq!(fields(&decoys), ["targets.perception"]);
        let typo = toml::to_string(&radar)
            .unwrap()
            .replace("show_secs", "show_sec");
        assert!(ScenarioDef::from_toml(&typo).is_err());
    }
}
