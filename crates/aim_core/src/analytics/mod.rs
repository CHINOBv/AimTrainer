//! Run analysis: flicks, corrections, tracking quality, skill profile and recommendations.
//!
//! Everything here is a pure function of a [`Recording`]: the run is re-simulated
//! ([`trace`]), click runs are split into flicks ([`flick`]), hold runs are measured as
//! tracking ([`tracking`]), and analyses are aggregated into an explainable
//! [`profile::SkillProfile`] using the data-driven [`rules`]. [`skills`] profiles the
//! benchmark subcategories from run history, with weekly trends. [`codec`] stores recordings as replay files, and [`path`]
//! turns one into per-target crosshair paths for the replay viewer.

pub mod audio_cue;
pub mod codec;
pub mod decision;
pub mod direction;
pub mod flick;
pub mod miss_compass;
pub mod moving;
pub mod pace;
pub mod path;
pub mod perception;
pub mod potential;
pub mod priority;
pub mod profile;
pub mod rules;
pub mod sens_suggest;
pub mod skills;
mod stats;
pub mod trace;
pub mod tracking;

pub use direction::{DirectionSummary, DistanceClass, DistanceSplit};
pub use flick::{Flick, FlickSummary};
pub use moving::{MovingAnalysis, MovingSummary};
pub use path::{Engagement, PathSummary, ReplayPath, ShotHeatmap, ShotPoint, SpeedBand};
pub use profile::{Finding, SkillProfile};
pub use skills::BenchmarkProfile;
pub use tracking::{TrackingAnalysis, TrackingSummary};

use crate::scenario::{Category, FireMode, Recording};

/// What was measured in a run: flicks for click scenarios, tracking for hold scenarios.
#[derive(Debug, Clone, PartialEq)]
pub enum AnalysisKind {
    /// Per-shot flick analysis of a click scenario.
    Flick(Vec<Flick>),
    /// Tracking analysis of a hold scenario.
    Tracking(TrackingAnalysis),
}

/// The analysis of one run.
#[derive(Debug, Clone, PartialEq)]
pub struct RunAnalysis {
    /// Id of the analyzed scenario.
    pub scenario_id: String,
    /// Category of the analyzed scenario.
    pub category: Category,
    /// The analysis, by fire mode.
    pub kind: AnalysisKind,
    /// Go/no-go decisions, for scenarios with decoys.
    pub decision: Option<decision::DecisionSummary>,
    /// Firing-while-moving analysis, for scenarios with player movement.
    pub moving: Option<MovingAnalysis>,
    /// Kill or score rate per time window, if the run was long and busy enough.
    pub pace: Option<pace::PaceAnalysis>,
    /// Target prioritization, if more than one target was visible at once.
    pub priority: Option<priority::PriorityAnalysis>,
    /// Near/far split of the run's flicks, or `None` for a tracking run.
    pub distance_split: Option<DistanceSplit>,
    /// Where missed shots went relative to the aim point, if any shot missed a target.
    pub miss_compass: Option<miss_compass::MissCompass>,
    /// The score with the costliest weakness fixed, for click runs where one is worth a tip.
    pub potential: Option<potential::PotentialTip>,
    /// Turn reaction and angular error per sound cue, for sound-localisation runs.
    pub audio_cue: Option<audio_cue::AudioCueAnalysis>,
    /// Per-area detection and false alarms, for peripheral detection runs.
    pub detection: Option<perception::DetectionAnalysis>,
    /// Correct and incorrect picks, for radar-recall runs.
    pub radar_recall: Option<perception::RadarRecall>,
}

impl RunAnalysis {
    /// Flick aggregates, or `None` for a tracking run.
    pub fn flick_summary(&self) -> Option<FlickSummary> {
        match &self.kind {
            AnalysisKind::Flick(f) => Some(FlickSummary::from_flicks(f)),
            AnalysisKind::Tracking(_) => None,
        }
    }

    /// Tracking aggregates, or `None` for a flick run.
    pub fn tracking_summary(&self) -> Option<TrackingSummary> {
        match &self.kind {
            AnalysisKind::Tracking(t) => Some(t.summary()),
            AnalysisKind::Flick(_) => None,
        }
    }

