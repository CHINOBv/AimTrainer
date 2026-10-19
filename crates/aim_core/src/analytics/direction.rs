//! Per-direction flick analysis: hit rate and reaction time for each screen quadrant and
//! distance class, and which axis (horizontal or vertical) the primary movement misses most.
//! See "Per-direction flicks" in `docs/training-science.md`.
//!
//! Only fresh flicks are classified: a retry after a miss starts next to the target, so its
//! direction says nothing about the player's flick to it.

use super::flick::{Flick, FlickKind, OVERSHOOT_RADII};
use super::stats::median;
use crate::i18n::{Lang, tr_in};
use crate::scenario::{ScenarioDef, SpawnArea};

/// A flick is far when its distance is above `(min + max) / FAR_DIVISOR` of the scenario's
/// possible target distances. Aim Lab's rule, used here as a tunable estimate.
pub const FAR_DIVISOR: f32 = 1.9;
/// A quadrant needs this many fresh flicks before it can be the weakest direction.
pub const MIN_QUADRANT_FLICKS: usize = 15;
/// An axis moved less than this (degrees) has no per-axis error: the start was already
/// aligned with the target on that axis.
pub const MIN_AXIS_AMPLITUDE_DEG: f32 = 2.0;
/// Each axis needs this many measured flicks before the axes are compared.
pub const MIN_AXIS_SAMPLES: usize = 20;
/// An axis is weak when its over/undershoot rate exceeds the other axis's by more than
/// this share (0.20 = 20 percentage points).
pub const WEAK_AXIS_MARGIN: f32 = 0.20;

/// Screen quadrant of a flick's direction, by the angle `atan2(dy, dx)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quadrant {
    /// (0°, 90°].
    UpRight,
    /// (90°, 180°].
    UpLeft,
    /// (−180°, −90°].
    DownLeft,
    /// (−90°, 0°].
    DownRight,
}

impl Quadrant {
    /// Every quadrant, counter-clockwise from up-right.
    pub const ALL: [Quadrant; 4] = [
        Quadrant::UpRight,
        Quadrant::UpLeft,
        Quadrant::DownLeft,
        Quadrant::DownRight,
    ];

    /// Quadrant of a screen vector (`.0` right, `.1` up), or `None` for a zero vector.
    pub fn of(direction_deg: (f32, f32)) -> Option<Self> {
        let (dx, dy) = direction_deg;
        if dx.hypot(dy) < 1e-6 {
            return None;
        }
        let mut angle = dy.atan2(dx).to_degrees();
        if angle <= -180.0 {
            // −180° (dy = −0.0) is the same direction as 180°, which is up-left.
            angle += 360.0;
        }
        Some(match angle {
            a if a > 90.0 => Quadrant::UpLeft,
            a if a > 0.0 => Quadrant::UpRight,
            a if a > -90.0 => Quadrant::DownRight,
            _ => Quadrant::DownLeft,
        })
    }

    /// Lowercase name, such as "up-left".
    pub fn label(self) -> &'static str {
        match self {
            Quadrant::UpRight => "up-right",
            Quadrant::UpLeft => "up-left",
            Quadrant::DownLeft => "down-left",
            Quadrant::DownRight => "down-right",
        }
    }

    /// [`Quadrant::label`] in `lang`.
    pub fn label_in(self, lang: Lang) -> &'static str {
        tr_in(lang, self.label())
    }
}

/// Near or far, relative to the scenario's possible target distances.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistanceClass {
    /// At or below the far threshold.
    Near,
    /// Above the far threshold.
    Far,
}

impl DistanceClass {
    /// Both classes, near first.
    pub const ALL: [DistanceClass; 2] = [DistanceClass::Near, DistanceClass::Far];

    /// Lowercase name.
    pub fn label(self) -> &'static str {
        match self {
            DistanceClass::Near => "near",
            DistanceClass::Far => "far",
        }
    }

    /// [`DistanceClass::label`] in `lang`.
    pub fn label_in(self, lang: Lang) -> &'static str {
        tr_in(lang, self.label())
    }
}

