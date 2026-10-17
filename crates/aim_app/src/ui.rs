//! egui front-end: HUD + crosshair while playing, menus, settings and results.

use crate::game::{ActiveRoutine, AppState, LastResult, RoutineProgress, Run, start_run};
use crate::history::History;
use crate::library::Library;
use crate::overlay::{Overlay, background};
use crate::settings::{Paths, Settings};
use aim_core::fov;
use aim_core::scenario::{Category, FireMode, ScenarioDef};
use aim_core::sensitivity::Game;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy_egui::egui::{self, Align2, Color32, FontId, Pos2, Stroke, Vec2 as EVec2};
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuState>()
            .init_resource::<CountMeter>()
            .add_systems(Update, count_meter)
            .add_systems(
                EguiPrimaryContextPass,
                (
                    hud.run_if(in_state(AppState::Playing)),
                    menu.run_if(in_state(AppState::Menu)),
                    results.run_if(in_state(AppState::Results)),
                ),
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

#[derive(Resource, Default)]
struct MenuState {
    tab: Tab,
    selected: Option<String>,
    dpi_target: u32,
}

/// Raw count accumulator used to verify DPI and that the OS delivers unaccelerated input.
#[derive(Resource, Default)]
struct CountMeter {
    active: bool,
    x: f64,
    y: f64,
}

fn count_meter(motion: Res<AccumulatedMouseMotion>, mut meter: ResMut<CountMeter>) {
    if meter.active {
        meter.x += motion.delta.x as f64;
        meter.y += motion.delta.y as f64;
    }
}

fn c32(rgb: [f32; 3]) -> Color32 {
    Color32::from_rgb(
        (rgb[0] * 255.0) as u8,
        (rgb[1] * 255.0) as u8,
        (rgb[2] * 255.0) as u8,
    )
}

fn draw_crosshair(ctx: &egui::Context, settings: &Settings, hit: bool) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("crosshair"),
    ));
    let ppp = ctx.pixels_per_point();
    let c = ctx.content_rect().center();
    // Snap to physical pixels so lines stay crisp and centered.
    let snap = |v: f32| (v * ppp).round() / ppp + 0.5 / ppp;
    let center = Pos2::new(snap(c.x), snap(c.y));
    let ch = &settings.crosshair;
    let (len, gap, th) = (ch.length / ppp, ch.gap / ppp, ch.thickness / ppp);
    let color = if hit { Color32::WHITE } else { c32(ch.color) };
    let arms = [
        EVec2::new(1.0, 0.0),
        EVec2::new(-1.0, 0.0),
        EVec2::new(0.0, 1.0),
        EVec2::new(0.0, -1.0),
    ];
    let segments: Vec<_> = arms
        .iter()
        .map(|d| (center + *d * gap, center + *d * (gap + len)))
        .collect();
    if ch.outline {
        for (a, b) in &segments {
            painter.line_segment([*a, *b], Stroke::new(th + 2.0 / ppp, Color32::BLACK));
        }
    }
    for (a, b) in &segments {
        painter.line_segment([*a, *b], Stroke::new(th, color));
    }
    if ch.center_dot {
        painter.circle_filled(center, th, color);
    }
}

