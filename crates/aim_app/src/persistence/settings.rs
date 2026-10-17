//! Persistent user settings and the folders the app reads and writes.

use aim_core::crosshair::{CUSTOM_COLOR_INDEX, Crosshair, PRESET_COLORS, Rgba};
use aim_core::sensitivity::Sensitivity;
use bevy::prelude::Resource;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Resource, Debug, Clone)]
pub struct Paths {
    pub settings_file: PathBuf,
    pub scenarios_dir: PathBuf,
    pub routines_dir: PathBuf,
    pub benchmarks_dir: PathBuf,
    pub history_file: PathBuf,
}

impl Paths {
    pub fn resolve() -> Self {
        let (config, data) = match ProjectDirs::from("dev", "CustomAimTrainer", "aimtrainer") {
            Some(d) => (d.config_dir().to_path_buf(), d.data_dir().to_path_buf()),
            None => (
                PathBuf::from("aimtrainer-config"),
                PathBuf::from("aimtrainer-data"),
            ),
        };
        let paths = Self {
            settings_file: config.join("settings.toml"),
            scenarios_dir: config.join("scenarios"),
            routines_dir: config.join("routines"),
            benchmarks_dir: config.join("benchmarks"),
            history_file: data.join("history.csv"),
        };
        for dir in [
            &paths.scenarios_dir,
            &paths.routines_dir,
            &paths.benchmarks_dir,
            &data,
        ] {
            if let Err(e) = fs::create_dir_all(dir) {
                bevy::log::warn!("could not create {}: {e}", dir.display());
            }
        }
        paths
    }
}

#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Your in-game sensitivity. Rotation uses its exact degrees per count.
    pub sensitivity: Sensitivity,
    /// VALORANT "ADS sensitivity multiplier" (1.0 = 100% horizontal monitor match).
    pub ads_multiplier: f64,
    #[serde(deserialize_with = "deserialize_crosshair")]
    pub crosshair: Crosshair,
    /// 0 = fully see-through overlay, 1 = opaque background.
    pub background_opacity: f32,
    pub fullscreen: bool,
    pub always_on_top: bool,
    /// Global hotkey on Windows/X11 (e.g. "ctrl+shift+F10"). On Wayland bind
    /// `aimtrainer toggle` in your compositor instead. Requires a restart.
    pub overlay_hotkey: String,
    /// Global hotkey that starts a queue warm-up (Windows/X11; `aimtrainer warmup` on Wayland).
    pub warmup_hotkey: String,
    /// Global hotkey for a buy-phase micro drill (Windows/X11; `aimtrainer micro` on Wayland).
    pub micro_hotkey: String,
    /// Length of the queue warm-up, minutes.
    pub warmup_minutes: u32,
    /// Daily training goal, minutes (30–50 min captures most daily gains, Listman 2021).
    pub daily_goal_minutes: f32,
    pub target_color: [f32; 3],
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sensitivity: Sensitivity::default(),
            ads_multiplier: 1.0,
            crosshair: Crosshair::default(),
            background_opacity: 0.92,
            fullscreen: true,
            always_on_top: true,
            overlay_hotkey: "ctrl+shift+F10".to_owned(),
            warmup_hotkey: "ctrl+shift+F9".to_owned(),
            micro_hotkey: "ctrl+shift+F8".to_owned(),
            warmup_minutes: 5,
            daily_goal_minutes: 30.0,
            target_color: [1.0, 0.27, 0.33],
        }
    }
}

/// Legacy `[crosshair]` keys written before the core crosshair model existed.
const LEGACY_CROSSHAIR_KEYS: [&str; 5] = ["color", "length", "thickness", "gap", "outline"];

