//! Main menu: scenarios, routines, benchmark and settings tabs.

use crate::audio::SoundPreview;
use crate::game::AppState;
use crate::game::FinderState;
use crate::overlay::{Overlay, background};
use crate::persistence::adaptive::AdaptiveLevels;
use crate::persistence::history::History;
use crate::persistence::library::Library;
use crate::persistence::method_progress::{DayProgress, MethodProgress};
use crate::persistence::missions::Missions;
use crate::persistence::replays;
use crate::persistence::settings::{Paths, Settings};
use crate::platform::frame_stats::FrameStats;
use crate::platform::hotkey::HotkeyCapture;
use crate::ui::UiAction;
use crate::ui::atoms::ERROR;
use crate::ui::molecules::tab_bar;
use crate::ui::navigation::Navigator;
use crate::ui::organisms::profile_code::{self, ProfileImport};
use crate::ui::organisms::progress_panel::{self, ProgressModel};
use crate::ui::organisms::replay_viewer::{self, ReplayViewerState};
use crate::ui::organisms::sens_converter::{ConverterState, sens_converter};
use crate::ui::organisms::sens_finder;
use crate::ui::organisms::sens_suggestion::{self, RECENT_RUNS, SensSuggestionState};
use crate::ui::organisms::settings_panels::{self, DpiMeter};
use crate::ui::organisms::system_check::{self, SystemModel};
use crate::ui::organisms::today_panel::{
    self, ChallengeModel, DayModel, MissionModel, MissionsModel, RoutineModel, TodayAction,
    TodayModel,
};
use crate::ui::organisms::{
    arena_list, benchmark_panel, feedback_settings, routine_list, scenario_browser,
};
use aim_core::fov;
use aim_core::training::method::{MethodDef, Weekday};
use aim_core::training::playlist;
use aim_core::training::resume::PositionStore;
use aim_core::training::store::RunStore;
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
            .add_systems(OnExit(AppState::Menu), stop_recording_on_exit)
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
    Tools,
    Settings,
}

const TABS: [(Tab, &str); 8] = [
    (Tab::Today, "Today"),
    (Tab::Progress, "Progress"),
    (Tab::Scenarios, "Scenarios"),
    (Tab::Routines, "Routines"),
    (Tab::Arenas, "Arenas"),
    (Tab::Benchmark, "Benchmark"),
    (Tab::Tools, "Tools"),
    (Tab::Settings, "Settings"),
];

#[derive(Resource, Default)]
struct MenuState {
    tab: Tab,
    selected: Option<String>,
    dpi_target: u32,
    meter: DpiMeter,
    profile_import: ProfileImport,
    replays: ReplayViewerState,
    converter: Option<ConverterState>,
    sens_suggestion: SensSuggestionState,
}

fn count_dpi_meter(motion: Res<AccumulatedMouseMotion>, mut state: ResMut<MenuState>) {
    if state.meter.active {
        state.meter.x += motion.delta.x as f64;
        state.meter.y += motion.delta.y as f64;
    }
}

/// Leave hotkey recording (it unregisters every global hotkey) once the hotkey rows are
/// no longer on screen.
fn stop_recording(capture: &mut HotkeyCapture) {
    if capture.recording.is_some() {
        capture.recording = None;
    }
}

fn stop_recording_on_exit(mut capture: ResMut<HotkeyCapture>) {
    stop_recording(&mut capture);
}

/// Per-player training state shown in the menu.
#[derive(SystemParam)]
struct TrainingState<'w> {
    progress: ResMut<'w, MethodProgress>,
    adaptive: Res<'w, AdaptiveLevels>,
    finder: ResMut<'w, FinderState>,
    frames: Res<'w, FrameStats>,
    missions: ResMut<'w, Missions>,
}

