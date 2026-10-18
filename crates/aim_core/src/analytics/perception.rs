//! Peripheral perception: detection of brief flashes in eight screen areas, with false
//! alarms, and the correct and incorrect picks of a radar-recall run.
//!
//! Each flash's area is the [`Compass`] direction of its aim point from the crosshair in
//! the frame it appeared, so it reflects where the player was looking at that moment. A
//! pure function of the traced [`Sample`]s. See "Peripheral perception" in
//! `docs/training-science.md`.

use super::decision::DecisionCounts;
use super::miss_compass::Compass;
use super::stats::median;
use super::trace::{Sample, tangent_offset_deg};

/// Flashes and detections in one screen area.
#[derive(Debug, Clone, PartialEq)]
pub struct AreaDetection {
    /// The area, as a direction from the crosshair.
    pub area: Compass,
    /// Flashes shown there.
    pub flashes: u32,
    /// Flashes clicked while they showed.
    pub detected: u32,
    /// Median seconds from a flash appearing to the click that detected it.
    pub median_reaction_secs: Option<f32>,
}

impl AreaDetection {
    /// Detected flashes over shown flashes, if any flash showed in the area.
    pub fn hit_rate(&self) -> Option<f32> {
        (self.flashes > 0).then(|| self.detected as f32 / self.flashes as f32)
    }
}

/// The detection result of a run.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectionAnalysis {
    /// Per-area results, in [`Compass::ALL`] order.
    pub areas: Vec<AreaDetection>,
    /// Clicks while no flash was showing.
    pub false_alarms: u32,
    /// Trials with no flash that ran their course.
    pub blank_trials: u32,
    /// Median reaction time over every detected flash, seconds.
    pub median_reaction_secs: Option<f32>,
}

impl DetectionAnalysis {
    /// Flashes shown in the run.
    pub fn flashes(&self) -> u32 {
        self.areas.iter().map(|a| a.flashes).sum()
    }

    /// Flashes detected in the run.
    pub fn detected(&self) -> u32 {
        self.areas.iter().map(|a| a.detected).sum()
    }
}

/// One flash followed through the run.
struct Flash {
    id: u64,
    area: Option<Compass>,
    visible_at_secs: f32,
    reaction_secs: Option<f32>,
}

/// Analyze a detection run. A click (a shot) that hit answers the oldest flash on screen;
/// a click that hit nothing is a false alarm.
pub fn detection(samples: &[Sample]) -> DetectionAnalysis {
    let mut flashes: Vec<Flash> = Vec::new();
    let mut false_alarms = 0;
    let mut blank_trials = 0;
    for s in samples {
        for t in &s.targets {
            if !flashes.iter().any(|f| f.id == t.id) {
                flashes.push(Flash {
                    id: t.id,
                    area: Compass::of_offset(tangent_offset_deg(s.view, t.aim_point)),
                    visible_at_secs: t.visible_at_secs,
                    reaction_secs: None,
                });
            }
        }
        blank_trials += s.blank_trials;
        if !s.shot {
            continue;
        }
        if !s.hit {
            false_alarms += 1;
            continue;
        }
        let answered = flashes
            .iter_mut()
            .filter(|f| f.reaction_secs.is_none() && s.target(f.id).is_some())
            .min_by(|a, b| a.visible_at_secs.total_cmp(&b.visible_at_secs));
        if let Some(f) = answered {
            f.reaction_secs = Some(s.time_secs - f.visible_at_secs);
        }
    }
    let areas = Compass::ALL
        .iter()
        .map(|&area| {
            let of: Vec<&Flash> = flashes.iter().filter(|f| f.area == Some(area)).collect();
            let reactions: Vec<f32> = of.iter().filter_map(|f| f.reaction_secs).collect();
            AreaDetection {
                area,
                flashes: of.len() as u32,
                detected: reactions.len() as u32,
                median_reaction_secs: median(&reactions),
            }
        })
        .collect();
    let all: Vec<f32> = flashes.iter().filter_map(|f| f.reaction_secs).collect();
    DetectionAnalysis {
        areas,
        false_alarms,
        blank_trials,
        median_reaction_secs: median(&all),
    }
}

/// The picks of a radar-recall run: shooting a target the radar showed is a correct pick,
/// shooting one it did not show is an incorrect pick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadarRecall {
    /// Targets the radar showed that were shot.
    pub correct_picks: u32,
    /// Targets the radar did not show that were shot.
    pub incorrect_picks: u32,
    /// Targets the radar showed that got away.
    pub missed: u32,
    /// Targets the radar did not show that were rightly left alone.
    pub left_alone: u32,
}

impl RadarRecall {
    /// The picks behind go/no-go outcome counts (radar decoys are the unshown targets).
    pub fn from_counts(c: &DecisionCounts) -> Self {
        Self {
            correct_picks: c.match_hits,
            incorrect_picks: c.nonmatch_hits,
            missed: c.match_waits,
            left_alone: c.nonmatch_waits,
        }
    }

