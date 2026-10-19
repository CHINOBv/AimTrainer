//! The Settings sub-tabs other than Feedback and Audio (see `feedback_settings`):
//! sensitivity (with the DPI check), crosshair (with VALORANT code import/export),
//! overlay, keys and training. Rows go through [`SettingsRows`] so the search can filter
//! them.

use super::dpi_check::DpiMeter;
use super::{hotkey_settings, restart_key_setting};
use crate::persistence::settings::{EnemyHighlight, Settings, TargetVisibility};
use crate::ui::molecules::SettingsRows;
use aim_core::crosshair::{
    CUSTOM_COLOR_INDEX, Crosshair, DOT_THICKNESS_PX, ERROR_MULTIPLIER_MAX, LINE_THICKNESS_MAX_PX,
    LineLimits, Lines, OUTLINE_THICKNESS_PX, PRESET_COLORS, Rgba, code, preset,
};
use aim_core::sensitivity::Game;
use aim_core::sensitivity::randomizer::{
    MAX_MULTIPLIER, MIN_INTERVAL_SECS, MIN_MULTIPLIER, RandomizerConfig,
};
use aim_core::training::ergonomics::{BreakPolicy, EVIDENCE_NOTE};
use aim_core::training::missions::Commitment;
use aim_core::training::warmup::{MIN_DOSE_SESSIONS, MIN_SESSION_RUNS, WarmupDose};
use bevy_egui::egui;

/// The Settings sub-tabs.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    #[default]
    Sensitivity,
    Crosshair,
    Feedback,
    Audio,
    Overlay,
    Keys,
    Training,
}