    /// Per-direction flick aggregates, or `None` for a tracking run.
    pub fn direction_summary(&self) -> Option<DirectionSummary> {
        self.distance_split?;
        Some(DirectionSummary::from_flicks(self.classified_flicks()))
    }

    /// Each flick with its near/far class; empty for a tracking run or without a split.
    pub fn classified_flicks(&self) -> impl Iterator<Item = (&Flick, DistanceClass)> {
        let flicks = match (&self.kind, self.distance_split) {
            (AnalysisKind::Flick(f), Some(split)) => Some((f, split)),
            _ => None,
        };
        flicks.into_iter().flat_map(|(f, split)| {
            f.iter()
                .map(move |fl| (fl, split.classify(fl.distance_deg)))
        })
    }
}

/// Analyze a run as flicks (click scenarios) or tracking (hold scenarios).
pub fn analyze(recording: &Recording) -> RunAnalysis {
    let samples = trace::trace(recording);
    let drill = recording.def.targets.perception.as_ref();
    let detection = drill.is_some_and(|p| p.is_detection());
    let kind = match recording.def.weapon.fire {
        // A detection click is not aimed, so it is no flick.
        FireMode::Click if detection => AnalysisKind::Flick(Vec::new()),
        FireMode::Click => AnalysisKind::Flick(flick::flicks(&samples)),
        FireMode::Hold => AnalysisKind::Tracking(tracking::analyze(&samples)),
    };
    let distance_split = match &kind {
        AnalysisKind::Flick(f) => DistanceSplit::for_run(&recording.def, f),
        AnalysisKind::Tracking(_) => None,
    };
    let potential = match &kind {
        AnalysisKind::Flick(f) => potential::potential(f, run_totals(recording, &samples)),
        AnalysisKind::Tracking(_) => None,
    };
    let decision = decision::analyze(&samples);
    let radar_recall = drill
        .filter(|p| p.is_radar())
        .and(decision.as_ref())
        .map(|d| perception::RadarRecall::from_counts(&d.counts));
    RunAnalysis {
        scenario_id: recording.def.id.clone(),
        category: recording.def.category,
        kind,
        decision,
        moving: recording.def.player.movement.then(|| {
            let limit = recording.def.weapon.kind.movement().accuracy_speed_limit();
            moving::analyze(&samples, limit)
        }),
        // Phases change the rate on purpose, so windows would measure the design, not stamina.
        pace: recording
            .def
            .phases
            .is_none()
            .then(|| pace::analyze(&samples, pace_unit(recording.def.weapon.fire)))
            .flatten(),
        priority: priority::analyze(&samples),
        distance_split,
        miss_compass: miss_compass::analyze(&samples),
        potential,
        audio_cue: audio_cue::analyze(&samples),
        detection: detection.then(|| perception::detection(&samples)),
        radar_recall,
    }
}

/// Final score, elapsed time and miss penalty of a traced run.
fn run_totals(recording: &Recording, samples: &[trace::Sample]) -> potential::RunTotals {
    let last = samples.last();
    potential::RunTotals {
        score: last.map_or(0.0, |s| s.score),
        elapsed_secs: last.map_or(0.0, |s| s.time_secs),
        miss_penalty: recording.def.scoring.miss_penalty,
        time_limited: recording.def.end.is_time(),
    }
}

