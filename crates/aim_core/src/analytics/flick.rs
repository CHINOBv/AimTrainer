//! Flick analysis for click scenarios: one [`Flick`] per shot, segmented from the samples.
//!
//! A flick runs from the moment the shot's target could first be aimed at (it became
//! visible, or the previous shot was fired, whichever is later) to the shot. Inside it, the
//! crosshair's angular speed profile is split into sub-movements (Meyer et al. 1988): the
//! first is the primary, ballistic movement; each later one is a correction.
//! See "Analytics metrics" in `docs/training-science.md`.

use super::stats::{mean, median, std_dev};
use super::trace::{Sample, TargetSample, tangent_offset_deg};
use crate::math::{ViewAngles, angle_between_deg};

/// Movement onset: the first frame whose speed reaches this fraction of the flick's peak
/// speed. 10% of peak is a common kinematic onset criterion.
pub const ONSET_FRACTION_OF_PEAK: f32 = 0.10;
/// Speed floor for onset and sub-movement detection, so sensor jitter is not a movement.
pub const MIN_MOVEMENT_SPEED_DEG_PER_SEC: f32 = 5.0;
/// A new sub-movement starts when speed falls to a valley at most this fraction of the
/// preceding peak and then rises again by at least `REACCELERATION_FRACTION` of that peak.
pub const VALLEY_FRACTION_OF_PEAK: f32 = 0.5;
/// A new sub-movement needs the speed to rise again by at least this fraction of the preceding peak.
pub const REACCELERATION_FRACTION: f32 = 0.2;
/// Primary endpoint error, in target radii along the movement direction, beyond which the
/// primary movement counts as an overshoot (positive) or undershoot (negative): it ended
/// outside the target's far or near edge.
pub const OVERSHOOT_RADII: f32 = 1.0;
/// ISO 9241-9 effective-width factor: `We = 4.133 · SD` of the endpoint error.
pub const EFFECTIVE_WIDTH_FACTOR: f32 = 4.133;
/// Fewer flicks than this give no throughput (the endpoint SD is unreliable).
pub const MIN_FLICKS_FOR_THROUGHPUT: usize = 5;

/// Whether a flick goes to a new target or retries one just missed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlickKind {
    /// Towards a target the previous shot did not just miss.
    Fresh,
    /// Another attempt at the target the previous shot missed.
    Correction,
}

/// The analysis of one shot: the crosshair's approach from the start of the flick to the shot.
#[derive(Debug, Clone, PartialEq)]
pub struct Flick {
    /// Whether this is a fresh target or a correction after a miss.
    pub kind: FlickKind,
    /// Id of the target the flick was aimed at.
    pub target_id: u64,
    /// The shot hit a counted zone.
    pub hit: bool,
    /// Angle from the crosshair at the start to the target at the shot, degrees.
    pub distance_deg: f32,
    /// Angular radius of the target's aim zone at the shot, degrees.
    pub target_radius_deg: f32,
    /// Start to movement onset. `None` when the crosshair never moved.
    pub reaction_secs: Option<f32>,
    /// Movement onset to the shot.
    pub movement_secs: Option<f32>,
    /// Highest crosshair angular speed in the flick, degrees/s.
    pub peak_speed_deg_per_sec: f32,
    /// Signed error at the end of the primary sub-movement, along the movement direction,
    /// in target radii: positive past the target, negative short of it.
    pub primary_error_radii: Option<f32>,
    /// Same as `primary_error_radii` but at the shot, in degrees (used for throughput).
    pub endpoint_error_deg: f32,
    /// Movement amplitude along the intended direction, start to shot, degrees.
    pub effective_distance_deg: f32,
    /// Sub-movements after the primary one.
    pub corrections: u32,
    /// End of the primary sub-movement to the shot.
    pub settle_secs: Option<f32>,
    /// Straight-line angle from start to shot divided by the angle travelled (1 = straight).
    pub path_efficiency: Option<f32>,
    /// First frame the crosshair was inside the target's aim zone to the shot, seconds.
    /// `None` when the crosshair never reached the zone during the flick.
    pub click_delay_secs: Option<f32>,
}

