//! Crosshair paths for the replay viewer: where the crosshair went relative to each target.
//!
//! A [`ReplayPath`] splits a run into [`Engagement`]s, one per target the player went for.
//! Each holds the crosshair's per-frame offset from that target's aim point (degrees, in the
//! target's tangent plane, so `(0, 0)` is dead center), the shots fired at it, and a short
//! summary taken from the flick analysis. Each path point carries the crosshair speed of its
//! frame, classed by [`SpeedBand`] and placed on a colour scale by [`speed_scale`].
//! [`shot_heatmap`] bins a run's shots around a normalized target, and
//! [`ReplayPath::summary`] gives the key counts used to compare two runs.
//!
//! Click scenarios: an engagement starts when its target could first be aimed at (it became
//! visible, or the previous engagement ended, whichever is later) and ends when the target
//! is killed or a shot goes to another target. Hold scenarios: an engagement is a run of
//! consecutive frames aimed at the same target, with no shot points.

use super::flick::{self, Flick};
use super::stats::median;
use super::trace::{self, Sample, tangent_offset_deg};
use crate::math::ViewAngles;
use crate::scenario::{FireMode, Recording};

/// Hold-mode runs shorter than this many frames are aim passing over a target, not an
/// engagement.
pub const MIN_HOLD_ENGAGEMENT_FRAMES: usize = 10;

/// One shot fired during an engagement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShotPoint {
    /// Crosshair offset from the target's aim point at the shot, degrees:
    /// `.0` horizontal (positive right), `.1` vertical (positive up).
    pub offset_deg: (f32, f32),
    /// The shot hit a counted zone.
    pub hit: bool,
}

/// The crosshair's approach to, and shots at, one target.
#[derive(Debug, Clone, PartialEq)]
pub struct Engagement {
    /// Id of the target.
    pub target_id: u64,
    /// Angular radius of the target's aim zone at the last frame, degrees.
    pub radius_deg: f32,
    /// Session time when the engagement started, seconds.
    pub start_secs: f32,
    /// Session time when the engagement ended, seconds.
    pub end_secs: f32,
    /// Crosshair offset from the target's aim point, one per frame, oldest first.
    pub path_deg: Vec<(f32, f32)>,
    /// Crosshair angular speed over each frame of `path_deg` (same length and order),
    /// degrees/s.
    pub speed_deg_per_sec: Vec<f32>,
    /// Shots fired at this target during the engagement, in order.
    pub shots: Vec<ShotPoint>,
    /// The engagement ended because a hit removed the target.
    pub killed: bool,
    /// Any flick of this engagement ended its primary movement past the target.
    pub overshot: bool,
    /// Any flick of this engagement ended its primary movement short of the target.
    pub undershot: bool,
    /// Corrective sub-movements across this engagement's flicks.
    pub corrections: u32,
    /// Reaction time of the engagement's first flick.
    pub reaction_secs: Option<f32>,
}

impl Engagement {
    /// Number of shots in this engagement that missed.
    pub fn misses(&self) -> usize {
        self.shots.iter().filter(|s| !s.hit).count()
    }
}

/// A whole run as engagements.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplayPath {
    /// Id of the scenario the run was played on.
    pub scenario_id: String,
    /// One engagement per target, in time order.
    pub engagements: Vec<Engagement>,
}

/// Key counts of a whole run, for comparing two runs of the same scenario.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathSummary {
    /// Number of engagements.
    pub engagements: usize,
    /// Engagements that ended in a kill.
    pub kills: usize,
    /// Shots fired at engaged targets.
    pub shots: usize,
    /// Shots that hit.
    pub hits: usize,
    /// Engagements with an overshoot.
    pub overshoots: usize,
    /// Engagements with an undershoot.
    pub undershoots: usize,
    /// Corrective sub-movements across all engagements.
    pub corrections: u32,
    /// Median of the engagements' first-flick reaction times, seconds.
    pub median_reaction_secs: Option<f32>,
}

impl ReplayPath {
    /// The run's key counts.
    pub fn summary(&self) -> PathSummary {
        let es = &self.engagements;
        let reactions: Vec<f32> = es.iter().filter_map(|e| e.reaction_secs).collect();
        PathSummary {
            engagements: es.len(),
            kills: es.iter().filter(|e| e.killed).count(),
            shots: es.iter().map(|e| e.shots.len()).sum(),
            hits: es.iter().map(|e| e.shots.len() - e.misses()).sum(),
            overshoots: es.iter().filter(|e| e.overshot).count(),
            undershoots: es.iter().filter(|e| e.undershot).count(),
            corrections: es.iter().map(|e| e.corrections).sum(),
            median_reaction_secs: median(&reactions),
        }
    }
}