/// Click runs are paced by kills; hold runs by score, which accrues while on target.
fn pace_unit(fire: FireMode) -> pace::PaceUnit {
    match fire {
        FireMode::Click => pace::PaceUnit::Kills,
        FireMode::Hold => pace::PaceUnit::Score,
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use crate::scenario::{Recording, ScenarioDef, builtin};
    use crate::sim::{AimerProfile, SimAimer};

    /// Record a whole built-in scenario played by a simulated aimer at 240 FPS.
    pub fn record(id: &str, profile: AimerProfile, seed: u64) -> Recording {
        let def = builtin::all()
            .into_iter()
            .find(|d| d.id == id)
            .expect("builtin scenario exists");
        record_def(def, profile, seed)
    }

    /// Record a whole run of `def` played by a simulated aimer at 240 FPS.
    pub fn record_def(def: ScenarioDef, profile: AimerProfile, seed: u64) -> Recording {
        let mut rec = Recording::new(def, seed);
        let mut live = rec.start();
        let mut aimer = SimAimer::new(profile, seed);
        while !live.is_finished() {
            let input = aimer.input(&live, 1.0 / 240.0);
            rec.inputs.push(input);
            live.update(input);
        }
        rec
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::AimerProfile;

    #[test]
    fn analysis_is_deterministic_and_matches_the_fire_mode() {
        let rec = test_support::record("micro-adjust", AimerProfile::PRESETS[1], 4);
        let a = analyze(&rec);
        assert_eq!(a, analyze(&rec));
        assert!(matches!(a.kind, AnalysisKind::Flick(ref f) if !f.is_empty()));
        let rec = test_support::record("track-precise", AimerProfile::PRESETS[1], 4);
        assert!(analyze(&rec).tracking_summary().is_some());
    }

    #[test]
    fn pace_counts_kills_in_click_runs_and_points_in_hold_runs() {
        let rec = test_support::record("flick-wide", AimerProfile::PRESETS[2], 4);
        let pace = analyze(&rec).pace.expect("click run has pace");
        assert_eq!(pace.unit, pace::PaceUnit::Kills);
        assert_eq!(pace.windows.len(), pace::WINDOWS);
        let rec = test_support::record("track-precise", AimerProfile::PRESETS[2], 4);
        let pace = analyze(&rec).pace.expect("hold run has pace");
        assert_eq!(pace.unit, pace::PaceUnit::Score);
    }

    fn flick_wide_with(extra: &str) -> crate::scenario::ScenarioDef {
        let src = crate::scenario::builtin::SOURCES
            .iter()
            .find(|(name, _)| *name == "flick-wide.toml")
            .expect("flick-wide exists")
            .1;
        let def = crate::scenario::ScenarioDef::from_toml(&format!("{src}\n{extra}")).unwrap();
        assert!(def.validate().is_empty(), "{:?}", def.validate());
        def
    }

    #[test]
    fn a_kills_ended_run_is_paced_over_the_time_it_lasted_and_prices_no_saved_time() {
        let def = flick_wide_with("[end]\nkind = \"kills\"\nkills = 24");
        let rec = test_support::record_def(def, AimerProfile::PRESETS[1], 4);
        let end = rec.replay(|_, _| {}).time();
        assert!(end < rec.def.duration_secs, "ends at its kills: {end}");
        let a = analyze(&rec);
        let pace = a.pace.expect("24 kills over more than 12 s");
        let last = pace.windows.last().unwrap();
        assert!((last.end_secs - end).abs() < 1e-3, "{last:?} vs {end}");
        let totals = run_totals(&rec, &trace::trace(&rec));
        assert!(!totals.time_limited);
        if let Some(tip) = a.potential {
            assert!(
                (tip.gain.points - tip.gain.penalty_points).abs() < 1e-4,
                "{tip:?}"
            );
        }
        let plain = test_support::record("flick-wide", AimerProfile::PRESETS[1], 4);
        assert!(run_totals(&plain, &trace::trace(&plain)).time_limited);
    }

    #[test]
    fn phased_runs_get_no_pace_analysis() {
        let def = flick_wide_with(
            "[[phases]]\nadvance = { kind = \"kills\", kills = 10 }\n\
             [[phases]]\ntargets = { max_alive = 2 }",
        );
        let rec = test_support::record_def(def, AimerProfile::PRESETS[2], 4);
        assert_eq!(rec.replay(|_, _| {}).phase_index(), Some(1));
        let a = analyze(&rec);
        assert_eq!(a.pace, None);
        assert!(matches!(a.kind, AnalysisKind::Flick(ref f) if !f.is_empty()));
    }

    #[test]
    fn prioritization_is_measured_only_with_several_targets_visible() {
        let rec = test_support::record("switch-wide", AimerProfile::PRESETS[2], 4);
        let p = analyze(&rec)
            .priority
            .expect("switch-wide shows three targets");
        assert_eq!(p.kills, rec.replay(|_, _| {}).stats().kills);
        let rec = test_support::record("flick-wide", AimerProfile::PRESETS[2], 4);
        assert_eq!(analyze(&rec).priority, None);
    }
}
