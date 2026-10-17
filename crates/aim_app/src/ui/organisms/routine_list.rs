//! Routine catalogue with a start button per routine.

use crate::persistence::library::Library;
use crate::ui::UiAction;
use aim_core::training::routine::RoutineDef;
use bevy_egui::egui;

/// `personalized`: the generated playlist for this player, if a benchmark exists.
pub fn routine_list(
    ui: &mut egui::Ui,
    lib: &Library,
    personalized: Option<&RoutineDef>,
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
            if ui.button("▶  Start routine").clicked() {
                action = Some(UiAction::StartRoutine(r.id.clone()));
            }
            ui.separator();
        }
    });
    action
}
