//! Firing while moving: speed at each shot, stopped vs moving accuracy, counter-strafing.
//!
//! Only scenarios with player movement are analyzed. A shot is *moving* when the player
//! was above the weapon's accuracy speed limit (27.5% of run speed for rifles, see
//! `docs/valorant-mechanics.md`) when it was fired; the session decides this and the
//! trace copies it into [`Sample::accurate`]. The counter-strafe check looks at shots that
//! follow movement: the player was above the limit at some point in the
//! [`AFTER_MOVEMENT_WINDOW_SECS`] before the shot (or at the shot). A good counter-strafe
//! brings the player under the limit before firing.

use super::stats::median;
use super::trace::Sample;

/// A shot "follows movement" when the player was moving above the accuracy limit within
/// this many seconds before it. Provisional: it covers VALORANT's measured stop (0.16 s
/// counter-strafing, 0.17 s releasing) plus a reaction margin, so a shot fired right after
/// a stop counts while a shot fired long after standing still does not.
pub const AFTER_MOVEMENT_WINDOW_SECS: f32 = 0.5;

/// One shot of a movement scenario.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovingShot {
    /// Session time of the shot, seconds.
    pub time_secs: f32,
    /// Player speed when the shot was fired, m/s.
    pub speed: f32,
    /// The player was at or below the accuracy speed limit.
    pub accurate: bool,
    /// The shot hit a counted zone.
    pub hit: bool,
    /// The player was above the accuracy limit within [`AFTER_MOVEMENT_WINDOW_SECS`] before
    /// the shot, or at the shot.
    pub after_movement: bool,
    /// For an accurate shot that follows movement: seconds from the first accurate frame
    /// after the last moving frame to the shot (0 when fired on that frame).
    pub stop_to_shot_secs: Option<f32>,
}

/// The shots of one or more movement runs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MovingAnalysis {
    /// Speed at or below which shots are accurate, m/s (the run's weapon).
    pub accuracy_speed_limit: f32,
    /// Every shot, in order.
    pub shots: Vec<MovingShot>,
}

/// Aggregates of [`MovingAnalysis`].
#[derive(Debug, Clone, PartialEq)]
pub struct MovingSummary {
    /// Number of shots.
    pub shots: usize,
    /// Median player speed at the shot, m/s.
    pub median_shot_speed: Option<f32>,
    /// Share of shots fired at or below the accuracy limit.
    pub accurate_share: Option<f32>,
    /// Share of shots fired above the accuracy limit.
    pub moving_share: Option<f32>,
    /// Hit rate of the shots fired at or below the accuracy limit.
    pub stopped_hit_rate: Option<f32>,
    /// Hit rate of the shots fired above the accuracy limit.
    pub moving_hit_rate: Option<f32>,
    /// Shots that follow movement (see [`AFTER_MOVEMENT_WINDOW_SECS`]).
    pub after_movement_shots: usize,
    /// Share of the shots that follow movement fired already accurate (counter-strafed).
    pub counter_strafe_share: Option<f32>,
    /// Median seconds from becoming accurate to firing, over counter-strafed shots.
    pub median_stop_to_shot_secs: Option<f32>,
    /// Shots fired at or below the accuracy limit.
    pub accurate_shots: usize,
    /// Shots that follow movement and were fired already accurate.
    pub counter_strafed_shots: usize,
}

fn share(part: usize, whole: usize) -> Option<f32> {
    (whole > 0).then(|| part as f32 / whole as f32)
}

impl MovingAnalysis {
    /// Add another run's shots (the accuracy limit of the first run is kept).
    pub fn merge(&mut self, other: &MovingAnalysis) {
        if self.shots.is_empty() && self.accuracy_speed_limit == 0.0 {
            self.accuracy_speed_limit = other.accuracy_speed_limit;
        }
        self.shots.extend_from_slice(&other.shots);
    }