/// Where a distance range came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeSource {
    /// The scenario's angular spawn area.
    SpawnArea,
    /// The run's own flick distances (the spawn area is in meters, such as `floor`).
    Observed,
}

/// The near/far split for one run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DistanceSplit {
    /// Smallest possible target distance, degrees.
    pub min_deg: f32,
    /// Largest possible target distance, degrees.
    pub max_deg: f32,
    /// Where `min_deg` and `max_deg` came from.
    pub source: RangeSource,
}

impl DistanceSplit {
    /// Distances strictly above this are far, degrees.
    pub fn far_above_deg(&self) -> f32 {
        (self.min_deg + self.max_deg) / FAR_DIVISOR
    }

    /// Near or far for a flick distance in degrees.
    pub fn classify(&self, distance_deg: f32) -> DistanceClass {
        if distance_deg > self.far_above_deg() {
            DistanceClass::Far
        } else {
            DistanceClass::Near
        }
    }

    /// The split for a run of `def`: from its spawn area when that is angular, otherwise
    /// from the range of `flicks`' distances. `None` when neither gives a range.
    pub fn for_run(def: &ScenarioDef, flicks: &[Flick]) -> Option<Self> {
        if let Some((min_deg, max_deg)) = spawn_distance_range_deg(&def.targets.spawn) {
            return Some(Self {
                min_deg,
                max_deg,
                source: RangeSource::SpawnArea,
            });
        }
        let mut distances = flicks.iter().map(|f| f.distance_deg);
        let first = distances.next()?;
        let (min_deg, max_deg) =
            distances.fold((first, first), |(lo, hi), d| (lo.min(d), hi.max(d)));
        Some(Self {
            min_deg,
            max_deg,
            source: RangeSource::Observed,
        })
    }
}

/// Possible angular distances from one aim point to the next target, degrees:
/// a crosshair-relative spawn gives its offset range; an angular box gives 0 up to its
/// diagonal (the farthest two spawns can be apart). A floor box is in meters: `None`.
pub fn spawn_distance_range_deg(spawn: &SpawnArea) -> Option<(f32, f32)> {
    match spawn {
        SpawnArea::AroundCrosshair { offset_deg, .. } => Some((offset_deg.min, offset_deg.max)),
        SpawnArea::Angular {
            yaw_deg, pitch_deg, ..
        } => Some((
            0.0,
            (yaw_deg.max - yaw_deg.min).hypot(pitch_deg.max - pitch_deg.min),
        )),
        SpawnArea::Floor { .. } => None,
    }
}

/// Fresh flicks of one quadrant and distance class.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectionBucket {
    /// The bucket's quadrant.
    pub quadrant: Quadrant,
    /// The bucket's distance class.
    pub distance: DistanceClass,
    /// Fresh flicks in the bucket.
    pub flicks: usize,
    /// How many of them hit.
    pub hits: usize,
    /// Median reaction time of the bucket's flicks, seconds.
    pub median_reaction_secs: Option<f32>,
}

impl DirectionBucket {
    /// Share of the bucket's flicks that hit, or `None` when it is empty.
    pub fn hit_rate(&self) -> Option<f32> {
        (self.flicks > 0).then(|| self.hits as f32 / self.flicks as f32)
    }
}

/// The quadrant whose hit rate is lowest, compared with every other fresh flick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeakestDirection {
    /// The weakest quadrant (near and far pooled).
    pub quadrant: Quadrant,
    /// Its hit rate (0-1).
    pub hit_rate: f32,
    /// Hit rate of the fresh flicks in every other quadrant (0-1).
    pub others_hit_rate: f32,
}

