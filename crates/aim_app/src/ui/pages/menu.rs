//! Main menu: scenarios, routines, benchmark and settings tabs.

use crate::game::AppState;
use crate::overlay::{Overlay, background};
use crate::persistence::history::History;
use crate::persistence::settings::{Paths, Settings};
use crate::ui::UiAction;
use crate::ui::atoms::ERROR;
use crate::ui::molecules::tab_bar;
use crate::ui::navigation::Navigator;
use crate::ui::organisms::settings_panels::{self, DpiMeter};
use crate::ui::organisms::{benchmark_panel, routine_list, scenario_browser};
use aim_core::fov;
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

pub struct MenuPage;

impl Plugin for MenuPage {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuState>()
            .add_systems(Update, count_dpi_meter)
            .add_systems(
                EguiPrimaryContextPass,
                menu.run_if(in_state(AppState::Menu)),
            );
    }
}

#[derive(Default, PartialEq, Clone, Copy)]
enum Tab {
    #[default]
    Scenarios,
    Routines,
    Benchmark,
    Settings,
}

const TABS: [(Tab, &str); 4] = [
    (Tab::Scenarios, "Scenarios"),
    (Tab::Routines, "Routines"),
    (Tab::Benchmark, "Benchmark"),
    (Tab::Settings, "Settings"),
];

#[derive(Resource, Default)]
struct MenuState {
    tab: Tab,
    selected: Option<String>,
    dpi_target: u32,
    meter: DpiMeter,
}

fn count_dpi_meter(motion: Res<AccumulatedMouseMotion>, mut state: ResMut<MenuState>) {
    if state.meter.active {
        state.meter.x += motion.delta.x as f64;
        state.meter.y += motion.delta.y as f64;
    }
}

/// Settings plus what is needed to persist and preview them.
#[derive(SystemParam)]
struct SettingsAccess<'w> {
    settings: ResMut<'w, Settings>,
    paths: Res<'w, Paths>,
    clear: ResMut<'w, ClearColor>,
}

fn menu(
    mut contexts: EguiContexts,
    mut state: ResMut<MenuState>,
    mut cfg: SettingsAccess,
    history: Res<History>,
    overlay: Res<Overlay>,
    mut nav: Navigator,
) -> Result {
    if !overlay.visible {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    // egui 0.36 panels are laid out inside a root `Ui` covering the viewport.
    let mut root = egui::Ui::new(
        ctx.clone(),
        "menu-root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    let before = cfg.settings.clone();
    let state = &mut *state;
    let lib = nav.library();
    let mut action: Option<UiAction> = None;

    egui::Panel::top("tabs").show(&mut root, |ui| {
        ui.horizontal(|ui| {
            ui.strong("CustomAimTrainer");
            ui.separator();
            tab_bar(ui, &mut state.tab, &TABS);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Quit").clicked() {
                    action = Some(UiAction::Quit);
                }
                let s = cfg.settings.sensitivity;
                ui.label(format!(
                    "{} {:.3} @ {} DPI · {:.1} cm/360",
                    s.game.display_name(),
                    s.value,
                    s.dpi,
                    s.cm_per_360()
                ));
            });
        });
    });

    if !lib.errors.is_empty() {
        egui::Panel::bottom("errors").show(&mut root, |ui| {
            ui.colored_label(ERROR, "Some files failed to load:");
            for e in &lib.errors {
                ui.monospace(e);
            }
        });
    }

    match state.tab {
        Tab::Scenarios => {
            egui::Panel::left("list")
                .resizable(true)
                .default_size(280.0)
                .show(&mut root, |ui| {
                    scenario_browser::scenario_list(ui, lib, &history, &mut state.selected);
                    ui.separator();
                    ui.small(format!(
                        "Custom scenarios: {}",
                        cfg.paths.scenarios_dir.display()
                    ));
                });
            egui::CentralPanel::default().show(&mut root, |ui| {
                match state.selected.as_deref().and_then(|id| lib.scenario(id)) {
                    Some(def) => {
                        action = action
                            .take()
                            .or(scenario_browser::scenario_details(ui, def, &history))
                    }
                    None => scenario_browser::empty_selection(ui),
                }
            });
        }
        Tab::Routines => {
            egui::CentralPanel::default().show(&mut root, |ui| {
                action = action.take().or(routine_list::routine_list(ui, lib));
                ui.small(format!(
                    "Custom routines: {}",
                    cfg.paths.routines_dir.display()
                ));
            });
        }
        Tab::Benchmark => {
            egui::CentralPanel::default().show(&mut root, |ui| {
                let bests = history.bests();
                action = action.take().or(benchmark_panel::benchmark_panel(
                    ui,
                    &lib.benchmarks,
                    &bests,
                ));
            });
        }
        Tab::Settings => {
            egui::CentralPanel::default().show(&mut root, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    settings_panels::sensitivity_panel(
                        ui,
                        &mut cfg.settings,
                        &mut state.dpi_target,
                        &mut state.meter,
                    );
                    ui.separator();
                    settings_panels::display_panel(ui, &mut cfg.settings);
                    ui.add_space(8.0);
                    ui.small(format!(
                        "Settings file: {}",
                        cfg.paths.settings_file.display()
                    ));
                    ui.small(format!(
                        "VALORANT FOV: {:.2}° vertical (103° horizontal at 16:9, Hor+)",
                        fov::valorant_vertical_fov_deg()
                    ));
                });
            });
        }
    }

    if *cfg.settings != before && cfg.settings.sensitivity.is_valid() {
        cfg.settings.save(&cfg.paths);
        *cfg.clear = background(&cfg.settings, overlay.visible);
    }
    if let Some(action) = action {
        nav.apply(action);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::assert_is_system;
    use bevy::prelude::Result;

    #[test]
    fn systems_have_no_conflicting_access() {
        assert_is_system::<(), Result, _>(super::menu);
        assert_is_system(super::count_dpi_meter);
    }
}