    /// Medians and shares over every shot.
    pub fn summary(&self) -> MovingSummary {
        let shots = &self.shots;
        let speeds: Vec<f32> = shots.iter().map(|s| s.speed).collect();
        let accurate = shots.iter().filter(|s| s.accurate).count();
        let moving = shots.len() - accurate;
        let hits = |want_accurate: bool| {
            shots
                .iter()
                .filter(|s| s.accurate == want_accurate && s.hit)
                .count()
        };
        let after: Vec<&MovingShot> = shots.iter().filter(|s| s.after_movement).collect();
        let countered = after.iter().filter(|s| s.accurate).count();
        let delays: Vec<f32> = after.iter().filter_map(|s| s.stop_to_shot_secs).collect();
        MovingSummary {
            shots: shots.len(),
            median_shot_speed: median(&speeds),
            accurate_share: share(accurate, shots.len()),
            moving_share: share(moving, shots.len()),
            stopped_hit_rate: share(hits(true), accurate),
            moving_hit_rate: share(hits(false), moving),
            after_movement_shots: after.len(),
            counter_strafe_share: share(countered, after.len()),
            median_stop_to_shot_secs: median(&delays),
            accurate_shots: accurate,
            counter_strafed_shots: countered,
        }
    }
}

/// Measure every shot of a movement run. `accuracy_speed_limit` is the weapon's limit in
/// m/s, kept for display; the accurate flag itself comes from the samples.
pub fn analyze(samples: &[Sample], accuracy_speed_limit: f32) -> MovingAnalysis {
    let mut shots = Vec::new();
    // Index of the latest sample above the accuracy limit, up to the current one.
    let mut last_moving: Option<usize> = None;
    for (i, s) in samples.iter().enumerate() {
        if !s.accurate {
            last_moving = Some(i);
        }
        if !s.shot {
            continue;
        }
        let after_movement = last_moving
            .is_some_and(|j| s.time_secs - samples[j].time_secs <= AFTER_MOVEMENT_WINDOW_SECS);
        let stop_to_shot_secs = match last_moving {
            Some(j) if after_movement && s.accurate => samples
                .get(j + 1)
                .map(|first_accurate| s.time_secs - first_accurate.time_secs),
            _ => None,
        };
        shots.push(MovingShot {
            time_secs: s.time_secs,
            speed: s.player_speed,
            accurate: s.accurate,
            hit: s.hit,
            after_movement,
            stop_to_shot_secs,
        });
    }
    MovingAnalysis {
        accuracy_speed_limit,
        shots,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::math::ViewAngles;
    use crate::movement::MoveInput;
    use crate::scenario::{FrameInput, Recording, builtin};

    fn sample(time_secs: f32, speed: f32, accurate: bool, shot: bool, hit: bool) -> Sample {
        Sample {
            time_secs,
            dt_secs: 0.1,
            view: ViewAngles::default(),
            view_velocity_deg_per_sec: (0.0, 0.0),
            angular_speed_deg_per_sec: 0.0,
            targets: Vec::new(),
            aimed: None,
            fire_held: shot,
            shot,
            hit,
            on_target: false,
            player_speed: speed,
            accurate,
            ..Default::default()
        }
    }

    /// Two shots while running, one right after a stop, then two shots long after it.
    fn timeline() -> Vec<Sample> {
        let mut v = vec![
            sample(0.1, 5.4, false, false, false),
            sample(0.2, 5.4, false, true, false),
            sample(0.3, 5.4, false, true, true),
            sample(0.4, 1.0, true, false, false),
            sample(0.5, 0.0, true, true, true),
        ];
        for k in 6..15 {
            v.push(sample(k as f32 / 10.0, 0.0, true, false, false));
        }
        v.push(sample(1.5, 0.5, true, true, false));
        v.push(sample(1.6, 0.0, true, true, true));
        v
    }

    #[test]
    fn shots_are_split_into_stopped_and_moving_with_a_counter_strafe_check() {
        let a = analyze(&timeline(), 1.485);
        assert_eq!(a.accuracy_speed_limit, 1.485);
        let flags: Vec<(bool, bool)> = a
            .shots
            .iter()
            .map(|s| (s.accurate, s.after_movement))
            .collect();
        assert_eq!(
            flags,
            vec![
                (false, true),
                (false, true),
                (true, true),
                (true, false),
                (true, false)
            ]
        );
        let delays: Vec<Option<f32>> = a.shots.iter().map(|s| s.stop_to_shot_secs).collect();
        assert_eq!(delays[..2], [None, None]);
        assert!((delays[2].unwrap() - 0.1).abs() < 1e-5, "{delays:?}");
        assert_eq!(delays[3..], [None, None]);

        let s = a.summary();
        assert_eq!(s.shots, 5);
        assert_eq!(s.median_shot_speed, Some(0.5));
        assert_eq!(s.accurate_share, Some(0.6));
        assert_eq!(s.moving_share, Some(0.4));
        assert_eq!(s.stopped_hit_rate, Some(2.0 / 3.0));
        assert_eq!(s.moving_hit_rate, Some(0.5));
        assert_eq!(s.after_movement_shots, 3);
        assert_eq!((s.accurate_shots, s.counter_strafed_shots), (3, 1));
        assert_eq!(s.counter_strafe_share, Some(1.0 / 3.0));
        assert!((s.median_stop_to_shot_secs.unwrap() - 0.1).abs() < 1e-5);
    }

    #[test]
    fn no_shots_give_counts_of_zero_and_no_rates() {
        let s = analyze(&[sample(0.1, 5.4, false, false, false)], 1.485).summary();
        assert_eq!(
            s,
            MovingSummary {
                shots: 0,
                median_shot_speed: None,
                accurate_share: None,
                moving_share: None,
                stopped_hit_rate: None,
                moving_hit_rate: None,
                after_movement_shots: 0,
                accurate_shots: 0,
                counter_strafed_shots: 0,
                counter_strafe_share: None,
                median_stop_to_shot_secs: None,
            }
        );
    }

    #[test]
    fn merging_runs_pools_their_shots() {
        let one = analyze(&timeline(), 1.485);
        let mut all = MovingAnalysis::default();
        all.merge(&one);
        all.merge(&one);
        assert_eq!(all.accuracy_speed_limit, 1.485);
        assert_eq!(all.shots.len(), 10);
        assert_eq!(all.summary().moving_share, Some(0.4));
    }

    fn frame(right: f32, fire: bool) -> FrameInput {
        FrameInput {
            dt: 1.0 / 240.0,
            view: ViewAngles::default(),
            fire_pressed: fire,
            fire_held: fire,
            movement: MoveInput {
                right,
                forward: 0.0,
                walk: false,
                crouch: false,
            },
        }
    }

    /// A counter-strafe run: strafe right and fire while running, counter-strafe and fire
    /// once accurate, then fire again after standing still for a second.
    pub(crate) fn strafe_recording() -> Recording {
        let def = builtin::all()
            .into_iter()
            .find(|d| d.id == "counter-strafe")
            .expect("builtin scenario exists");
        let mut rec = Recording::new(def, 7);
        let mut push = |n: usize, right: f32, fire_last: bool| {
            for k in 0..n {
                rec.inputs.push(frame(right, fire_last && k + 1 == n));
            }
        };
        push(120, 1.0, true); // 0.5 s running, shot while running
        push(24, -1.0, false); // 0.1 s counter-strafe
        push(12, 0.0, true); // release, shot once accurate
        push(240, 0.0, true); // 1 s still, shot standing
        rec
    }

    #[test]
    fn a_recorded_counter_strafe_run_is_measured_from_the_session() {
        let rec = strafe_recording();
        let a = crate::analytics::analyze(&rec);
        let m = a.moving.expect("movement scenarios are analyzed");
        let session = rec.replay(|_, _| {});
        assert_eq!(m.shots.len() as u32, session.stats().shots);
        assert_eq!(
            m.shots.iter().filter(|s| !s.accurate).count() as u32,
            session.stats().moving_shots
        );
        assert_eq!(
            m.accuracy_speed_limit,
            session.movement_profile().accuracy_speed_limit()
        );
        let flags: Vec<(bool, bool)> = m
            .shots
            .iter()
            .map(|s| (s.accurate, s.after_movement))
            .collect();
        assert_eq!(flags, vec![(false, true), (true, true), (true, false)]);
        assert!(m.shots[0].speed > m.accuracy_speed_limit);
        assert!(
            m.shots[1]
                .stop_to_shot_secs
                .is_some_and(|d| (0.0..0.1).contains(&d))
        );
        assert_eq!(m.shots[2].speed, 0.0);
    }

    #[test]
    fn scenarios_without_movement_have_no_moving_analysis() {
        let rec = crate::analytics::test_support::record(
            "flick-wide",
            crate::sim::AimerProfile::PRESETS[1],
            1,
        );
        assert_eq!(crate::analytics::analyze(&rec).moving, None);
    }
}
