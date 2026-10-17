//! Routine catalogue with a start button per routine, and a resume button for routines
//! split earlier today.

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
            ui.heading(&p.name);
            ui.label(&p.description);
            ui.label(format!(
                "{} runs · ≈{:.0} min",
                p.run_count(&lib.scenarios),
                p.minutes(&lib.scenarios)
            ));
            if ui.button("▶  Start personalized playlist").clicked() {
                action = Some(UiAction::StartRoutineDef(Box::new(p.clone())));
            }
            ui.separator();
        }
        for r in &lib.routines {
            ui.heading(&r.name);
            ui.label(&r.description);
            let runs = r.run_count(&lib.scenarios);
            ui.label(format!(
                "{runs} runs · ≈{:.0} min",
                r.minutes(&lib.scenarios)
            ));
            ui.horizontal(|ui| {
                if ui.button("▶  Start routine").clicked() {
                    action = Some(UiAction::StartRoutine(r.id.clone()));
                }
                if let Some(p) = resumable(&r.id)
                    && ui
                        .button(format!(
                            "⏵ Resume at run {}/{}",
                            p.next_index + 1,
                            p.total_runs
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
