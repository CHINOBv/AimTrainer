//! Run analysis: flicks, corrections, tracking quality, skill profile and recommendations.
//!
//! Everything here is a pure function of a [`Recording`]: the run is re-simulated
//! ([`trace`]), click runs are split into flicks ([`flick`]), hold runs are measured as
//! tracking ([`tracking`]), and analyses are aggregated into an explainable
//! [`profile::SkillProfile`] using the data-driven [`rules`]. [`skills`] profiles the
//! benchmark subcategories from run history, with weekly trends. [`codec`] stores recordings as replay files, and [`path`]
//! turns one into per-target crosshair paths for the replay viewer.

pub mod codec;
pub mod decision;
pub mod flick;
pub mod path;
pub mod profile;
pub mod rules;
pub mod skills;
mod stats;
pub mod trace;
pub mod tracking;

pub use flick::{Flick, FlickSummary};
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
}

/// Analyze a run as flicks (click scenarios) or tracking (hold scenarios).
pub fn analyze(recording: &Recording) -> RunAnalysis {
    let samples = trace::trace(recording);
    let kind = match recording.def.weapon.fire {
        FireMode::Click => AnalysisKind::Flick(flick::flicks(&samples)),
        FireMode::Hold => AnalysisKind::Tracking(tracking::analyze(&samples)),
    };
    RunAnalysis {
        scenario_id: recording.def.id.clone(),
        category: recording.def.category,
        kind,
        decision: decision::analyze(&samples),
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use crate::scenario::{Recording, builtin};
    use crate::sim::{AimerProfile, SimAimer};

    /// Record a whole built-in scenario played by a simulated aimer at 240 FPS.
    pub fn record(id: &str, profile: AimerProfile, seed: u64) -> Recording {
        let def = builtin::all()
            .into_iter()
            .find(|d| d.id == id)
            .expect("builtin scenario exists");
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
}