impl Flick {
    /// Whether the primary movement ended past the target's far edge.
    pub fn overshot(&self) -> bool {
        self.primary_error_radii
            .is_some_and(|e| e > OVERSHOOT_RADII)
    }
    /// Whether the primary movement ended short of the target's near edge.
    pub fn undershot(&self) -> bool {
        self.primary_error_radii
            .is_some_and(|e| e < -OVERSHOOT_RADII)
    }
}

/// Aggregates over a set of flicks.
#[derive(Debug, Clone, PartialEq)]
pub struct FlickSummary {
    /// Number of flicks summarized.
    pub flicks: usize,
    /// Share of flicks that hit (0-1).
    pub hit_rate: f32,
    /// Median over fresh flicks only (corrections react to the miss, not a new target).
    pub median_reaction_secs: Option<f32>,
    /// Median onset-to-shot time over all flicks, seconds.
    pub median_movement_secs: Option<f32>,
    /// Median of the flicks' peak crosshair speed, degrees/s.
    pub median_peak_speed_deg_per_sec: Option<f32>,
    /// Share of flicks with a detected primary movement that overshot / undershot.
    pub overshoot_rate: f32,
    /// Share of flicks with a detected primary movement that undershot (0-1).
    pub undershoot_rate: f32,
    /// Mean number of corrective sub-movements per flick.
    pub mean_corrections: f32,
    /// Median time from the end of the primary movement to the shot, seconds.
    pub median_settle_secs: Option<f32>,
    /// Median path efficiency (1 = straight line).
    pub median_path_efficiency: Option<f32>,
    /// Fitts' law effective throughput (ISO 9241-9), bits/s.
    pub throughput_bits_per_sec: Option<f32>,
    /// Median time from first entering the target to the shot, over flicks that entered it.
    pub median_click_delay_secs: Option<f32>,
}

impl FlickSummary {
    /// Summarize `flicks`; every statistic is `None` or zero for an empty set.
    pub fn from_flicks(flicks: &[Flick]) -> Self {
        let collect = |f: fn(&Flick) -> Option<f32>| -> Vec<f32> {
            flicks.iter().filter_map(f).collect::<Vec<f32>>()
        };
        let fresh: Vec<f32> = flicks
            .iter()
            .filter(|f| f.kind == FlickKind::Fresh)
            .filter_map(|f| f.reaction_secs)
            .collect();
        let with_primary = flicks
            .iter()
            .filter(|f| f.primary_error_radii.is_some())
            .count();
        let rate = |n: usize, d: usize| if d == 0 { 0.0 } else { n as f32 / d as f32 };
        Self {
            flicks: flicks.len(),
            hit_rate: rate(flicks.iter().filter(|f| f.hit).count(), flicks.len()),
            median_reaction_secs: median(&fresh),
            median_movement_secs: median(&collect(|f| f.movement_secs)),
            median_peak_speed_deg_per_sec: median(
                &flicks
                    .iter()
                    .map(|f| f.peak_speed_deg_per_sec)
                    .collect::<Vec<_>>(),
            ),
            overshoot_rate: rate(flicks.iter().filter(|f| f.overshot()).count(), with_primary),
            undershoot_rate: rate(
                flicks.iter().filter(|f| f.undershot()).count(),
                with_primary,
            ),
            mean_corrections: mean(
                &flicks
                    .iter()
                    .map(|f| f.corrections as f32)
                    .collect::<Vec<_>>(),
            )
            .unwrap_or(0.0),
            median_settle_secs: median(&collect(|f| f.settle_secs)),
            median_path_efficiency: median(&collect(|f| f.path_efficiency)),
            throughput_bits_per_sec: throughput(flicks),
            median_click_delay_secs: median(&collect(|f| f.click_delay_secs)),
        }
    }
}