fn hud(
    mut contexts: EguiContexts,
    settings: Res<Settings>,
    overlay: Res<Overlay>,
    mut run: ResMut<Run>,
    mut next: ResMut<NextState<AppState>>,
    mut routine: ResMut<ActiveRoutine>,
) -> Result {
    if !overlay.visible {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    draw_crosshair(ctx, &settings, run.hit_flash > 0.0);

    let s = &run.session;
    let stats = s.stats();
    let hold = s.def().weapon.fire == FireMode::Hold;
    let accuracy = s.result().accuracy * 100.0;
    egui::Area::new(egui::Id::new("hud"))
        .anchor(Align2::CENTER_TOP, [0.0, 16.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(format!("{:.1}", s.remaining()))
                        .font(FontId::proportional(28.0))
                        .strong(),
                );
                ui.label(format!(
                    "{}  ·  score {:.0}  ·  {accuracy:.0}% acc  ·  {} kills",
                    s.def().name,
                    stats.score.max(0.0),
                    stats.kills
                ));
                if hold {
                    ui.label(format!(
                        "on target {:.1}s / firing {:.1}s",
                        stats.on_target_secs, stats.hold_secs
                    ));
                }
            });
        });

    if s.def().player.movement {
        let accurate = s.is_accurate();
        let speed = s.player_speed();
        let limit = s.movement_profile().accuracy_speed_limit();
        egui::Area::new(egui::Id::new("speed"))
            .anchor(Align2::CENTER_BOTTOM, [0.0, -40.0])
            .show(ctx, |ui| {
                let (text, color) = if accurate {
                    ("ACCURATE", Color32::from_rgb(80, 220, 120))
                } else {
                    ("MOVING", Color32::from_rgb(240, 80, 80))
                };
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new(text)
                            .color(color)
                            .strong()
                            .font(FontId::proportional(20.0)),
                    );
                    ui.label(format!(
                        "{speed:.2} m/s  (accurate ≤ {limit:.2})  ·  moving shots {}",
                        stats.moving_shots
                    ));
                });
            });
    }

    if run.countdown > 0.0 {
        egui::Area::new(egui::Id::new("countdown"))
            .anchor(Align2::CENTER_CENTER, [0.0, -80.0])
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!("{}", run.countdown.ceil() as i32))
                        .font(FontId::proportional(56.0))
                        .strong(),
                );
            });
    }

    if run.paused {
        egui::Window::new("Paused")
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(&run.session.def().description);
                ui.separator();
                if ui.button("Resume (Esc)").clicked() {
                    run.paused = false;
                }
                if ui.button("Quit to menu").clicked() {
                    routine.0 = None;
                    next.set(AppState::Menu);
                }
            });
    }
    Ok(())
}

fn scenario_list(ui: &mut egui::Ui, lib: &Library, history: &History, state: &mut MenuState) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        for cat in Category::ALL {
            let items: Vec<&ScenarioDef> =
                lib.scenarios.iter().filter(|d| d.category == cat).collect();
            if items.is_empty() {
                continue;
            }
            ui.heading(cat.label());
            for d in items {
                let best = history
                    .best(&d.id)
                    .map_or(String::new(), |b| format!("  (PB {b:.0})"));
                let selected = state.selected.as_deref() == Some(d.id.as_str());
                if ui
                    .selectable_label(selected, format!("{}{best}", d.name))
                    .clicked()
                {
                    state.selected = Some(d.id.clone());
                }
            }
            ui.add_space(6.0);
        }
    });
}