impl SettingsTab {
    /// Every sub-tab with its label, in display order.
    pub const ALL: [(SettingsTab, &'static str); 7] = [
        (SettingsTab::Sensitivity, "Sensitivity"),
        (SettingsTab::Crosshair, "Crosshair"),
        (SettingsTab::Feedback, "Feedback"),
        (SettingsTab::Audio, "Audio"),
        (SettingsTab::Overlay, "Overlay"),
        (SettingsTab::Keys, "Keys"),
        (SettingsTab::Training, "Training"),
    ];
}

/// Put every setting shown on `tab` back to its default. Settings on other tabs, and
/// state that is not a preference (favourites, the first-run flag), are kept.
pub fn reset_section(settings: &mut Settings, tab: SettingsTab) {
    let d = Settings::default();
    match tab {
        SettingsTab::Sensitivity => {
            settings.sensitivity = d.sensitivity;
            settings.ads_multiplier = d.ads_multiplier;
        }
        SettingsTab::Crosshair => settings.crosshair = d.crosshair,
        SettingsTab::Feedback => {
            settings.hit_feedback = d.hit_feedback;
            settings.feel = d.feel;
        }
        SettingsTab::Audio => settings.sound = d.sound,
        SettingsTab::Overlay => {
            settings.background_opacity = d.background_opacity;
            settings.target_color = d.target_color;
            settings.target_visibility = d.target_visibility;
            settings.offscreen_arrows = d.offscreen_arrows;
            settings.fullscreen = d.fullscreen;
            settings.always_on_top = d.always_on_top;
        }
        SettingsTab::Keys => {
            settings.restart_key = d.restart_key;
            settings.overlay_hotkey = d.overlay_hotkey;
            settings.warmup_hotkey = d.warmup_hotkey;
            settings.micro_hotkey = d.micro_hotkey;
        }
        SettingsTab::Training => {
            settings.warmup_minutes = d.warmup_minutes;
            settings.personal_warmup = d.personal_warmup;
            settings.daily_goal_minutes = d.daily_goal_minutes;
            settings.mission_commitment = d.mission_commitment;
            settings.arena_coaching = d.arena_coaching;
            settings.sens_randomizer = d.sens_randomizer;
            settings.breaks = d.breaks;
        }
    }
}

/// Game picker that keeps the physical sensitivity (cm/360) when the game changes.
pub fn game_combo(ui: &mut egui::Ui, settings: &mut Settings) {
    let s = &mut settings.sensitivity;
    egui::ComboBox::from_id_salt("game")
        .selected_text(s.game.display_name())
        .show_ui(ui, |ui| {
            for g in Game::ALL {
                if ui.selectable_label(s.game == g, g.display_name()).clicked() && s.game != g {
                    *s = s.convert_to(g);
                }
            }
        });
}

pub fn sensitivity_panel(
    ui: &mut egui::Ui,
    rows: &mut SettingsRows,
    settings: &mut Settings,
    dpi_target: &mut u32,
    meter: &mut DpiMeter,
) {
    rows.section(ui, "Sensitivity");
    rows.row(ui, "Game", |ui| game_combo(ui, settings));
    rows.row(ui, "In-game sensitivity", |ui| {
        ui.add(
            egui::DragValue::new(&mut settings.sensitivity.value)
                .speed(0.001)
                .range(0.001..=100.0)
                .max_decimals(4),
        );
    });
    rows.row(ui, "Mouse DPI", |ui| {
        ui.add(
            egui::DragValue::new(&mut settings.sensitivity.dpi)
                .speed(10)
                .range(50..=64000),
        );
    });
    rows.row(ui, "ADS multiplier", |ui| {
        ui.add(
            egui::DragValue::new(&mut settings.ads_multiplier)
                .speed(0.01)
                .range(0.01..=5.0),
        );
    });
    let s = settings.sensitivity;
    rows.block(ui, "cm/360 in/360 eDPI degrees per count", |ui| {
        ui.label(format!(
            "{:.2} cm/360  ·  {:.2} in/360  ·  eDPI {:.0}  ·  {:.5}°/count",
            s.cm_per_360(),
            s.inches_per_360(),
            s.edpi(),
            s.deg_per_count()
        ));
    });
    if *dpi_target == 0 {
        *dpi_target = s.dpi;
    }
    rows.row(ui, "Change DPI to", |ui| {
        ui.add(egui::DragValue::new(dpi_target).speed(10).range(50..=64000));
        if ui.button("keep cm/360").clicked() {
            settings.sensitivity = s.with_dpi(*dpi_target);
        }
    });
    rows.block(
        ui,
        "Measure DPI check verify raw input mouse acceleration ruler",
        |ui| {
            ui.collapsing("Measure DPI (verify raw input)", |ui| {
                super::dpi_check::dpi_check(ui, settings, meter);
            });
        },
    );
    if !rows.searching() {
        ui.small("Converting from another game? Use the Tools tab.");
    }
}

pub fn crosshair_panel(ui: &mut egui::Ui, rows: &mut SettingsRows, settings: &mut Settings) {
    rows.section(ui, "Crosshair");
    rows.block(
        ui,
        "Crosshair preset color outlines center dot inner outer lines thickness length offset opacity VALORANT code import export",
        |ui| crosshair_editor(ui, &mut settings.crosshair),
    );
}

pub fn overlay_panel(ui: &mut egui::Ui, rows: &mut SettingsRows, settings: &mut Settings) {
    rows.section(ui, "Overlay");
    rows.row(ui, "Background opacity", |ui| {
        ui.add(egui::Slider::new(
            &mut settings.background_opacity,
            0.0..=1.0,
        ));
    });
    rows.row(ui, "Target color", |ui| {
        ui.color_edit_button_rgb(&mut settings.target_color);
    });
    rows.block(
        ui,
        "Enemy highlight colour color-blind target outline environment tint",
        |ui| target_visibility(ui, settings),
    );
    rows.check(
        ui,
        "Arrows towards off-screen targets",
        &mut settings.offscreen_arrows,
        "Scenarios only; arenas have their own option under Feedback.",
    );
    rows.check(
        ui,
        "Borderless fullscreen (restart)",
        &mut settings.fullscreen,
        "",
    );
    rows.check(
        ui,
        "Always on top (restart; X11/Windows)",
        &mut settings.always_on_top,
        "",
    );
}

/// `recording_hotkey`: the global hotkey slot waiting for a key press (see `hotkey_settings`).
pub fn keys_panel(
    ui: &mut egui::Ui,
    rows: &mut SettingsRows,
    settings: &mut Settings,
    recording_hotkey: &mut Option<usize>,
) {
    rows.section(ui, "Gameplay keys");
    rows.block(ui, "Restart run key", |ui| {
        restart_key_setting::restart_key_setting(ui, &mut settings.restart_key)
    });
    rows.section(ui, "Global hotkeys (Windows/X11)");
    rows.block(
        ui,
        "Global hotkeys toggle overlay queue warm-up buy-phase micro drill Wayland Hyprland",
        |ui| {
            hotkey_settings::hotkey_settings(ui, settings, recording_hotkey);
            ui.label("Wayland/Hyprland: bind `aimtrainer toggle`, `aimtrainer warmup` and `aimtrainer micro` in your compositor (see README).");
        },
    );
}

/// The Training section. `dose` is the personal warm-up dose, which replaces the queue
/// warm-up length when known.
pub fn training_panel(
    ui: &mut egui::Ui,
    rows: &mut SettingsRows,
    settings: &mut Settings,
    dose: Option<&WarmupDose>,
) {
    rows.section(ui, "Training");
    rows.row(ui, "Queue warm-up length", |ui| {
        ui.add(
            egui::DragValue::new(&mut settings.warmup_minutes)
                .range(1..=30)
                .suffix(" min"),
        );
        ui.checkbox(&mut settings.personal_warmup, "Use my personal dose")
            .on_hover_text("When on, the queue warm-up lasts as long as your scores need to level off, once there is enough history. Off keeps the length set here.");
        if settings.personal_warmup {
            ui.label(personal_warmup_text(dose));
        }
    });
    rows.row(ui, "Daily goal", |ui| {
        ui.add(
            egui::DragValue::new(&mut settings.daily_goal_minutes)
                .range(5.0..=120.0)
                .suffix(" min"),
        );
    });
    rows.row(ui, "Mission commitment", |ui| {
        for c in Commitment::ALL {
            ui.selectable_value(&mut settings.mission_commitment, c, c.label())
                .on_hover_text("Low: 2 missions a day. Medium: 3, one of them mastery. High: 4, two mastery, closer to your bests. Changing it regenerates today's missions.");
        }
    });
    rows.check(
        ui,
        "Live coaching overlay in arenas",
        &mut settings.arena_coaching,
        "Crosshair height against head level, and whether you are still enough to shoot accurately.",
    );
    rows.block(ui, "Sensitivity randomiser drill", |ui| {
        sens_randomizer(ui, &mut settings.sens_randomizer)
    });
}

/// Break prompts and the weekly volume cap. Every default is an estimate.
pub fn breaks_panel(ui: &mut egui::Ui, rows: &mut SettingsRows, b: &mut BreakPolicy) {
    rows.section(ui, "Breaks and wrist care");
    rows.check(ui, "Break reminders", &mut b.enabled, EVIDENCE_NOTE);
    rows.row(ui, "Break after", |ui| {
        ui.add(
            egui::DragValue::new(&mut b.continuous_minutes)
                .range(10..=240)
                .suffix(" min of play"),
        );
    });
    rows.row(ui, "Break length", |ui| {
        ui.add(
            egui::DragValue::new(&mut b.break_minutes)
                .range(1..=60)
                .suffix(" min"),
        )
        .on_hover_text("Time away from runs that counts as a break and resets the count.");
    });
    rows.row(ui, "Weekly cap", |ui| {
        ui.add(
            egui::DragValue::new(&mut b.weekly_cap_minutes)
                .range(30..=5000)
                .suffix(" min / 7 days"),
        )
        .on_hover_text("An informational note when your aim training in the last 7 days reaches this. The default (about 1 h a day) is an estimate.");
    });
}

/// The personal warm-up line shown next to the queue warm-up length.
pub fn personal_warmup_text(dose: Option<&WarmupDose>) -> String {
    match dose {
        Some(d) => format!(
            "Personal warm-up: {:.0} min (from {} sessions), used instead of this length",
            d.minutes, d.sessions
        ),
        None => format!(
            "Personal warm-up: not enough data yet ({MIN_DOSE_SESSIONS} sessions of {MIN_SESSION_RUNS}+ runs)"
        ),
    }
}

/// The sensitivity randomiser drill. It only scales the look while playing; the saved
/// sensitivity is never changed.
fn sens_randomizer(ui: &mut egui::Ui, cfg: &mut RandomizerConfig) {
    ui.checkbox(&mut cfg.enabled, "Sensitivity randomiser drill")
        .on_hover_text("Every interval your look sensitivity changes to a random multiplier within the range. Your saved sensitivity is never changed. No study shows it helps; use it as an occasional robustness drill.");
    if !cfg.enabled {
        return;
    }
    ui.horizontal(|ui| {
        let range = MIN_MULTIPLIER..=MAX_MULTIPLIER;
        ui.label("Range ×");
        ui.add(
            egui::DragValue::new(&mut cfg.min)
                .range(range.clone())
                .speed(0.01)
                .fixed_decimals(2),
        );
        ui.label("to ×");
        ui.add(
            egui::DragValue::new(&mut cfg.max)
                .range(range)
                .speed(0.01)
                .fixed_decimals(2),
        );
        ui.label("every");
        ui.add(
            egui::DragValue::new(&mut cfg.interval_secs)
                .range(MIN_INTERVAL_SECS..=120.0)
                .suffix(" s"),
        );
        ui.label("easing");
        let max_ease = cfg.interval_secs.max(MIN_INTERVAL_SECS);
        ui.add(
            egui::DragValue::new(&mut cfg.smoothing_secs)
                .range(0.0..=max_ease)
                .speed(0.1)
                .suffix(" s"),
        );
    });
}

/// Enemy highlight presets (VALORANT's colour-blind options), target outline and
/// environment tint.
fn target_visibility(ui: &mut egui::Ui, settings: &mut Settings) {
    ui.horizontal(|ui| {
        ui.label("Enemy highlight");
        let current = EnemyHighlight::of(settings.target_color);
        egui::ComboBox::from_id_salt("enemy-highlight")
            .selected_text(current.label())
            .show_ui(ui, |ui| {
                for (h, rgb) in EnemyHighlight::PRESETS {
                    if ui.selectable_label(current == h, h.label()).clicked() {
                        settings.target_color = rgb;
                    }
                }
                ui.add_enabled(false, egui::Button::selectable(current == EnemyHighlight::Custom, "Custom (pick a colour above)"));
            });
    })
    .response
    .on_hover_text("VALORANT's enemy highlight options. Purple suits tritanopia; yellow suits deuteranopia and protanopia.");
    let vis = &mut settings.target_visibility;
    ui.horizontal(|ui| {
        ui.checkbox(&mut vis.outline, "Target outline");
        ui.add_enabled_ui(vis.outline, |ui| {
            ui.color_edit_button_rgb(&mut vis.outline_color);
            ui.add(
                egui::Slider::new(&mut vis.outline_percent, TargetVisibility::OUTLINE_PERCENT)
                    .suffix("%")
                    .text("thickness"),
            );
        });
    })
    .response
    .on_hover_text("A solid rim around every target (never on decoys). Applies to targets spawned after the change.");
    ui.horizontal(|ui| {
        let mut tinted = vis.environment_tint.is_some();
        if ui.checkbox(&mut tinted, "Environment tint").changed() {
            vis.environment_tint = tinted.then_some([0.35, 0.4, 0.5]);
        }
        if let Some(tint) = vis.environment_tint.as_mut() {
            ui.color_edit_button_rgb(tint);
        }
    })
    .response
    .on_hover_text("Colour of the floor grid and play area, to raise contrast with the targets.");
}

/// Text and feedback of the "Import VALORANT code" field, kept in egui memory.
#[derive(Debug, Clone, Default)]
struct ImportState {
    text: String,
    message: Option<Result<(), String>>,
}

fn crosshair_editor(ui: &mut egui::Ui, ch: &mut Crosshair) {
    ui.horizontal(|ui| {
        ui.label("Preset");
        egui::ComboBox::from_id_salt("crosshair_preset")
            .selected_text(
                preset::all()
                    .iter()
                    .find(|p| p.crosshair() == *ch)
                    .map_or("custom", |p| p.name.as_str()),
            )
            .show_ui(ui, |ui| {
                for p in preset::all() {
                    if ui.selectable_label(p.crosshair() == *ch, &p.name).clicked() {
                        *ch = p.crosshair();
                    }
                }
            });
    });
    code_import_export(ui, ch);

    egui::Grid::new("crosshair").num_columns(2).show(ui, |ui| {
        ui.label("Color");
        ui.horizontal(|ui| {
            let name = PRESET_COLORS
                .get(usize::from(ch.color_index))
                .map_or("Custom", |(n, _)| *n);
            egui::ComboBox::from_id_salt("crosshair_color")
                .selected_text(name)
                .show_ui(ui, |ui| {
                    for (i, (n, _)) in PRESET_COLORS.iter().enumerate() {
                        ui.selectable_value(&mut ch.color_index, i as u8, *n);
                    }
                    ui.selectable_value(&mut ch.color_index, CUSTOM_COLOR_INDEX, "Custom");
                });
            if ch.color_index == CUSTOM_COLOR_INDEX {
                let c = ch.custom_color;
                let mut rgba = [c.r, c.g, c.b, c.a];
                if ui.color_edit_button_srgba_unmultiplied(&mut rgba).changed() {
                    let [r, g, b, a] = rgba;
                    ch.custom_color = Rgba { r, g, b, a };
                }
            }
        });
        ui.end_row();

        ui.checkbox(&mut ch.outlines, "Outlines");
        ui.horizontal(|ui| {
            ui.add_enabled_ui(ch.outlines, |ui| {
                ui.add(
                    egui::Slider::new(&mut ch.outline_thickness_px, OUTLINE_THICKNESS_PX)
                        .text("px"),
                );
                ui.add(opacity_slider(&mut ch.outline_opacity));
            });
        });
        ui.end_row();

        ui.checkbox(&mut ch.center_dot, "Center dot");
        ui.horizontal(|ui| {
            ui.add_enabled_ui(ch.center_dot, |ui| {
                ui.add(egui::Slider::new(&mut ch.dot_thickness_px, DOT_THICKNESS_PX).text("px"));
                ui.add(opacity_slider(&mut ch.dot_opacity));
            });
        });
        ui.end_row();
    });
    ui.collapsing("Inner lines", |ui| {
        lines_editor(ui, "inner", &mut ch.inner, LineLimits::INNER)
    });
    ui.collapsing("Outer lines", |ui| {
        lines_editor(ui, "outer", &mut ch.outer, LineLimits::OUTER)
    });
    ui.collapsing("Code-only options (not simulated)", |ui| {
        ui.label("Kept so VALORANT codes round-trip; the trainer draws a static crosshair.");
        ui.checkbox(
            &mut ch.fade_with_firing_error,
            "Fade crosshair with firing error",
        );
        ui.checkbox(&mut ch.show_spectated, "Show spectated player's crosshair");
        ui.checkbox(
            &mut ch.override_firing_offset,
            "Override firing error offset with crosshair offset",
        );
        for (name, lines) in [("Inner", &mut ch.inner), ("Outer", &mut ch.outer)] {
            ui.horizontal(|ui| {
                ui.label(name);
                ui.checkbox(&mut lines.movement_error, "movement error");
                ui.add(error_multiplier(&mut lines.movement_error_multiplier));
                ui.checkbox(&mut lines.firing_error, "firing error");
                ui.add(error_multiplier(&mut lines.firing_error_multiplier));
            });
        }
    });
}

fn opacity_slider(value: &mut f64) -> egui::Slider<'_> {
    egui::Slider::new(value, 0.0..=1.0)
        .step_by(0.001)
        .text("opacity")
}