/// Settings plus what is needed to persist and preview them.
#[derive(SystemParam)]
struct SettingsAccess<'w> {
    settings: ResMut<'w, Settings>,
    paths: Res<'w, Paths>,
    clear: ResMut<'w, ClearColor>,
    hotkey_capture: ResMut<'w, HotkeyCapture>,
    sound_previews: MessageWriter<'w, SoundPreview>,
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
        stop_recording(&mut cfg.hotkey_capture);
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
            training.missions.refresh(
                &today,
                local_day_number(),
                cfg.settings.mission_commitment,
                &cfg.paths,
            );
            let missions = missions_model(lib, &training.missions, cfg.settings.mission_commitment);
            egui::CentralPanel::default().show(&mut root, |ui| {
                match today_model(lib, &history, &training.progress, &today, weekday, missions) {
                    Some(model) => today_action = today_panel::today_panel(ui, &model),
                    None => {
                        ui.label("No weekly training method is available.");
                    }
                }
            });
        }
        Tab::Progress => {
            egui::CentralPanel::default().show(&mut root, |ui| {
                egui::ScrollArea::vertical().id_salt("progress").show(ui, |ui| {
                    let model = progress_model(lib, &history, cfg.settings.daily_goal_minutes);
                    action = action.take().or(progress_panel::progress_panel(ui, &model));
                    if let Some(bench) = lib.benchmarks.first() {
                        let skills = aim_core::analytics::BenchmarkProfile::build(
                            bench,
                            history.runs(),
                            now_secs(),
                            TREND_WEEKS,
                        );
                        let name =
                            |id: &str| lib.scenario(id).map_or(id.to_owned(), |d| d.name.clone());
                        action = action
                            .take()
                            .or(progress_panel::skills_section(ui, &skills, name));
                    }
                    ui.separator();
                    // One directory scan per frame feeds both the suggestion and the replays.
                    let entries = replays::list(&cfg.paths);
                    state.sens_suggestion.refresh(
                        entries.iter().map(|e| e.timestamp_millis).collect(),
                        || recent_click_analyses(&entries),
                    );
                    sens_suggestion::sens_suggestion(
                        ui,
                        &mut state.sens_suggestion,
                        &mut cfg.settings,
                    );
                    ui.separator();
                    ui.heading("Replays");
                    ui.small("Your crosshair path around each target: overshoot, corrections and misses at a glance.");
                    replay_viewer::replay_viewer(ui, &mut state.replays, &entries, |e| {
                        replays::load(&e.path).ok().map(|r| aim_core::analytics::path::replay_path(&r))
                    });
                });
            });
        }
        Tab::Scenarios => {
            egui::Panel::left("list")
                .resizable(true)
                .default_size(280.0)
                .show(&mut root, |ui| {
                    // Pinned below the list, which scrolls and would otherwise push it off-screen.
                    egui::Panel::bottom("custom_scenarios").show(ui, |ui| {
                        ui.small(format!(
                            "Custom scenarios: {}",
                            cfg.paths.scenarios_dir.display()
                        ));
                    });
                    scenario_browser::scenario_list(ui, lib, &history, &mut state.selected);
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
                action = action.take().or(routine_list::routine_list(
                    ui,
                    lib,
                    personalized.as_ref(),
                    |id| {
                        // A routine edited since it was split may have another run count.
                        let runs = lib
                            .routines
                            .iter()
                            .find(|r| r.id == id)
                            .map_or(0, |r| r.run_count(&lib.scenarios));
                        training
                            .progress
                            .resumable(&crate::persistence::method_progress::local_date(), id)
                            .filter(|p| p.total_runs == runs)
                            .cloned()
                    },
                ));
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
        Tab::Tools => {
            egui::CentralPanel::default().show(&mut root, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Sensitivity converter");
                    let converter = state.converter.get_or_insert_with(|| {
                        ConverterState::from_sensitivity(cfg.settings.sensitivity)
                    });
                    sens_converter(ui, &mut cfg.settings, converter);
                    ui.separator();
                    action = action.take().or(sens_finder::sens_finder(
                        ui,
                        &mut training.finder.0,
                        &mut cfg.settings,
                    ));
                    ui.separator();
                    let system = SystemModel {
                        frames: training.frames.summary(),
                        vsync_off: true,
                        fullscreen: cfg.settings.fullscreen,
                        wayland: crate::platform::is_wayland(),
                        windows: cfg!(windows),
                    };
                    system_check::system_check(ui, &system);
                });
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
                    settings_panels::display_panel(
                        ui,
                        &mut cfg.settings,
                        &mut cfg.hotkey_capture.recording,
                    );
                    ui.separator();
                    if let Some(clip) = feedback_settings::sound_panel(ui, &mut cfg.settings.sound)
                    {
                        cfg.sound_previews.write(SoundPreview(clip));
                    }
                    ui.separator();
                    let crosshair = cfg.settings.crosshair.color();
                    feedback_settings::hit_feedback_panel(
                        ui,
                        &mut cfg.settings.hit_feedback,
                        crosshair,
                    );
                    ui.separator();
                    profile_code::profile_code(ui, &mut cfg.settings, &mut state.profile_import);
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

    if state.tab != Tab::Settings {
        stop_recording(&mut cfg.hotkey_capture);
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

/// Analyses of the newest [`RECENT_RUNS`] click-run replays (`entries` is newest first),
/// for the sensitivity suggestion. Unreadable files are skipped.
fn recent_click_analyses(
    entries: &[replays::ReplayEntry],
) -> Vec<aim_core::analytics::RunAnalysis> {
    entries
        .iter()
        .filter_map(|e| replays::load(&e.path).ok())
        .filter(|r| r.def.weapon.fire == aim_core::scenario::FireMode::Click)
        .take(RECENT_RUNS)
        .map(|r| aim_core::analytics::analyze(&r))
        .collect()
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
    let records = history.runs();
    let (now, offset) = (now_secs(), local_utc_offset_secs());
    let name = |id: &str| lib.scenario(id).map_or(id.to_owned(), |d| d.name.clone());
    let mut readiness = Vec::new();
    let mut readiness_missing = Vec::new();
    for id in READINESS_SCENARIOS {
        match progress::readiness(records, id, now, offset) {
            Some(r) => readiness.push((name(id), r)),
            None => readiness_missing.push((id.to_owned(), name(id))),
        }
    }
    let bench = lib.benchmarks.first();
    let bests = history.bests();
    ProgressModel {
        goal: progress::daily_goal(records, goal_minutes, now, offset),
        readiness,
        readiness_missing,
        fatigue: progress::fatigue(progress::current_session(records)),
        trend: bench.map_or_else(Vec::new, |b| {
            progress::benchmark_trend(b, records, now, TREND_WEEKS)
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
        crate::persistence::method_progress::local_date(),
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

/// Today's daily challenge (UTC day, so it is the same for every player).
pub fn todays_challenge(lib: &Library) -> Option<aim_core::training::challenge::DailyChallenge> {
    aim_core::training::challenge::daily((now_secs() / 86_400) as i64, &lib.scenarios)
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
            .method_day(date, id)
            .map(|d| (d.next_index, d.total_runs)),
    })
}

fn today_model(
    lib: &Library,
    history: &History,
    progress: &MethodProgress,
    date: &str,
    weekday: Weekday,
    missions: MissionsModel,
) -> Option<TodayModel> {
    let (method, _, energy, selected) = method_context(lib, history, progress)?;
    let plan = method.day(weekday);
    let day = match &plan.routine {
        Some(id) => DayModel::Routine(routine_model(lib, progress, date, id)?),
        None => {
            // The flex day's weakest skill comes from the benchmark tier the player trains at.
            let tier = method
                .tier(&selected)
                .unwrap_or_else(|| method.suggested_tier(energy));
            let result = method
                .benchmark_for(tier, &lib.benchmarks)
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
        challenge: todays_challenge(lib).map(|c| ChallengeModel {
            scenario_name: lib
                .scenario(&c.scenario_id)
                .map_or(c.scenario_id.clone(), |d| d.name.clone()),
            best_today: history.best(&c.run_key()),
        }),
        missions,
    })
}

/// Local day number (days since 0001-01-01), the seed input for today's missions.
fn local_day_number() -> i64 {
    i64::from(chrono::Local::now().date_naive().num_days_from_ce())
}

/// Today's missions as the Today page shows them.
fn missions_model(
    lib: &Library,
    missions: &Missions,
    commitment: aim_core::training::missions::Commitment,
) -> MissionsModel {
    let list = missions.0.today.as_ref().map_or(&[][..], |d| &d.missions);
    MissionsModel {
        commitment: commitment.label(),
        missions: list
            .iter()
            .map(|m| MissionModel {
                scenario_id: m.scenario.clone(),
                scenario_name: lib
                    .scenario(&m.scenario)
                    .map_or(m.scenario.clone(), |d| d.name.clone()),
                conditions: m.conditions.iter().map(|c| c.describe()).collect(),
                mastery: m.mastery,
                done: m.done,
                best_met: m.best_met,
            })
            .collect(),
    }
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
                .method_day(date, &routine)
                .filter(|d| !d.is_done())?;
            Some(UiAction::StartRoutineAt {
                routine,
                level: d.level,
                index: d.next_index,
                seed: d.seed,
            })
        }
        TodayAction::WarmUp => Some(UiAction::StartRoutine("quick-warmup".to_owned())),
        TodayAction::PlayMission(scenario) => Some(UiAction::StartScenario(scenario)),
        TodayAction::Challenge => todays_challenge(lib).map(|c| UiAction::StartChallenge {
            key: c.run_key(),
            scenario: c.scenario_id,
            seed: c.seed,
        }),
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
        assert_is_system(super::stop_recording_on_exit);
    }
}
