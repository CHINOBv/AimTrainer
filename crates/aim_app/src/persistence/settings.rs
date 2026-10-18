//! Persistent user settings and the folders the app reads and writes.

use crate::audio::synth::{HitSound, KillSound};
use crate::game::feedback::{HitMarkerStyle, MarkerColor, TargetEffect};
use aim_core::crosshair::{CUSTOM_COLOR_INDEX, Crosshair, PRESET_COLORS, Rgba};
use aim_core::sensitivity::Sensitivity;
use aim_core::sensitivity::randomizer::RandomizerConfig;
use aim_core::training::missions::Commitment;
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
    /// In-window key that restarts the current run (a `KeyCode` name, e.g. "KeyR").
    pub restart_key: String,
    /// Length of the queue warm-up, minutes.
    pub warmup_minutes: u32,
    /// Daily training goal, minutes (30–50 min captures most daily gains, Listman 2021).
    pub daily_goal_minutes: f32,
    pub target_color: [f32; 3],
    /// Draw an arrow at the screen edge towards each target outside the view (scenarios only).
    pub offscreen_arrows: bool,
    pub sound: SoundSettings,
    pub hit_feedback: HitFeedbackSettings,
    /// How many daily missions are offered and how hard they are.
    pub mission_commitment: Commitment,
    /// Target outline and environment tint (colour-vision and visibility options).
    pub target_visibility: TargetVisibility,
    /// Sensitivity randomiser drill (off by default; never changes `sensitivity`).
    pub sens_randomizer: RandomizerConfig,
    /// Live coaching overlay in arenas (crosshair placement and movement accuracy).
    pub arena_coaching: bool,
}

/// VALORANT's "Enemy Highlight Color" options (General → Accessibility), applied to
/// `Settings::target_color`. Only the option names come from VALORANT (see
/// `docs/valorant-mechanics.md`); the RGB values are our own estimates, not Riot assets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnemyHighlight {
    /// The default. It is the trainer's original target colour.
    Red,
    /// For tritanopia.
    Purple,
    /// For deuteranopia and protanopia.
    Yellow,
    /// Any other colour.
    Custom,
}

impl EnemyHighlight {
    /// The presets and their colours (estimates).
    pub const PRESETS: [(EnemyHighlight, [f32; 3]); 3] = [
        (EnemyHighlight::Red, [1.0, 0.27, 0.33]),
        (EnemyHighlight::Purple, [0.75, 0.3, 1.0]),
        (EnemyHighlight::Yellow, [1.0, 0.92, 0.2]),
    ];

    /// Name shown in the settings.
    pub fn label(self) -> &'static str {
        match self {
            EnemyHighlight::Red => "Red (default)",
            EnemyHighlight::Purple => "Purple (tritanopia)",
            EnemyHighlight::Yellow => "Yellow (deuteranopia/protanopia)",
            EnemyHighlight::Custom => "Custom",
        }
    }

    /// The option a stored colour corresponds to.
    pub fn of(rgb: [f32; 3]) -> Self {
        Self::PRESETS
            .iter()
            .find(|(_, c)| *c == rgb)
            .map_or(EnemyHighlight::Custom, |(h, _)| *h)
    }
}

/// Target outline and environment tint. Missing keys keep today's look: no outline and
/// the original grid colours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TargetVisibility {
    /// Draw a solid outline around targets (decoys never get one).
    pub outline: bool,
    pub outline_color: [f32; 3],
    /// Outline thickness as a percentage of the target's size.
    pub outline_percent: u8,
    /// Colour of the floor grid and play area; `None` keeps the original colours.
    pub environment_tint: Option<[f32; 3]>,
}

impl TargetVisibility {
    pub const OUTLINE_PERCENT: std::ops::RangeInclusive<u8> = 2..=30;

    /// Scale of the outline shell relative to the target, when the outline is on.
    pub fn outline_scale(&self) -> Option<f32> {
        self.outline.then(|| {
            let pct = self
                .outline_percent
                .clamp(*Self::OUTLINE_PERCENT.start(), *Self::OUTLINE_PERCENT.end());
            1.0 + f32::from(pct) / 100.0
        })
    }
}

