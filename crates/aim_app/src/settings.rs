//! Persistent user settings and the folders the app reads and writes.

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
    pub crosshair: Crosshair,
    /// 0 = fully see-through overlay, 1 = opaque background.
    pub background_opacity: f32,
    pub fullscreen: bool,
    pub always_on_top: bool,
    /// Global hotkey on Windows/X11 (e.g. "ctrl+shift+F10"). On Wayland bind
    /// `aimtrainer toggle` in your compositor instead. Requires a restart.
    pub overlay_hotkey: String,
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
            target_color: [1.0, 0.27, 0.33],
        }
    }
}

/// Crosshair in screen pixels, modelled on VALORANT's inner lines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Crosshair {
    pub color: [f32; 3],
    pub length: f32,
    pub thickness: f32,
    pub gap: f32,
    pub outline: bool,
    pub center_dot: bool,
}

impl Default for Crosshair {
    fn default() -> Self {
        Self {
            color: [0.0, 1.0, 1.0],
            length: 4.0,
            thickness: 2.0,
            gap: 2.0,
            outline: true,
            center_dot: false,
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
