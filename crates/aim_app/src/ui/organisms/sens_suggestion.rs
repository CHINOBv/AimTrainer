//! Passive sensitivity suggestion on the Progress page, from the over/undershoot bias of
//! recent click runs. The core computes it ([`aim_core::analytics::sens_suggest`]); this
//! panel caches it per set of replays, shows the evidence and applies only the overall
//! multiplier when the player asks.

use crate::persistence::settings::Settings;
use crate::ui::atoms::{GOOD, WARN};
use aim_core::analytics::sens_suggest::{
    self, AxisSuggestion, MAX_DECREASE_MULTIPLIER, MAX_INCREASE_MULTIPLIER, MIN_SENS_FLICKS,
    NO_CHANGE_BAND, SensSuggestion,
};
use aim_core::analytics::{AnalysisKind, RunAnalysis};
use aim_core::sensitivity::Sensitivity;
use bevy_egui::egui;

/// How many of the newest click-run replays feed the suggestion.
pub const RECENT_RUNS: usize = 20;

/// The suggestion for the current set of replays, computed once per set.
#[derive(Debug, Default)]
pub struct SensSuggestionState {
    /// Identifies the replays the suggestion was built from.
    key: Option<Vec<u64>>,
    /// Click runs analyzed.
    pub runs: usize,
    pub suggestion: Option<SensSuggestion>,
    /// The sensitivity the player applied from this suggestion. Apply stays hidden until new
    /// runs arrive, so it is never applied twice on the same evidence.
    pub applied: Option<Sensitivity>,
}

impl SensSuggestionState {
    /// Recompute when `key` (the replays on disk) changed; `analyze_runs` returns the
    /// analyses of the recent click runs.
    pub fn refresh(&mut self, key: Vec<u64>, analyze_runs: impl FnOnce() -> Vec<RunAnalysis>) {
        if self.key.as_ref() == Some(&key) {
            return;
        }
        let analyses = analyze_runs();
        self.runs = analyses.len();
        self.suggestion = sens_suggest::suggest(analyses.iter().flat_map(|a| match &a.kind {
            AnalysisKind::Flick(f) => f.as_slice(),
            AnalysisKind::Tracking(_) => &[],
        }));
        self.key = Some(key);
        self.applied = None;
    }

    /// The sensitivity to show: the applied one once applied (the settings already hold it,
    /// so multiplying again would show a value nobody chose), otherwise the suggestion.
    pub fn shown_sensitivity(&self, current: Sensitivity, multiplier: f32) -> Sensitivity {
        self.applied
            .unwrap_or_else(|| suggested_sensitivity(current, multiplier))
    }
}

/// `current` with its sensitivity multiplied by `multiplier`, through cm/360 like the
/// sensitivity finder (same game and DPI).
pub fn suggested_sensitivity(current: Sensitivity, multiplier: f32) -> Sensitivity {
    Sensitivity::from_cm_per_360(
        current.game,
        current.cm_per_360() / multiplier as f64,
        current.dpi,
    )
}

fn bias_text(bias: f32) -> String {
    let side = if bias >= 0.0 { "past" } else { "short of" };
    format!("{:.0}% {side} the target", bias.abs() * 100.0)
}

fn axis_line(name: &str, axis: Option<AxisSuggestion>) -> String {
    match axis {
        Some(a) => format!(
            "{name}: first movements end {} on average ({} flicks) → ×{:.2}",
            bias_text(a.mean_bias),
            a.samples,
            a.multiplier
        ),
        None => format!("{name}: fewer than {MIN_SENS_FLICKS} measured flicks"),
    }
}

