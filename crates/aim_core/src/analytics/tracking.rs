//! Tracking analysis for hold scenarios: time on target, error from the target center,
//! lag versus lead, how quickly the player follows a target that changes direction, how
//! well the crosshair speed matches the target's, and how much on-target time rides the
//! target's edge.
//! See "Analytics metrics" in `docs/training-science.md`.

use super::stats::{mean, median};
use super::trace::{Sample, tangent_offset_deg};
use crate::math::angle_between_deg;

/// Target angular speed below which its direction is not meaningful (lag/lead and
/// reversals are not measured).
pub const MIN_TARGET_SPEED_DEG_PER_SEC: f32 = 5.0;
/// Reversals the player has not followed within this window are dropped (they are
/// lapses, not reactions).
pub const MAX_REVERSAL_RESPONSE_SECS: f32 = 1.0;
/// Relative speed-match tolerance: the crosshair matches a moving target when its angular
/// speed is within this fraction of the target's (provisional estimate).
pub const SPEED_MATCH_TOLERANCE: f32 = 0.25;
/// Absolute floor of the speed-match tolerance, deg/s, so slow targets do not demand an
/// unrealistically exact speed (provisional estimate).
pub const SPEED_MATCH_FLOOR_DEG_PER_SEC: f32 = 2.0;
/// On-target time farther than this fraction of the aim zone's radius from its center
/// counts as edge tracking (provisional estimate).
pub const EDGE_FRACTION: f32 = 0.7;

/// Largest crosshair speed difference, deg/s, that still matches a target moving at
/// `target_speed_deg_per_sec`.
pub fn speed_tolerance_deg_per_sec(target_speed_deg_per_sec: f32) -> f32 {
    (SPEED_MATCH_TOLERANCE * target_speed_deg_per_sec).max(SPEED_MATCH_FLOOR_DEG_PER_SEC)
}

/// Raw accumulators; [`TrackingAnalysis::merge`] adds runs together before summarizing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackingAnalysis {
    /// Seconds firing with a target visible.
    pub hold_secs: f32,
    /// Seconds of that on a counted zone (the session's own hit test).
    pub on_target_secs: f32,
    /// Per-frame angle from the crosshair to the target's aim point, degrees.
    pub errors_deg: Vec<f32>,
    /// Seconds behind / ahead of a moving target, along its direction of motion.
    pub lag_secs: f32,
    /// Seconds ahead of a moving target, along its direction of motion.
    pub lead_secs: f32,
    /// Time-weighted sum of the signed along-motion error (negative = behind), deg·s.
    pub along_error_deg_secs: f32,
    /// Per target reversal: seconds until the crosshair moved the new way.
    pub reversal_responses_secs: Vec<f32>,
    /// Seconds firing at a moving target with the crosshair speed within
    /// [`speed_tolerance_deg_per_sec`] of the target's speed.
    pub speed_matched_secs: f32,
    /// Seconds firing at a moving target with the crosshair slower than that tolerance.
    pub speed_under_secs: f32,
    /// Seconds firing at a moving target with the crosshair faster than that tolerance.
    pub speed_over_secs: f32,
    /// Seconds on a counted zone with the crosshair inside the aim zone's radius.
    pub in_zone_secs: f32,
    /// Seconds of that farther than [`EDGE_FRACTION`] of the radius from the aim point.
    pub edge_secs: f32,
}

/// Aggregates of a tracking analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackingSummary {
    /// Seconds spent firing with a target visible.
    pub hold_secs: f32,
    /// Share of firing time on a counted zone (0-1).
    pub on_target_ratio: f32,
    /// Mean angle from the crosshair to the target's aim point, degrees.
    pub mean_error_deg: Option<f32>,
    /// Median angle from the crosshair to the target's aim point, degrees.
    pub median_error_deg: Option<f32>,
    /// Share of moving-target time spent behind the target (0.5 = balanced).
    pub lag_ratio: Option<f32>,
    /// Mean signed error along the target's motion, degrees (negative = lagging).
    pub mean_along_error_deg: Option<f32>,
    /// Number of target direction reversals that were followed.
    pub reversals: usize,
    /// Median seconds to follow a reversal.
    pub median_reversal_response_secs: Option<f32>,
    /// Share of moving-target firing time with the crosshair speed matching the target's.
    pub speed_match_ratio: Option<f32>,
    /// Share of that time with the crosshair too slow.
    pub speed_under_ratio: Option<f32>,
    /// Share of that time with the crosshair too fast.
    pub speed_over_ratio: Option<f32>,
    /// Share of in-zone time spent near the edge rather than the center (0-1).
    pub edge_ratio: Option<f32>,
}

