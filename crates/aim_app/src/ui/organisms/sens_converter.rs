//! Sensitivity converter between games. The math lives in
//! [`aim_core::sensitivity::matching`]; this view only collects inputs and shows results.

use crate::persistence::settings::Settings;
use aim_core::sensitivity::matching::{self, Matching, Side};
use aim_core::sensitivity::{FovAxis, Game, Sensitivity};
use bevy_egui::egui;

/// Matching method picked in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Distance360,
    Mdh0,
    Mdh75,
    Mdh100,
    Mdv0,
    Custom,
}

impl Method {
    pub const ALL: [Method; 6] = [
        Method::Distance360,
        Method::Mdh0,
        Method::Mdh75,
        Method::Mdh100,
        Method::Mdv0,
        Method::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Method::Distance360 => "360° distance (same cm/360)",
            Method::Mdh0 => "Monitor distance horizontal 0%",
            Method::Mdh75 => "Monitor distance horizontal 75%",
            Method::Mdh100 => "Monitor distance horizontal 100%",
            Method::Mdv0 => "Monitor distance vertical 0%",
            Method::Custom => "Monitor distance, custom %",
        }
    }
}

/// Display aspect ratios offered in the UI.
const ASPECTS: [(&str, f64); 4] = [
    ("16:9", 16.0 / 9.0),
    ("16:10", 16.0 / 10.0),
    ("4:3", 4.0 / 3.0),
    ("21:9", 21.0 / 9.0),
];

/// Inputs of the converter panel; lives across frames in egui memory.
#[derive(Debug, Clone, PartialEq)]
pub struct ConverterState {
    pub source: Sensitivity,
    /// In-game FOV of the source game (used only when that game's FOV is adjustable).
    pub source_fov_deg: f64,
    pub method: Method,
    pub custom_axis: FovAxis,
    pub custom_percent: f64,
    pub monitor_aspect: f64,
}

impl ConverterState {
    pub fn from_sensitivity(source: Sensitivity) -> Self {
        Self {
            source,
            source_fov_deg: default_fov_deg(source.game).unwrap_or(0.0),
            method: Method::Distance360,
            custom_axis: FovAxis::Horizontal,
            custom_percent: 75.0,
            monitor_aspect: 16.0 / 9.0,
        }
    }

    /// The core matching rule for the selected method.
    pub fn matching(&self) -> Matching {
        match self.method {
            Method::Distance360 => Matching::Distance360,
            Method::Mdh0 => Matching::MDH_0,
            Method::Mdh75 => Matching::MDH_75,
            Method::Mdh100 => Matching::MDH_100,
            Method::Mdv0 => Matching::MDV_0,
            Method::Custom => Matching::Monitor {
                axis: self.custom_axis,
                percent: self.custom_percent,
            },
        }
    }

    /// The source FOV passed to the core: `None` (the default) for fixed-FOV games.
    fn source_fov(&self) -> Option<f64> {
        self.source
            .game
            .fov_model()
            .filter(|m| m.adjustable)
            .map(|_| self.source_fov_deg)
    }

    pub fn convert(&self, target: Game) -> Result<Sensitivity, matching::MatchError> {
        matching::convert(
            &self.source,
            self.source_fov(),
            Side::new(target, None),
            self.matching(),
            self.monitor_aspect,
        )
    }
}

impl Default for ConverterState {
    fn default() -> Self {
        Self::from_sensitivity(Sensitivity::default())
    }
}

fn default_fov_deg(game: Game) -> Option<f64> {
    game.fov_model().map(|m| m.default_deg)
}