impl WeakestDirection {
    /// How much lower the weakest quadrant's hit rate is than the others' (0-1).
    pub fn gap(&self) -> f32 {
        self.others_hit_rate - self.hit_rate
    }
}

/// Horizontal or vertical screen axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Left-right.
    Horizontal,
    /// Up-down.
    Vertical,
}

impl Axis {
    /// Lowercase name.
    pub fn label(self) -> &'static str {
        match self {
            Axis::Horizontal => "horizontal",
            Axis::Vertical => "vertical",
        }
    }

    /// [`Axis::label`] in `lang`.
    pub fn label_in(self, lang: Lang) -> &'static str {
        tr_in(lang, self.label())
    }
}

/// Primary-movement accuracy along one screen axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxisAccuracy {
    /// Flicks measured on this axis.
    pub samples: usize,
    /// Share whose primary movement ended more than one target radius past the target on
    /// this axis.
    pub overshoot_rate: f32,
    /// Share that ended more than one target radius short of it on this axis.
    pub undershoot_rate: f32,
}

impl AxisAccuracy {
    /// Overshoot plus undershoot rate.
    pub fn miss_rate(&self) -> f32 {
        self.overshoot_rate + self.undershoot_rate
    }

    fn from_errors(errors: &[f32]) -> Option<Self> {
        if errors.is_empty() {
            return None;
        }
        let share = |n: usize| n as f32 / errors.len() as f32;
        Some(Self {
            samples: errors.len(),
            overshoot_rate: share(errors.iter().filter(|&&e| e > OVERSHOOT_RADII).count()),
            undershoot_rate: share(errors.iter().filter(|&&e| e < -OVERSHOOT_RADII).count()),
        })
    }
}

/// Signed primary-movement error of a fresh flick on each screen axis, in target radii:
/// positive past the target in the direction the axis moved, negative short of it. An axis
/// moved less than [`MIN_AXIS_AMPLITUDE_DEG`] has no error.
pub fn axis_errors_radii(flick: &Flick) -> (Option<f32>, Option<f32>) {
    let Some(offset) = flick
        .primary_offset_deg
        .filter(|_| flick.kind == FlickKind::Fresh)
    else {
        return (None, None);
    };
    let radius = flick.target_radius_deg.max(1e-3);
    let along = |moved: f32, ended: f32| {
        (moved.abs() >= MIN_AXIS_AMPLITUDE_DEG).then(|| ended * moved.signum() / radius)
    };
    (
        along(flick.direction_deg.0, offset.0),
        along(flick.direction_deg.1, offset.1),
    )
}

/// Per-direction aggregates over fresh flicks.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectionSummary {
    /// One bucket per quadrant and distance class, in `Quadrant::ALL` order, near first.
    pub buckets: Vec<DirectionBucket>,
    /// The quadrant far below the others, when at least one quadrant and the rest each
    /// hold [`MIN_QUADRANT_FLICKS`] fresh flicks.
    pub weakest: Option<WeakestDirection>,
    /// Primary-movement accuracy on the horizontal axis.
    pub horizontal: Option<AxisAccuracy>,
    /// Primary-movement accuracy on the vertical axis.
    pub vertical: Option<AxisAccuracy>,
    /// The axis whose over/undershoot rate is more than [`WEAK_AXIS_MARGIN`] above the
    /// other's, when both have [`MIN_AXIS_SAMPLES`].
    pub weak_axis: Option<Axis>,
}