/// ISO 9241-9 effective throughput over fresh flicks with a detected movement:
/// `We = 4.133 · SD(endpoint error)`, `IDe = log2(De / We + 1)` with `De` the mean
/// effective distance, and `TP = IDe / MT` with `MT` the mean movement time.
pub fn throughput(flicks: &[Flick]) -> Option<f32> {
    let used: Vec<&Flick> = flicks
        .iter()
        .filter(|f| f.kind == FlickKind::Fresh && f.movement_secs.is_some())
        .collect();
    if used.len() < MIN_FLICKS_FOR_THROUGHPUT {
        return None;
    }
    let errors: Vec<f32> = used.iter().map(|f| f.endpoint_error_deg).collect();
    let distances: Vec<f32> = used.iter().map(|f| f.effective_distance_deg).collect();
    let times: Vec<f32> = used.iter().filter_map(|f| f.movement_secs).collect();
    let we = EFFECTIVE_WIDTH_FACTOR * std_dev(&errors)?;
    let de = mean(&distances)?;
    let mt = mean(&times)?;
    if we <= 0.0 || mt <= 0.0 || de <= 0.0 {
        return None;
    }
    Some((de / we + 1.0).log2() / mt)
}

fn unit(v: (f32, f32)) -> Option<(f32, f32)> {
    let len = (v.0 * v.0 + v.1 * v.1).sqrt();
    (len > 1e-6).then(|| (v.0 / len, v.1 / len))
}

fn dot(a: (f32, f32), b: (f32, f32)) -> f32 {
    a.0 * b.0 + a.1 * b.1
}

/// Signed error of `view` relative to `target`, along `dir`, degrees.
fn along_error_deg(view: ViewAngles, target: ViewAngles, dir: (f32, f32)) -> f32 {
    dot(tangent_offset_deg(target, view), dir)
}

/// Sub-movements of a speed profile, as `(start, end)` index pairs (end inclusive).
fn submovements(speeds: &[f32], onset: usize, floor: f32) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = onset;
    let mut peak = 0.0_f32;
    let mut valley: Option<(usize, f32)> = None;
    let mut moving = true;
    for (i, &v) in speeds.iter().enumerate().skip(onset) {
        if !moving {
            if v >= floor {
                moving = true;
                start = i;
                peak = v;
                valley = None;
            }
            continue;
        }
        if v < floor {
            out.push((start, i));
            moving = false;
            continue;
        }
        match valley {
            Some((vi, vv)) if v >= vv + REACCELERATION_FRACTION * peak => {
                out.push((start, vi));
                start = vi;
                peak = v;
                valley = None;
            }
            Some((_, vv)) if v < vv => valley = Some((i, v)),
            None if v <= VALLEY_FRACTION_OF_PEAK * peak => valley = Some((i, v)),
            _ => {}
        }
        if valley.is_none() {
            peak = peak.max(v);
        }
    }
    if moving {
        out.push((start, speeds.len().saturating_sub(1)));
    }
    out
}

