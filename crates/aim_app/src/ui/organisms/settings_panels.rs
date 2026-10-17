//! Sensitivity (with converter and DPI check) and display settings forms.

use crate::persistence::settings::Settings;
use aim_core::sensitivity::Game;
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
        ui.label(
            "Press Start, move the mouse exactly 10 cm to the right along a ruler, then Stop.",
        );
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
}

pub fn display_panel(ui: &mut egui::Ui, settings: &mut Settings) {
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
