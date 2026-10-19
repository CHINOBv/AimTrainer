//! End-of-run results: the visual summary first (score, gauges, main fix, charts), the
//! full stats and analysis panels behind a collapsed "Details" section, then routine
//! navigation.

use crate::game::{LastResult, RoutineProgress};
use crate::ui::UiAction;
use crate::ui::molecules::gauge::GaugeModel;
use crate::ui::molecules::stat_row;
use crate::ui::organisms::analysis_panel::{RunReport, analysis_panel};
use crate::ui::organisms::audio_cue_panel::audio_cue_panel;
use crate::ui::organisms::decision_panel::decision_panel;
use crate::ui::organisms::miss_compass::miss_compass;
use crate::ui::organisms::moving_panel::moving_panel;
use crate::ui::organisms::perception_panel::{detection_panel, radar_recall_panel};
use crate::ui::organisms::potential_tip::potential_tip;
use crate::ui::organisms::proof_panel::proof_code;
use crate::ui::organisms::results_visuals::{ScoreHero, VisualsView, results_visuals};
use aim_core::scenario::{Recording, RunResult};
use bevy_egui::egui::{self, Align2};

/// What the card needs beyond the result itself.
pub struct ResultsContext<'a> {
    /// What "Retry" does (scenario at the same level, or the same arena).
    pub retry: UiAction,
    pub routine: Option<&'a RoutineProgress>,
    /// Display name of the next routine run, if there is one.
    pub next_run: Option<&'a str>,
    /// Space was pressed this frame (shortcut for the primary button).
    pub space: bool,
    /// Analysis of this run, computed once per result.
    pub report: Option<&'a RunReport>,
    /// Score and its change against the personal best.
    pub hero: ScoreHero,
    /// Accuracy, then the analysis gauges.
    pub gauges: Vec<GaugeModel>,
}

const ANALYSIS_MAX_HEIGHT_PX: f32 = 280.0;
const DETAILS: &str = "Details";

/// Everything below the visual summary, collapsed by default: the full stats grid, the
/// potential-score estimate and every analysis panel. Returns "Practice" clicks.
fn details(ui: &mut egui::Ui, r: &RunResult, report: Option<&RunReport>) -> Option<UiAction> {
    let mut action = None;
    egui::ScrollArea::vertical()
        .max_height(ANALYSIS_MAX_HEIGHT_PX)
        .show(ui, |ui| {
            egui::Grid::new("stats").num_columns(2).show(ui, |ui| {
                stat_row(ui, "Accuracy", format!("{:.1}%", r.accuracy * 100.0));
                stat_row(
                    ui,
                    "Kills",
                    format!("{} ({:.1}/min)", r.kills, r.kills_per_min),
                );
                if let Some(t) = r.avg_time_to_kill {
                    stat_row(ui, "Avg time to kill", format!("{:.0} ms", t * 1000.0));
                }
                if let Some(spread) = report.and_then(|rep| rep.ttk_spread.as_ref()) {
                    stat_row(ui, "Time to kill (median)", spread.as_str());
                }
                if let Some(e) = report.and_then(|rep| rep.damage_efficiency.as_ref()) {
                    stat_row(ui, "Damage efficiency", e.as_str());
                }
                stat_row(
                    ui,
                    "Headshot rate",
                    format!("{:.1}%", r.headshot_rate * 100.0),
                );
                if r.moving_shots > 0 {
                    stat_row(ui, "Moving shots", r.moving_shots.to_string());
                }
                if r.expired > 0 {
                    stat_row(ui, "Targets missed (expired)", r.expired.to_string());
                }
            });
            let Some(report) = report else {
                return;
            };
            if let Some(line) = &report.potential {
                ui.separator();
                potential_tip(ui, line);
            }
            ui.separator();
            if let Some(a) = analysis_panel(ui, report) {
                action = Some(a);
            }
            if let Some(decision) = &report.decision {
                decision_panel(ui, decision);
            }
            if let Some(grid) = &report.miss_compass {
                miss_compass(ui, grid);
            }
            if let Some(m) = &report.moving {
                moving_panel(ui, m);
            }
            if let Some(cues) = &report.audio_cue {
                audio_cue_panel(ui, cues);
            }
            if let Some(d) = &report.detection {
                detection_panel(ui, d);
            }
            if let Some(r) = &report.radar_recall {
                radar_recall_panel(ui, r);
            }
        });
    action
}

/// "Copy proof code": copies the run's share code to the clipboard and says so. The
/// message is kept per result in egui's memory.
fn proof_button(ui: &mut egui::Ui, rec: &Recording, result: &RunResult) {
    let id = egui::Id::new(("proof-status", rec.seed, rec.inputs.len()));
    ui.horizontal(|ui| {
        if ui
            .button("Copy proof code")
            .on_hover_text("A code a friend can paste in Progress → Verify a proof to replay this run and confirm its score.")
            .clicked()
        {
            let status = match proof_code(rec, result) {
                Ok(code) => {
                    ui.ctx().copy_text(code);
                    "Proof code copied to the clipboard.".to_owned()
                }
                Err(e) => e,
            };
            ui.data_mut(|d| d.insert_temp(id, status));
        }
        if let Some(status) = ui.data(|d| d.get_temp::<String>(id)) {
            ui.small(status);
        }
    });
}

pub fn results_card(
    ctx: &egui::Context,
    last: &LastResult,
    rc: &ResultsContext,
) -> Option<UiAction> {
    let r = &last.result;
    let mut action = None;
    egui::Window::new("Results")
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.heading(&last.name);
            let can_watch = last
                .recording
                .as_ref()
                .is_some_and(|r| !r.inputs.is_empty());
            let view = VisualsView {
                hero: &rc.hero,
                gauges: &rc.gauges,
                report: rc.report,
                can_watch,
            };
            if let Some(a) = results_visuals(ui, &view) {
                action = Some(a);
            }
            for note in &last.notes {
                ui.label(note);
            }
            ui.separator();
            egui::CollapsingHeader::new(DETAILS)
                .default_open(false)
                .show(ui, |ui| {
                    if let Some(a) = details(ui, r, rc.report) {
                        action = Some(a);
                    }
                    if let Some(rec) = last.recording.as_ref().filter(|r| !r.inputs.is_empty()) {
                        proof_button(ui, rec, r);
                    }
                });
            ui.separator();
            if let Some(p) = rc.routine {
                ui.label(format!("{} · run {}/{}", p.name, p.index + 1, p.plan.len()));
                if let Some(note) = p
                    .plan
                    .get(p.index + 1)
                    .map(|n| &n.note)
                    .filter(|n| !n.is_empty())
                {
                    ui.small(format!("Next: {note}"));
                }
            }
            ui.horizontal(|ui| {
                if let Some(name) = rc.next_run {
                    if ui.button(format!("▶ Next: {name} (Space)")).clicked() || rc.space {
                        action = Some(UiAction::NextRoutineRun);
                    }
                } else if let Some(level) = last.adaptive_next {
                    let base = aim_core::scenario::parse_run_key(&r.scenario_id)
                        .0
                        .to_owned();
                    ui.label(format!("Adaptive level → {level:+.2}"));
                    if ui.button("▶ Next adaptive run (Space)").clicked() || rc.space {
                        action = Some(UiAction::StartAdaptive(base));
                    }
                } else if ui.button("↻ Retry (Space)").clicked() || rc.space {
                    action = Some(rc.retry.clone());
                }
                if ui.button("Menu").clicked() {
                    action = Some(UiAction::BackToMenu);
                }
            });
        });
    action
}
