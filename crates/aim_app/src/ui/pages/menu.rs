//! Main menu: scenarios, routines, benchmark and settings tabs.

use crate::audio::SoundPreview;
use crate::game::AppState;
use crate::game::FinderState;
use crate::game::{ActiveRoutine, start_run};
use crate::overlay::{Overlay, background};
use crate::persistence::adaptive::AdaptiveLevels;
use crate::persistence::coach::context::{ContextData, load_context};
use crate::persistence::coach::{self, TurnRequest};
use crate::persistence::history::{self, History};
use crate::persistence::journal::{self, JournalStore};
use crate::persistence::library::Library;
use crate::persistence::method_progress::{DayProgress, MethodProgress};
use crate::persistence::missions::Missions;
use crate::persistence::replays;
use crate::persistence::settings::{Paths, Settings};
use crate::platform::frame_stats::FrameStats;
use crate::platform::hotkey::HotkeyCapture;
use crate::ui::UiAction;
use crate::ui::atoms::{ERROR, GOOD};
use crate::ui::molecules::{SettingsRows, tab_bar};
use crate::ui::navigation::Navigator;
use crate::ui::organisms::coach_panel::{self, CardOutcome, CoachAction, CoachState};
use crate::ui::organisms::dpi_check::DpiMeter;
use crate::ui::organisms::journal_panel;
use crate::ui::organisms::profile_code::{self, ProfileImport};
use crate::ui::organisms::progress_panel::{self, ProgressModel};
use crate::ui::organisms::replay_viewer::{self, ReplayViewerState};
use crate::ui::organisms::sens_converter::{ConverterState, sens_converter};
use crate::ui::organisms::sens_finder;
use crate::ui::organisms::sens_suggestion::{self, RECENT_RUNS, SensSuggestionState};
use crate::ui::organisms::settings_panels::{self, SettingsTab};
use crate::ui::organisms::system_check::{self, SystemModel};
use crate::ui::organisms::today_panel::{
    self, ChallengeModel, DayModel, MissionModel, MissionsModel, QuickModel, RoutineModel,
    TodayAction, TodayModel,
};
use crate::ui::organisms::{
    arena_list, benchmark_panel, feedback_settings, first_run, routine_list, scenario_browser,
};
use aim_core::fov;
use aim_core::scenario::ScenarioDef;
use aim_core::training::method::{MethodDef, Weekday};
use aim_core::training::playlist;
use aim_core::training::resume::{PositionStore, RoutinePosition};
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
            .init_resource::<SettingsSaver>()
            .init_resource::<CoachPlay>()
            .add_systems(
                Update,
                (
                    count_dpi_meter.run_if(in_state(AppState::Menu)),
                    save_settings,
                    start_coach_scenario,
                ),
            )
            .add_systems(Update, journal::poll_journal)
            .add_systems(
                Last,
                (flush_settings_on_exit, journal::flush_journal_on_exit),
            )
            .add_systems(OnExit(AppState::Menu), stop_recording_on_exit)
            .add_systems(
                EguiPrimaryContextPass,
                menu.run_if(in_state(AppState::Menu)),
            );
    }
}

#[derive(Debug, Default, PartialEq, Clone, Copy)]
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
    Coach,
}

const TABS: [(Tab, &str); 9] = [
    (Tab::Today, "Today"),
    (Tab::Progress, "Progress"),
    (Tab::Scenarios, "Scenarios"),
    (Tab::Routines, "Routines"),
    (Tab::Arenas, "Arenas"),
    (Tab::Benchmark, "Benchmark"),
    (Tab::Tools, "Tools"),
    (Tab::Settings, "Settings"),
    (Tab::Coach, "Coach"),
];

/// Keys 1–9 select the tabs in [`TABS`] order.
const TAB_KEYS: [egui::Key; 9] = [
    egui::Key::Num1,
    egui::Key::Num2,
    egui::Key::Num3,
    egui::Key::Num4,
    egui::Key::Num5,
    egui::Key::Num6,
    egui::Key::Num7,
    egui::Key::Num8,
    egui::Key::Num9,
];

fn tab_for_key(key: egui::Key) -> Option<Tab> {
    TAB_KEYS.iter().position(|&k| k == key).map(|i| TABS[i].0)
}