/// Converter form plus a results table for every game. "Use" replaces the trainer's
/// sensitivity with that row.
pub fn sens_converter(ui: &mut egui::Ui, settings: &mut Settings, state: &mut ConverterState) {
    egui::Grid::new("sens_converter_inputs")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("From game");
            egui::ComboBox::from_id_salt("sens_converter_game")
                .selected_text(state.source.game.display_name())
                .show_ui(ui, |ui| {
                    for g in Game::ALL {
                        if ui
                            .selectable_label(state.source.game == g, g.display_name())
                            .clicked()
                            && state.source.game != g
                        {
                            state.source = state.source.convert_to(g);
                            state.source_fov_deg = default_fov_deg(g).unwrap_or(0.0);
                        }
                    }
                });
            ui.end_row();

            ui.label("Sensitivity")
                .on_hover_text(state.source.game.profile().value_note);
            ui.add(
                egui::DragValue::new(&mut state.source.value)
                    .speed(0.001)
                    .range(0.0001..=1000.0)
                    .max_decimals(4),
            );
            ui.end_row();

            ui.label("DPI");
            ui.add(
                egui::DragValue::new(&mut state.source.dpi)
                    .speed(10)
                    .range(50..=64000),
            );
            ui.end_row();

            ui.label("FOV");
            match state.source.game.fov_model() {
                Some(m) if m.adjustable => {
                    ui.add(
                        egui::DragValue::new(&mut state.source_fov_deg)
                            .speed(0.5)
                            .range(1.0..=179.0)
                            .suffix("°"),
                    );
                }
                Some(m) => {
                    ui.label(format!("fixed at {}°", m.default_deg));
                }
                None => {
                    ui.label("unknown (360° matching only)");
                }
            }
            ui.end_row();

            ui.label("Matching");
            egui::ComboBox::from_id_salt("sens_converter_method")
                .selected_text(state.method.label())
                .show_ui(ui, |ui| {
                    for m in Method::ALL {
                        ui.selectable_value(&mut state.method, m, m.label());
                    }
                });
            ui.end_row();

            if state.method == Method::Custom {
                ui.label("Custom");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut state.custom_percent)
                            .speed(1.0)
                            .range(0.0..=500.0)
                            .suffix(" %"),
                    );
                    ui.radio_value(&mut state.custom_axis, FovAxis::Horizontal, "horizontal");
                    ui.radio_value(&mut state.custom_axis, FovAxis::Vertical, "vertical");
                });
                ui.end_row();
            }

            ui.label("Monitor");
            let aspect_label = ASPECTS
                .iter()
                .find(|(_, a)| (*a - state.monitor_aspect).abs() < 1e-9)
                .map_or("custom", |(l, _)| *l);
            egui::ComboBox::from_id_salt("sens_converter_aspect")
                .selected_text(aspect_label)
                .show_ui(ui, |ui| {
                    for (label, aspect) in ASPECTS {
                        ui.selectable_value(&mut state.monitor_aspect, aspect, label);
                    }
                });
            ui.end_row();
        });
    if ui.button("Load my trainer sensitivity").clicked() {
        *state = ConverterState {
            method: state.method,
            custom_axis: state.custom_axis,
            custom_percent: state.custom_percent,
            monitor_aspect: state.monitor_aspect,
            ..ConverterState::from_sensitivity(settings.sensitivity)
        };
    }

    ui.add_space(6.0);
    ui.label("Target games use their default FOV.");
    egui::Grid::new("sens_converter_results")
        .num_columns(4)
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Game");
            ui.strong("Sensitivity");
            ui.strong("cm/360");
            ui.label("");
            ui.end_row();
            for g in Game::ALL {
                ui.label(g.display_name());
                match state.convert(g) {
                    Ok(out) => {
                        ui.label(format!("{:.4}", out.value));
                        ui.label(format!("{:.2}", out.cm_per_360()));
                        let current = settings.sensitivity == out;
                        if ui
                            .add_enabled(!current, egui::Button::new("Use"))
                            .on_hover_text("Set this as the trainer's sensitivity")
                            .clicked()
                        {
                            settings.sensitivity = out;
                        }
                    }
                    Err(e) => {
                        ui.label("—").on_hover_text(e.to_string());
                        ui.label("");
                        ui.label("");
                    }
                }
                ui.end_row();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methods_map_to_core_matching_rules() {
        let mut s = ConverterState {
            method: Method::Mdh0,
            ..ConverterState::default()
        };
        assert_eq!(s.matching(), Matching::MDH_0);
        s.method = Method::Mdh75;
        assert_eq!(s.matching(), Matching::MDH_75);
        s.method = Method::Custom;
        s.custom_axis = FovAxis::Vertical;
        s.custom_percent = 75.0;
        assert_eq!(
            s.matching(),
            Matching::Monitor {
                axis: FovAxis::Vertical,
                percent: 75.0
            }
        );
    }

    #[test]
    fn fixed_fov_games_ignore_the_fov_input() {
        let mut s = ConverterState::from_sensitivity(Sensitivity::valorant(0.4, 800));
        s.source_fov_deg = 60.0;
        assert_eq!(s.source_fov(), None);
        s.source = Sensitivity {
            game: Game::Overwatch2,
            value: 5.0,
            dpi: 800,
        };
        assert_eq!(s.source_fov(), Some(60.0));
    }

    #[test]
    fn deadlock_rows_fail_only_for_monitor_matching() {
        let mut s = ConverterState::default();
        assert!(s.convert(Game::Deadlock).is_ok());
        s.method = Method::Mdh0;
        assert!(s.convert(Game::Deadlock).is_err());
        assert!(s.convert(Game::Cs2).is_ok());
    }
}