fn error_multiplier(value: &mut f64) -> egui::DragValue<'_> {
    egui::DragValue::new(value)
        .speed(0.01)
        .range(0.0..=ERROR_MULTIPLIER_MAX)
        .prefix("×")
}

fn lines_editor(ui: &mut egui::Ui, salt: &str, lines: &mut Lines, limits: LineLimits) {
    ui.checkbox(&mut lines.show, "Show");
    ui.add_enabled_ui(lines.show, |ui| {
        egui::Grid::new(("crosshair_lines", salt))
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Thickness (px)");
                ui.add(egui::Slider::new(
                    &mut lines.thickness_px,
                    0..=LINE_THICKNESS_MAX_PX,
                ));
                ui.end_row();
                ui.label("Length (px)");
                ui.add(egui::Slider::new(
                    &mut lines.length_px,
                    0..=limits.max_length_px,
                ));
                ui.end_row();
                ui.checkbox(&mut lines.separate_vertical, "Vertical length");
                ui.add_enabled(
                    lines.separate_vertical,
                    egui::Slider::new(&mut lines.vertical_length_px, 0..=limits.max_length_px),
                );
                ui.end_row();
                ui.label("Offset (px)");
                ui.add(egui::Slider::new(
                    &mut lines.offset_px,
                    0..=limits.max_offset_px,
                ));
                ui.end_row();
                ui.label("Opacity");
                ui.add(opacity_slider(&mut lines.opacity));
                ui.end_row();
            });
    });
}