/// Number keys switch tabs unless a text field has focus or a key is being recorded
/// (`on_keys_tab`: the Keys sub-tab, whose restart-key row records bare keys).
fn number_keys_enabled(text_focus: bool, recording_hotkey: bool, on_keys_tab: bool) -> bool {
    !(text_focus || recording_hotkey || on_keys_tab)
}

/// Whether the restart-key row (which records bare keys, digits included) can be on
/// screen: on the Keys sub-tab, or in any Settings search, which shows rows from every
/// sub-tab.
fn bare_key_rows_shown(tab: Tab, settings_tab: SettingsTab, query: &str) -> bool {
    tab == Tab::Settings && (settings_tab == SettingsTab::Keys || !query.trim().is_empty())
}

/// The routine "Continue" resumes: today's unfinished method day, else the newest
/// unfinished routine from today. `runs_of` gives a routine's current run count; a
/// position whose count differs belongs to an edited routine and is skipped.
fn continue_position(
    progress: &MethodProgress,
    date: &str,
    runs_of: impl Fn(&str) -> Option<usize>,
) -> Option<RoutinePosition> {
    let open = |p: &&RoutinePosition| {
        p.date == date && !p.is_done() && runs_of(&p.routine) == Some(p.total_runs)
    };
    progress
        .today
        .iter()
        .find(open)
        .or_else(|| progress.routines.iter().rev().find(open))
        .cloned()
}

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
    settings_tab: SettingsTab,
    /// Text of the Settings search box.
    settings_query: String,
    /// The section whose "Reset" was clicked once and waits for confirmation.
    confirm_reset: Option<SettingsTab>,
    scenario_filter: scenario_browser::ScenarioFilter,
    wizard_step: first_run::WizardStep,
    coach: CoachState,
    /// History export in progress or finished (Progress tab).
    export: history::HistoryExport,
    /// Path typed for the export; empty uses the default.
    export_path: String,
    /// The ranked journal's entry form and breathing timer (Progress tab).
    journal: journal_panel::JournalState,
    /// Benchmark plateaus, recomputed when the history changes (Progress tab).
    plateaus: HistoryCache<Vec<aim_core::training::progress::Plateau>>,
}

/// A value derived from the history, recomputed only after the history changed.
#[derive(Debug)]
struct HistoryCache<T> {
    value: Option<T>,
}

impl<T> Default for HistoryCache<T> {
    fn default() -> Self {
        Self { value: None }
    }
}

impl<T> HistoryCache<T> {
    /// Forget the value when the history changed this frame.
    fn note_change(&mut self, changed: bool) {
        if changed {
            self.value = None;
        }
    }

    /// The cached value, computed by `compute` when missing.
    fn get(&mut self, compute: impl FnOnce() -> T) -> &T {
        self.value.get_or_insert_with(compute)
    }
}

/// The Settings tab: search, sub-tabs, a reset per section, and the rows.
fn settings_page(root: &mut egui::Ui, state: &mut MenuState, cfg: &mut SettingsAccess) {
    egui::Panel::top("settings_tabs").show(root, |ui| {
        ui.horizontal(|ui| {
            ui.label("Search");
            ui.add(
                egui::TextEdit::singleline(&mut state.settings_query)
                    .hint_text("Search settings")
                    .desired_width(180.0),
            );
            if !state.settings_query.is_empty() && ui.small_button("Clear").clicked() {
                state.settings_query.clear();
            }
            ui.separator();
            tab_bar(ui, &mut state.settings_tab, &SettingsTab::ALL);
        });
        let searching = !state.settings_query.trim().is_empty();
        ui.horizontal(|ui| {
            if searching {
                ui.small("Showing matches from every section.");
            } else if state.confirm_reset == Some(state.settings_tab) {
                ui.label("Reset this section to its defaults?");
                if ui.button("Reset").clicked() {
                    settings_panels::reset_section(&mut cfg.settings, state.settings_tab);
                    state.confirm_reset = None;
                }
                if ui.button("Cancel").clicked() {
                    state.confirm_reset = None;
                }
            } else if ui.button("Reset section to defaults").clicked() {
                state.confirm_reset = Some(state.settings_tab);
            }
        });
    });
    // Pinned below the rows, which scroll.
    egui::Panel::bottom("settings_footer").show(root, |ui| {
        ui.small(format!(
            "Settings file: {}  ·  VALORANT FOV: {:.2}° vertical (103° horizontal at 16:9, Hor+)",
            cfg.paths.settings_file.display(),
            fov::valorant_vertical_fov_deg()
        ));
    });
    egui::CentralPanel::default().show(root, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("settings")
            .show(ui, |ui| {
                let query = state.settings_query.clone();
                let mut rows = SettingsRows::new(&query);
                let tabs: Vec<SettingsTab> = if rows.searching() {
                    SettingsTab::ALL.iter().map(|t| t.0).collect()
                } else {
                    vec![state.settings_tab]
                };
                for tab in tabs {
                    settings_section(ui, &mut rows, tab, state, cfg);
                }
            });
    });
}