/// Crosshair speeds below this are fine adjustment, degrees/s. A display estimate for the
/// replay trail, not a measured VALORANT value.
pub const SLOW_SPEED_DEG_PER_SEC: f32 = 30.0;
/// Crosshair speeds at or above this are a fast flick, degrees/s. A display estimate.
pub const FAST_SPEED_DEG_PER_SEC: f32 = 300.0;

/// Coarse class of a crosshair speed, split at [`SLOW_SPEED_DEG_PER_SEC`] and
/// [`FAST_SPEED_DEG_PER_SEC`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeedBand {
    /// Below the slow limit: fine adjustment or holding still.
    Slow,
    /// Between the limits.
    Medium,
    /// At or above the fast limit: a flick.
    Fast,
}

impl SpeedBand {
    /// The band of `speed_deg_per_sec`.
    pub fn of(speed_deg_per_sec: f32) -> Self {
        if speed_deg_per_sec >= FAST_SPEED_DEG_PER_SEC {
            Self::Fast
        } else if speed_deg_per_sec >= SLOW_SPEED_DEG_PER_SEC {
            Self::Medium
        } else {
            Self::Slow
        }
    }
}

/// Position of a speed on a logarithmic colour scale, `0..=1`. The slow limit maps to 1/3
/// and the fast limit to 2/3, so the scale's thirds are the [`SpeedBand`]s; each third
/// spans a tenfold speed range. Non-finite or tiny speeds map to 0.
pub fn speed_scale(speed_deg_per_sec: f32) -> f32 {
    // Each third is a factor of ten wide, so the scale starts a tenth below the slow limit.
    let floor = SLOW_SPEED_DEG_PER_SEC / 10.0;
    if !speed_deg_per_sec.is_finite() || speed_deg_per_sec <= floor {
        return 0.0;
    }
    ((speed_deg_per_sec / floor).log10() / 3.0).clamp(0.0, 1.0)
}

/// Default heatmap cells per side: odd, so the target center falls in the middle cell.
pub const HEATMAP_BINS: usize = 15;
/// Default heatmap half-width, in target radii.
pub const HEATMAP_EXTENT_RADII: f32 = 3.0;

/// Shot counts on a square grid around the target, in target radii (each shot's offset
/// divided by its engagement's radius), so targets of different sizes share one grid.
///
/// Cells are row-major: row 0 is the top (most positive vertical offset) and column 0 the
/// left. The grid spans `±extent_radii` on both axes.
#[derive(Debug, Clone, PartialEq)]
pub struct ShotHeatmap {
    /// Cells per side.
    pub bins: usize,
    /// Half-width of the grid, in target radii.
    pub extent_radii: f32,
    /// Hits per cell, `bins * bins`.
    pub hits: Vec<u32>,
    /// Misses per cell, `bins * bins`.
    pub misses: Vec<u32>,
    /// Hits that landed beyond the grid.
    pub outside_hits: u32,
    /// Misses that landed beyond the grid.
    pub outside_misses: u32,
}

impl ShotHeatmap {
    /// Index of the cell at `column`, `row`.
    pub fn index(&self, column: usize, row: usize) -> usize {
        row * self.bins + column
    }

    /// Largest hits + misses count of any cell.
    pub fn max_count(&self) -> u32 {
        self.hits
            .iter()
            .zip(&self.misses)
            .map(|(h, m)| h + m)
            .max()
            .unwrap_or(0)
    }

    /// All hits and all misses, including those beyond the grid.
    pub fn total(&self) -> (u32, u32) {
        (
            self.hits.iter().sum::<u32>() + self.outside_hits,
            self.misses.iter().sum::<u32>() + self.outside_misses,
        )
    }

    /// Cell of a normalized offset, or `None` beyond the grid. The outer edges belong to
    /// the outer cells.
    fn cell(&self, x: f32, y: f32) -> Option<usize> {
        let e = self.extent_radii;
        if !(x.abs() <= e && y.abs() <= e) {
            return None;
        }
        let size = 2.0 * e / self.bins as f32;
        let last = self.bins - 1;
        let column = (((x + e) / size) as usize).min(last);
        let row = (((e - y) / size) as usize).min(last);
        Some(self.index(column, row))
    }
}