fn code_import_export(ui: &mut egui::Ui, ch: &mut Crosshair) {
    let id = egui::Id::new("crosshair_code_import");
    let mut import = ui
        .data_mut(|d| d.get_temp::<ImportState>(id))
        .unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label("Import VALORANT code");
        ui.add(
            egui::TextEdit::singleline(&mut import.text)
                .hint_text("0;P;c;5;h;0;…")
                .desired_width(220.0),
        );
        if ui.button("Apply").clicked() {
            import.message = Some(match code::parse(&import.text) {
                Ok(parsed) => {
                    *ch = parsed;
                    import.text.clear();
                    Ok(())
                }
                Err(e) => Err(e.to_string()),
            });
        }
    });
    match &import.message {
        Some(Ok(())) => {
            ui.label("Imported the primary crosshair.");
        }
        Some(Err(e)) => {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("Could not import: {e}"),
            );
        }
        None => {}
    }
    ui.data_mut(|d| d.insert_temp(id, import));

    let exported = code::encode(ch);
    ui.horizontal(|ui| {
        ui.label("Your code");
        ui.add(egui::TextEdit::singleline(&mut exported.as_str()).desired_width(220.0));
        if ui.button("Copy code").clicked() {
            ui.ctx().copy_text(exported.clone());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_personal_warmup_line_names_the_dose_and_its_sessions() {
        let dose = WarmupDose {
            minutes: 7.0,
            sessions: 4,
        };
        assert_eq!(
            personal_warmup_text(Some(&dose)),
            "Personal warm-up: 7 min (from 4 sessions), used instead of this length"
        );
        assert!(personal_warmup_text(None).contains("not enough data"));
    }

    /// Every preference changed away from its default.
    fn changed() -> Settings {
        use crate::persistence::settings::FeelPreset;
        let mut s = Settings {
            sensitivity: aim_core::sensitivity::Sensitivity::valorant(0.9, 1600),
            ads_multiplier: 0.8,
            background_opacity: 0.1,
            fullscreen: false,
            always_on_top: false,
            overlay_hotkey: "F1".into(),
            warmup_hotkey: "F2".into(),
            micro_hotkey: "F3".into(),
            restart_key: "F5".into(),
            warmup_minutes: 9,
            personal_warmup: false,
            daily_goal_minutes: 50.0,
            target_color: [0.0, 1.0, 0.0],
            offscreen_arrows: false,
            mission_commitment: Commitment::High,
            arena_coaching: true,
            ..Settings::default()
        };
        s.crosshair.center_dot = !s.crosshair.center_dot;
        s.sound.volume_percent = 7;
        s.sound.footsteps = false;
        s.hit_feedback.marker_ms = 300;
        s.feel.apply_preset(FeelPreset::Juicy);
        s.target_visibility.outline = true;
        s.sens_randomizer.enabled = true;
        s.breaks.enabled = false;
        s
    }

    #[test]
    fn resetting_a_section_only_touches_its_own_settings() {
        let mut s = changed();
        reset_section(&mut s, SettingsTab::Audio);
        assert_eq!(s.sound, Settings::default().sound);
        assert_eq!(
            Settings {
                sound: changed().sound,
                ..s.clone()
            },
            changed(),
            "nothing outside Audio changed"
        );

        let mut s = changed();
        reset_section(&mut s, SettingsTab::Feedback);
        assert_eq!(s.feel, Settings::default().feel);
        assert_eq!(s.hit_feedback, Settings::default().hit_feedback);
        assert_eq!(s.sound, changed().sound);

        let mut s = changed();
        reset_section(&mut s, SettingsTab::Keys);
        assert_eq!(s.restart_key, "KeyR");
        assert_eq!(s.sensitivity, changed().sensitivity);
    }

    #[test]
    fn resetting_every_section_restores_every_default() {
        let mut s = changed();
        for (tab, _) in SettingsTab::ALL {
            reset_section(&mut s, tab);
        }
        assert_eq!(s, Settings::default());
    }
}