impl TrackingAnalysis {
    /// Add another run's accumulators to this one.
    pub fn merge(&mut self, other: &TrackingAnalysis) {
        self.hold_secs += other.hold_secs;
        self.on_target_secs += other.on_target_secs;
        self.errors_deg.extend_from_slice(&other.errors_deg);
        self.lag_secs += other.lag_secs;
        self.lead_secs += other.lead_secs;
        self.along_error_deg_secs += other.along_error_deg_secs;
        self.reversal_responses_secs
            .extend_from_slice(&other.reversal_responses_secs);
        self.speed_matched_secs += other.speed_matched_secs;
        self.speed_under_secs += other.speed_under_secs;
        self.speed_over_secs += other.speed_over_secs;
        self.in_zone_secs += other.in_zone_secs;
        self.edge_secs += other.edge_secs;
    }

    /// Reduce the accumulators to ratios, means and medians.
    pub fn summary(&self) -> TrackingSummary {
        let moving_secs = self.lag_secs + self.lead_secs;
        let speed_secs = self.speed_matched_secs + self.speed_under_secs + self.speed_over_secs;
        let speed_share = |secs: f32| (speed_secs > 0.0).then(|| secs / speed_secs);
        TrackingSummary {
            hold_secs: self.hold_secs,
            on_target_ratio: if self.hold_secs > 0.0 {
                self.on_target_secs / self.hold_secs
            } else {
                0.0
            },
            mean_error_deg: mean(&self.errors_deg),
            median_error_deg: median(&self.errors_deg),
            lag_ratio: (moving_secs > 0.0).then(|| self.lag_secs / moving_secs),
            mean_along_error_deg: (moving_secs > 0.0)
                .then(|| self.along_error_deg_secs / moving_secs),
            reversals: self.reversal_responses_secs.len(),
            median_reversal_response_secs: median(&self.reversal_responses_secs),
            speed_match_ratio: speed_share(self.speed_matched_secs),
            speed_under_ratio: speed_share(self.speed_under_secs),
            speed_over_ratio: speed_share(self.speed_over_secs),
            edge_ratio: (self.in_zone_secs > 0.0).then(|| self.edge_secs / self.in_zone_secs),
        }
    }
}

fn unit(v: (f32, f32)) -> Option<(f32, f32)> {
    let len = (v.0 * v.0 + v.1 * v.1).sqrt();
    (len > 1e-6).then(|| (v.0 / len, v.1 / len))
}

fn dot(a: (f32, f32), b: (f32, f32)) -> f32 {
    a.0 * b.0 + a.1 * b.1
}

struct Reversal {
    target_id: u64,
    at_secs: f32,
    new_dir: (f32, f32),
}