/// Bin every shot of `engagements` into a `bins × bins` grid spanning `±extent_radii`
/// target radii. Shots of an engagement without a positive radius cannot be normalized and
/// are left out.
pub fn shot_heatmap(engagements: &[Engagement], bins: usize, extent_radii: f32) -> ShotHeatmap {
    let bins = bins.max(1);
    let mut h = ShotHeatmap {
        bins,
        extent_radii,
        hits: vec![0; bins * bins],
        misses: vec![0; bins * bins],
        outside_hits: 0,
        outside_misses: 0,
    };
    for e in engagements.iter().filter(|e| e.radius_deg > 0.0) {
        for s in &e.shots {
            let (x, y) = (s.offset_deg.0 / e.radius_deg, s.offset_deg.1 / e.radius_deg);
            match (h.cell(x, y), s.hit) {
                (Some(i), true) => h.hits[i] += 1,
                (Some(i), false) => h.misses[i] += 1,
                (None, true) => h.outside_hits += 1,
                (None, false) => h.outside_misses += 1,
            }
        }
    }
    h
}

/// Offset of `view` from `target`, degrees, in the target's tangent plane.
fn offset(target: ViewAngles, view: ViewAngles) -> (f32, f32) {
    tangent_offset_deg(target, view)
}

/// Build the engagement for `target_id` over `samples[first..=last]`. Frames where the target
/// is not visible reuse its last known aim point.
fn engagement(samples: &[Sample], first: usize, last: usize, target_id: u64) -> Engagement {
    let frames = &samples[first..=last];
    let mut known = frames
        .iter()
        .find_map(|s| s.target(target_id))
        .copied()
        .expect("an engagement's target is visible in at least one frame");
    let mut path_deg = Vec::with_capacity(frames.len());
    let mut speed_deg_per_sec = Vec::with_capacity(frames.len());
    let mut shots = Vec::new();
    for s in frames {
        if let Some(t) = s.target(target_id) {
            known = *t;
        }
        let o = offset(known.aim_point, s.view);
        path_deg.push(o);
        speed_deg_per_sec.push(s.angular_speed_deg_per_sec);
        if s.shot && s.aimed_target().is_some_and(|t| t.id == target_id) {
            shots.push(ShotPoint {
                offset_deg: o,
                hit: s.hit,
            });
        }
    }
    Engagement {
        target_id,
        radius_deg: known.radius_deg,
        start_secs: frames[0].time_secs - frames[0].dt_secs,
        end_secs: frames[frames.len() - 1].time_secs,
        path_deg,
        speed_deg_per_sec,
        shots,
        killed: false,
        overshot: false,
        undershot: false,
        corrections: 0,
        reaction_secs: None,
    }
}

fn summarize(e: &mut Engagement, flicks: &[Flick]) {
    e.overshot = flicks.iter().any(Flick::overshot);
    e.undershot = flicks.iter().any(Flick::undershot);
    e.corrections = flicks.iter().map(|f| f.corrections).sum();
    e.reaction_secs = flicks.first().and_then(|f| f.reaction_secs);
}

/// The target `id` is gone after frame `i`: removed by this frame, or the run ended here.
fn gone_after(samples: &[Sample], i: usize, id: u64) -> bool {
    samples.get(i + 1).is_none_or(|n| n.target(id).is_none())
}

fn click_engagements(samples: &[Sample]) -> Vec<Engagement> {
    // `flick::flicks` yields one flick per shot that has an aimed target, in order; the loop
    // below visits the same shots, so the n-th such shot owns the n-th flick.
    let flicks = flick::flicks(samples);
    let mut out = Vec::new();
    // (target id, first frame, index of its first flick)
    let mut open: Option<(u64, usize, usize)> = None;
    let mut boundary_secs = 0.0_f32;
    let mut shot_index = 0;
    for (i, s) in samples.iter().enumerate() {
        if !s.shot {
            continue;
        }
        let Some(target) = s.aimed_target().copied() else {
            continue;
        };
        if let Some((id, first, f0)) = open
            && id != target.id
        {
            // Switched away without a kill: end the engagement at the previous frame.
            let mut e = engagement(samples, first, i - 1, id);
            summarize(&mut e, &flicks[f0..shot_index]);
            boundary_secs = e.end_secs;
            out.push(e);
            open = None;
        }
        if open.is_none() {
            let start_secs = boundary_secs.max(target.visible_at_secs);
            let first = samples[..i]
                .iter()
                .rposition(|p| p.time_secs <= start_secs)
                .map_or(0, |p| p + 1);
            open = Some((target.id, first, shot_index));
        }
        shot_index += 1;
        if s.hit && gone_after(samples, i, target.id) {
            let (id, first, f0) = open.take().expect("opened above");
            let mut e = engagement(samples, first, i, id);
            summarize(&mut e, &flicks[f0..shot_index]);
            e.killed = true;
            boundary_secs = e.end_secs;
            out.push(e);
        }
    }
    if let Some((id, first, f0)) = open {
        let last = samples.len() - 1;
        let mut e = engagement(samples, first, last, id);
        summarize(&mut e, &flicks[f0..shot_index]);
        out.push(e);
    }
    out
}