fn scenario_details(ui: &mut egui::Ui, def: &ScenarioDef, history: &History) -> bool {
    ui.heading(&def.name);
    ui.label(format!(
        "{} · {:.0}s · {} · {}",
        def.category.label(),
        def.duration_secs,
        def.weapon.kind.stats().display_name,
        if def.weapon.fire == FireMode::Hold {
            "hold to fire"
        } else {
            "click"
        }
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
}

fn sens_settings(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    state: &mut MenuState,
    meter: &mut CountMeter,
) {
    let s = &mut settings.sensitivity;
    ui.heading("Sensitivity");
    egui::Grid::new("sens").num_columns(2).show(ui, |ui| {
        ui.label("Game");
        egui::ComboBox::from_id_salt("game")
            .selected_text(s.game.display_name())
            .show_ui(ui, |ui| {
                for g in Game::ALL {
                    if ui.selectable_label(s.game == g, g.display_name()).clicked() && s.game != g {
                        // Switching game keeps your physical sensitivity (cm/360).
                        *s = s.convert_to(g);
                    }
                }
            });
        ui.end_row();
        ui.label("In-game sensitivity");
        ui.add(
            egui::DragValue::new(&mut s.value)
                .speed(0.001)
                .range(0.001..=100.0)
                .max_decimals(4),
        );
        ui.end_row();
        ui.label("Mouse DPI");
        ui.add(egui::DragValue::new(&mut s.dpi).speed(10).range(50..=64000));
        ui.end_row();
        ui.label("ADS multiplier");
        ui.add(
            egui::DragValue::new(&mut settings.ads_multiplier)
                .speed(0.01)
                .range(0.01..=5.0),
        );
        ui.end_row();
    });
    let s = settings.sensitivity;
    ui.add_space(4.0);
    ui.label(format!(
        "{:.2} cm/360  ·  {:.2} in/360  ·  eDPI {:.0}  ·  {:.5}°/count",
        s.cm_per_360(),
        s.inches_per_360(),
        s.edpi(),
        s.deg_per_count()
    ));
    ui.collapsing("Same feel in other games", |ui| {
        for g in Game::ALL {
            ui.label(format!(
                "{}: {:.4}",
                g.display_name(),
                s.convert_to(g).value
            ));
        }
    });
    ui.horizontal(|ui| {
        if state.dpi_target == 0 {
            state.dpi_target = s.dpi;
        }
        ui.label("Change DPI to");
        ui.add(
            egui::DragValue::new(&mut state.dpi_target)
                .speed(10)
                .range(50..=64000),
        );
        if ui.button("keep cm/360").clicked() {
            settings.sensitivity = s.with_dpi(state.dpi_target);
        }
    });

    ui.add_space(8.0);
    ui.collapsing("DPI check (verify raw input)", |ui| {
        ui.label(
            "Press Start, move the mouse exactly 10 cm to the right along a ruler, then Stop.",
        );
        ui.horizontal(|ui| {
            if ui
                .button(if meter.active { "Stop" } else { "Start" })
                .clicked()
            {
                if !meter.active {
                    meter.x = 0.0;
                    meter.y = 0.0;
                }
                meter.active = !meter.active;
            }
            let measured_dpi = meter.x.abs() / (10.0 / 2.54);
            ui.label(format!(
                "counts {:.0}  →  measured DPI ≈ {measured_dpi:.0} (configured {})",
                meter.x, s.dpi
            ));
        });
        ui.label(
            "Expect a match within a few %. A large mismatch means wrong DPI or OS acceleration.",
        );
    });
}

fn display_settings(ui: &mut egui::Ui, settings: &mut Settings) {
    ui.heading("Crosshair");
    let ch = &mut settings.crosshair;
    egui::Grid::new("crosshair").num_columns(2).show(ui, |ui| {
        ui.label("Color");
        ui.color_edit_button_rgb(&mut ch.color);
        ui.end_row();
        ui.label("Length (px)");
        ui.add(egui::Slider::new(&mut ch.length, 0.0..=20.0).step_by(1.0));
        ui.end_row();
        ui.label("Thickness (px)");
        ui.add(egui::Slider::new(&mut ch.thickness, 1.0..=10.0).step_by(1.0));
        ui.end_row();
        ui.label("Gap (px)");
        ui.add(egui::Slider::new(&mut ch.gap, 0.0..=20.0).step_by(1.0));
        ui.end_row();
        ui.checkbox(&mut ch.outline, "Outline");
        ui.checkbox(&mut ch.center_dot, "Center dot");
        ui.end_row();
    });
    ui.add_space(8.0);
    ui.heading("Overlay");
    ui.add(
        egui::Slider::new(&mut settings.background_opacity, 0.0..=1.0).text("background opacity"),
    );
    ui.horizontal(|ui| {
        ui.label("Target color");
        ui.color_edit_button_rgb(&mut settings.target_color);
    });
    ui.checkbox(&mut settings.fullscreen, "Borderless fullscreen (restart)");
    ui.checkbox(
        &mut settings.always_on_top,
        "Always on top (restart; X11/Windows)",
    );
    ui.horizontal(|ui| {
        ui.label("Global hotkey (restart, Windows/X11)");
        ui.text_edit_singleline(&mut settings.overlay_hotkey);
    });
    ui.label("Wayland/Hyprland: bind `aimtrainer toggle` in your compositor (see README).");
}

#[allow(clippy::too_many_arguments)]
fn menu(
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut next: ResMut<NextState<AppState>>,
    mut state: ResMut<MenuState>,
    mut settings: ResMut<Settings>,
    mut meter: ResMut<CountMeter>,
    mut routine: ResMut<ActiveRoutine>,
    mut clear: ResMut<ClearColor>,
    lib: Res<Library>,
    history: Res<History>,
    paths: Res<Paths>,
    overlay: Res<Overlay>,
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
    let before = settings.clone();
    let mut start: Option<ScenarioDef> = None;
    let mut quit = false;

    egui::Panel::top("tabs").show(&mut root, |ui| {
        ui.horizontal(|ui| {
            ui.strong("CustomAimTrainer");
            ui.separator();
            for (tab, label) in [
                (Tab::Scenarios, "Scenarios"),
                (Tab::Routines, "Routines"),
                (Tab::Benchmark, "Benchmark"),
                (Tab::Settings, "Settings"),
            ] {
                ui.selectable_value(&mut state.tab, tab, label);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Quit").clicked() {
                    quit = true;
                }
                let s = settings.sensitivity;
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
            ui.colored_label(
                Color32::from_rgb(240, 120, 80),
                "Some files failed to load:",
            );
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
                    scenario_list(ui, &lib, &history, &mut state);
                    ui.separator();
                    ui.small(format!(
                        "Custom scenarios: {}",
                        paths.scenarios_dir.display()
                    ));
                });
            egui::CentralPanel::default().show(&mut root, |ui| {
                match state.selected.as_deref().and_then(|id| lib.scenario(id)) {
                    Some(def) => {
                        if scenario_details(ui, def, &history) {
                            start = Some(def.clone());
                            routine.0 = None;
                        }
                    }
                    None => {
                        ui.heading("Pick a scenario");
                        ui.label(
                            "Esc pauses a run. Right mouse = ADS zoom for weapons with a scope.",
                        );
                    }
                }
            });
        }
        Tab::Routines => {
            egui::CentralPanel::default().show(&mut root, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for r in &lib.routines {
                        ui.heading(&r.name);
                        ui.label(&r.description);
                        let plan = r.plan(&lib.scenarios, rand_seed());
                        ui.label(format!(
                            "{} runs · ≈{:.0} min",
                            plan.len(),
                            r.minutes(&lib.scenarios)
                        ));
                        if ui.button("▶  Start routine").clicked()
                            && let Some(def) =
                                plan.first().and_then(|p| lib.scenario(&p.scenario_id))
                        {
                            start = Some(def.clone());
                            routine.0 = Some(RoutineProgress {
                                name: r.name.clone(),
                                plan,
                                index: 0,
                            });
                        }
                        ui.separator();
                    }
                    ui.small(format!("Custom routines: {}", paths.routines_dir.display()));
                });
            });
        }
        Tab::Benchmark => {
            egui::CentralPanel::default().show(&mut root, |ui| {
                let bests = history.bests();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for b in &lib.benchmarks {
                        ui.heading(&b.name);
                        ui.label(&b.description);
                        if !b.calibrated {
                            ui.colored_label(
                                Color32::from_rgb(240, 200, 80),
                                "Provisional thresholds (simulated players).",
                            );
                        }
                        let res = b.evaluate(&bests);
                        let overall = res
                            .overall
                            .map_or("— (score every subcategory)".to_owned(), |e| {
                                format!("{e:.0} energy · {}", b.rank_for(e).unwrap_or("Unranked"))
                            });
                        ui.strong(format!("Overall: {overall}"));
                        egui::Grid::new(&b.id)
                            .striped(true)
                            .num_columns(4)
                            .show(ui, |ui| {
                                for (sub, r) in b.subcategories.iter().zip(&res.subcategories) {
                                    ui.label(format!("{} / {}", sub.skill, sub.name));
                                    ui.label(r.energy.map_or("—".to_owned(), |e| {
                                        format!("{e:.0} · {}", b.rank_for(e).unwrap_or("Unranked"))
                                    }));
                                    for s in &sub.scenarios {
                                        let best = bests
                                            .get(&s.id)
                                            .map_or("—".to_owned(), |v| format!("{v:.0}"));
                                        if ui.button(format!("▶ {} ({best})", s.id)).clicked()
                                            && let Some(def) = lib.scenario(&s.id)
                                        {
                                            start = Some(def.clone());
                                            routine.0 = None;
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                        ui.separator();
                    }
                });
            });
        }
        Tab::Settings => {
            egui::CentralPanel::default().show(&mut root, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    sens_settings(ui, &mut settings, &mut state, &mut meter);
                    ui.separator();
                    display_settings(ui, &mut settings);
                    ui.add_space(8.0);
                    ui.small(format!("Settings file: {}", paths.settings_file.display()));
                    ui.small(format!(
                        "VALORANT FOV: {:.2}° vertical (103° horizontal at 16:9, Hor+)",
                        fov::valorant_vertical_fov_deg()
                    ));
                });
            });
        }
    }

    if *settings != before && settings.sensitivity.is_valid() {
        settings.save(&paths);
        *clear = background(&settings, overlay.visible);
    }
    if let Some(def) = start {
        start_run(&mut commands, &mut next, def);
    }
    if quit {
        commands.write_message(AppExit::Success);
    }
    Ok(())
}