impl DirectionSummary {
    /// Summarize flicks already classified as near or far; retries are ignored.
    pub fn from_flicks<'a>(flicks: impl IntoIterator<Item = (&'a Flick, DistanceClass)>) -> Self {
        let mut grouped: Vec<(Quadrant, DistanceClass, Vec<&Flick>)> = Quadrant::ALL
            .iter()
            .flat_map(|&q| DistanceClass::ALL.map(|d| (q, d, Vec::new())))
            .collect();
        let (mut horizontal, mut vertical) = (Vec::new(), Vec::new());
        for (flick, distance) in flicks {
            if flick.kind != FlickKind::Fresh {
                continue;
            }
            let (h, v) = axis_errors_radii(flick);
            horizontal.extend(h);
            vertical.extend(v);
            if let Some(q) = Quadrant::of(flick.direction_deg)
                && let Some(group) = grouped.iter_mut().find(|g| g.0 == q && g.1 == distance)
            {
                group.2.push(flick);
            }
        }
        let buckets: Vec<DirectionBucket> = grouped
            .into_iter()
            .map(|(quadrant, distance, fl)| DirectionBucket {
                quadrant,
                distance,
                flicks: fl.len(),
                hits: fl.iter().filter(|f| f.hit).count(),
                median_reaction_secs: median(
                    &fl.iter()
                        .filter_map(|f| f.reaction_secs)
                        .collect::<Vec<_>>(),
                ),
            })
            .collect();
        let horizontal = AxisAccuracy::from_errors(&horizontal);
        let vertical = AxisAccuracy::from_errors(&vertical);
        Self {
            weakest: weakest(&buckets),
            weak_axis: weak_axis(horizontal, vertical),
            buckets,
            horizontal,
            vertical,
        }
    }

    /// The bucket for one quadrant and distance class.
    pub fn bucket(&self, quadrant: Quadrant, distance: DistanceClass) -> &DirectionBucket {
        self.buckets
            .iter()
            .find(|b| b.quadrant == quadrant && b.distance == distance)
            .expect("every quadrant and distance class has a bucket")
    }
}

fn weakest(buckets: &[DirectionBucket]) -> Option<WeakestDirection> {
    let count = |pick: &dyn Fn(&DirectionBucket) -> bool| {
        buckets
            .iter()
            .filter(|b| pick(b))
            .fold((0, 0), |(n, h), b| (n + b.flicks, h + b.hits))
    };
    let (total, total_hits) = count(&|_| true);
    Quadrant::ALL
        .iter()
        .filter_map(|&q| {
            let (n, hits) = count(&|b| b.quadrant == q);
            let (others, others_hits) = (total - n, total_hits - hits);
            (n >= MIN_QUADRANT_FLICKS && others >= MIN_QUADRANT_FLICKS).then(|| WeakestDirection {
                quadrant: q,
                hit_rate: hits as f32 / n as f32,
                others_hit_rate: others_hits as f32 / others as f32,
            })
        })
        .min_by(|a, b| a.hit_rate.total_cmp(&b.hit_rate))
}

