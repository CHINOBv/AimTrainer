//! Scenario list grouped by category, and the details/play panel.

use crate::persistence::history::History;
use crate::persistence::library::Library;
use crate::ui::UiAction;
use crate::ui::molecules::scenario_item;
use crate::ui::organisms::history_chart::history_chart;
use aim_core::scenario::{Category, FireMode, ScenarioDef};
use aim_core::training::store::RunStore;
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

/// Runs drawn in the history chart, and listed as numbers under it.
const CHART_RUNS: usize = 30;
const LISTED_RUNS: usize = 10;

pub fn scenario_details(
    ui: &mut egui::Ui,
    def: &ScenarioDef,
    history: &History,
    adaptive_level: f32,
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
    let charted = history.recent(&def.id, CHART_RUNS);
    let recent = &charted[charted.len().saturating_sub(LISTED_RUNS)..];
    if !recent.is_empty() {
        ui.add_space(6.0);
        history_chart(ui, &charted);
        let list: Vec<String> = recent.iter().map(|s| format!("{s:.0}")).collect();
        ui.label(format!("Last runs: {}", list.join(", ")));
    }
    ui.add_space(10.0);
    let mut action = None;
    ui.horizontal(|ui| {
        if ui
            .add_sized([160.0, 36.0], egui::Button::new("▶  Play"))
            .clicked()
        {
            action = Some(UiAction::StartScenario(def.id.clone()));
        }
        let adaptive = egui::Button::new(format!("▶  Adaptive ({adaptive_level:+.2})"));
        if ui
            .add_sized([200.0, 36.0], adaptive)
            .on_hover_text(
                "Difficulty follows you to keep success near 80%, where learning is fastest.",
            )
            .clicked()
        {
            action = Some(UiAction::StartAdaptive(def.id.clone()));
        }
    });
    action
}

pub fn empty_selection(ui: &mut egui::Ui) {
    ui.heading("Pick a scenario");
    ui.label("Esc pauses a run. Right mouse = ADS zoom for weapons with a scope.");
}
