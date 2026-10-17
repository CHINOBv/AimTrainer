//! Tracking analysis for hold scenarios: time on target, error from the target center,
//! lag versus lead, and how quickly the player follows a target that changes direction.
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
    pub lead_secs: f32,
    /// Time-weighted sum of the signed along-motion error (negative = behind), deg·s.
    pub along_error_deg_secs: f32,
    /// Per target reversal: seconds until the crosshair moved the new way.
    pub reversal_responses_secs: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrackingSummary {
    pub hold_secs: f32,
    pub on_target_ratio: f32,
    pub mean_error_deg: Option<f32>,
    pub median_error_deg: Option<f32>,
    /// Share of moving-target time spent behind the target (0.5 = balanced).
    pub lag_ratio: Option<f32>,
    /// Mean signed error along the target's motion, degrees (negative = lagging).
    pub mean_along_error_deg: Option<f32>,
    pub reversals: usize,
    pub median_reversal_response_secs: Option<f32>,
}

impl TrackingAnalysis {
    pub fn merge(&mut self, other: &TrackingAnalysis) {
        self.hold_secs += other.hold_secs;
        self.on_target_secs += other.on_target_secs;
        self.errors_deg.extend_from_slice(&other.errors_deg);
        self.lag_secs += other.lag_secs;
        self.lead_secs += other.lead_secs;
        self.along_error_deg_secs += other.along_error_deg_secs;
        self.reversal_responses_secs
            .extend_from_slice(&other.reversal_responses_secs);
    }

    pub fn summary(&self) -> TrackingSummary {
        let moving_secs = self.lag_secs + self.lead_secs;
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
            out.errors_deg.push(angle_between_deg(
                s.view.direction(),
                target.aim_point.direction(),
            ));
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
    use crate::analytics::trace::trace;
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
    }
}