/// Draw the suggestion and its evidence; "Apply" changes only the overall sensitivity.
pub fn sens_suggestion(
    ui: &mut egui::Ui,
    state: &mut SensSuggestionState,
    settings: &mut Settings,
) {
    ui.heading("Sensitivity suggestion");
    ui.small(format!(
        "From where your first movement ends in your last {RECENT_RUNS} click runs: consistent \
         overshoot suggests a lower sensitivity, undershoot a higher one. Nothing changes unless you \
         apply it. Complements the sensitivity finder in Tools."
    ));
    let Some(s) = state.suggestion else {
        ui.label(format!(
            "Not enough data yet: needs {MIN_SENS_FLICKS} measured flicks ({} click runs analyzed).",
            state.runs
        ));
        return;
    };
    ui.label(format!(
        "Overall: first movements end {} on average ({} flicks from {} runs).",
        bias_text(s.mean_bias),
        s.samples,
        state.runs
    ));
    ui.small(axis_line("Horizontal", s.horizontal));
    ui.small(format!(
        "{} (information only: mouse look uses one sensitivity for both axes)",
        axis_line("Vertical", s.vertical)
    ));
    ui.small(format!(
        "Multipliers are clamped to ×{MAX_DECREASE_MULTIPLIER:.2}–×{MAX_INCREASE_MULTIPLIER:.2}; \
         within ±{:.0}% means no change. Thresholds are provisional.",
        NO_CHANGE_BAND * 100.0
    ));
    if !s.suggests_change() {
        ui.colored_label(GOOD, format!("No change needed (×{:.2}).", s.multiplier));
        return;
    }
    let rec = state.shown_sensitivity(settings.sensitivity, s.multiplier);
    let text = format!(
        "{}: ×{:.2} = {:.1} cm/360 ({} {:.3} at {} DPI)",
        if state.applied.is_some() {
            "Applied"
        } else {
            "Suggested"
        },
        s.multiplier,
        rec.cm_per_360(),
        rec.game.display_name(),
        rec.value,
        rec.dpi
    );
    ui.colored_label(WARN, text);
    if state.applied.is_some() {
        ui.small("Play a few click runs before trusting a new suggestion.");
    } else if ui.button("Apply overall multiplier").clicked() {
        settings.sensitivity = rec;
        state.applied = Some(rec);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::games::Game;

    #[test]
    fn applying_a_multiplier_scales_the_sensitivity_and_keeps_game_and_dpi() {
        let current = Sensitivity::from_cm_per_360(Game::Valorant, 40.0, 800);
        let lower = suggested_sensitivity(current, 0.9);
        assert_eq!((lower.game, lower.dpi), (current.game, current.dpi));
        assert!((lower.value / current.value - 0.9).abs() < 1e-6);
        assert!((lower.cm_per_360() - 40.0 / 0.9).abs() < 1e-4);
        let same = suggested_sensitivity(current, 1.0);
        assert!((same.value - current.value).abs() < 1e-12);
    }

    #[test]
    fn the_suggestion_is_recomputed_only_when_the_recent_runs_change() {
        let mut state = SensSuggestionState::default();
        let mut loads = 0;
        state.refresh(vec![1, 2], || {
            loads += 1;
            Vec::new()
        });
        state.refresh(vec![1, 2], || {
            loads += 1;
            Vec::new()
        });
        assert_eq!(loads, 1);
        assert_eq!((state.runs, state.suggestion), (0, None));
        state.applied = Some(Sensitivity::from_cm_per_360(Game::Valorant, 40.0, 800));
        state.refresh(vec![1, 2, 3], || {
            loads += 1;
            Vec::new()
        });
        assert_eq!(loads, 2);
        assert!(state.applied.is_none(), "new runs re-enable Apply");
    }

    #[test]
    fn after_applying_the_panel_shows_the_applied_value_not_a_second_multiplication() {
        let before = Sensitivity::from_cm_per_360(Game::Valorant, 40.0, 800);
        let mut state = SensSuggestionState::default();
        let applied = state.shown_sensitivity(before, 0.9);
        assert!((applied.cm_per_360() - 40.0 / 0.9).abs() < 1e-4);
        state.applied = Some(applied);
        // The settings now hold the applied value; it must not be multiplied again.
        let shown = state.shown_sensitivity(applied, 0.9);
        assert!((shown.cm_per_360() - applied.cm_per_360()).abs() < 1e-9);
    }
}
