//! Sensitivity (with converter and DPI check) and display settings forms, including the
//! crosshair editor with VALORANT code import/export.

use super::{hotkey_settings, restart_key_setting};
use crate::persistence::settings::Settings;
use aim_core::crosshair::{
    CUSTOM_COLOR_INDEX, Crosshair, DOT_THICKNESS_PX, ERROR_MULTIPLIER_MAX, LINE_THICKNESS_MAX_PX,
    LineLimits, Lines, OUTLINE_THICKNESS_PX, PRESET_COLORS, Rgba, code, preset,
};
use aim_core::sensitivity::Game;
use aim_core::training::missions::Commitment;
use bevy_egui::egui;

/// Raw mouse counts accumulated while the DPI check runs.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct DpiMeter {
    pub active: bool,
    pub x: f64,
    pub y: f64,
}

impl DpiMeter {
    /// Ruler distance the player is asked to move, in cm.
    pub const DISTANCE_CM: f64 = 10.0;

    pub fn measured_dpi(&self) -> f64 {
        self.x.abs() / (Self::DISTANCE_CM / 2.54)
    }
}

pub fn sensitivity_panel(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    dpi_target: &mut u32,
    meter: &mut DpiMeter,
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
    ui.horizontal(|ui| {
        if *dpi_target == 0 {
            *dpi_target = s.dpi;
        }
        ui.label("Change DPI to");
        ui.add(egui::DragValue::new(dpi_target).speed(10).range(50..=64000));
        if ui.button("keep cm/360").clicked() {
            settings.sensitivity = s.with_dpi(*dpi_target);
        }
    });

    ui.add_space(8.0);
    ui.collapsing("DPI check (verify raw input)", |ui| {
        ui.label(format!(
            "Press Start, move the mouse exactly {} cm to the right along a ruler, then Stop.",
            DpiMeter::DISTANCE_CM
        ));
        ui.horizontal(|ui| {
            if ui
                .button(if meter.active { "Stop" } else { "Start" })
                .clicked()
            {
                if !meter.active {
                    *meter = DpiMeter::default();
                }
                meter.active = !meter.active;
            }
            ui.label(format!(
                "counts {:.0}  →  measured DPI ≈ {:.0} (configured {})",
                meter.x,
                meter.measured_dpi(),
                s.dpi
            ));
        });
        ui.label(
            "Expect a match within a few %. A large mismatch means wrong DPI or OS acceleration.",
        );
    });

    ui.add_space(8.0);
    ui.small("Converting from another game? Use the Tools tab.");
}

/// `recording_hotkey`: the global hotkey slot waiting for a key press (see `hotkey_settings`).
pub fn display_panel(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    recording_hotkey: &mut Option<usize>,
) {
    ui.heading("Crosshair");
    crosshair_editor(ui, &mut settings.crosshair);
    ui.add_space(8.0);
    ui.heading("Overlay");
    ui.add(
        egui::Slider::new(&mut settings.background_opacity, 0.0..=1.0).text("background opacity"),
    );
    ui.horizontal(|ui| {
        ui.label("Target color");
        ui.color_edit_button_rgb(&mut settings.target_color);
    });
    ui.checkbox(
        &mut settings.offscreen_arrows,
        "Arrows towards off-screen targets",
    );
    ui.checkbox(&mut settings.fullscreen, "Borderless fullscreen (restart)");
    ui.checkbox(
        &mut settings.always_on_top,
        "Always on top (restart; X11/Windows)",
    );
    ui.horizontal(|ui| {
        ui.label("Queue warm-up length");
        ui.add(
            egui::DragValue::new(&mut settings.warmup_minutes)
                .range(1..=30)
                .suffix(" min"),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Daily goal");
        ui.add(
            egui::DragValue::new(&mut settings.daily_goal_minutes)
                .range(5.0..=120.0)
                .suffix(" min"),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Mission commitment");
        for c in Commitment::ALL {
            ui.selectable_value(&mut settings.mission_commitment, c, c.label());
        }
    })
    .response
    .on_hover_text("Low: 2 missions a day. Medium: 3, one of them mastery. High: 4, two mastery, closer to your bests. Changing it regenerates today's missions.");
    ui.add_space(8.0);
    ui.heading("Gameplay keys");
    restart_key_setting::restart_key_setting(ui, &mut settings.restart_key);
    ui.add_space(8.0);
    ui.heading("Global hotkeys (Windows/X11)");
    hotkey_settings::hotkey_settings(ui, settings, recording_hotkey);
    ui.label("Wayland/Hyprland: bind `aimtrainer toggle`, `aimtrainer warmup` and `aimtrainer micro` in your compositor (see README).");
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
    fn dpi_meter_converts_counts_over_ten_centimeters() {
        // 800 DPI over 10 cm = 800 * 10 / 2.54 ≈ 3149.6 counts.
        let m = DpiMeter {
            active: false,
            x: -3149.6,
            y: 0.0,
        };
        assert!((m.measured_dpi() - 800.0).abs() < 0.1);
    }
}