    /// Correct picks over all picks, if any target was shot.
    pub fn pick_accuracy(&self) -> Option<f32> {
        let picks = self.correct_picks + self.incorrect_picks;
        (picks > 0).then(|| self.correct_picks as f32 / picks as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::trace::TargetSample;
    use crate::math::ViewAngles;

    const DT: f32 = 0.05;

    fn flash(id: u64, yaw: f32, pitch: f32, visible_at_secs: f32) -> TargetSample {
        TargetSample {
            id,
            aim_point: ViewAngles::new(yaw, pitch),
            radius_deg: 0.5,
            visible_at_secs,
            decoy: false,
        }
    }

    fn sample(i: usize, targets: Vec<TargetSample>, shot: bool, hit: bool) -> Sample {
        Sample {
            time_secs: (i + 1) as f32 * DT,
            dt_secs: DT,
            targets,
            shot,
            hit,
            ..Default::default()
        }
    }

    #[test]
    fn flashes_are_binned_by_screen_area_and_clicks_are_detections_or_false_alarms() {
        let up = flash(1, 0.0, 10.0, 0.1);
        let left = flash(2, -10.0, 0.0, 0.5);
        let down_right = flash(3, 7.0, -7.0, 1.0);
        let samples = vec![
            sample(0, vec![], true, false), // a false alarm before any flash
            sample(1, vec![up], false, false),
            sample(2, vec![up], false, false),
            sample(3, vec![up], true, true), // detected at 0.2 s, 0.1 s after it appeared
            sample(9, vec![left], false, false),
            sample(10, vec![left], false, false), // missed: no click
            sample(19, vec![down_right], false, false),
            sample(21, vec![down_right], true, true),
            Sample {
                blank_trials: 1,
                ..sample(30, vec![], false, false)
            },
            sample(31, vec![], true, false),
        ];
        let a = detection(&samples);
        assert_eq!((a.false_alarms, a.blank_trials), (2, 1));
        assert_eq!((a.flashes(), a.detected()), (3, 2));
        let area = |c: Compass| a.areas.iter().find(|x| x.area == c).unwrap().clone();
        let up = area(Compass::Up);
        assert_eq!((up.flashes, up.detected), (1, 1));
        assert!(
            (up.median_reaction_secs.unwrap() - 0.1).abs() < 1e-5,
            "{up:?}"
        );
        assert_eq!(up.hit_rate(), Some(1.0));
        let left = area(Compass::Left);
        assert_eq!(
            (left.flashes, left.detected, left.hit_rate()),
            (1, 0, Some(0.0))
        );
        assert_eq!(left.median_reaction_secs, None);
        let dr = area(Compass::DownRight);
        assert!(
            (dr.median_reaction_secs.unwrap() - 0.1).abs() < 1e-5,
            "{dr:?}"
        );
        assert_eq!(area(Compass::Right).hit_rate(), None);
        assert_eq!(a.areas.len(), 8);
        assert_eq!(
            a.areas.iter().map(|x| x.area).collect::<Vec<_>>(),
            Compass::ALL.to_vec()
        );
    }

    #[test]
    fn areas_are_measured_from_where_the_crosshair_was_when_the_flash_appeared() {
        // The crosshair looks 20° right; a flash straight ahead is to its left.
        let mut s = sample(0, vec![flash(1, 0.0, 0.0, 0.0)], false, false);
        s.view = ViewAngles::new(20.0, 0.0);
        let a = detection(&[s]);
        assert_eq!(a.areas[6].area, Compass::Left);
        assert_eq!(a.areas[6].flashes, 1);
    }

    #[test]
    fn recorded_perception_runs_are_analyzed_by_their_drill() {
        use crate::analytics::{analyze, test_support::record};
        use crate::sim::AimerProfile;
        let rec = record("peripheral-detect", AimerProfile::PRESETS[1], 5);
        let stats = rec.replay(|_, _| {}).stats().clone();
        let a = analyze(&rec);
        let d = a.detection.clone().expect("a detection run");
        assert_eq!(d.false_alarms, stats.false_alarms);
        assert_eq!(d.blank_trials, stats.blank_trials);
        assert_eq!(d.detected(), stats.kills);
        assert!(d.false_alarms > 0 && d.blank_trials > 0, "{d:?}");
        assert!(
            d.areas.iter().filter(|x| x.flashes > 0).count() >= 6,
            "{d:?}"
        );
        assert!(d.median_reaction_secs.is_some());
        assert_eq!(
            a.flick_summary().map(|f| f.flicks),
            Some(0),
            "clicks are not flicks"
        );
        assert_eq!(a.radar_recall, None);

        let rec = record("radar-recall", AimerProfile::PRESETS[1], 5);
        let stats = rec.replay(|_, _| {}).stats().clone();
        let a = analyze(&rec);
        let r = a.radar_recall.expect("a radar run");
        assert_eq!(
            (r.correct_picks, r.incorrect_picks, r.missed, r.left_alone),
            (
                stats.kills,
                stats.decoy_hits,
                stats.expired,
                stats.decoy_waits
            )
        );
        assert!(r.correct_picks > r.incorrect_picks, "{r:?}");
        assert_eq!(a.detection, None);
        let plain = analyze(&record("go-no-go", AimerProfile::PRESETS[1], 5));
        assert_eq!((plain.detection, plain.radar_recall), (None, None));
    }

    #[test]
    fn radar_picks_come_from_the_decision_counts() {
        let r = RadarRecall::from_counts(&DecisionCounts {
            match_hits: 6,
            match_waits: 2,
            nonmatch_hits: 2,
            nonmatch_waits: 5,
        });
        assert_eq!(
            (r.correct_picks, r.incorrect_picks, r.missed, r.left_alone),
            (6, 2, 2, 5)
        );
        assert_eq!(r.pick_accuracy(), Some(0.75));
        assert_eq!(
            RadarRecall::from_counts(&DecisionCounts::default()).pick_accuracy(),
            None
        );
    }
}
