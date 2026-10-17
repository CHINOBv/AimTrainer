//! Scenario list grouped by category, and the details/play panel.

use crate::persistence::history::History;
use crate::persistence::library::Library;
use crate::ui::UiAction;
use crate::ui::molecules::scenario_item;
use aim_core::scenario::{Category, FireMode, ScenarioDef};
use bevy_egui::egui;

pub fn scenario_list(
    ui: &mut egui::Ui,
    lib: &Library,
    history: &History,
    selected: &mut Option<String>,
) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        for cat in Category::ALL {
            let items: Vec<&ScenarioDef> =
                lib.scenarios.iter().filter(|d| d.category == cat).collect();
            if items.is_empty() {
                continue;
            }
            ui.heading(cat.label());
            for d in items {
                let is_selected = selected.as_deref() == Some(d.id.as_str());
                if scenario_item(ui, &d.name, history.best(&d.id), is_selected) {
                    *selected = Some(d.id.clone());
                }
            }
            ui.add_space(6.0);
        }
    });
}

pub fn scenario_details(
    ui: &mut egui::Ui,
    def: &ScenarioDef,
    history: &History,
) -> Option<UiAction> {
    ui.heading(&def.name);
    let fire = if def.weapon.fire == FireMode::Hold {
        "hold to fire"
    } else {
        "click"
    };
    ui.label(format!(
        "{} · {:.0}s · {} · {fire}",
        def.category.label(),
        def.duration_secs,
        def.weapon.kind.stats().display_name,
    ));
    ui.add_space(6.0);
    ui.label(&def.description);
    if def.player.movement {
        ui.label("Movement: WASD (Shift = walk). Counter-strafe before shooting.");
    }
    let recent = history.recent(&def.id, 10);
    if !recent.is_empty() {
        ui.add_space(6.0);
        let list: Vec<String> = recent.iter().map(|s| format!("{s:.0}")).collect();
        ui.label(format!("Last runs: {}", list.join(", ")));
    }
    ui.add_space(10.0);
    ui.add_sized([160.0, 36.0], egui::Button::new("▶  Play"))
        .clicked()
        .then(|| UiAction::StartScenario(def.id.clone()))
}

pub fn empty_selection(ui: &mut egui::Ui) {
    ui.heading("Pick a scenario");
    ui.label("Esc pauses a run. Right mouse = ADS zoom for weapons with a scope.");
}
