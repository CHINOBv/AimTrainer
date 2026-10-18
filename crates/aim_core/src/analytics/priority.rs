//! Target prioritization in scenarios with more than one target visible at once.
//!
//! Two questions: did the player kill the nearest target first, and did the crosshair
//! rest on a target that the player then left without firing? Every threshold here is
//! provisional; see "Target prioritization" in `docs/training-science.md`.

use super::trace::{Sample, TargetSample};
use crate::math::{ViewAngles, angle_between_deg};
use std::collections::BTreeMap;

/// Seconds the crosshair must rest on a target, without firing, before leaving it counts
/// as a missed opportunity.
pub const MISSED_DWELL_SECS: f32 = 0.30;
/// Fewer contested kills than this give no nearest-first share.
pub const MIN_CONTESTED_KILLS: u32 = 5;
/// Fewer kills plus missed opportunities than this give no missed-opportunity rate.
pub const MIN_ENGAGEMENTS: u32 = 5;

/// Prioritization counts of one run, or of several runs merged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PriorityAnalysis {
    /// Targets killed.
    pub kills: u32,
    /// Kills whose engagement started with two or more targets visible.
    pub contested_kills: u32,
    /// Contested kills of the target nearest to the crosshair when the engagement started.
    pub nearest_kills: u32,
    /// Times the crosshair rested on a target for at least [`MISSED_DWELL_SECS`] without
    /// firing and then left it while it was still there.
    pub missed_opportunities: u32,
}

impl PriorityAnalysis {
    /// Share of contested kills that went to the nearest target, once there are at least
    /// [`MIN_CONTESTED_KILLS`].
    pub fn nearest_share(&self) -> Option<f32> {
        (self.contested_kills >= MIN_CONTESTED_KILLS)
            .then(|| self.nearest_kills as f32 / self.contested_kills as f32)
    }

    /// Missed opportunities ÷ (kills + missed opportunities), once that sum reaches
    /// [`MIN_ENGAGEMENTS`].
    pub fn missed_rate(&self) -> Option<f32> {
        let engagements = self.kills + self.missed_opportunities;
        (engagements >= MIN_ENGAGEMENTS)
            .then(|| self.missed_opportunities as f32 / engagements as f32)
    }

    /// Add another run's counts.
    pub fn merge(&mut self, other: &PriorityAnalysis) {
        self.kills += other.kills;
        self.contested_kills += other.contested_kills;
        self.nearest_kills += other.nearest_kills;
        self.missed_opportunities += other.missed_opportunities;
    }
}

/// Prioritization of a traced run, or `None` when no more than one target was ever
/// visible at once.
pub fn analyze(samples: &[Sample]) -> Option<PriorityAnalysis> {
    let kills_in = |i: usize| {
        let before = i.checked_sub(1).map_or(0, |p| samples[p].kills);
        samples[i].kills.saturating_sub(before)
    };
    // A kill frame keeps the dead target next to its respawn, so it does not count.
    let multi = (0..samples.len()).any(|i| kills_in(i) == 0 && samples[i].targets.len() >= 2);
    if !multi {
        return None;
    }
    let mut p = PriorityAnalysis::default();
    let mut first_seen: BTreeMap<u64, usize> = BTreeMap::new();
    let mut engagement_from = 0;
    let mut dwell: Option<Dwell> = None;
    for (i, s) in samples.iter().enumerate() {
        for t in &s.targets {
            first_seen.entry(t.id).or_insert(i);
        }
        let killed = kills_in(i);
        if killed > 0 {
            p.kills += killed;
            if let Some(target) = s.aimed_target() {
                // The engagement starts after the previous kill, or when the target appeared.
                let start = &samples[engagement_from.max(first_seen[&target.id])];
                if start.targets.len() >= 2 && start.target(target.id).is_some() {
                    p.contested_kills += 1;
                    if start.aimed_target().map(|t| t.id) == Some(target.id) {
                        p.nearest_kills += 1;
                    }
                }
            }
            engagement_from = i + 1;
        }
        p.missed_opportunities += u32::from(track_dwell(&mut dwell, s));
    }
    Some(p)
}

