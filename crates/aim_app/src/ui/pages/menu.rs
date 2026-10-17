//! Main menu: scenarios, routines, benchmark and settings tabs.

use crate::game::AppState;
use crate::overlay::{Overlay, background};
use crate::persistence::adaptive::AdaptiveLevels;
use crate::persistence::history::History;
use crate::persistence::library::Library;
use crate::persistence::method_progress::{DayProgress, MethodProgress};
use crate::persistence::settings::{Paths, Settings};
use crate::ui::UiAction;
use crate::ui::atoms::ERROR;
use crate::ui::molecules::tab_bar;
use crate::ui::navigation::Navigator;
use crate::ui::organisms::progress_panel::{self, ProgressModel};
use crate::ui::organisms::settings_panels::{self, DpiMeter};
use crate::ui::organisms::today_panel::{self, DayModel, RoutineModel, TodayAction, TodayModel};
use crate::ui::organisms::{arena_list, benchmark_panel, routine_list, scenario_browser};
use aim_core::fov;
use aim_core::training::method::{MethodDef, Weekday};
use aim_core::training::playlist;
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use chrono::Datelike;

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
    Today,
    Progress,
    Scenarios,
    Routines,
    Arenas,
    Benchmark,
    Settings,
}

const TABS: [(Tab, &str); 7] = [
    (Tab::Today, "Today"),
    (Tab::Progress, "Progress"),
    (Tab::Scenarios, "Scenarios"),
    (Tab::Routines, "Routines"),
    (Tab::Arenas, "Arenas"),
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

/// Per-player training state shown in the menu.
#[derive(SystemParam)]
struct TrainingState<'w> {
    progress: ResMut<'w, MethodProgress>,
    adaptive: Res<'w, AdaptiveLevels>,
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
    mut training: TrainingState,
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

    let (today, weekday) = local_today();
    let mut today_action: Option<TodayAction> = None;
    match state.tab {
        Tab::Today => {
            egui::CentralPanel::default().show(&mut root, |ui| {
                match today_model(lib, &history, &training.progress, &today, weekday) {
                    Some(model) => today_action = today_panel::today_panel(ui, &model),
                    None => {
                        ui.label("No weekly training method is available.");
                    }
                }
            });
        }
        Tab::Progress => {
            egui::CentralPanel::default().show(&mut root, |ui| {
                let model = progress_model(lib, &history, cfg.settings.daily_goal_minutes);
                action = action.take().or(progress_panel::progress_panel(ui, &model));
            });
        }
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
                        action = action.take().or(scenario_browser::scenario_details(
                            ui,
                            def,
                            &history,
                            training.adaptive.level(&def.id),
                        ))
                    }
                    None => scenario_browser::empty_selection(ui),
                }
            });
        }
        Tab::Routines => {
            egui::CentralPanel::default().show(&mut root, |ui| {
                let personalized = lib.benchmarks.first().map(|b| {
                    playlist::personalized(b, &b.evaluate(&history.bests()), &lib.scenarios, 30.0)
                });
                action =
                    action
                        .take()
                        .or(routine_list::routine_list(ui, lib, personalized.as_ref()));
                ui.small(format!(
                    "Custom routines: {}",
                    cfg.paths.routines_dir.display()
                ));
            });
        }
        Tab::Arenas => {
            egui::CentralPanel::default().show(&mut root, |ui| {
                let arenas = aim_world::arena::builtin::all();
                action = action
                    .take()
                    .or(arena_list::arena_list(ui, &arenas, |id| history.best(id)));
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
    if let Some(t) = today_action {
        action =
            apply_today(t, lib, &history, &mut training.progress, &cfg.paths, &today).or(action);
    }
    if let Some(action) = action {
        nav.apply(action);
    }
    Ok(())
}

/// Scenarios whose readiness is checked: one smooth, one flick, one precision drill.
const READINESS_SCENARIOS: [&str; 3] = ["track-precise", "flick-headshot", "micro-adjust"];
const TREND_WEEKS: u32 = 8;

/// Seconds east of UTC for the player's clock (days start at local midnight).
pub fn local_utc_offset_secs() -> i64 {
    chrono::Local::now().offset().local_minus_utc() as i64
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn progress_model(lib: &Library, history: &History, goal_minutes: f32) -> ProgressModel {
    use aim_core::training::progress;
    let records = history.run_records();
    let (now, offset) = (now_secs(), local_utc_offset_secs());
    let name = |id: &str| lib.scenario(id).map_or(id.to_owned(), |d| d.name.clone());
    let mut readiness = Vec::new();
    let mut readiness_missing = Vec::new();
    for id in READINESS_SCENARIOS {
        match progress::readiness(&records, id, now, offset) {
            Some(r) => readiness.push((name(id), r)),
            None => readiness_missing.push((id.to_owned(), name(id))),
        }
    }
    let bench = lib.benchmarks.first();
    let bests = history.bests();
    ProgressModel {
        goal: progress::daily_goal(&records, goal_minutes, now, offset),
        readiness,
        readiness_missing,
        fatigue: progress::fatigue(progress::current_session(&records)),
        trend: bench.map_or_else(Vec::new, |b| {
            progress::benchmark_trend(b, &records, now, TREND_WEEKS)
        }),
        bests: bench
            .map(|b| {
                b.subcategories
                    .iter()
                    .flat_map(|sub| {
                        sub.scenarios
                            .iter()
                            .map(move |s| (sub.name.clone(), s.id.clone()))
                    })
                    .map(|(sub, id)| (sub, name(&id), bests.get(&id).copied()))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Local date (`YYYY-MM-DD`) and weekday: the method day follows the player's clock.
fn local_today() -> (String, Weekday) {
    let now = chrono::Local::now();
    (
        now.format("%Y-%m-%d").to_string(),
        Weekday::from_monday_index(now.weekday().num_days_from_monday()),
    )
}

/// The method, the tier in effect and the benchmark energy behind the suggestion.
fn method_context<'a>(
    lib: &'a Library,
    history: &History,
    progress: &MethodProgress,
) -> Option<(&'a MethodDef, f32, Option<f32>, String)> {
    let method = lib.methods.first()?;
    let result = method
        .benchmark
        .as_ref()
        .and_then(|id| lib.benchmarks.iter().find(|b| &b.id == id))
        .map(|b| b.evaluate(&history.bests()));
    let energy = result.as_ref().and_then(|r| r.overall);
    let suggested = method.suggested_tier(energy);
    let tier = progress
        .tier
        .as_deref()
        .and_then(|id| method.tier(id))
        .unwrap_or(suggested);
    Some((method, tier.level, energy, tier.id.clone()))
}

fn routine_model(
    lib: &Library,
    progress: &MethodProgress,
    date: &str,
    id: &str,
) -> Option<RoutineModel> {
    let r = lib.routines.iter().find(|r| r.id == id)?;
    Some(RoutineModel {
        id: r.id.clone(),
        name: r.name.clone(),
        description: r.description.clone(),
        runs: r.run_count(&lib.scenarios),
        minutes: r.minutes(&lib.scenarios),
        progress: progress
            .resumable(date, id)
            .map(|d| (d.next_index, d.total_runs)),
    })
}

fn today_model(
    lib: &Library,
    history: &History,
    progress: &MethodProgress,
    date: &str,
    weekday: Weekday,
) -> Option<TodayModel> {
    let (method, _, energy, selected) = method_context(lib, history, progress)?;
    let plan = method.day(weekday);
    let day = match &plan.routine {
        Some(id) => DayModel::Routine(routine_model(lib, progress, date, id)?),
        None => {
            let result = method
                .benchmark
                .as_ref()
                .and_then(|id| lib.benchmarks.iter().find(|b| &b.id == id))
                .map(|b| b.evaluate(&history.bests()));
            let weakest = result
                .as_ref()
                .and_then(|r| method.weakest_day(r))
                .and_then(|d| method.day(d).routine.clone())
                .and_then(|id| routine_model(lib, progress, date, &id));
            DayModel::Flex { weakest }
        }
    };
    Some(TodayModel {
        weekday: weekday.label(),
        method_name: method.name.clone(),
        method_description: method.description.clone(),
        credit: method.credit.clone(),
        tiers: method
            .tiers
            .iter()
            .map(|t| (t.id.clone(), t.name.clone()))
            .collect(),
        selected_tier: selected,
        suggested_tier: method.suggested_tier(energy).id.clone(),
        benchmark_energy: energy,
        day,
    })
}

/// Turn a Today action into progress updates and the navigation action to apply.
fn apply_today(
    action: TodayAction,
    lib: &Library,
    history: &History,
    progress: &mut MethodProgress,
    paths: &Paths,
    date: &str,
) -> Option<UiAction> {
    match action {
        TodayAction::SelectTier(id) => {
            progress.tier = Some(id);
            progress.today = None;
            progress.save(paths);
            None
        }
        TodayAction::Start(routine) => {
            let (_, level, _, _) = method_context(lib, history, progress)?;
            let total_runs = lib
                .routines
                .iter()
                .find(|r| r.id == routine)?
                .run_count(&lib.scenarios);
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos() as u64);
            progress.today = Some(DayProgress {
                date: date.to_owned(),
                routine: routine.clone(),
                level,
                seed,
                next_index: 0,
                total_runs,
            });
            progress.save(paths);
            Some(UiAction::StartRoutineAt {
                routine,
                level,
                index: 0,
                seed,
            })
        }
        TodayAction::Resume(routine) => {
            let d = progress
                .resumable(date, &routine)
                .filter(|d| !d.is_done())?;
            Some(UiAction::StartRoutineAt {
                routine,
                level: d.level,
                index: d.next_index,
                seed: d.seed,
            })
        }
        TodayAction::WarmUp => Some(UiAction::StartRoutine("quick-warmup".to_owned())),
    }
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
