//! Crosshair paths for the replay viewer: where the crosshair went relative to each target.
//!
//! A [`ReplayPath`] splits a run into [`Engagement`]s, one per target the player went for.
//! Each holds the crosshair's per-frame offset from that target's aim point (degrees, in the
//! target's tangent plane, so `(0, 0)` is dead center), the shots fired at it, and a short
//! summary taken from the flick analysis.
//!
//! Click scenarios: an engagement starts when its target could first be aimed at (it became
//! visible, or the previous engagement ended, whichever is later) and ends when the target
//! is killed or a shot goes to another target. Hold scenarios: an engagement is a run of
//! consecutive frames aimed at the same target, with no shot points.

use super::flick::{self, Flick};
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
    let mut shots = Vec::new();
    for s in frames {
        if let Some(t) = s.target(target_id) {
            known = *t;
        }
        let o = offset(known.aim_point, s.view);
        path_deg.push(o);
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
}