fn settings_section(
    ui: &mut egui::Ui,
    rows: &mut SettingsRows,
    tab: SettingsTab,
    state: &mut MenuState,
    cfg: &mut SettingsAccess,
) {
    let settings = &mut *cfg.settings;
    match tab {
        SettingsTab::Sensitivity => {
            settings_panels::sensitivity_panel(
                ui,
                rows,
                settings,
                &mut state.dpi_target,
                &mut state.meter,
            );
            rows.block(ui, "Profile code share import export", |ui| {
                ui.add_space(8.0);
                profile_code::profile_code(ui, settings, &mut state.profile_import);
            });
        }
        SettingsTab::Crosshair => settings_panels::crosshair_panel(ui, rows, settings),
        SettingsTab::Feedback => feedback_settings::feedback_panel(ui, rows, settings),
        SettingsTab::Audio => {
            if let Some(clip) = feedback_settings::audio_panel(ui, rows, &mut settings.sound) {
                cfg.sound_previews.write(SoundPreview(clip));
            }
        }
        SettingsTab::Overlay => settings_panels::overlay_panel(ui, rows, settings),
        SettingsTab::Keys => {
            settings_panels::keys_panel(ui, rows, settings, &mut cfg.hotkey_capture.recording)
        }
        SettingsTab::Training => {
            settings_panels::training_panel(ui, rows, settings, cfg.personal_warmup.0.as_ref());
            settings_panels::breaks_panel(ui, rows, &mut settings.breaks);
        }
    }
}

/// Feed the DPI helper the raw x counts of this frame while Shift is held.
fn count_dpi_meter(
    motion: Res<AccumulatedMouseMotion>,
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<MenuState>,
) {
    if !state.meter.armed {
        return;
    }
    let held = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    state.meter.update(held, motion.delta.x as f64);
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
    coach_play: ResMut<'w, CoachPlay>,
    journal: ResMut<'w, JournalStore>,
}

