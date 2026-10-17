//! Routine catalogue with a start button per routine.

use crate::persistence::library::Library;
use crate::ui::UiAction;
use bevy_egui::egui;

pub fn routine_list(ui: &mut egui::Ui, lib: &Library) -> Option<UiAction> {
    let mut action = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
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