fn analyze_one(
    segment: &[Sample],
    start_view: ViewAngles,
    target: &TargetSample,
    kind: FlickKind,
    start_secs: f32,
) -> Flick {
    let shot = segment
        .last()
        .expect("a segment holds at least the shot frame");
    let goal = target.aim_point;
    let radius = target.radius_deg.max(1e-3);
    let dir = unit(tangent_offset_deg(start_view, goal)).unwrap_or((1.0, 0.0));
    let speeds: Vec<f32> = segment
        .iter()
        .map(|s| s.angular_speed_deg_per_sec)
        .collect();
    let peak = speeds.iter().copied().fold(0.0, f32::max);
    let floor = MIN_MOVEMENT_SPEED_DEG_PER_SEC.max(ONSET_FRACTION_OF_PEAK * peak);
    let onset = (peak >= MIN_MOVEMENT_SPEED_DEG_PER_SEC)
        .then(|| speeds.iter().position(|&v| v >= floor))
        .flatten();
    // Each frame's error is measured against where that frame saw the target, which matters
    // for moving targets.
    let goal_at = |s: &Sample| s.target(target.id).map_or(goal, |t| t.aim_point);

    let (mut reaction, mut movement, mut primary_error, mut settle) = (None, None, None, None);
    let mut corrections = 0;
    if let Some(onset) = onset {
        let onset_time = segment[onset].time_secs - segment[onset].dt_secs;
        reaction = Some((onset_time - start_secs).max(0.0));
        movement = Some((shot.time_secs - onset_time).max(0.0));
        let subs = submovements(&speeds, onset, floor);
        corrections = subs.len().saturating_sub(1) as u32;
        if let Some(&(_, end)) = subs.first() {
            let s = &segment[end];
            primary_error = Some(along_error_deg(s.view, goal_at(s), dir) / radius);
            settle = Some((shot.time_secs - s.time_secs).max(0.0));
        }
    }
    let travelled: f32 = segment
        .iter()
        .map(|s| s.angular_speed_deg_per_sec * s.dt_secs)
        .sum();
    let straight = angle_between_deg(start_view.direction(), shot.view.direction());
    // Hesitation: how long the crosshair had already been on the target (since it first got
    // there) when the shot was fired.
    let click_delay = segment
        .iter()
        .find(|s| {
            let (point, radius) = s
                .target(target.id)
                .map_or((goal, target.radius_deg), |t| (t.aim_point, t.radius_deg));
            angle_between_deg(s.view.direction(), point.direction()) <= radius
        })
        .map(|s| (shot.time_secs - s.time_secs).max(0.0));
    Flick {
        kind,
        target_id: target.id,
        hit: shot.hit,
        distance_deg: angle_between_deg(start_view.direction(), goal.direction()),
        target_radius_deg: target.radius_deg,
        reaction_secs: reaction,
        movement_secs: movement,
        peak_speed_deg_per_sec: peak,
        primary_error_radii: primary_error,
        endpoint_error_deg: along_error_deg(shot.view, goal, dir),
        effective_distance_deg: dot(tangent_offset_deg(start_view, shot.view), dir),
        corrections,
        settle_secs: settle,
        path_efficiency: (travelled > 1e-3).then(|| (straight / travelled).min(1.0)),
        click_delay_secs: click_delay,
    }
}