/// The crosshair resting on one target.
struct Dwell {
    id: u64,
    secs: f32,
    fired: bool,
}

/// Advance the dwell with frame `s`; true when a dwell just ended as a missed opportunity.
fn track_dwell(dwell: &mut Option<Dwell>, s: &Sample) -> bool {
    let under = s
        .aimed_target()
        .filter(|t| under_crosshair(s.view, t))
        .map(|t| t.id);
    let firing = s.shot || s.fire_held;
    if let (Some(d), Some(id)) = (dwell.as_mut(), under)
        && d.id == id
    {
        d.secs += s.dt_secs;
        d.fired |= firing;
        return false;
    }
    // A target that vanished (killed or expired) was not left by the player.
    let missed = dwell
        .take()
        .is_some_and(|d| !d.fired && d.secs >= MISSED_DWELL_SECS && s.target(d.id).is_some());
    *dwell = under.map(|id| Dwell {
        id,
        secs: s.dt_secs,
        fired: firing,
    });
    missed
}

fn under_crosshair(view: ViewAngles, t: &TargetSample) -> bool {
    angle_between_deg(view.direction(), t.aim_point.direction()) <= t.radius_deg
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::test_support::record;
    use crate::analytics::trace::trace;
    use crate::sim::AimerProfile;

    const FRAME_SECS: f32 = 0.1;

    fn target(id: u64, yaw_deg: f32) -> TargetSample {
        TargetSample {
            id,
            aim_point: ViewAngles::new(yaw_deg, 0.0),
            radius_deg: 1.0,
            visible_at_secs: 0.0,
            decoy: false,
        }
    }

    /// Builds frames one at a time, tracking time and the running kill count.
    #[derive(Default)]
    struct Run {
        samples: Vec<Sample>,
        kills: u32,
    }

    impl Run {
        fn frame(&mut self, yaw_deg: f32, targets: &[TargetSample], fire: bool, kill: bool) {
            self.kills += u32::from(kill);
            let view = ViewAngles::new(yaw_deg, 0.0);
            let aimed = targets
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    (
                        i,
                        angle_between_deg(view.direction(), t.aim_point.direction()),
                    )
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i);
            self.samples.push(Sample {
                time_secs: (self.samples.len() + 1) as f32 * FRAME_SECS,
                dt_secs: FRAME_SECS,
                view,
                targets: targets.to_vec(),
                aimed,
                shot: fire,
                fire_held: fire,
                hit: kill,
                kills: self.kills,
                ..Default::default()
            });
        }

        /// `n` frames looking at `yaw_deg` without firing.
        fn look(&mut self, n: usize, yaw_deg: f32, targets: &[TargetSample]) {
            for _ in 0..n {
                self.frame(yaw_deg, targets, false, false);
            }
        }
    }

    #[test]
    fn single_target_runs_have_no_prioritization() {
        let mut run = Run::default();
        let a = target(1, 10.0);
        run.look(3, 0.0, &[a]);
        // The respawn shares the kill frame with the dead target; that is not two at once.
        let b = target(2, -20.0);
        run.frame(10.0, &[a, b], true, true);
        run.look(3, 10.0, &[b]);
        assert_eq!(analyze(&run.samples), None);
    }

    #[test]
    fn each_contested_kill_checks_the_nearest_target_at_engagement_start() {
        let (a, b, c) = (target(1, 10.0), target(2, 40.0), target(3, -5.0));
        let mut run = Run::default();
        // Engagement 1 starts at yaw 0 with A (10°) and B (40°): A is nearest and dies.
        run.look(3, 0.0, &[a, b]);
        run.frame(10.0, &[a, b], true, true);
        // Engagement 2 starts at yaw 10 with B (30° away) and C (15° away): B dies first.
        run.look(1, 10.0, &[b, c]);
        run.look(2, 25.0, &[b, c]);
        run.frame(40.0, &[b, c], true, true);
        // Engagement 3 has only C visible: a kill, but not contested.
        run.look(2, 20.0, &[c]);
        run.frame(-5.0, &[c], true, true);
        let p = analyze(&run.samples).expect("two targets were visible at once");
        assert_eq!(
            p,
            PriorityAnalysis {
                kills: 3,
                contested_kills: 2,
                nearest_kills: 1,
                missed_opportunities: 0,
            }
        );
    }

    #[test]
    fn a_late_target_does_not_contest_an_engagement_already_under_way() {
        let (a, b, c) = (target(1, 10.0), target(2, 40.0), target(3, 30.0));
        let mut run = Run::default();
        run.look(2, 0.0, &[a, b]);
        run.frame(10.0, &[a, b], true, true);
        // B is alone, then C appears at 30° while the view is at 35°: C is nearest then,
        // but the player kills B, which became available earlier.
        run.look(2, 20.0, &[b]);
        run.look(1, 35.0, &[b, c]);
        run.frame(40.0, &[b, c], true, true);
        let p = analyze(&run.samples).unwrap();
        // B's engagement started right after A's kill, with B alone, so it is uncontested.
        assert_eq!((p.kills, p.contested_kills, p.nearest_kills), (2, 1, 1));
    }

    #[test]
    fn resting_on_a_target_without_firing_and_leaving_it_is_a_missed_opportunity() {
        let (a, b) = (target(1, 10.0), target(2, 40.0));
        let mut run = Run::default();
        run.look(2, 0.0, &[a, b]);
        // 0.4 s on A without firing, then away while A is still there: missed.
        run.look(4, 10.0, &[a, b]);
        run.look(1, 25.0, &[a, b]);
        // 0.2 s on B: too short to count.
        run.look(2, 40.0, &[a, b]);
        run.look(1, 25.0, &[a, b]);
        // 0.4 s on A, firing during it: not missed.
        run.look(2, 10.0, &[a, b]);
        run.frame(10.0, &[a, b], true, false);
        run.look(1, 10.0, &[a, b]);
        run.look(1, 25.0, &[a, b]);
        // 0.4 s on B ended by B disappearing: not left by the player.
        run.look(4, 40.0, &[a, b]);
        run.look(1, 40.0, &[a]);
        // 0.4 s on A at the end of the run: never left.
        run.look(4, 10.0, &[a]);
        let p = analyze(&run.samples).unwrap();
        assert_eq!(p.missed_opportunities, 1);
        assert_eq!(p.kills, 0);
    }

    #[test]
    fn shares_need_enough_kills_and_merge_adds_counts() {
        let mut p = PriorityAnalysis {
            kills: 4,
            contested_kills: 4,
            nearest_kills: 3,
            missed_opportunities: 0,
        };
        assert_eq!(p.nearest_share(), None);
        assert_eq!(p.missed_rate(), None);
        p.merge(&PriorityAnalysis {
            kills: 4,
            contested_kills: 4,
            nearest_kills: 1,
            missed_opportunities: 2,
        });
        assert_eq!(
            p,
            PriorityAnalysis {
                kills: 8,
                contested_kills: 8,
                nearest_kills: 4,
                missed_opportunities: 2,
            }
        );
        assert_eq!(p.nearest_share(), Some(0.5));
        assert_eq!(p.missed_rate(), Some(0.2));
    }

    #[test]
    fn simulated_runs_count_kills_only_in_multi_target_scenarios() {
        let rec = record("switch-wide", AimerProfile::PRESETS[2], 3);
        let stats = rec.replay(|_, _| {}).stats().clone();
        let p = analyze(&trace(&rec)).expect("switch-wide shows three targets");
        assert_eq!(p.kills, stats.kills);
        assert!(
            p.contested_kills > 0 && p.contested_kills <= p.kills,
            "{p:?}"
        );
        assert!(p.nearest_kills <= p.contested_kills, "{p:?}");
        let rec = record("flick-wide", AimerProfile::PRESETS[2], 3);
        assert_eq!(analyze(&trace(&rec)), None);
    }
}