/// Settings plus what is needed to persist and preview them.
#[derive(SystemParam)]
struct SettingsAccess<'w> {
    settings: ResMut<'w, Settings>,
    paths: Res<'w, Paths>,
    clear: ResMut<'w, ClearColor>,
    hotkey_capture: ResMut<'w, HotkeyCapture>,
    sound_previews: MessageWriter<'w, SoundPreview>,
    saver: ResMut<'w, SettingsSaver>,
    time: Res<'w, Time<Real>>,
    personal_warmup: Res<'w, crate::ui::launcher::PersonalWarmup>,
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
    let now = cfg.time.elapsed_secs_f64();
    let saved = cfg.saver.debounce.show_saved(now);
    let state = &mut *state;
    // Read every frame: a change seen while another tab is open must still refresh caches.
    let history_changed = history.is_changed();
    state.plateaus.note_change(history_changed);
    let lib = nav.library();
    let mut action: Option<UiAction> = None;

    let bare_keys_shown = bare_key_rows_shown(state.tab, state.settings_tab, &state.settings_query);
    if number_keys_enabled(
        ctx.egui_wants_keyboard_input(),
        cfg.hotkey_capture.recording.is_some(),
        bare_keys_shown,
    ) && let Some(tab) = ctx.input(|i| {
        TAB_KEYS
            .iter()
            .find(|&&k| i.key_pressed(k))
            .and_then(|&k| tab_for_key(k))
    }) {
        state.tab = tab;
    }

    egui::Panel::top("tabs").show(&mut root, |ui| {
        ui.horizontal(|ui| {
            ui.strong("CustomAimTrainer");
            ui.separator();
            tab_bar(ui, &mut state.tab, &TABS);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Quit").clicked() {
                    action = Some(UiAction::Quit);
                }
                if saved {
                    ui.colored_label(GOOD, "Saved ✔");
                    // Repaint so the message disappears without waiting for input.
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(250));
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
            let quick = quick_model(lib, &history, &training.progress, &today);
            egui::CentralPanel::default().show(&mut root, |ui| {
                today_action = today_panel::quick_bar(ui, &quick);
                match today_model(lib, &history, &training.progress, &today, weekday, missions) {
                    Some(model) => {
                        today_action = today_panel::today_panel(ui, &model).or(today_action.take())
                    }
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
                    ui.separator();
                    let readiness = journal_panel::journal_readiness(&model.readiness);
                    let fatigued = !model.fatigue.is_empty();
                    let view = journal_panel::JournalView {
                        journal: &training.journal.journal,
                        problem: training.journal.problem(),
                        readiness,
                        fatigued,
                    };
                    match journal_panel::journal_panel(ui, &mut state.journal, &view) {
                        Some(journal_panel::JournalAction::Log(draft)) => {
                            let entry = journal_entry(draft, now_secs(), readiness, fatigued);
                            if let Err(e) = training.journal.add(entry) {
                                bevy::log::warn!("journal entry refused: {e}");
                            }
                        }
                        Some(journal_panel::JournalAction::Run(a)) => action = Some(a),
                        None => {}
                    }
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
                        let plateaus = state.plateaus.get(|| {
                            aim_core::training::progress::plateaus(
                                bench,
                                history.runs(),
                                now_secs(),
                                aim_core::training::progress::PLATEAU_WEEKS,
                            )
                        });
                        progress_panel::plateau_section(ui, plateaus);
                    }
                    state.export.poll();
                    let default_path = cfg
                        .paths
                        .history_file
                        .with_file_name("history-export-<unix time>.csv")
                        .display()
                        .to_string();
                    let view = progress_panel::ExportView {
                        default_path: &default_path,
                        running: state.export.is_running(),
                        status: state.export.status(),
                    };
                    if progress_panel::export_section(ui, &mut state.export_path, &view) {
                        let out = match state.export_path.trim() {
                            "" => history::default_export_path(&cfg.paths, now_secs()),
                            p => std::path::PathBuf::from(p),
                        };
                        state.export.start(cfg.paths.history_file.clone(), out);
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
                    scenario_browser::scenario_list(
                        ui,
                        lib,
                        &history,
                        &mut state.selected,
                        &mut state.scenario_filter,
                        &mut cfg.settings.favorites,
                    );
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
            // Pinned below the list, which scrolls and would otherwise push it off-screen.
            egui::Panel::bottom("custom_routines").show(&mut root, |ui| {
                ui.small(format!(
                    "Custom routines: {}",
                    cfg.paths.routines_dir.display()
                ));
            });
            egui::CentralPanel::default().show(&mut root, |ui| {
                let personalized = lib.benchmarks.first().map(|b| {
                    playlist::personalized(b, &b.evaluate(&history.bests()), &lib.scenarios, 30.0)
                });
                // The list's scroll area has no salt of its own; this scope gives it one.
                let list = ui.push_id("routines", |ui| {
                    routine_list::routine_list(ui, lib, personalized.as_ref(), |id| {
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
                    })
                });
                action = action.take().or(list.inner);
            });
        }
        Tab::Arenas => {
            egui::CentralPanel::default().show(&mut root, |ui| {
                let arenas = aim_world::arena::builtin::all();
                let coaching = &mut cfg.settings.arena_coaching;
                action = action.take().or(arena_list::arena_list(
                    ui,
                    &arenas,
                    |id| history.best(id),
                    coaching,
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
        Tab::Tools => {
            egui::CentralPanel::default().show(&mut root, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("tools")
                    .show(ui, |ui| {
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
        Tab::Settings => settings_page(&mut root, state, &mut cfg),
        Tab::Coach => {
            let coach = &mut state.coach;
            refresh_coach(
                coach,
                &cfg.settings.coach,
                &cfg.paths,
                lib,
                &history,
                &training.missions,
            );
            let memory_file = cfg.paths.coach_memory_file.display().to_string();
            let mut coach_action = None;
            egui::CentralPanel::default().show(&mut root, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("coach")
                    .show(ui, |ui| {
                        coach_action = coach_panel::coach_panel(
                            ui,
                            coach,
                            &mut cfg.settings.coach,
                            &memory_file,
                        );
                    });
            });
            if let Some(a) = coach_action {
                apply_coach(
                    a,
                    coach,
                    &cfg.settings.coach,
                    &cfg.paths,
                    lib,
                    &mut training.coach_play,
                );
            }
            if coach.busy() {
                // Keep the spinner moving and collect the answer without waiting for input.
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
    }

    first_run::first_run_wizard(ctx, &mut state.wizard_step, &mut cfg.settings);
    let hotkeys_shown = state.tab == Tab::Settings
        && (state.settings_tab == SettingsTab::Keys || !state.settings_query.trim().is_empty());
    if !hotkeys_shown {
        stop_recording(&mut cfg.hotkey_capture);
    }
    if *cfg.settings != before && cfg.settings.sensitivity.is_valid() {
        // The file is written later by `save_settings`; the background updates now.
        cfg.saver.debounce.changed(now);
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

/// Seconds without a further change before the settings file is written.
const SAVE_DELAY_SECS: f64 = 0.5;
/// How long the "Saved" message stays after a write, seconds.
const SAVED_SHOWN_SECS: f64 = 2.0;

/// When to write the settings file: once changes have settled for [`SAVE_DELAY_SECS`], and
/// never while an earlier write is still running (writes then land in order).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct SaveDebounce {
    /// Time of the newest change not yet written.
    last_change: Option<f64>,
    /// Time the newest write finished.
    saved_at: Option<f64>,
}

impl SaveDebounce {
    fn changed(&mut self, now: f64) {
        self.last_change = Some(now);
    }

    fn pending(&self) -> bool {
        self.last_change.is_some()
    }

    /// True when a write should start now; the change then counts as written.
    fn should_write(&mut self, now: f64, writing: bool) -> bool {
        match self.last_change {
            Some(t) if !writing && now - t >= SAVE_DELAY_SECS => {
                self.last_change = None;
                true
            }
            _ => false,
        }
    }

    fn finished(&mut self, now: f64) {
        self.saved_at = Some(now);
    }

    /// Whether to show the "Saved" message.
    fn show_saved(&self, now: f64) -> bool {
        !self.pending() && self.saved_at.is_some_and(|t| now - t < SAVED_SHOWN_SECS)
    }
}

/// Writes the settings file off the frame: debounced, on a background thread.
#[derive(Resource, Default)]
struct SettingsSaver {
    debounce: SaveDebounce,
    writing: Option<std::thread::JoinHandle<bool>>,
}

impl SettingsSaver {
    /// Collect a finished write and start the next one when it is due.
    fn update(&mut self, settings: &Settings, paths: &Paths, now: f64) {
        if self.writing.as_ref().is_some_and(|h| h.is_finished()) {
            // "Saved" only after a successful write; a failed one is logged by `save`.
            let written = self
                .writing
                .take()
                .is_some_and(|h| h.join().unwrap_or(false));
            if written {
                self.debounce.finished(now);
            }
        }
        // An invalid sensitivity is never written; the change stays pending until fixed.
        if !settings.sensitivity.is_valid() {
            return;
        }
        if self.debounce.should_write(now, self.writing.is_some()) {
            let (settings, paths) = (settings.clone(), paths.clone());
            let spawned = std::thread::Builder::new()
                .name("settings-save".into())
                .spawn(move || settings.save(&paths));
            match spawned {
                Ok(handle) => self.writing = Some(handle),
                Err(e) => bevy::log::error!("could not start the settings writer: {e}"),
            }
        }
    }

    /// Finish every write before the app exits.
    fn flush(&mut self, settings: &Settings, paths: &Paths) {
        if let Some(handle) = self.writing.take() {
            let _ = handle.join();
        }
        if self.debounce.pending() && settings.sensitivity.is_valid() {
            settings.save(paths);
            self.debounce = SaveDebounce::default();
        }
    }
}

fn save_settings(
    mut saver: ResMut<SettingsSaver>,
    settings: Res<Settings>,
    paths: Res<Paths>,
    time: Res<Time<Real>>,
) {
    saver.update(&settings, &paths, time.elapsed_secs_f64());
}

fn flush_settings_on_exit(
    mut exits: MessageReader<AppExit>,
    mut saver: ResMut<SettingsSaver>,
    settings: Res<Settings>,
    paths: Res<Paths>,
) {
    if exits.read().next().is_some() {
        saver.flush(&settings, &paths);
    }
}

/// A generated scenario the player chose to play from the Coach page. It is not in the
/// library, so it starts here rather than through the navigator's id lookup.
#[derive(Resource, Default)]
struct CoachPlay(Option<ScenarioDef>);

fn start_coach_scenario(
    mut play: ResMut<CoachPlay>,
    mut commands: Commands,
    mut next: ResMut<NextState<AppState>>,
    mut routine: ResMut<ActiveRoutine>,
) {
    if let Some(def) = play.0.take() {
        routine.0 = None;
        start_run(&mut commands, &mut next, def);
    }
}

/// Collect finished coach requests and start the background ones the page needs: the key
/// status for the current provider and the stats summary when the history changed.
fn refresh_coach(
    state: &mut CoachState,
    config: &aim_ai::CoachConfig,
    paths: &Paths,
    lib: &Library,
    history: &History,
    missions: &Missions,
) {
    state.poll();
    if !config.enabled {
        // Nothing is sent while the coach is off.
        state.cancel_sending();
    }
    let account = (coach::key_account(config), config.api_key_env.clone());
    if config.enabled && state.key_checked_for.as_ref() != Some(&account) && !state.key.is_pending()
    {
        state.key_checked_for = Some(account);
        let config = config.clone();
        state
            .key
            .spawn("coach-key", move || Ok(coach::key_status(&config)));
    }
    let records = history.runs();
    let key = (records.len(), records.last().map_or(0, |r| r.timestamp));
    if state.context_key != Some(key) && !state.context.is_pending() {
        state.context_key = Some(key);
        let data = ContextData {
            records: records.to_vec(),
            analyses: Vec::new(),
            benchmark: lib.benchmarks.first().cloned(),
            missions: Some(missions.0.clone()),
            readiness_scenarios: READINESS_SCENARIOS.map(str::to_owned).to_vec(),
            now: now_secs(),
            utc_offset_secs: local_utc_offset_secs(),
        };
        let paths = paths.clone();
        state
            .context
            .spawn("coach-context", move || Ok(load_context(&paths, data)));
    }
}

/// Run a Coach page action; every request goes to a background job.
fn apply_coach(
    action: CoachAction,
    state: &mut CoachState,
    config: &aim_ai::CoachConfig,
    paths: &Paths,
    lib: &Library,
    play: &mut CoachPlay,
) {
    match action {
        CoachAction::Send(message) => {
            let Some(context) = state.context_value.clone() else {
                return;
            };
            if !config.enabled {
                return;
            }
            let request = TurnRequest {
                config: config.clone(),
                memory_file: paths.coach_memory_file.clone(),
                context,
                scenarios: lib.scenarios.clone(),
                routines: lib.routines.clone(),
                benchmark: lib.benchmarks.first().cloned(),
                history: state.history.clone(),
                message,
                now: now_secs(),
            };
            if state
                .turn
                .spawn("coach-turn", move || coach::run_request(request))
            {
                state.input.clear();
                state.notice = None;
            }
        }
        CoachAction::Cancel => state.turn.cancel(),
        CoachAction::TestConnection => {
            state.test_result = None;
            let config = config.clone();
            state
                .test
                .spawn("coach-test", move || coach::test_connection(&config));
        }
        CoachAction::StoreKey => {
            // The typed key leaves the page state now; it is never shown again.
            let key = aim_ai::ApiKey::new(std::mem::take(&mut state.key_input));
            let config = config.clone();
            state
                .key
                .spawn("coach-key", move || coach::store_key(&config, key));
        }
        CoachAction::ForgetKey => {
            let config = config.clone();
            state
                .key
                .spawn("coach-key", move || coach::forget_key(&config));
        }
        CoachAction::RefreshContext => state.context_key = None,
        CoachAction::WipeMemory => {
            let file = paths.coach_memory_file.clone();
            state
                .wipe
                .spawn("coach-wipe", move || coach::wipe_memory(&file));
        }
        CoachAction::Check(i) => {
            if let Some(def) = state.cards.get(i).map(|c| c.def.clone()) {
                state.card_job.spawn("coach-check", move || {
                    Ok(CardOutcome::Checked(i, coach::check_scenario(&def)))
                });
            }
        }
        CoachAction::Save(i) => {
            if let Some(def) = state.cards.get(i).map(|c| c.def.clone()) {
                let dir = paths.scenarios_dir.clone();
                state.card_job.spawn("coach-save", move || {
                    Ok(CardOutcome::Saved(
                        i,
                        coach::save_generated_scenario(&dir, &def),
                    ))
                });
            }
        }
        CoachAction::Play(i) => play.0 = state.cards.get(i).map(|c| c.def.clone()),
        CoachAction::ClearChat => {
            state.history.clear();
            state.cards.clear();
            state.notice = None;
        }
    }
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

/// A journal entry from the form, stamped `now` with today's readiness and fatigue.
fn journal_entry(
    draft: journal_panel::Draft,
    now: u64,
    readiness: Option<aim_core::training::progress::Warmth>,
    fatigued: bool,
) -> aim_core::training::journal::JournalEntry {
    aim_core::training::journal::JournalEntry {
        timestamp: now,
        outcome: draft.outcome,
        rr_change: draft.rr_change,
        self_rating: draft.self_rating,
        tag: draft.tag,
        readiness,
        fatigued,
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

/// The Today shortcuts: the last scenario played and a routine to continue.
fn quick_model(
    lib: &Library,
    history: &History,
    progress: &MethodProgress,
    date: &str,
) -> QuickModel {
    let last_scenario =
        scenario_browser::recent_scenarios(history.runs(), |id| lib.scenario(id).is_some(), 1)
            .into_iter()
            .next()
            .and_then(|id| lib.scenario(&id).map(|d| (id, d.name.clone())));
    let routine_of = |id: &str| lib.routines.iter().find(|r| r.id == id);
    let routine = continue_position(progress, date, |id| {
        routine_of(id).map(|r| r.run_count(&lib.scenarios))
    })
    .and_then(|p| {
        let name = routine_of(&p.routine)?.name.clone();
        Some((p, name))
    });
    QuickModel {
        last_scenario,
        routine,
    }
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
        TodayAction::QuickPlay(scenario) => Some(UiAction::StartScenario(scenario)),
        TodayAction::Continue(p) => Some(UiAction::StartRoutineAt {
            routine: p.routine,
            level: p.level,
            index: p.next_index,
            seed: p.seed,
        }),
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
    use super::*;
    use bevy::ecs::system::assert_is_system;
    use bevy::prelude::Result;

    #[test]
    fn systems_have_no_conflicting_access() {
        assert_is_system::<(), Result, _>(super::menu);
        assert_is_system(super::count_dpi_meter);
        assert_is_system(super::stop_recording_on_exit);
        assert_is_system(super::save_settings);
        assert_is_system(super::flush_settings_on_exit);
        assert_is_system(super::start_coach_scenario);
        assert_is_system(crate::persistence::journal::poll_journal);
        assert_is_system(crate::persistence::journal::flush_journal_on_exit);
    }

    #[test]
    fn history_caches_recompute_only_after_the_history_changed() {
        let mut cache = super::HistoryCache::<u32>::default();
        let mut computed = 0;
        let mut get = |cache: &mut super::HistoryCache<u32>, changed: bool| {
            cache.note_change(changed);
            *cache.get(|| {
                computed += 1;
                computed
            })
        };
        assert_eq!(get(&mut cache, false), 1, "computed when first needed");
        assert_eq!(
            get(&mut cache, false),
            1,
            "kept while the history is unchanged"
        );
        assert_eq!(get(&mut cache, true), 2, "recomputed after a change");
        assert_eq!(get(&mut cache, false), 2);
    }

    #[test]
    fn journal_entries_carry_the_form_and_todays_readiness_and_fatigue() {
        use aim_core::training::journal::{JournalTag, Outcome};
        use aim_core::training::progress::Warmth;
        let draft = super::journal_panel::Draft {
            outcome: Outcome::Loss,
            rr_change: Some(-21),
            self_rating: 2,
            tag: Some(JournalTag::Tired),
        };
        let e = super::journal_entry(draft, 1_800_000_000, Some(Warmth::Warming), true);
        assert_eq!(e.timestamp, 1_800_000_000);
        assert_eq!(e.outcome, Outcome::Loss);
        assert_eq!(e.rr_change, Some(-21));
        assert_eq!(e.self_rating, 2);
        assert_eq!(e.tag, Some(JournalTag::Tired));
        assert_eq!(e.readiness, Some(Warmth::Warming));
        assert!(e.fatigued);
        assert!(e.validate().is_ok());
    }

    #[test]
    fn number_keys_one_to_nine_pick_the_tabs_in_order() {
        use egui::Key;
        assert_eq!(tab_for_key(Key::Num1), Some(Tab::Today));
        assert_eq!(tab_for_key(Key::Num3), Some(Tab::Scenarios));
        assert_eq!(tab_for_key(Key::Num8), Some(Tab::Settings));
        assert_eq!(tab_for_key(Key::Num9), Some(Tab::Coach));
        assert_eq!(tab_for_key(Key::Num0), None);
        assert_eq!(tab_for_key(Key::A), None);
    }

    #[test]
    fn the_restart_key_row_shows_on_the_keys_tab_and_in_any_search() {
        assert!(bare_key_rows_shown(Tab::Settings, SettingsTab::Keys, ""));
        assert!(
            bare_key_rows_shown(Tab::Settings, SettingsTab::Audio, "restart"),
            "a search shows rows from every sub-tab"
        );
        assert!(!bare_key_rows_shown(
            Tab::Settings,
            SettingsTab::Audio,
            "  "
        ));
        assert!(!bare_key_rows_shown(
            Tab::Today,
            SettingsTab::Keys,
            "restart"
        ));
    }

    #[test]
    fn number_keys_are_off_while_typing_or_recording_a_key() {
        assert!(number_keys_enabled(false, false, false));
        assert!(
            !number_keys_enabled(true, false, false),
            "a text field has focus"
        );
        assert!(
            !number_keys_enabled(false, true, false),
            "recording a hotkey"
        );
        assert!(
            !number_keys_enabled(false, false, true),
            "the Keys sub-tab records bare keys"
        );
    }

    fn position(routine: &str, date: &str, next: usize, total: usize) -> RoutinePosition {
        RoutinePosition {
            date: date.into(),
            routine: routine.into(),
            level: 0.0,
            seed: 7,
            next_index: next,
            total_runs: total,
        }
    }

    #[test]
    fn continue_offers_todays_unfinished_routine_method_day_first() {
        let runs = |_: &str| Some(10);
        let day = "2026-10-18";
        let mut p = MethodProgress {
            tier: None,
            today: Some(position("method", day, 3, 10)),
            routines: vec![position("other", day, 2, 10)],
        };
        assert_eq!(
            continue_position(&p, day, runs).map(|c| c.routine),
            Some("method".into())
        );
        p.today = Some(position("method", day, 10, 10));
        assert_eq!(
            continue_position(&p, day, runs).map(|c| c.routine),
            Some("other".into()),
            "a finished method day falls back to another routine"
        );
        p.routines = vec![position("other", "2026-10-17", 2, 10)];
        assert_eq!(continue_position(&p, day, runs), None, "yesterday's run");
        p.routines = vec![position("edited", day, 2, 10)];
        assert_eq!(
            continue_position(&p, day, |_| Some(12)),
            None,
            "a routine edited since has another run count"
        );
    }

    #[test]
    fn settings_are_written_once_after_the_last_change_settles() {
        let mut d = SaveDebounce::default();
        assert!(!d.should_write(10.0, false), "nothing changed yet");
        d.changed(1.0);
        d.changed(1.3);
        assert!(!d.should_write(1.6, false), "0.3 s after the last change");
        assert!(d.pending());
        assert!(d.should_write(1.3 + SAVE_DELAY_SECS, false));
        assert!(!d.pending());
        assert!(!d.should_write(5.0, false), "one write per settled change");
    }

    #[test]
    fn a_write_in_flight_delays_the_next_one() {
        let mut d = SaveDebounce::default();
        d.changed(0.0);
        assert!(
            !d.should_write(2.0, true),
            "the previous write is still running"
        );
        assert!(d.pending());
        assert!(d.should_write(2.1, false));
    }

    #[test]
    fn saved_shows_briefly_after_a_write_and_hides_on_a_new_change() {
        let mut d = SaveDebounce::default();
        assert!(!d.show_saved(0.0));
        d.finished(3.0);
        assert!(d.show_saved(3.5));
        assert!(!d.show_saved(3.0 + SAVED_SHOWN_SECS));
        d.changed(3.6);
        assert!(!d.show_saved(3.7), "a newer change is not saved yet");
    }
}