/// Segment `samples` into one flick per shot.
pub fn flicks(samples: &[Sample]) -> Vec<Flick> {
    let mut out = Vec::new();
    let mut last_shot: Option<(usize, u64, bool)> = None; // (index, target, hit)
    for (i, s) in samples.iter().enumerate() {
        if !s.shot {
            continue;
        }
        let Some(target) = s.aimed_target().copied() else {
            continue;
        };
        let last_shot_secs = last_shot.map_or(0.0, |(j, _, _)| samples[j].time_secs);
        let start_secs = last_shot_secs.max(target.visible_at_secs);
        let first = samples[..i]
            .iter()
            .rposition(|p| p.time_secs <= start_secs)
            .map_or(0, |p| p + 1);
        let start_view = first
            .checked_sub(1)
            .map_or(ViewAngles::default(), |p| samples[p].view);
        let kind = match last_shot {
            Some((_, id, false)) if id == target.id => FlickKind::Correction,
            _ => FlickKind::Fresh,
        };
        out.push(analyze_one(
            &samples[first..=i],
            start_view,
            &target,
            kind,
            start_secs,
        ));
        last_shot = Some((i, target.id, s.hit));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::test_support::record;
    use crate::analytics::trace::trace;
    use crate::scenario::{FrameInput, Recording};
    use crate::sim::AimerProfile;

    fn summary(id: &str, p: usize, seed: u64) -> FlickSummary {
        FlickSummary::from_flicks(&flicks(&trace(&record(id, AimerProfile::PRESETS[p], seed))))
    }

    #[test]
    fn elite_reacts_faster_and_lands_closer_than_recruit() {
        let recruit = summary("flick-wide", 0, 21);
        let elite = summary("flick-wide", 3, 21);
        let r = recruit.median_reaction_secs.unwrap();
        let e = elite.median_reaction_secs.unwrap();
        assert!(e < r, "elite {e} vs recruit {r}");
        // The simulated aimer's reaction delays are 0.20 s and 0.32 s.
        assert!((0.15..0.30).contains(&e), "elite reaction {e}");
        assert!((0.25..0.45).contains(&r), "recruit reaction {r}");
        assert!(
            elite.overshoot_rate + elite.undershoot_rate
                < recruit.overshoot_rate + recruit.undershoot_rate
        );
        let (te, tr) = (
            elite.throughput_bits_per_sec.unwrap(),
            recruit.throughput_bits_per_sec.unwrap(),
        );
        assert!(te > tr, "throughput elite {te} vs recruit {tr}");
    }

    #[test]
    fn single_minimum_jerk_movements_are_straight_and_uncorrected() {
        let s = summary("flick-wide", 2, 4);
        assert!(s.mean_corrections < 0.2, "{s:?}");
        assert!(s.median_path_efficiency.unwrap() > 0.95, "{s:?}");
    }

    /// Drive a session with a scripted view path: flick past the first target, stop, come
    /// back, and click.
    fn overshooting_recording() -> Recording {
        let def = crate::scenario::builtin::all()
            .into_iter()
            .find(|d| d.id == "flick-wide")
            .unwrap();
        let mut rec = Recording::new(def, 9);
        let mut live = rec.start();
        let dt = 1.0 / 240.0;
        let idle = |view| FrameInput {
            dt,
            view,
            ..Default::default()
        };
        // Wait until a target is visible.
        while live.targets().next().is_none() {
            let i = idle(ViewAngles::default());
            rec.inputs.push(i);
            live.update(i);
        }
        let target = live.targets().next().unwrap().clone();
        let goal = ViewAngles::from_direction(target.position - live.eye_position());
        let from = ViewAngles::default();
        let (gy, gp) = (goal.yaw_deg, goal.pitch_deg);
        let past = ViewAngles::new(gy * 1.25, gp * 1.25);
        let push = |rec: &mut Recording, live: &mut crate::scenario::Session, i: FrameInput| {
            rec.inputs.push(i);
            live.update(i);
        };
        for _ in 0..48 {
            push(&mut rec, &mut live, idle(from)); // 0.2 s reaction
        }
        let lerp = |a: ViewAngles, b: ViewAngles, t: f32| {
            let s = t * t * t * (10.0 - 15.0 * t + 6.0 * t * t);
            ViewAngles::new(
                a.yaw_deg + (b.yaw_deg - a.yaw_deg) * s,
                a.pitch_deg + (b.pitch_deg - a.pitch_deg) * s,
            )
        };
        for k in 1..=48 {
            push(&mut rec, &mut live, idle(lerp(from, past, k as f32 / 48.0)));
        }
        for _ in 0..24 {
            push(&mut rec, &mut live, idle(past));
        }
        for k in 1..=24 {
            push(&mut rec, &mut live, idle(lerp(past, goal, k as f32 / 24.0)));
        }
        push(
            &mut rec,
            &mut live,
            FrameInput {
                dt,
                view: goal,
                fire_pressed: true,
                fire_held: true,
                ..Default::default()
            },
        );
        rec
    }

    #[test]
    fn an_overshoot_and_its_correction_are_detected() {
        let fl = flicks(&trace(&overshooting_recording()));
        assert_eq!(fl.len(), 1);
        let f = &fl[0];
        assert!(f.overshot(), "{f:?}");
        assert!(!f.undershot());
        assert_eq!(f.corrections, 1, "{f:?}");
        assert!(f.hit);
        assert!(f.path_efficiency.unwrap() < 0.8, "{f:?}");
        let reaction = f.reaction_secs.unwrap();
        assert!((0.15..0.30).contains(&reaction), "{reaction}");
        assert!(f.settle_secs.unwrap() > 0.1);
    }

    const DT: f32 = 1.0 / 240.0;

    /// Hand-built samples: one fixed target at yaw 10°, radius 1°, visible from the start;
    /// the crosshair follows `yaws` (pitch 0) and the last frame fires.
    fn scripted(yaws: &[f32]) -> Vec<Sample> {
        let target = TargetSample {
            id: 1,
            aim_point: ViewAngles::new(10.0, 0.0),
            radius_deg: 1.0,
            visible_at_secs: 0.0,
        };
        let mut previous = 0.0;
        yaws.iter()
            .enumerate()
            .map(|(i, &yaw)| {
                let last = i + 1 == yaws.len();
                let speed = (yaw - previous).abs() / DT;
                previous = yaw;
                Sample {
                    time_secs: (i + 1) as f32 * DT,
                    dt_secs: DT,
                    view: ViewAngles::new(yaw, 0.0),
                    view_velocity_deg_per_sec: (speed, 0.0),
                    angular_speed_deg_per_sec: speed,
                    targets: vec![target],
                    aimed: Some(0),
                    fire_held: last,
                    shot: last,
                    hit: last && (yaw - 10.0).abs() <= 1.0,
                    on_target: false,
                }
            })
            .collect()
    }

    #[test]
    fn click_delay_runs_from_first_entering_the_target_to_the_shot() {
        // Enters the 1° radius at frame 9 (yaw 9.5) and fires at frame 13.
        let yaws = [
            0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 4.0, 6.0, 8.0, 9.5, 10.0, 10.0, 10.0, 10.0,
        ];
        let fl = flicks(&scripted(&yaws));
        assert_eq!(fl.len(), 1);
        let delay = fl[0]
            .click_delay_secs
            .expect("the crosshair entered the target");
        assert!((delay - 4.0 * DT).abs() < 1e-6, "{delay}");
        assert!(fl[0].hit);
        // Entering, leaving and re-entering still counts from the first entry (frame 6).
        let yaws = [0.0, 0.0, 0.0, 3.0, 6.0, 8.0, 9.2, 11.5, 11.5, 10.5, 10.0];
        let delay = flicks(&scripted(&yaws))[0].click_delay_secs.unwrap();
        assert!((delay - 4.0 * DT).abs() < 1e-6, "{delay}");
    }

    #[test]
    fn click_delay_is_none_when_the_crosshair_never_reached_the_target() {
        let yaws = [0.0, 0.0, 2.0, 4.0, 6.0, 7.0, 7.0];
        let fl = flicks(&scripted(&yaws));
        assert_eq!(fl[0].click_delay_secs, None);
        assert!(!fl[0].hit);
    }

    #[test]
    fn summary_reports_the_median_click_delay_of_flicks_that_reached_the_target() {
        let mut fl = flicks(&scripted(&[0.0, 0.0, 5.0, 9.5, 10.0, 10.0]));
        fl.extend(flicks(&scripted(&[
            0.0, 0.0, 5.0, 9.5, 10.0, 10.0, 10.0, 10.0,
        ])));
        fl.extend(flicks(&scripted(&[0.0, 0.0, 5.0, 7.0])));
        let s = FlickSummary::from_flicks(&fl);
        // Delays 2·DT and 4·DT; the miss that never entered is left out.
        let median = s.median_click_delay_secs.unwrap();
        assert!((median - 3.0 * DT).abs() < 1e-6, "{median}");
        assert_eq!(s.flicks, 3);
        assert!((s.hit_rate - 2.0 / 3.0).abs() < 1e-6);
        assert_eq!(FlickSummary::from_flicks(&[]).median_click_delay_secs, None);
    }

    #[test]
    fn submovement_split_on_reacceleration_without_stopping() {
        let speeds = [0.0, 50.0, 100.0, 60.0, 40.0, 70.0, 30.0, 0.0];
        assert_eq!(submovements(&speeds, 1, 5.0), vec![(1, 4), (4, 7)]);
        let one = [0.0, 50.0, 100.0, 80.0, 40.0, 10.0, 0.0];
        assert_eq!(submovements(&one, 1, 5.0).len(), 1);
    }
}