/// Analyze the hold-mode `samples` of one run.
pub fn analyze(samples: &[Sample]) -> TrackingAnalysis {
    let mut out = TrackingAnalysis::default();
    // Last significant target direction per tracked target, and a reversal awaiting a response.
    let mut last_dir: Option<(u64, (f32, f32))> = None;
    let mut pending: Option<Reversal> = None;
    for (i, s) in samples.iter().enumerate() {
        let Some(target) = s.aimed_target() else {
            last_dir = None;
            pending = None;
            continue;
        };
        if s.fire_held {
            out.hold_secs += s.dt_secs;
            if s.on_target {
                out.on_target_secs += s.dt_secs;
            }
            let error = angle_between_deg(s.view.direction(), target.aim_point.direction());
            out.errors_deg.push(error);
            // Hits on another zone (a humanoid's body) are outside the aim zone and do not
            // say where the crosshair sat on the target.
            if s.on_target && error <= target.radius_deg {
                out.in_zone_secs += s.dt_secs;
                if error > EDGE_FRACTION * target.radius_deg {
                    out.edge_secs += s.dt_secs;
                }
            }
        }
        let previous = i.checked_sub(1).and_then(|p| samples[p].target(target.id));
        let Some(previous) = previous.filter(|_| s.dt_secs > 0.0) else {
            continue;
        };
        let step = tangent_offset_deg(previous.aim_point, target.aim_point);
        let velocity = (step.0 / s.dt_secs, step.1 / s.dt_secs);
        let speed = dot(velocity, velocity).sqrt();
        if speed < MIN_TARGET_SPEED_DEG_PER_SEC {
            continue;
        }
        let dir = unit(velocity).expect("speed is above a positive threshold");
        if s.fire_held {
            let along = dot(tangent_offset_deg(target.aim_point, s.view), dir);
            if along < 0.0 {
                out.lag_secs += s.dt_secs;
            } else {
                out.lead_secs += s.dt_secs;
            }
            out.along_error_deg_secs += along * s.dt_secs;

            let speed_diff = s.angular_speed_deg_per_sec - speed;
            let tolerance = speed_tolerance_deg_per_sec(speed);
            if speed_diff < -tolerance {
                out.speed_under_secs += s.dt_secs;
            } else if speed_diff > tolerance {
                out.speed_over_secs += s.dt_secs;
            } else {
                out.speed_matched_secs += s.dt_secs;
            }
        }

        if let Some(r) = &pending {
            let followed = dot(s.view_velocity_deg_per_sec, r.new_dir) > 0.0;
            let elapsed = s.time_secs - r.at_secs;
            if r.target_id != target.id || elapsed > MAX_REVERSAL_RESPONSE_SECS {
                pending = None;
            } else if followed {
                out.reversal_responses_secs.push(elapsed);
                pending = None;
            }
        }
        if let Some((id, d)) = last_dir
            && id == target.id
            && dot(d, dir) < 0.0
        {
            pending = Some(Reversal {
                target_id: id,
                at_secs: s.time_secs,
                new_dir: dir,
            });
        }
        last_dir = Some((target.id, dir));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::test_support::record;
    use crate::analytics::trace::{TargetSample, trace};
    use crate::math::ViewAngles;
    use crate::sim::AimerProfile;

    fn summary(id: &str, p: usize) -> TrackingSummary {
        analyze(&trace(&record(id, AimerProfile::PRESETS[p], 13))).summary()
    }

    #[test]
    fn higher_tracking_gain_stays_on_target_longer_and_reacts_faster() {
        for id in ["track-precise", "track-reactive-strafe"] {
            let low = summary(id, 0);
            let high = summary(id, 3);
            assert!(
                high.on_target_ratio > low.on_target_ratio,
                "{id}: {low:?} vs {high:?}"
            );
            assert!(high.mean_error_deg.unwrap() < low.mean_error_deg.unwrap());
            assert!(high.reversals > 0 && low.reversals > 0, "{id}");
            assert!(
                high.median_reversal_response_secs.unwrap()
                    < low.median_reversal_response_secs.unwrap(),
                "{id}: {low:?} vs {high:?}"
            );
        }
    }

    #[test]
    fn a_first_order_lag_follower_is_mostly_behind() {
        let s = summary("track-precise", 1);
        assert!(s.lag_ratio.unwrap() > 0.6, "{s:?}");
        assert!(s.mean_along_error_deg.unwrap() < 0.0);
    }

    #[test]
    fn merging_adds_time_and_samples() {
        let a = analyze(&trace(&record(
            "track-precise",
            AimerProfile::PRESETS[1],
            1,
        )));
        let mut m = a.clone();
        m.merge(&a);
        assert_eq!(m.errors_deg.len(), 2 * a.errors_deg.len());
        assert!((m.summary().on_target_ratio - a.summary().on_target_ratio).abs() < 1e-5);
        assert!(a.speed_matched_secs > 0.0 && a.in_zone_secs > 0.0, "{a:?}");
        assert_eq!(m.speed_matched_secs, 2.0 * a.speed_matched_secs);
        assert_eq!(m.speed_under_secs, 2.0 * a.speed_under_secs);
        assert_eq!(m.speed_over_secs, 2.0 * a.speed_over_secs);
        assert_eq!(m.in_zone_secs, 2.0 * a.in_zone_secs);
        assert_eq!(m.edge_secs, 2.0 * a.edge_secs);
        let (ms, s) = (m.summary(), a.summary());
        assert!((ms.speed_match_ratio.unwrap() - s.speed_match_ratio.unwrap()).abs() < 1e-5);
        assert!((ms.edge_ratio.unwrap() - s.edge_ratio.unwrap()).abs() < 1e-5);
    }

    #[test]
    fn existing_summary_values_are_unchanged() {
        // Pinned from the analysis before speed match and edge tracking were added.
        let s = summary("track-precise", 1);
        let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
        assert!(close(s.hold_secs, 60.000546), "{s:?}");
        assert!(close(s.on_target_ratio, 0.48003784));
        assert!(close(s.mean_error_deg.unwrap(), 1.5948012));
        assert!(close(s.median_error_deg.unwrap(), 1.6806281));
        assert!(close(s.lag_ratio.unwrap(), 0.9625336));
        assert!(close(s.mean_along_error_deg.unwrap(), -1.5188543));
        assert_eq!(s.reversals, 38);
        assert!(close(s.median_reversal_response_secs.unwrap(), 0.07914734));
    }

    const DT: f32 = 0.1;

    /// A real frame with a visible target. Synthetic frames copy it with struct update
    /// syntax so they keep compiling when `Sample` or `TargetSample` gain fields.
    fn base() -> (Sample, TargetSample) {
        let samples = trace(&record("track-precise", AimerProfile::PRESETS[1], 1));
        let s = samples
            .into_iter()
            .find(|s| s.aimed_target().is_some())
            .expect("a tracking run has a visible target");
        let t = *s.aimed_target().unwrap();
        (s, t)
    }

    /// Frame `i` firing at a target at `target`, the crosshair at `view` moving right at
    /// `player_speed` deg/s.
    // The struct update is deliberate: it keeps the helper compiling when `Sample` gains
    // fields, even though today every field is set.
    #[allow(clippy::needless_update)]
    fn frame(
        (s, t): &(Sample, TargetSample),
        i: usize,
        target: ViewAngles,
        view: ViewAngles,
        player_speed: f32,
        on_target: bool,
    ) -> Sample {
        Sample {
            time_secs: (i + 1) as f32 * DT,
            dt_secs: DT,
            view,
            view_velocity_deg_per_sec: (player_speed, 0.0),
            angular_speed_deg_per_sec: player_speed,
            targets: vec![TargetSample {
                aim_point: target,
                ..*t
            }],
            aimed: Some(0),
            fire_held: true,
            shot: false,
            hit: false,
            on_target,
            ..s.clone()
        }
    }

    /// A target moving right at `target_speed` deg/s, followed 0.5° behind by a crosshair
    /// whose speed in frame `i` is `player_speeds[i]` (frame 0 has no target velocity).
    fn horizontal_run(target_speed: f32, player_speeds: &[f32]) -> TrackingAnalysis {
        let b = base();
        let samples: Vec<Sample> = player_speeds
            .iter()
            .enumerate()
            .map(|(i, &v)| {
                let yaw = target_speed * DT * i as f32;
                let target = ViewAngles::new(yaw, 0.0);
                frame(&b, i, target, ViewAngles::new(yaw - 0.5, 0.0), v, true)
            })
            .collect();
        analyze(&samples)
    }

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn speed_match_splits_moving_time_into_matched_too_slow_and_too_fast() {
        // Target 20°/s, tolerance max(25% · 20, 2) = 5°/s: 20, 24, 16 match, 10 is slow, 30 fast.
        let a = horizontal_run(20.0, &[20.0, 20.0, 10.0, 30.0, 24.0, 16.0]);
        assert!(approx(a.speed_matched_secs, 0.3), "{a:?}");
        assert!(approx(a.speed_under_secs, 0.1), "{a:?}");
        assert!(approx(a.speed_over_secs, 0.1), "{a:?}");
        let s = a.summary();
        assert!(approx(s.speed_match_ratio.unwrap(), 0.6), "{s:?}");
        assert!(approx(s.speed_under_ratio.unwrap(), 0.2), "{s:?}");
        assert!(approx(s.speed_over_ratio.unwrap(), 0.2), "{s:?}");
    }

    #[test]
    fn slow_targets_use_the_absolute_speed_tolerance() {
        // Target 6°/s: 25% is 1.5°/s, below the 2°/s floor, so 7.5 matches and 8.5 is fast.
        let a = horizontal_run(6.0, &[6.0, 7.5, 8.5]);
        assert!(approx(a.speed_matched_secs, 0.1), "{a:?}");
        assert!(approx(a.speed_over_secs, 0.1), "{a:?}");
        assert_eq!(a.speed_under_secs, 0.0);
        assert!(approx(a.summary().speed_match_ratio.unwrap(), 0.5));
    }

    #[test]
    fn edge_share_counts_on_target_time_past_the_edge_fraction_of_the_radius() {
        let b = base();
        let r = b.1.radius_deg;
        let target = ViewAngles::new(0.0, 0.0);
        // (offset in radii, on_target): centered, two edge frames, a hit outside the aim
        // zone (another zone) and an off-target frame; the last two are not counted.
        let frames = [
            (0.2, true),
            (0.8, true),
            (0.9, true),
            (1.5, true),
            (0.5, false),
        ];
        let samples: Vec<Sample> = frames
            .iter()
            .enumerate()
            .map(|(i, &(k, on))| frame(&b, i, target, ViewAngles::new(0.0, k * r), 0.0, on))
            .collect();
        let a = analyze(&samples);
        assert!(approx(a.in_zone_secs, 0.3), "{a:?}");
        assert!(approx(a.edge_secs, 0.2), "{a:?}");
        let s = a.summary();
        assert!(approx(s.edge_ratio.unwrap(), 2.0 / 3.0), "{s:?}");
        // A static target gives no speed match.
        assert_eq!(s.speed_match_ratio, None);
        assert_eq!(s.speed_under_ratio, None);
        assert_eq!(s.speed_over_ratio, None);
    }

    #[test]
    fn no_firing_gives_no_speed_match_or_edge_share() {
        let s = TrackingAnalysis::default().summary();
        assert_eq!(s.speed_match_ratio, None);
        assert_eq!(s.edge_ratio, None);
    }
}