impl Default for TargetVisibility {
    fn default() -> Self {
        Self {
            outline: false,
            outline_color: [1.0, 1.0, 1.0],
            outline_percent: 8,
            environment_tint: None,
        }
    }
}

/// Visual feedback on hits. Missing keys (older files) take the subtle defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HitFeedbackSettings {
    pub marker: HitMarkerStyle,
    /// How long the hit marker shows, milliseconds.
    pub marker_ms: u16,
    pub marker_color: MarkerColor,
    pub target_effect: TargetEffect,
}

impl HitFeedbackSettings {
    pub const MARKER_MS: std::ops::RangeInclusive<u16> = 40..=400;

    /// How long the hit marker shows, seconds, within the allowed range.
    pub fn marker_secs(&self) -> f32 {
        let ms = self
            .marker_ms
            .clamp(*Self::MARKER_MS.start(), *Self::MARKER_MS.end());
        f32::from(ms) / 1000.0
    }
}

impl Default for HitFeedbackSettings {
    fn default() -> Self {
        Self {
            marker: HitMarkerStyle::default(),
            marker_ms: 120,
            marker_color: MarkerColor::default(),
            target_effect: TargetEffect::default(),
        }
    }
}

/// Sound effects for shots. Missing keys (older files) take the defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundSettings {
    pub enabled: bool,
    /// Master volume of the effects, percent (0–100).
    pub volume_percent: u8,
    pub hit: HitSound,
    pub kill: KillSound,
    /// A soft sound on missed shots.
    pub miss: bool,
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            volume_percent: 50,
            hit: HitSound::default(),
            kill: KillSound::default(),
            miss: false,
        }
    }
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
            restart_key: crate::platform::game_key::DEFAULT_RESTART.to_owned(),
            warmup_minutes: 5,
            daily_goal_minutes: 30.0,
            target_color: [1.0, 0.27, 0.33],
            offscreen_arrows: true,
            sound: SoundSettings::default(),
            hit_feedback: HitFeedbackSettings::default(),
            mission_commitment: Commitment::default(),
            target_visibility: TargetVisibility::default(),
            sens_randomizer: RandomizerConfig::default(),
            arena_coaching: false,
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
    fn offscreen_arrows_default_on_for_old_settings_files() {
        let settings: Settings = toml::from_str("fullscreen = false").unwrap();
        assert!(settings.offscreen_arrows);
        let off: Settings = toml::from_str("offscreen_arrows = false").unwrap();
        assert!(!off.offscreen_arrows);
    }

    #[test]
    fn arena_coaching_is_off_for_old_settings_files_and_persists() {
        let settings: Settings = toml::from_str("fullscreen = false").unwrap();
        assert!(!settings.arena_coaching);
        let on = Settings {
            arena_coaching: true,
            ..Settings::default()
        };
        let back: Settings = toml::from_str(&toml::to_string(&on).unwrap()).unwrap();
        assert!(back.arena_coaching);
    }

    #[test]
    fn restart_key_defaults_to_r_for_old_settings_files() {
        let settings: Settings = toml::from_str("fullscreen = false").unwrap();
        assert_eq!(settings.restart_key, "KeyR");
        let custom: Settings = toml::from_str("restart_key = \"F5\"").unwrap();
        assert_eq!(custom.restart_key, "F5");
    }

    #[test]
    fn sound_defaults_on_at_half_volume_for_old_settings_files() {
        let sound = toml::from_str::<Settings>("fullscreen = false")
            .unwrap()
            .sound;
        assert!(sound.enabled);
        assert_eq!(sound.volume_percent, 50);
        assert_eq!((sound.hit, sound.kill), (HitSound::Click, KillSound::Chime));
        assert!(!sound.miss, "miss sounds are opt-in");
    }

    #[test]
    fn partial_sound_tables_keep_the_other_defaults() {
        let src = "[sound]\nhit = \"ding\"\nmiss = true\n";
        let sound = toml::from_str::<Settings>(src).unwrap().sound;
        assert_eq!(sound.hit, HitSound::Ding);
        assert!(sound.miss && sound.enabled);
        assert_eq!(sound.kill, KillSound::Chime);
    }

    #[test]
    fn hit_feedback_defaults_are_subtle_for_old_settings_files() {
        let fb = toml::from_str::<Settings>("fullscreen = false")
            .unwrap()
            .hit_feedback;
        assert_eq!(fb.marker, HitMarkerStyle::Cross);
        assert_eq!(fb.marker_color, MarkerColor::White);
        assert_eq!(fb.target_effect, TargetEffect::Flash);
        assert!(HitFeedbackSettings::MARKER_MS.contains(&fb.marker_ms));
        assert!(fb.marker_ms <= 150, "a short marker by default");
    }

    #[test]
    fn hit_feedback_reads_kebab_case_styles() {
        let src = "[hit_feedback]\nmarker = \"brackets\"\nmarker_color = \"crosshair\"\ntarget_effect = \"ring\"\n";
        let fb = toml::from_str::<Settings>(src).unwrap().hit_feedback;
        assert_eq!(fb.marker, HitMarkerStyle::Brackets);
        assert_eq!(fb.marker_color, MarkerColor::Crosshair);
        assert_eq!(fb.target_effect, TargetEffect::Ring);
        assert_eq!(fb.marker_ms, HitFeedbackSettings::default().marker_ms);
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
    fn sens_randomizer_is_off_for_old_settings_files_and_round_trips() {
        let old: Settings = toml::from_str("fullscreen = false").unwrap();
        assert_eq!(old.sens_randomizer, RandomizerConfig::default());
        assert!(!old.sens_randomizer.enabled);
        let src = "[sens_randomizer]\nenabled = true\nmin = 0.9\n";
        let on: Settings = toml::from_str(src).unwrap();
        assert!(on.sens_randomizer.enabled);
        assert_eq!(on.sens_randomizer.min, 0.9);
        assert_eq!(on.sens_randomizer.max, RandomizerConfig::default().max);
        let text = toml::to_string_pretty(&on).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), on);
        assert_eq!(on.sensitivity, Settings::default().sensitivity);
    }

    #[test]
    fn mission_commitment_defaults_to_medium_and_reads_kebab_case() {
        let old: Settings = toml::from_str("fullscreen = false").unwrap();
        assert_eq!(old.mission_commitment, Commitment::Medium);
        let high: Settings = toml::from_str("mission_commitment = \"high\"").unwrap();
        assert_eq!(high.mission_commitment, Commitment::High);
    }

    #[test]
    fn red_highlight_is_the_default_and_keeps_the_original_target_colour() {
        let settings: Settings = toml::from_str("fullscreen = false").unwrap();
        assert_eq!(settings.target_color, [1.0, 0.27, 0.33]);
        assert_eq!(
            EnemyHighlight::of(settings.target_color),
            EnemyHighlight::Red
        );
        for (h, rgb) in EnemyHighlight::PRESETS {
            assert_eq!(EnemyHighlight::of(rgb), h);
        }
        assert_eq!(EnemyHighlight::of([0.1, 0.2, 0.3]), EnemyHighlight::Custom);
    }

    #[test]
    fn target_visibility_defaults_keep_todays_look() {
        let v = toml::from_str::<Settings>("fullscreen = false")
            .unwrap()
            .target_visibility;
        assert_eq!(v, TargetVisibility::default());
        assert!(!v.outline && v.environment_tint.is_none());
        assert_eq!(v.outline_scale(), None);
    }

    #[test]
    fn target_visibility_round_trips_and_clamps_the_outline() {
        let settings = Settings {
            target_visibility: TargetVisibility {
                outline: true,
                outline_color: [0.0, 1.0, 0.0],
                outline_percent: 200,
                environment_tint: Some([0.2, 0.3, 0.4]),
            },
            ..Settings::default()
        };
        let text = toml::to_string_pretty(&settings).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), settings);
        assert_eq!(settings.target_visibility.outline_scale(), Some(1.3));
        let partial: Settings = toml::from_str("[target_visibility]\noutline = true\n").unwrap();
        assert_eq!(partial.target_visibility.outline_scale(), Some(1.08));
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
