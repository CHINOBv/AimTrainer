//! Run analysis: flicks, corrections, tracking quality, skill profile and recommendations.
//!
//! Everything here is a pure function of a [`Recording`]: the run is re-simulated
//! ([`trace`]), click runs are split into flicks ([`flick`]), hold runs are measured as
//! tracking ([`tracking`]), and analyses are aggregated into an explainable
//! [`profile::SkillProfile`]. [`codec`] stores recordings as replay files.

pub mod codec;
pub mod flick;
pub mod profile;
mod stats;
pub mod trace;
pub mod tracking;

pub use flick::{Flick, FlickSummary};
pub use profile::{Finding, SkillProfile};
pub use tracking::{TrackingAnalysis, TrackingSummary};

use crate::scenario::{Category, FireMode, Recording};

#[derive(Debug, Clone, PartialEq)]
pub enum AnalysisKind {
    Flick(Vec<Flick>),
    Tracking(TrackingAnalysis),
}

/// The analysis of one run.
#[derive(Debug, Clone, PartialEq)]
pub struct RunAnalysis {
    pub scenario_id: String,
    pub category: Category,
    pub kind: AnalysisKind,
}

impl RunAnalysis {
    pub fn flick_summary(&self) -> Option<FlickSummary> {
        match &self.kind {
            AnalysisKind::Flick(f) => Some(FlickSummary::from_flicks(f)),
            AnalysisKind::Tracking(_) => None,
        }
    }

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