fn rand_seed() -> u64 {
    // Stable within a second so the shown plan matches the one that starts.
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[allow(clippy::too_many_arguments)]
fn results(
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut next: ResMut<NextState<AppState>>,
    mut routine: ResMut<ActiveRoutine>,
    last: Option<Res<LastResult>>,
    lib: Res<Library>,
    overlay: Res<Overlay>,
    keys: Res<ButtonInput<KeyCode>>,
) -> Result {
    if !overlay.visible {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let Some(last) = last else {
        next.set(AppState::Menu);
        return Ok(());
    };
    let r = &last.result;
    let next_run = routine
        .0
        .as_ref()
        .and_then(|p| p.plan.get(p.index + 1))
        .and_then(|p| lib.scenario(&p.scenario_id));

    egui::Window::new("Results")
        .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.heading(&last.name);
            ui.label(
                egui::RichText::new(format!("{:.0}", r.score))
                    .font(FontId::proportional(40.0))
                    .strong(),
            );
            match last.previous_best {
                Some(pb) if r.score > pb => {
                    ui.colored_label(
                        Color32::from_rgb(80, 220, 120),
                        format!("New personal best! (was {pb:.0})"),
                    );
                }
                Some(pb) => {
                    ui.label(format!("Personal best {pb:.0}"));
                }
                None => {
                    ui.label("First run recorded.");
                }
            }
            egui::Grid::new("stats").num_columns(2).show(ui, |ui| {
                ui.label("Accuracy");
                ui.label(format!("{:.1}%", r.accuracy * 100.0));
                ui.end_row();
                ui.label("Kills");
                ui.label(format!("{} ({:.1}/min)", r.kills, r.kills_per_min));
                ui.end_row();
                if let Some(t) = r.avg_time_to_kill {
                    ui.label("Avg time to kill");
                    ui.label(format!("{:.0} ms", t * 1000.0));
                    ui.end_row();
                }
                ui.label("Headshot rate");
                ui.label(format!("{:.1}%", r.headshot_rate * 100.0));
                ui.end_row();
                if r.moving_shots > 0 {
                    ui.label("Moving shots");
                    ui.label(r.moving_shots.to_string());
                    ui.end_row();
                }
                if r.expired > 0 {
                    ui.label("Targets missed (expired)");
                    ui.label(r.expired.to_string());
                    ui.end_row();
                }
            });
            ui.separator();
            if let Some(p) = &routine.0 {
                ui.label(format!("{} · run {}/{}", p.name, p.index + 1, p.plan.len()));
                if let Some(note) = p
                    .plan
                    .get(p.index + 1)
                    .map(|n| &n.note)
                    .filter(|n| !n.is_empty())
                {
                    ui.small(format!("Next: {note}"));
                }
            }
            ui.horizontal(|ui| {
                if let Some(def) = next_run {
                    if ui.button(format!("▶ Next: {} (Space)", def.name)).clicked()
                        || keys.just_pressed(KeyCode::Space)
                    {
                        if let Some(p) = routine.0.as_mut() {
                            p.index += 1;
                        }
                        start_run(&mut commands, &mut next, def.clone());
                    }
                } else if (ui.button("↻ Retry (Space)").clicked()
                    || keys.just_pressed(KeyCode::Space))
                    && let Some(def) = lib.scenario(&r.scenario_id)
                {
                    routine.0 = None;
                    start_run(&mut commands, &mut next, def.clone());
                }
                if ui.button("Menu").clicked() {
                    routine.0 = None;
                    next.set(AppState::Menu);
                }
            });
        });
    Ok(())
}
