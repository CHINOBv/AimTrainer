//! Passive sensitivity suggestion from the over/undershoot bias of recent flicks.
//! See "Passive sensitivity suggestion" in `docs/training-science.md`.
//!
//! The primary sub-movement of a flick is the part planned before any visual correction, so
//! where it ends shows the gain between hand and crosshair: ending past the target means the
//! hand movement turned the view too far (sensitivity too high for the player's habits),
//! ending short means too little. The bias is the mean of `error / distance`, signed along
//! the movement; a multiplier of `1 / (1 + bias)` would have made the average primary
//! movement land on the target. Nothing here is applied automatically.

use super::direction::MIN_AXIS_AMPLITUDE_DEG;
use super::flick::{Flick, FlickKind, dot};
use super::stats::mean;

/// Fewer measured flicks than this give no suggestion (per axis: no axis suggestion).
pub const MIN_SENS_FLICKS: usize = 40;
/// The suggestion never lowers sensitivity below this multiplier in one step.
pub const MAX_DECREASE_MULTIPLIER: f32 = 0.85;
/// The suggestion never raises sensitivity above this multiplier in one step.
pub const MAX_INCREASE_MULTIPLIER: f32 = 1.15;
/// A multiplier within this share of 1 (0.03 = ±3%) is reported as "no change".
pub const NO_CHANGE_BAND: f32 = 0.03;
/// A flick whose primary movement ended more than this share of the distance past or short
/// of the target (0.5 = halfway) is dropped: that "primary" was a blip or an aborted start,
/// not one planned movement, and would swing the mean.
pub const MAX_RELATIVE_ERROR: f32 = 0.5;

/// The evidence and multiplier for one screen axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisSuggestion {
    /// Flicks that moved at least [`MIN_AXIS_AMPLITUDE_DEG`] on this axis.
    pub samples: usize,
    /// Mean signed relative error on this axis: positive past the target, negative short.
    pub mean_bias: f32,
    /// Suggested sensitivity multiplier for this axis, clamped.
    pub multiplier: f32,
}

/// A suggested sensitivity change with the evidence behind it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SensSuggestion {
    /// Flicks measured along their movement direction.
    pub samples: usize,
    /// Mean signed relative error along the movement: positive overshoot, negative undershoot.
    pub mean_bias: f32,
    /// Suggested overall sensitivity multiplier, clamped. The only one that can be applied:
    /// mouse look uses one sensitivity for both axes.
    pub multiplier: f32,
    /// Horizontal evidence, when enough flicks moved horizontally.
    pub horizontal: Option<AxisSuggestion>,
    /// Vertical evidence (information only), when enough flicks moved vertically.
    pub vertical: Option<AxisSuggestion>,
}

impl SensSuggestion {
    /// Whether the overall multiplier is outside the ±[`NO_CHANGE_BAND`] band.
    pub fn suggests_change(&self) -> bool {
        (self.multiplier - 1.0).abs() > NO_CHANGE_BAND
    }
}

fn multiplier(bias: f32) -> f32 {
    let gain = 1.0 + bias;
    if gain <= 0.0 {
        return MAX_INCREASE_MULTIPLIER;
    }
    (1.0 / gain).clamp(MAX_DECREASE_MULTIPLIER, MAX_INCREASE_MULTIPLIER)
}

/// Relative primary errors of one fresh flick: along its movement, then horizontal and
/// vertical. Each is `None` when the flick (or that axis) moved less than
/// [`MIN_AXIS_AMPLITUDE_DEG`], or the error is beyond ±[`MAX_RELATIVE_ERROR`]. All are
/// `None` when the flick had no primary movement, was a retry, or its along error is out of
/// bounds (its "primary" was not one planned movement to the target).
fn relative_errors(f: &Flick) -> [Option<f32>; 3] {
    let Some(offset) = f.primary_offset_deg.filter(|_| f.kind == FlickKind::Fresh) else {
        return [None; 3];
    };
    let d = f.direction_deg;
    let bounded = |e: f32| (e.abs() <= MAX_RELATIVE_ERROR).then_some(e);
    let axis = |moved: f32, ended: f32| {
        (moved.abs() >= MIN_AXIS_AMPLITUDE_DEG)
            .then(|| bounded(ended / moved))
            .flatten()
    };
    let length_sq = dot(d, d);
    let Some(along) = (length_sq.sqrt() >= MIN_AXIS_AMPLITUDE_DEG)
        .then(|| bounded(dot(offset, d) / length_sq))
        .flatten()
    else {
        return [None; 3];
    };
    [Some(along), axis(d.0, offset.0), axis(d.1, offset.1)]
}