/// Reads `[crosshair]`, accepting the legacy flat format (`color`, `length`,
/// `thickness`, `gap`, `outline`, `center_dot`). Legacy files drew only opaque inner
/// lines with an opaque 1 px outline, so the mapping keeps that look.
fn deserialize_crosshair<'de, D>(deserializer: D) -> Result<Crosshair, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mut table = toml::Table::deserialize(deserializer)?;
    let legacy: Vec<(&str, toml::Value)> = LEGACY_CROSSHAIR_KEYS
        .iter()
        .filter_map(|&k| table.remove(k).map(|v| (k, v)))
        .collect();
    let mut crosshair: Crosshair = table.try_into().map_err(serde::de::Error::custom)?;
    if legacy.is_empty() {
        return Ok(crosshair);
    }
    crosshair.inner.opacity = 1.0;
    crosshair.outer.show = false;
    crosshair.outline_opacity = 1.0;
    let px = |v: &toml::Value, min: f64, max: f64| -> Option<u8> {
        let n = v.as_float().or_else(|| v.as_integer().map(|i| i as f64))?;
        Some(n.round().clamp(min, max) as u8)
    };
    for (key, value) in &legacy {
        match *key {
            "color" => {
                if let Ok(rgb) = value.clone().try_into::<[f32; 3]>() {
                    set_legacy_color(&mut crosshair, rgb);
                }
            }
            "length" => {
                if let Some(n) = px(value, 0.0, 20.0) {
                    crosshair.inner.length_px = n;
                    crosshair.inner.vertical_length_px = n;
                }
            }
            "thickness" => {
                if let Some(n) = px(value, 1.0, 10.0) {
                    crosshair.inner.thickness_px = n;
                    crosshair.dot_thickness_px = n.min(6);
                }
            }
            "gap" => {
                if let Some(n) = px(value, 0.0, 20.0) {
                    crosshair.inner.offset_px = n;
                }
            }
            "outline" => {
                if let Some(on) = value.as_bool() {
                    crosshair.outlines = on;
                }
            }
            _ => {}
        }
    }
    Ok(crosshair)
}

/// Picks the matching VALORANT preset color, or a custom color otherwise.
fn set_legacy_color(crosshair: &mut Crosshair, rgb: [f32; 3]) {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let color = Rgba::opaque(byte(rgb[0]), byte(rgb[1]), byte(rgb[2]));
    match PRESET_COLORS.iter().position(|&(_, c)| c == color) {
        Some(i) => crosshair.color_index = i as u8,
        None => {
            crosshair.color_index = CUSTOM_COLOR_INDEX;
            crosshair.custom_color = color;
        }
    }
}

impl Settings {
    pub fn load(paths: &Paths) -> Self {
        match fs::read_to_string(&paths.settings_file) {
            Ok(src) => toml::from_str(&src).unwrap_or_else(|e| {
                bevy::log::warn!(
                    "invalid {}: {e}; using defaults",
                    paths.settings_file.display()
                );
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, paths: &Paths) {
        let result = toml::to_string_pretty(self)
            .map_err(|e| e.to_string())
            .and_then(|s| fs::write(&paths.settings_file, s).map_err(|e| e.to_string()));
        if let Err(e) = result {
            bevy::log::error!("could not save settings: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_toml() {
        let mut settings = Settings::default();
        settings.crosshair.color_index = CUSTOM_COLOR_INDEX;
        settings.crosshair.custom_color = Rgba::opaque(1, 2, 3);
        settings.crosshair.outer.offset_px = 15;
        let text = toml::to_string_pretty(&settings).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), settings);
    }

    #[test]
    fn missing_crosshair_uses_the_valorant_default() {
        let settings: Settings = toml::from_str("fullscreen = false").unwrap();
        assert_eq!(settings.crosshair, Crosshair::default());
        assert!(!settings.fullscreen);
    }

    #[test]
    fn legacy_crosshair_settings_still_load() {
        let src = r#"
            ads_multiplier = 1.0
            [sensitivity]
            game = "valorant"
            value = 0.35
            dpi = 800
            [crosshair]
            color = [0.0, 1.0, 1.0]
            length = 4.0
            thickness = 2.0
            gap = 2.0
            outline = true
            center_dot = true
        "#;
        let settings: Settings = toml::from_str(src).unwrap();
        assert_eq!(settings.sensitivity, Sensitivity::valorant(0.35, 800));
        let ch = settings.crosshair;
        assert_eq!(ch.color_index, 5, "cyan maps to VALORANT preset 5");
        assert_eq!(ch.inner.length_px, 4);
        assert_eq!(ch.inner.thickness_px, 2);
        assert_eq!(ch.inner.offset_px, 2);
        assert_eq!(ch.inner.opacity, 1.0);
        assert!(ch.outlines && ch.center_dot);
        assert_eq!(ch.outline_opacity, 1.0);
        assert!(!ch.outer.show, "the legacy crosshair had no outer lines");
    }

    #[test]
    fn legacy_custom_colors_become_custom_rgba() {
        let src = "[crosshair]\ncolor = [1.0, 0.5, 0.0]\noutline = false\n";
        let ch = toml::from_str::<Settings>(src).unwrap().crosshair;
        assert_eq!(ch.color_index, CUSTOM_COLOR_INDEX);
        assert_eq!(ch.custom_color, Rgba::opaque(255, 128, 0));
        assert!(!ch.outlines);
    }
}