fn hold_engagements(samples: &[Sample]) -> Vec<Engagement> {
    let mut out = Vec::new();
    let mut run: Option<(u64, usize)> = None;
    let close = |out: &mut Vec<Engagement>, id, first, last| {
        if last + 1 - first >= MIN_HOLD_ENGAGEMENT_FRAMES {
            out.push(engagement(samples, first, last, id));
        }
    };
    for (i, s) in samples.iter().enumerate() {
        let aimed = s.aimed_target().map(|t| t.id);
        match (run, aimed) {
            (Some((id, _)), Some(a)) if id == a => {}
            _ => {
                if let Some((id, first)) = run {
                    close(&mut out, id, first, i - 1);
                }
                run = aimed.map(|a| (a, i));
            }
        }
    }
    if let Some((id, first)) = run {
        close(&mut out, id, first, samples.len() - 1);
    }
    out
}

/// Re-simulate `recording` and split it into engagements.
pub fn replay_path(recording: &Recording) -> ReplayPath {
    let samples = trace::trace(recording);
    let engagements = if samples.is_empty() {
        Vec::new()
    } else {
        match recording.def.weapon.fire {
            FireMode::Click => click_engagements(&samples),
            FireMode::Hold => hold_engagements(&samples),
        }
    };
    ReplayPath {
        scenario_id: recording.def.id.clone(),
        engagements,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::test_support::record;
    use crate::sim::AimerProfile;

    fn length(o: (f32, f32)) -> f32 {
        (o.0 * o.0 + o.1 * o.1).sqrt()
    }

    #[test]
    fn a_click_run_gives_one_engagement_per_kill_with_hits_inside_the_radius() {
        for id in ["micro-adjust", "flick-wide"] {
            let rec = record(id, AimerProfile::PRESETS[1], 9);
            let kills = rec.replay(|_, _| {}).stats().kills as usize;
            let path = replay_path(&rec);
            assert_eq!(path.scenario_id, id);
            assert!(kills > 0, "{id}: the sim aimer kills something");
            let killed = path.engagements.iter().filter(|e| e.killed).count();
            assert_eq!(killed, kills, "{id}: one killed engagement per kill");
            for e in &path.engagements {
                assert!(!e.shots.is_empty());
                assert!(!e.path_deg.is_empty() && e.start_secs <= e.end_secs);
                for shot in e.shots.iter().filter(|s| s.hit) {
                    // The aim zone uses atan(r/d), the hit test the true sphere (asin), and
                    // the tangent offset approximates off the horizon: allow a small margin.
                    assert!(
                        length(shot.offset_deg) <= e.radius_deg * 1.15 + 0.05,
                        "{id}: hit at {:?} outside radius {}",
                        shot.offset_deg,
                        e.radius_deg
                    );
                }
                if e.killed {
                    assert!(e.shots.last().unwrap().hit);
                    assert_eq!(e.path_deg.last(), Some(&e.shots.last().unwrap().offset_deg));
                }
            }
        }
    }

    #[test]
    fn engagement_summaries_come_from_the_flick_analysis() {
        let rec = record("flick-wide", AimerProfile::PRESETS[3], 2);
        let flicks = flick::flicks(&trace::trace(&rec));
        let path = replay_path(&rec);
        let shots: usize = path.engagements.iter().map(|e| e.shots.len()).sum();
        assert_eq!(shots, flicks.len());
        let corrections: u32 = path.engagements.iter().map(|e| e.corrections).sum();
        assert_eq!(
            corrections,
            flicks.iter().map(|f| f.corrections).sum::<u32>()
        );
        let overshoots = path.engagements.iter().filter(|e| e.overshot).count();
        assert!(overshoots <= flicks.iter().filter(|f| f.overshot()).count());
        assert_eq!(path, replay_path(&rec), "deterministic");
    }

    #[test]
    fn a_hold_run_gives_aimed_runs_without_shots() {
        let rec = record("track-precise", AimerProfile::PRESETS[1], 4);
        let path = replay_path(&rec);
        assert!(!path.engagements.is_empty());
        assert!(
            path.engagements
                .iter()
                .all(|e| e.shots.is_empty() && e.path_deg.len() >= MIN_HOLD_ENGAGEMENT_FRAMES)
        );
    }

    #[test]
    fn an_empty_recording_has_no_engagements() {
        let rec = Recording::new(crate::scenario::builtin::all().remove(0), 1);
        assert!(replay_path(&rec).engagements.is_empty());
    }

    #[test]
    fn existing_engagement_values_are_unchanged() {
        // Pinned from the baseline before speeds and heatmaps were added.
        let close = |a: f32, b: f32| (a - b).abs() < 1e-3;
        let path = replay_path(&record("micro-adjust", AimerProfile::PRESETS[1], 9));
        let es = &path.engagements;
        assert_eq!(path.scenario_id, "micro-adjust");
        assert_eq!(es.len(), 67);
        assert_eq!(es.iter().map(|e| e.path_deg.len()).sum::<usize>(), 14338);
        assert_eq!(es.iter().map(|e| e.shots.len()).sum::<usize>(), 101);
        assert_eq!(
            es.iter()
                .map(|e| e.shots.iter().filter(|s| s.hit).count())
                .sum::<usize>(),
            67
        );
        assert_eq!(es.iter().map(|e| e.corrections).sum::<u32>(), 174);
        assert_eq!(es.iter().filter(|e| e.overshot).count(), 0);
        assert_eq!(es.iter().filter(|e| e.undershot).count(), 50);
        let e = &es[0];
        assert_eq!((e.target_id, e.path_deg.len(), e.killed), (1, 345, true));
        assert!(close(e.radius_deg, 0.687_516_4));
        assert!(close(e.start_secs, 0.0) && close(e.end_secs, 1.437_505));
        assert!(close(e.path_deg[0].0, 0.437_966_85) && close(e.path_deg[0].1, -4.549_467_6));
        let hits: Vec<bool> = e.shots.iter().map(|s| s.hit).collect();
        assert_eq!(hits, vec![false, false, true]);
        assert!(close(e.shots[2].offset_deg.0, -0.110_272_855));
        assert!(close(e.shots[2].offset_deg.1, 0.515_594));
        assert!(close(e.reaction_secs.unwrap(), 0.35));

        let path = replay_path(&record("track-precise", AimerProfile::PRESETS[1], 4));
        let e = &path.engagements[0];
        assert_eq!(path.engagements.len(), 1);
        assert_eq!(
            (e.path_deg.len(), e.shots.len(), e.killed),
            (14401, 0, false)
        );
        assert!(close(e.radius_deg, 1.705_213_7) && close(e.end_secs, 60.0));
        assert_eq!((e.corrections, e.reaction_secs), (0, None));
    }

    #[test]
    fn each_path_point_has_the_crosshair_speed_of_its_frame() {
        for (id, seed) in [("micro-adjust", 9), ("track-precise", 4)] {
            let rec = record(id, AimerProfile::PRESETS[1], seed);
            let samples = trace::trace(&rec);
            let path = replay_path(&rec);
            for e in &path.engagements {
                assert_eq!(e.speed_deg_per_sec.len(), e.path_deg.len(), "{id}");
                let expected: Vec<f32> = samples
                    .iter()
                    .filter(|s| {
                        s.time_secs > e.start_secs + 1e-6 && s.time_secs <= e.end_secs + 1e-6
                    })
                    .map(|s| s.angular_speed_deg_per_sec)
                    .collect();
                assert_eq!(e.speed_deg_per_sec, expected, "{id}");
            }
            assert!(
                path.engagements
                    .iter()
                    .flat_map(|e| &e.speed_deg_per_sec)
                    .any(|v| *v > 0.0),
                "{id}: the crosshair moved"
            );
        }
    }

    #[test]
    fn the_speed_scale_puts_the_slow_and_fast_limits_at_its_thirds() {
        assert_eq!(speed_scale(0.0), 0.0);
        assert_eq!(speed_scale(f32::NAN), 0.0);
        assert!((speed_scale(SLOW_SPEED_DEG_PER_SEC) - 1.0 / 3.0).abs() < 1e-5);
        assert!((speed_scale(FAST_SPEED_DEG_PER_SEC) - 2.0 / 3.0).abs() < 1e-5);
        assert_eq!(speed_scale(1e6), 1.0);
        assert!(speed_scale(50.0) < speed_scale(100.0));
        assert_eq!(SpeedBand::of(SLOW_SPEED_DEG_PER_SEC - 0.1), SpeedBand::Slow);
        assert_eq!(SpeedBand::of(SLOW_SPEED_DEG_PER_SEC), SpeedBand::Medium);
        assert_eq!(
            SpeedBand::of(FAST_SPEED_DEG_PER_SEC - 0.1),
            SpeedBand::Medium
        );
        assert_eq!(SpeedBand::of(FAST_SPEED_DEG_PER_SEC), SpeedBand::Fast);
    }

    fn shots_at(radius_deg: f32, shots: &[((f32, f32), bool)]) -> Engagement {
        Engagement {
            target_id: 1,
            radius_deg,
            start_secs: 0.0,
            end_secs: 1.0,
            path_deg: vec![(0.0, 0.0)],
            speed_deg_per_sec: vec![0.0],
            shots: shots
                .iter()
                .map(|&(offset_deg, hit)| ShotPoint { offset_deg, hit })
                .collect(),
            killed: false,
            overshot: false,
            undershot: false,
            corrections: 0,
            reaction_secs: None,
        }
    }

    #[test]
    fn the_heatmap_bins_shots_by_target_radii_with_hits_and_misses_apart() {
        // 3 × 3 cells over ±1.5 radii: each cell is one radius wide, row 0 on top.
        let es = [
            shots_at(
                2.0,
                &[((0.0, 0.0), true), ((0.5, -0.5), true), ((2.5, 2.5), false)],
            ),
            shots_at(
                0.5,
                &[
                    ((0.0, 0.1), false),
                    ((-0.6, 0.0), false),
                    ((0.75, -0.75), true),
                    ((5.0, 0.0), true),
                ],
            ),
            // No usable radius: its shots cannot be normalized and are left out.
            shots_at(0.0, &[((0.0, 0.0), true)]),
        ];
        let h = shot_heatmap(&es, 3, 1.5);
        assert_eq!((h.bins, h.extent_radii), (3, 1.5));
        assert_eq!(h.hits, vec![0, 0, 0, 0, 2, 0, 0, 0, 1]);
        assert_eq!(h.misses, vec![0, 0, 1, 1, 1, 0, 0, 0, 0]);
        assert_eq!((h.outside_hits, h.outside_misses), (1, 0));
        assert_eq!(h.index(2, 0), 2);
        assert_eq!(h.max_count(), 3);
        assert_eq!(h.total(), (4, 3));

        let empty = shot_heatmap(&[], 4, 2.0);
        assert_eq!(empty.hits, vec![0; 16]);
        assert_eq!(empty.misses, vec![0; 16]);
        assert_eq!((empty.max_count(), empty.total()), (0, (0, 0)));
    }

    #[test]
    fn the_run_summary_counts_the_engagement_mistakes() {
        let mut a = shots_at(1.0, &[((0.0, 0.0), false), ((0.0, 0.0), true)]);
        a.killed = true;
        a.overshot = true;
        a.corrections = 2;
        a.reaction_secs = Some(0.3);
        let mut b = shots_at(1.0, &[((0.0, 0.0), true)]);
        b.undershot = true;
        b.overshot = true;
        b.corrections = 1;
        b.reaction_secs = Some(0.2);
        let mut c = shots_at(1.0, &[]);
        c.reaction_secs = Some(0.6);
        let path = ReplayPath {
            scenario_id: "x".into(),
            engagements: vec![a, b, c, shots_at(1.0, &[])],
        };
        assert_eq!(
            path.summary(),
            PathSummary {
                engagements: 4,
                kills: 1,
                shots: 3,
                hits: 2,
                overshoots: 2,
                undershoots: 1,
                corrections: 3,
                median_reaction_secs: Some(0.3),
            }
        );
        let empty = ReplayPath {
            scenario_id: "x".into(),
            engagements: vec![],
        };
        assert_eq!(empty.summary().engagements, 0);
        assert_eq!(empty.summary().median_reaction_secs, None);
    }
}