fn weak_axis(horizontal: Option<AxisAccuracy>, vertical: Option<AxisAccuracy>) -> Option<Axis> {
    let (h, v) = (horizontal?, vertical?);
    if h.samples < MIN_AXIS_SAMPLES || v.samples < MIN_AXIS_SAMPLES {
        return None;
    }
    let diff = h.miss_rate() - v.miss_rate();
    if diff > WEAK_AXIS_MARGIN {
        Some(Axis::Horizontal)
    } else if -diff > WEAK_AXIS_MARGIN {
        Some(Axis::Vertical)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::flick::{Flick, FlickKind};
    use crate::analytics::test_support::record;
    use crate::analytics::{AnalysisKind, analyze};
    use crate::scenario::builtin;
    use crate::sim::AimerProfile;

    fn flick(direction_deg: (f32, f32), hit: bool) -> Flick {
        Flick {
            kind: FlickKind::Fresh,
            target_id: 0,
            hit,
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
            primary_offset_deg: None,
        }
    }

    fn with_offset(direction_deg: (f32, f32), offset: (f32, f32)) -> Flick {
        Flick {
            primary_offset_deg: Some(offset),
            ..flick(direction_deg, true)
        }
    }

    #[test]
    fn quadrants_follow_the_angle_with_half_open_boundaries() {
        use Quadrant::*;
        let cases = [
            ((1.0, 0.0), DownRight), // 0° belongs to (−90°, 0°]
            ((1.0, 1e-4), UpRight),  // just above 0°
            ((1.0, 1.0), UpRight),   // 45°
            ((0.0, 1.0), UpRight),   // 90° belongs to (0°, 90°]
            ((-1e-4, 1.0), UpLeft),  // just past 90°
            ((-1.0, 1.0), UpLeft),   // 135°
            ((-1.0, 0.0), UpLeft),   // 180° belongs to (90°, 180°]
            ((-1.0, -0.0), UpLeft),  // −180° is the same direction as 180°
            ((-1.0, -1e-4), DownLeft),
            ((-1.0, -1.0), DownLeft), // −135°
            ((0.0, -1.0), DownLeft),  // −90° belongs to (−180°, −90°]
            ((1e-4, -1.0), DownRight),
            ((1.0, -1.0), DownRight), // −45°
        ];
        for (v, q) in cases {
            assert_eq!(Quadrant::of(v), Some(q), "{v:?}");
        }
        assert_eq!(Quadrant::of((0.0, 0.0)), None);
        assert_eq!(UpLeft.label(), "up-left");
    }

    #[test]
    fn far_means_above_min_plus_max_over_1_9() {
        let split = DistanceSplit {
            min_deg: 20.0,
            max_deg: 47.0,
            source: RangeSource::SpawnArea,
        };
        let far = (20.0 + 47.0) / 1.9;
        assert!((split.far_above_deg() - far).abs() < 1e-5);
        assert_eq!(split.classify(far), DistanceClass::Near);
        assert_eq!(split.classify(far + 0.01), DistanceClass::Far);
        assert_eq!(split.classify(far - 0.01), DistanceClass::Near);
    }

    fn def(id: &str) -> crate::scenario::ScenarioDef {
        builtin::all().into_iter().find(|d| d.id == id).unwrap()
    }

    #[test]
    fn the_distance_range_comes_from_the_spawn_area_when_it_is_angular() {
        // Crosshair-relative: the offset range itself.
        let split = DistanceSplit::for_run(&def("flick-wide"), &[]).unwrap();
        assert_eq!((split.min_deg, split.max_deg), (20.0, 47.0));
        assert_eq!(split.source, RangeSource::SpawnArea);
        // Angular box yaw [−60, 60] × pitch [−2, 10]: from 0 to the box diagonal.
        let split = DistanceSplit::for_run(&def("switch-wide"), &[]).unwrap();
        assert_eq!(split.min_deg, 0.0);
        assert!((split.max_deg - 120.0_f32.hypot(12.0)).abs() < 1e-4);
        assert_eq!(split.source, RangeSource::SpawnArea);
    }

    #[test]
    fn floor_spawns_fall_back_to_the_observed_flick_distances() {
        let fl = [
            flick((4.0, 0.0), true),
            flick((0.0, 9.0), true),
            flick((-12.0, 0.0), false),
        ];
        let split = DistanceSplit::for_run(&def("static-headline"), &fl).unwrap();
        assert_eq!((split.min_deg, split.max_deg), (4.0, 12.0));
        assert_eq!(split.source, RangeSource::Observed);
        assert!(DistanceSplit::for_run(&def("static-headline"), &[]).is_none());
    }

    #[test]
    fn buckets_count_fresh_flicks_with_hit_rate_and_median_reaction() {
        let split = DistanceSplit {
            min_deg: 0.0,
            max_deg: 19.0,
            source: RangeSource::Observed,
        }; // far above 10°
        let mut fl = vec![
            flick((5.0, 5.0), true),   // up-right near
            flick((5.0, 5.0), false),  // up-right near
            flick((20.0, 1.0), true),  // up-right far
            flick((-3.0, -3.0), true), // down-left near
        ];
        fl[1].reaction_secs = Some(0.35);
        let mut retry = flick((5.0, 5.0), false);
        retry.kind = FlickKind::Correction; // retries are left out
        fl.push(retry);
        let s =
            DirectionSummary::from_flicks(fl.iter().map(|f| (f, split.classify(f.distance_deg))));
        assert_eq!(s.buckets.len(), 8);
        let ur_near = s.bucket(Quadrant::UpRight, DistanceClass::Near);
        assert_eq!((ur_near.flicks, ur_near.hits), (2, 1));
        assert_eq!(ur_near.hit_rate(), Some(0.5));
        assert!((ur_near.median_reaction_secs.unwrap() - 0.30).abs() < 1e-6);
        let ur_far = s.bucket(Quadrant::UpRight, DistanceClass::Far);
        assert_eq!((ur_far.flicks, ur_far.hit_rate()), (1, Some(1.0)));
        let dl = s.bucket(Quadrant::DownLeft, DistanceClass::Near);
        assert_eq!((dl.flicks, dl.hits), (1, 1));
        let empty = s.bucket(Quadrant::UpLeft, DistanceClass::Far);
        assert_eq!(
            (empty.flicks, empty.hit_rate(), empty.median_reaction_secs),
            (0, None, None)
        );
        // Too few flicks per quadrant for a weakest direction.
        assert_eq!(s.weakest, None);
    }

    #[test]
    fn the_weakest_direction_is_the_quadrant_far_below_the_others() {
        let mut fl = Vec::new();
        for i in 0..MIN_QUADRANT_FLICKS {
            fl.push(flick((-5.0, 5.0), i < 3)); // up-left: 3 of 15 hit
            fl.push(flick((5.0, 5.0), true)); // up-right: all hit
            fl.push(flick((5.0, -5.0), i % 5 != 0)); // down-right: 12 of 15 hit
        }
        fl.push(flick((-5.0, -5.0), false)); // down-left: too few to judge alone
        let s = DirectionSummary::from_flicks(fl.iter().map(|f| (f, DistanceClass::Near)));
        let w = s.weakest.expect("up-left is judged");
        assert_eq!(w.quadrant, Quadrant::UpLeft);
        assert!((w.hit_rate - 0.2).abs() < 1e-6);
        // Every other fresh flick, including the lone down-left miss: 27 of 31.
        assert!((w.others_hit_rate - 27.0 / 31.0).abs() < 1e-6);
        assert!((w.gap() - (27.0 / 31.0 - 0.2)).abs() < 1e-6);
    }

    #[test]
    fn per_axis_errors_are_in_target_radii_past_or_short_of_the_target() {
        // Right and up; primary ends 1.5° right of and 0.5° below the target (radius 1°).
        let f = with_offset((10.0, 4.0), (1.5, -0.5));
        assert_eq!(axis_errors_radii(&f), (Some(1.5), Some(-0.5)));
        // Moving left, ending left of the target is past it.
        let f = with_offset((-10.0, -4.0), (-2.0, 0.5));
        assert_eq!(axis_errors_radii(&f), (Some(2.0), Some(-0.5)));
        // An axis moved less than the minimum amplitude has no error on that axis.
        let f = with_offset((10.0, 1.0), (1.0, 1.0));
        assert_eq!(axis_errors_radii(&f), (Some(1.0), None));
        // No primary movement, or a retry: nothing.
        assert_eq!(axis_errors_radii(&flick((10.0, 4.0), true)), (None, None));
        let mut retry = with_offset((10.0, 4.0), (1.0, 1.0));
        retry.kind = FlickKind::Correction;
        assert_eq!(axis_errors_radii(&retry), (None, None));
    }

    #[test]
    fn the_weak_axis_misses_clearly_more_often_than_the_other() {
        let mut fl = Vec::new();
        for i in 0..20 {
            // Horizontal: 6 of every 10 end beyond one radius (3 past, 3 short).
            let ex = match i % 10 {
                0..3 => 1.5,
                3..6 => -1.5,
                _ => 0.2,
            };
            // Vertical: 1 of every 10 beyond one radius.
            let ey = if i % 10 == 0 { 1.2 } else { 0.1 };
            fl.push(with_offset((10.0, 5.0), (ex, ey)));
        }
        let s = DirectionSummary::from_flicks(fl.iter().map(|f| (f, DistanceClass::Near)));
        let h = s.horizontal.unwrap();
        assert_eq!(h.samples, 20);
        assert!((h.overshoot_rate - 0.3).abs() < 1e-6 && (h.undershoot_rate - 0.3).abs() < 1e-6);
        assert!((h.miss_rate() - 0.6).abs() < 1e-6);
        let v = s.vertical.unwrap();
        assert!((v.overshoot_rate - 0.1).abs() < 1e-6 && v.undershoot_rate == 0.0);
        assert_eq!(s.weak_axis, Some(Axis::Horizontal));
        // Swapping the axes makes vertical the weak one.
        let swapped: Vec<Flick> = fl
            .iter()
            .map(|f| {
                let (ox, oy) = f.primary_offset_deg.unwrap();
                with_offset((10.0, 5.0), (oy, ox))
            })
            .collect();
        let s = DirectionSummary::from_flicks(swapped.iter().map(|f| (f, DistanceClass::Near)));
        assert_eq!(s.weak_axis, Some(Axis::Vertical));

        // Exactly the 20-point margin (40% vs 20%) is not past it: no weak axis.
        let balanced: Vec<Flick> = (0..20)
            .map(|i| {
                let ex = if i % 10 < 4 { 1.5 } else { 0.0 };
                let ey = if i % 10 < 2 { 1.5 } else { 0.0 };
                with_offset((10.0, 5.0), (ex, ey))
            })
            .collect();
        let s = DirectionSummary::from_flicks(balanced.iter().map(|f| (f, DistanceClass::Near)));
        assert_eq!(s.weak_axis, None);
        // Too few samples on an axis: no weak axis.
        let s = DirectionSummary::from_flicks(
            fl[..MIN_AXIS_SAMPLES - 1]
                .iter()
                .map(|f| (f, DistanceClass::Near)),
        );
        assert_eq!(s.weak_axis, None);
    }

    #[test]
    fn analyze_splits_click_runs_by_their_scenario_and_skips_tracking() {
        let a = analyze(&record("flick-wide", AimerProfile::PRESETS[2], 5));
        let split = a.distance_split.unwrap();
        assert_eq!(
            (split.min_deg, split.max_deg, split.source),
            (20.0, 47.0, RangeSource::SpawnArea)
        );
        let s = a.direction_summary().unwrap();
        let AnalysisKind::Flick(fl) = &a.kind else {
            panic!("click run")
        };
        let fresh = fl.iter().filter(|f| f.kind == FlickKind::Fresh).count();
        assert_eq!(s.buckets.iter().map(|b| b.flicks).sum::<usize>(), fresh);

        let floor = analyze(&record("static-headline", AimerProfile::PRESETS[2], 5));
        assert_eq!(floor.distance_split.unwrap().source, RangeSource::Observed);

        let tracking = analyze(&record("track-precise", AimerProfile::PRESETS[2], 5));
        assert_eq!(tracking.distance_split, None);
        assert_eq!(tracking.direction_summary(), None);
    }

    #[test]
    fn direction_labels_are_shown_in_spanish() {
        assert_eq!(Quadrant::UpLeft.label_in(Lang::En), "up-left");
        assert_eq!(Quadrant::UpLeft.label_in(Lang::Es), "arriba-izquierda");
        assert_eq!(DistanceClass::Far.label_in(Lang::Es), "lejos");
        assert_eq!(Axis::Vertical.label_in(Lang::Es), "vertical");
        assert_eq!(Axis::Horizontal.label_in(Lang::En), "horizontal");
    }
}
