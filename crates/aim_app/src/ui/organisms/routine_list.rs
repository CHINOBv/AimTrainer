//! Routine catalogue with a start button per routine, and a resume button for routines
//! split earlier today.

use crate::i18n::{tr, tr_fmt};
use crate::persistence::library::Library;
use crate::ui::UiAction;
use aim_core::training::resume::RoutinePosition;
use aim_core::training::routine::RoutineDef;
use bevy_egui::egui;

/// `personalized`: the generated playlist for this player, if a benchmark exists.
pub fn routine_list(
    ui: &mut egui::Ui,
    lib: &Library,
    personalized: Option<&RoutineDef>,
    resumable: impl Fn(&str) -> Option<RoutinePosition>,
) -> Option<UiAction> {
    let mut action = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        if let Some(p) = personalized {
            ui.heading(tr(&p.name));
            ui.label(tr(&p.description));
            ui.label(runs_line(
                p.run_count(&lib.scenarios),
                p.minutes(&lib.scenarios),
            ));
            if ui.button(tr("▶  Start personalized playlist")).clicked() {
                action = Some(UiAction::StartRoutineDef(Box::new(p.clone())));
            }
            ui.separator();
        }
        for r in &lib.routines {
            ui.heading(tr(&r.name));
            ui.label(tr(&r.description));
            let runs = r.run_count(&lib.scenarios);
            ui.label(runs_line(runs, r.minutes(&lib.scenarios)));
            ui.horizontal(|ui| {
                if ui.button(tr("▶  Start routine")).clicked() {
                    action = Some(UiAction::StartRoutine(r.id.clone()));
                }
                if let Some(p) = resumable(&r.id)
                    && ui
                        .button(tr_fmt(
                            "⏵ Resume at run {}/{}",
                            &[&(p.next_index + 1), &p.total_runs],
                        ))
                        .clicked()
                {
                    action = Some(UiAction::StartRoutineAt {
                        routine: r.id.clone(),
                        level: p.level,
                        index: p.next_index,
                        seed: p.seed,
                    });
                }
            });
            ui.separator();
        }
    });
    action
}

/// "12 runs · ≈9 min" in the current language.
fn runs_line(runs: usize, minutes: f32) -> String {
    tr_fmt("{} runs · ≈{} min", &[&runs, &format!("{minutes:.0}")])
}