/// Suggest a sensitivity multiplier from `flicks` (typically the fresh flicks of the
/// player's recent click runs). `None` below [`MIN_SENS_FLICKS`] measured flicks.
pub fn suggest<'a>(flicks: impl IntoIterator<Item = &'a Flick>) -> Option<SensSuggestion> {
    let mut errors: [Vec<f32>; 3] = Default::default();
    for f in flicks {
        for (list, e) in errors.iter_mut().zip(relative_errors(f)) {
            list.extend(e);
        }
    }
    let [along, horizontal, vertical] = errors;
    let axis = |e: &[f32]| {
        let bias = mean(e).filter(|_| e.len() >= MIN_SENS_FLICKS)?;
        Some(AxisSuggestion {
            samples: e.len(),
            mean_bias: bias,
            multiplier: multiplier(bias),
        })
    };
    let overall = axis(&along)?;
    Some(SensSuggestion {
        samples: overall.samples,
        mean_bias: overall.mean_bias,
        multiplier: overall.multiplier,
        horizontal: axis(&horizontal),
        vertical: axis(&vertical),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::flick::{Flick, FlickKind};

    /// A fresh flick of `direction_deg` whose primary movement ended at `offset_deg`.
    fn flick(direction_deg: (f32, f32), offset_deg: (f32, f32)) -> Flick {
        Flick {
            kind: FlickKind::Fresh,
            target_id: 0,
            hit: true,
            distance_deg: direction_deg.0.hypot(direction_deg.1),
            target_radius_deg: 1.0,
            reaction_secs: Some(0.25),
            movement_secs: Some(0.3),
            peak_speed_deg_per_sec: 300.0,
            primary_error_radii: None,
            endpoint_error_deg: 0.0,
            effective_distance_deg: 0.0,
            corrections: 0,
            settle_secs: None,
            path_efficiency: None,
            click_delay_secs: None,
            direction_deg,
            primary_offset_deg: Some(offset_deg),
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn fewer_flicks_than_the_minimum_give_no_suggestion() {
        let fl = vec![flick((20.0, 10.0), (2.0, 1.0)); MIN_SENS_FLICKS - 1];
        assert_eq!(suggest(&fl), None);
        assert!(suggest(&vec![flick((20.0, 10.0), (2.0, 1.0)); MIN_SENS_FLICKS]).is_some());
    }

    #[test]
    fn consistent_overshoot_suggests_a_lower_sensitivity_per_axis() {
        // 10% past horizontally, 5% short vertically.
        let fl = vec![flick((20.0, 10.0), (2.0, -0.5)); MIN_SENS_FLICKS];
        let s = suggest(&fl).unwrap();
        assert_eq!(s.samples, MIN_SENS_FLICKS);
        // Along the movement: (2·20 − 0.5·10) / (20² + 10²) = 0.07.
        assert!(close(s.mean_bias, 0.07), "{s:?}");
        assert!(close(s.multiplier, 1.0 / 1.07));
        let h = s.horizontal.unwrap();
        assert_eq!(h.samples, MIN_SENS_FLICKS);
        assert!(close(h.mean_bias, 0.1) && close(h.multiplier, 1.0 / 1.1));
        let v = s.vertical.unwrap();
        assert!(close(v.mean_bias, -0.05) && close(v.multiplier, 1.0 / 0.95));
        assert!(h.multiplier < 1.0 && v.multiplier > 1.0);
        assert!(s.suggests_change());
    }

    #[test]
    fn consistent_undershoot_suggests_a_higher_sensitivity() {
        let fl = vec![flick((-30.0, 0.0), (3.0, 0.0)); MIN_SENS_FLICKS]; // 10% short, moving left
        let s = suggest(&fl).unwrap();
        assert!(close(s.mean_bias, -0.1) && close(s.multiplier, 1.0 / 0.9));
        assert!(s.horizontal.is_some());
        // The vertical axis never moved: no vertical evidence.
        assert_eq!(s.vertical, None);
    }

    #[test]
    fn multipliers_are_clamped_to_the_provisional_bounds() {
        let over = vec![flick((20.0, 0.0), (8.0, 0.0)); MIN_SENS_FLICKS]; // 40% past
        let s = suggest(&over).unwrap();
        assert!(close(s.mean_bias, 0.4));
        assert_eq!(s.multiplier, MAX_DECREASE_MULTIPLIER);
        let under = vec![flick((20.0, 0.0), (-8.0, 0.0)); MIN_SENS_FLICKS]; // 40% short
        assert_eq!(suggest(&under).unwrap().multiplier, MAX_INCREASE_MULTIPLIER);
        assert_eq!(
            (MAX_DECREASE_MULTIPLIER, MAX_INCREASE_MULTIPLIER),
            (0.85, 1.15)
        );
    }

    #[test]
    fn flicks_whose_primary_ended_beyond_half_the_distance_are_dropped() {
        let mut fl = vec![flick((20.0, 0.0), (1.0, 0.0)); MIN_SENS_FLICKS]; // 5% past
        fl.push(flick((20.0, 0.0), (200.0, 0.0))); // 10× past
        fl.push(flick((20.0, 0.0), (-19.0, 0.0))); // a blip: stopped at 5% of the way
        fl.push(flick((20.0, 0.0), (10.0, 0.0))); // exactly 50% past is kept
        let s = suggest(&fl).unwrap();
        assert_eq!(s.samples, MIN_SENS_FLICKS + 1, "{s:?}");
        let expected = (0.05 * MIN_SENS_FLICKS as f32 + 0.5) / (MIN_SENS_FLICKS + 1) as f32;
        assert!(close(s.mean_bias, expected), "{s:?}");
        assert_eq!(s.horizontal.unwrap().samples, MIN_SENS_FLICKS + 1);
        // Below the minimum once the outliers are dropped: no suggestion.
        let mut few = vec![flick((20.0, 0.0), (1.0, 0.0)); MIN_SENS_FLICKS - 1];
        few.push(flick((20.0, 0.0), (-19.0, 0.0)));
        assert_eq!(suggest(&few), None);
    }

    #[test]
    fn small_biases_inside_the_band_suggest_no_change() {
        let fl = vec![flick((20.0, 0.0), (0.4, 0.0)); MIN_SENS_FLICKS]; // 2% past
        let s = suggest(&fl).unwrap();
        assert!(!s.suggests_change(), "{s:?}");
        let fl = vec![flick((20.0, 0.0), (1.0, 0.0)); MIN_SENS_FLICKS]; // 5% past
        assert!(suggest(&fl).unwrap().suggests_change());
    }

    #[test]
    fn retries_short_flicks_and_missing_primaries_are_ignored() {
        let good = flick((20.0, 0.0), (2.0, 0.0));
        let mut retry = good.clone();
        retry.kind = FlickKind::Correction;
        let tiny = flick((1.0, 0.5), (1.0, 0.5)); // below the 2° amplitude floor
        let mut no_primary = good.clone();
        no_primary.primary_offset_deg = None;
        let mut fl = vec![good; MIN_SENS_FLICKS];
        fl.extend(vec![retry; 10]);
        fl.extend(vec![tiny; 10]);
        fl.extend(vec![no_primary; 10]);
        let s = suggest(&fl).unwrap();
        assert_eq!(s.samples, MIN_SENS_FLICKS);
        assert!(close(s.mean_bias, 0.1));
    }

    #[test]
    fn the_unbiased_simulated_aimer_gets_no_change() {
        use crate::analytics::test_support::record;
        use crate::analytics::{AnalysisKind, analyze};
        use crate::sim::AimerProfile;
        for p in 1..4 {
            let mut fl = Vec::new();
            for seed in 1..4 {
                if let AnalysisKind::Flick(f) =
                    analyze(&record("flick-wide", AimerProfile::PRESETS[p], seed)).kind
                {
                    fl.extend(f);
                }
            }
            let s = suggest(&fl).expect("enough flicks");
            assert!(!s.suggests_change(), "preset {p}: {s:?}");
        }
    }
}
