//! Persistent user settings and the folders the app reads and writes.

use crate::audio::synth::{HitSound, KillSound};
use crate::game::feedback::{HitMarkerStyle, MarkerColor, TargetEffect};
use aim_core::crosshair::{CUSTOM_COLOR_INDEX, Crosshair, PRESET_COLORS, Rgba};
use aim_core::i18n::Lang;
use aim_core::sensitivity::Sensitivity;
use aim_core::sensitivity::randomizer::RandomizerConfig;
use aim_core::training::ergonomics::BreakPolicy;
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
    /// The AI coach's memory (SQLite), next to the history. `aimtrainer-mcp` uses the
    /// same file.
    pub coach_memory_file: PathBuf,
}

/// File name of the coach memory in the data folder.
pub const COACH_MEMORY_FILE: &str = "coach.db";

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
            coach_memory_file: data.join(COACH_MEMORY_FILE),
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
    /// UI language. Old files and unknown codes load as English.
    #[serde(deserialize_with = "deserialize_language")]
    pub language: Lang,
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
    /// Seconds of countdown before a run starts, within [`COUNTDOWN_RANGE`] (0 starts at
    /// once). Movement scenarios let the player move during it. Read it through
    /// [`Settings::countdown_secs`].
    pub countdown_secs: f32,
    /// Length of the queue warm-up, minutes.
    pub warmup_minutes: u32,
    /// Size the queue warm-up by the player's personal dose when there is enough history.
    pub personal_warmup: bool,
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
    /// Game-feel effects (HUD indicators, transitions, crosshair extras).
    pub feel: FeelSettings,
    /// Scenario ids starred in the scenario list, in the order they were starred.
    pub favorites: Vec<String>,
    /// The AI coach's provider settings. Off by default; API keys are never kept here
    /// (they live in the OS keyring or an environment variable).
    #[serde(deserialize_with = "deserialize_coach")]
    pub coach: aim_ai::CoachConfig,
    /// Break prompts and the weekly volume cap (`[breaks]`; `enabled = false` turns them off).
    #[serde(deserialize_with = "deserialize_breaks")]
    pub breaks: BreakPolicy,
    /// The first-run wizard was finished or skipped. False only for a new player (no
    /// settings file yet); a file without this key comes from an existing player.
    #[serde(default = "first_run_done_in_old_files")]
    pub first_run_done: bool,
    /// The last Movement Lab settings (`[movement_lab]`).
    #[serde(deserialize_with = "deserialize_movement_lab")]
    pub movement_lab: aim_core::scenario::lab::LabSettings,
}

fn first_run_done_in_old_files() -> bool {
    true
}

/// An unknown language code (`language = "fr"`) falls back to English instead of
/// resetting every setting.
fn deserialize_language<'de, D>(deserializer: D) -> Result<Lang, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = toml::Value::deserialize(deserializer)?;
    Ok(value.try_into().unwrap_or_else(|_| {
        bevy::log::warn!("unknown language in the settings; using English");
        Lang::En
    }))
}

/// Allowed start countdown, seconds.
pub const COUNTDOWN_RANGE: std::ops::RangeInclusive<f32> = 0.0..=5.0;
/// The start countdown of files written before it was configurable, seconds.
pub const DEFAULT_COUNTDOWN_SECS: f32 = 3.0;

impl Settings {
    /// The start countdown in seconds, held inside [`COUNTDOWN_RANGE`]; a value that is
    /// not a number falls back to [`DEFAULT_COUNTDOWN_SECS`].
    pub fn countdown_secs(&self) -> f32 {
        if self.countdown_secs.is_nan() {
            return DEFAULT_COUNTDOWN_SECS;
        }
        self.countdown_secs
            .clamp(*COUNTDOWN_RANGE.start(), *COUNTDOWN_RANGE.end())
    }
}

/// An invalid `[coach]` table (for example one holding an `api_key`, which
/// `CoachConfig` refuses) resets only the coach, switched off, instead of every setting.
/// The next save then drops the offending line from the file.
fn deserialize_coach<'de, D>(deserializer: D) -> Result<aim_ai::CoachConfig, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // Any value, so that a non-table (`coach = 1`) also resets only the coach.
    let value = toml::Value::deserialize(deserializer)?;
    Ok(value.try_into().unwrap_or_else(|_| {
        // The error is not logged: a type error quotes the value, which may be a key.
        bevy::log::warn!("invalid [coach] settings; the coach is reset and off");
        aim_ai::CoachConfig::default()
    }))
}

/// An invalid `[breaks]` table (an unknown key, a wrong type or a zero length) resets only
/// the break policy to its defaults instead of every setting.
fn deserialize_breaks<'de, D>(deserializer: D) -> Result<BreakPolicy, D::Error>
where
    D: serde::Deserializer<'de>,
{
    // Any value, so that a non-table (`breaks = 45`) also resets only the breaks.
    let value = toml::Value::deserialize(deserializer)?;
    let policy = value
        .try_into::<BreakPolicy>()
        .ok()
        .filter(|p| p.validate().is_empty());
    Ok(policy.unwrap_or_else(|| {
        bevy::log::warn!("invalid [breaks] settings; using the default break policy");
        BreakPolicy::default()
    }))
}

/// An invalid `[movement_lab]` table (a wrong type, or not a table) resets only the lab to
/// its defaults instead of every setting. Out-of-range numbers are kept: the lab clamps
/// them when it builds the drill.
fn deserialize_movement_lab<'de, D>(
    deserializer: D,
) -> Result<aim_core::scenario::lab::LabSettings, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = toml::Value::deserialize(deserializer)?;
    Ok(value.try_into().unwrap_or_else(|_| {
        bevy::log::warn!("invalid [movement_lab] settings; using the lab defaults");
        aim_core::scenario::lab::LabSettings::default()
    }))
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
    /// Volume of the player's shot sounds (hit, kill, miss), percent of the master volume.
    pub shots_percent: u8,
    /// Volume of world sounds in arenas (bot gunfire, footsteps), percent of the master volume.
    pub world_percent: u8,
    /// Volume of menu sounds (UI clicks), percent of the master volume.
    pub ui_percent: u8,
    /// Volume of cues (damage taken, death, streaks, round and timer cues), percent of the
    /// master volume.
    pub cues_percent: u8,
    /// Positional bot gunfire in arenas.
    pub bot_gunfire: bool,
    /// Positional bot footsteps in arenas.
    pub footsteps: bool,
    /// A sound when the player takes damage.
    pub damage_taken: bool,
    /// A sound when the player dies.
    pub death: bool,
    /// Multi-kill and kill-streak sounds.
    pub streak: bool,
    /// Countdown beeps and round start and end stings.
    pub round_cues: bool,
    /// Ticks in the last seconds of a timed run.
    pub low_time_warning: bool,
    /// Click sounds in the menus (off by default).
    pub ui_clicks: bool,
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            volume_percent: 50,
            hit: HitSound::default(),
            kill: KillSound::default(),
            miss: false,
            shots_percent: 100,
            world_percent: 100,
            ui_percent: 100,
            cues_percent: 100,
            bot_gunfire: true,
            footsteps: true,
            damage_taken: true,
            death: true,
            streak: true,
            round_cues: true,
            low_time_warning: true,
            ui_clicks: false,
        }
    }
}

/// An effect that can be switched off and scaled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Toggle {
    pub enabled: bool,
    /// Strength of the effect, percent (100 = the designed strength).
    pub intensity_percent: u8,
}

impl Toggle {
    /// On at `intensity_percent`.
    pub const fn on(intensity_percent: u8) -> Self {
        Self {
            enabled: true,
            intensity_percent,
        }
    }

    /// Off; switching it on uses `intensity_percent`.
    pub const fn off(intensity_percent: u8) -> Self {
        Self {
            enabled: false,
            intensity_percent,
        }
    }
}

/// A missing key in a toggle table takes this value, so a table that only sets
/// `intensity_percent` turns the effect on.
impl Default for Toggle {
    fn default() -> Self {
        Self::on(100)
    }
}

/// A named set of feel toggles, offered in the first-run wizard and the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FeelPreset {
    /// Information only: damage direction, health bar and death screen.
    Minimal,
    /// The defaults.
    #[default]
    Standard,
    /// Every effect, including screen punch and crosshair bloom.
    Juicy,
}

/// Game-feel effects: HUD indicators, transitions and crosshair extras. Missing keys
/// (older files) take the Standard preset, which keeps today's look for the effects that
/// already existed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FeelSettings {
    /// Arena: an arrow around the crosshair pointing to where damage came from, sized by
    /// the damage.
    pub damage_indicator: Toggle,
    /// Arena: the red screen-edge flash when the player takes damage.
    pub damage_vignette: Toggle,
    /// Arena: a list of recent kills.
    pub kill_feed: bool,
    /// Arena: a banner for multi-kills and kill streaks.
    pub streak_banner: bool,
    /// Arena: the player's health bar, with a ghost of the damage just taken.
    pub health_bar: bool,
    /// Arena: the health bar pulses at low health.
    pub low_hp_pulse: bool,
    /// Arena: after a death, the direction of the killing shot and the respawn timer.
    pub death_screen: bool,
    /// Animated countdown, a "GO" pop and an end-of-round banner.
    pub round_transitions: bool,
    /// Arena: a short screen-space jolt on damage. It never moves the camera. Off by default.
    pub screen_punch: Toggle,
    /// The crosshair opens up with recoil and movement inaccuracy. Off by default.
    pub crosshair_bloom: bool,
    /// Arena: arrows at the screen edge towards enemies in sight but outside the view.
    pub arena_arrows: bool,
    /// Arena: a brief flash on a kill.
    pub kill_flash_arena: bool,
    /// Different hit markers for head, body and kill.
    pub distinct_markers: bool,
    /// The preset last applied. Changing a single key afterwards does not change it.
    pub preset: FeelPreset,
}

impl FeelSettings {
    /// The toggles of `preset`.
    pub fn for_preset(preset: FeelPreset) -> Self {
        let standard = Self {
            damage_indicator: Toggle::on(100),
            damage_vignette: Toggle::on(100),
            kill_feed: true,
            streak_banner: true,
            health_bar: true,
            low_hp_pulse: true,
            death_screen: true,
            round_transitions: true,
            screen_punch: Toggle::off(100),
            crosshair_bloom: false,
            arena_arrows: true,
            kill_flash_arena: true,
            distinct_markers: true,
            preset,
        };
        match preset {
            FeelPreset::Standard => standard,
            FeelPreset::Minimal => Self {
                damage_indicator: Toggle::on(60),
                damage_vignette: Toggle::off(100),
                kill_feed: false,
                streak_banner: false,
                low_hp_pulse: false,
                round_transitions: false,
                arena_arrows: false,
                kill_flash_arena: false,
                distinct_markers: false,
                ..standard
            },
            FeelPreset::Juicy => Self {
                screen_punch: Toggle::on(100),
                crosshair_bloom: true,
                ..standard
            },
        }
    }

    /// Set every toggle to `preset`'s values.
    pub fn apply_preset(&mut self, preset: FeelPreset) {
        *self = Self::for_preset(preset);
    }
}

impl Default for FeelSettings {
    fn default() -> Self {
        Self::for_preset(FeelPreset::Standard)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Lang::En,
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
            countdown_secs: DEFAULT_COUNTDOWN_SECS,
            warmup_minutes: 5,
            personal_warmup: true,
            daily_goal_minutes: 30.0,
            target_color: [1.0, 0.27, 0.33],
            offscreen_arrows: true,
            sound: SoundSettings::default(),
            hit_feedback: HitFeedbackSettings::default(),
            mission_commitment: Commitment::default(),
            target_visibility: TargetVisibility::default(),
            sens_randomizer: RandomizerConfig::default(),
            arena_coaching: false,
            feel: FeelSettings::default(),
            favorites: Vec::new(),
            coach: aim_ai::CoachConfig::default(),
            breaks: BreakPolicy::default(),
            first_run_done: false,
            movement_lab: aim_core::scenario::lab::LabSettings::default(),
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
            Ok(src) => Self::parse(&src),
            Err(_) => Self::default(),
        }
    }

    /// Settings from an existing file's text; invalid text falls back to the defaults.
    fn parse(src: &str) -> Self {
        toml::from_str(src).unwrap_or_else(|e| {
            bevy::log::warn!(
                "invalid settings file ({}); using defaults",
                parse_error_message(src, &e)
            );
            // The file exists, so this is not a new player.
            Self {
                first_run_done: true,
                ..Self::default()
            }
        })
    }

    /// Writes the settings file; returns whether it was written (errors are logged).
    pub fn save(&self, paths: &Paths) -> bool {
        let result = toml::to_string_pretty(self)
            .map_err(|e| e.to_string())
            .and_then(|s| fs::write(&paths.settings_file, s).map_err(|e| e.to_string()));
        if let Err(e) = &result {
            bevy::log::error!("could not save settings: {e}");
        }
        result.is_ok()
    }
}

/// Where a settings parse error is, without its text: toml quotes the offending line,
/// which could hold a hand-typed API key (docs/coach.md: keys are never logged).
fn parse_error_message(src: &str, e: &toml::de::Error) -> String {
    match e.span() {
        Some(span) => {
            let line = src[..span.start.min(src.len())].matches('\n').count() + 1;
            format!("error at line {line}")
        }
        None => "error at an unknown position".to_owned(),
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
    fn save_reports_whether_the_file_was_written() {
        let dir = std::env::temp_dir().join(format!("aimtrainer-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let paths = |file: std::path::PathBuf| Paths {
            settings_file: file,
            scenarios_dir: dir.clone(),
            routines_dir: dir.clone(),
            benchmarks_dir: dir.clone(),
            history_file: dir.join("history.csv"),
            coach_memory_file: dir.join("coach.db"),
        };
        assert!(Settings::default().save(&paths(dir.join("settings.toml"))));
        assert!(
            !Settings::default().save(&paths(dir.join("missing").join("settings.toml"))),
            "a folder that does not exist cannot be written"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn language_is_english_for_old_settings_files_and_round_trips() {
        let old: Settings = toml::from_str("fullscreen = false").unwrap();
        assert_eq!(old.language, Lang::En);
        let unknown: Settings = toml::from_str(
            "language = \"fr\"
fullscreen = false",
        )
        .unwrap();
        assert_eq!(
            unknown.language,
            Lang::En,
            "an unknown code keeps the other settings"
        );
        assert!(!unknown.fullscreen);
        let spanish = Settings {
            language: Lang::Es,
            ..Settings::default()
        };
        let text = toml::to_string_pretty(&spanish).unwrap();
        assert!(text.contains("language = \"es\""), "{text}");
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), spanish);
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
    fn the_countdown_is_three_seconds_for_old_settings_files_and_held_in_range() {
        let old: Settings = toml::from_str("fullscreen = false").unwrap();
        assert_eq!(old.countdown_secs(), 3.0);
        let quick: Settings = toml::from_str("countdown_secs = 1.5").unwrap();
        assert_eq!(quick.countdown_secs(), 1.5);
        let back: Settings = toml::from_str(&toml::to_string(&quick).unwrap()).unwrap();
        assert_eq!(back.countdown_secs(), 1.5);
        for (stored, used) in [(0.0, 0.0), (-2.0, 0.0), (9.0, 5.0), (f32::NAN, 3.0)] {
            let s = Settings {
                countdown_secs: stored,
                ..Settings::default()
            };
            assert_eq!(s.countdown_secs(), used, "{stored}");
        }
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
    fn a_parse_error_message_never_quotes_the_settings_text() {
        let src = "[coach]\napi_key = sk-secret-123\n";
        let message = parse_error_message(src, &toml::from_str::<Settings>(src).unwrap_err());
        assert!(!message.contains("sk-secret"), "{message}");
        assert!(message.contains("line 2"), "{message}");
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
    fn feel_defaults_to_standard_and_keeps_todays_damage_vignette_for_old_files() {
        let feel = toml::from_str::<Settings>("fullscreen = false")
            .unwrap()
            .feel;
        assert_eq!(feel, FeelSettings::default());
        assert_eq!(feel.preset, FeelPreset::Standard);
        assert_eq!(
            feel.damage_vignette,
            Toggle {
                enabled: true,
                intensity_percent: 100
            },
            "today's 0.25 s vignette at its current strength"
        );
        assert!(!feel.screen_punch.enabled, "screen punch is opt-in");
        assert!(!feel.crosshair_bloom, "crosshair bloom is opt-in");
        assert!(feel.damage_indicator.enabled && feel.health_bar && feel.death_screen);
    }

    #[test]
    fn partial_feel_tables_keep_the_other_defaults_and_round_trip() {
        let src = "[feel]\nkill_feed = false\npreset = \"juicy\"\n[feel.screen_punch]\nintensity_percent = 40\n";
        let settings: Settings = toml::from_str(src).unwrap();
        let feel = &settings.feel;
        assert!(!feel.kill_feed);
        assert_eq!(feel.preset, FeelPreset::Juicy);
        assert_eq!(
            feel.screen_punch,
            Toggle {
                enabled: true,
                intensity_percent: 40
            },
            "a toggle table without `enabled` turns the effect on"
        );
        assert!(feel.streak_banner && feel.arena_arrows);
        let text = toml::to_string_pretty(&settings).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), settings);
    }

    #[test]
    fn feel_presets_set_every_toggle() {
        let mut feel = FeelSettings::default();
        feel.apply_preset(FeelPreset::Minimal);
        assert_eq!(feel.preset, FeelPreset::Minimal);
        assert!(!feel.damage_vignette.enabled && !feel.screen_punch.enabled);
        assert!(!feel.kill_feed && !feel.streak_banner && !feel.low_hp_pulse);
        assert!(!feel.round_transitions && !feel.crosshair_bloom && !feel.arena_arrows);
        assert!(!feel.kill_flash_arena && !feel.distinct_markers);
        assert!(
            feel.damage_indicator.enabled && feel.health_bar && feel.death_screen,
            "minimal keeps the information, drops the decoration"
        );

        feel.apply_preset(FeelPreset::Juicy);
        assert_eq!(feel.preset, FeelPreset::Juicy);
        assert!(feel.screen_punch.enabled && feel.crosshair_bloom);
        assert!(feel.damage_vignette.enabled && feel.kill_feed && feel.distinct_markers);

        feel.kill_feed = false;
        feel.apply_preset(FeelPreset::Standard);
        assert_eq!(feel, FeelSettings::default());
    }

    #[test]
    fn sound_categories_default_to_full_volume_and_ui_clicks_off_for_old_files() {
        let sound = toml::from_str::<Settings>("fullscreen = false")
            .unwrap()
            .sound;
        assert_eq!(
            [
                sound.shots_percent,
                sound.world_percent,
                sound.ui_percent,
                sound.cues_percent
            ],
            [100; 4]
        );
        assert!(sound.bot_gunfire && sound.footsteps && sound.damage_taken && sound.death);
        assert!(sound.streak && sound.round_cues && sound.low_time_warning);
        assert!(!sound.ui_clicks, "UI clicks are opt-in");
    }

    #[test]
    fn sound_categories_round_trip_and_partial_tables_keep_defaults() {
        let src = "[sound]\nworld_percent = 30\nfootsteps = false\nui_clicks = true\n";
        let settings: Settings = toml::from_str(src).unwrap();
        let sound = &settings.sound;
        assert_eq!((sound.world_percent, sound.shots_percent), (30, 100));
        assert!(!sound.footsteps && sound.ui_clicks && sound.bot_gunfire);
        assert_eq!(sound.volume_percent, 50);
        let text = toml::to_string_pretty(&settings).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), settings);
    }

    #[test]
    fn favorites_are_empty_for_old_settings_files_and_round_trip() {
        let old: Settings = toml::from_str("fullscreen = false").unwrap();
        assert!(old.favorites.is_empty());
        let starred = Settings {
            favorites: vec!["flick-headshot".into(), "track-precise".into()],
            ..Settings::default()
        };
        let text = toml::to_string_pretty(&starred).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), starred);
    }

    #[test]
    fn the_first_run_wizard_shows_only_without_a_settings_file() {
        assert!(
            !Settings::default().first_run_done,
            "no settings file yet: a new player"
        );
        assert!(
            Settings::parse("fullscreen = false").first_run_done,
            "a file written before the wizard existed"
        );
        assert!(
            Settings::parse("not = [valid").first_run_done,
            "an unreadable file still means an existing player"
        );
        let pending = Settings::default();
        let text = toml::to_string_pretty(&pending).unwrap();
        assert!(
            !Settings::parse(&text).first_run_done,
            "a file saved while the wizard is still open keeps it open"
        );
    }

    #[test]
    fn legacy_custom_colors_become_custom_rgba() {
        let src = "[crosshair]\ncolor = [1.0, 0.5, 0.0]\noutline = false\n";
        let ch = toml::from_str::<Settings>(src).unwrap().crosshair;
        assert_eq!(ch.color_index, CUSTOM_COLOR_INDEX);
        assert_eq!(ch.custom_color, Rgba::opaque(255, 128, 0));
        assert!(!ch.outlines);
    }

    #[test]
    fn movement_lab_uses_the_defaults_for_old_settings_files_and_round_trips() {
        use aim_core::scenario::lab::{LabMotion, LabSettings};
        let old: Settings = toml::from_str("fullscreen = false").unwrap();
        assert_eq!(old.movement_lab, LabSettings::default());
        assert!(!old.fullscreen, "the rest of the file still loads");
        let custom = Settings {
            movement_lab: LabSettings {
                motion: LabMotion::Dodge,
                max_alive: 5,
                until_killed: true,
                ..LabSettings::default()
            },
            ..Settings::default()
        };
        let text = toml::to_string_pretty(&custom).unwrap();
        assert!(text.contains("[movement_lab]"), "{text}");
        let back: Settings = toml::from_str(&text).unwrap();
        assert_eq!(back.movement_lab, custom.movement_lab);
        let partial = Settings::parse("[movement_lab]\nmax_alive = 4\n");
        assert_eq!(partial.movement_lab.max_alive, 4);
        assert_eq!(
            partial.movement_lab.motion,
            LabMotion::Strafe,
            "missing keys default"
        );
    }

    #[test]
    fn an_invalid_movement_lab_table_resets_only_the_lab() {
        for table in [
            "[movement_lab]\nmax_alive = \"many\"\n",
            "movement_lab = 3\n",
        ] {
            let settings = Settings::parse(&format!("fullscreen = false\n{table}"));
            assert_eq!(
                settings.movement_lab,
                aim_core::scenario::lab::LabSettings::default(),
                "{table}"
            );
            assert!(!settings.fullscreen, "other settings survive: {table}");
        }
    }

    #[test]
    fn breaks_use_the_default_policy_for_old_files_and_round_trip() {
        let old: Settings = toml::from_str("fullscreen = false").unwrap();
        assert_eq!(old.breaks, BreakPolicy::default());
        let custom = Settings {
            breaks: BreakPolicy {
                continuous_minutes: 30,
                break_minutes: 10,
                weekly_cap_minutes: 300,
                enabled: false,
            },
            ..Settings::default()
        };
        let text = toml::to_string_pretty(&custom).unwrap();
        assert!(text.contains("[breaks]"), "{text}");
        let back: Settings = toml::from_str(&text).unwrap();
        assert_eq!(back.breaks, custom.breaks);
        let partial = Settings::parse("[breaks]\nenabled = false\n");
        assert!(!partial.breaks.enabled);
        assert_eq!(
            partial.breaks.continuous_minutes, 45,
            "missing keys default"
        );
    }

    #[test]
    fn an_invalid_breaks_table_resets_only_the_breaks() {
        for table in [
            "[breaks]\nenabled = \"yes\"\n",
            "[breaks]\nsnooze = 3\n",
            "[breaks]\ncontinuous_minutes = 0\n",
        ] {
            let settings = Settings::parse(&format!("fullscreen = false\n{table}"));
            assert!(
                !settings.fullscreen,
                "the rest of the file still loads: {table}"
            );
            assert_eq!(settings.breaks, BreakPolicy::default(), "{table}");
        }
    }

    #[test]
    fn the_coach_is_off_for_old_settings_files_and_round_trips() {
        let old: Settings = toml::from_str("fullscreen = false").unwrap();
        assert_eq!(old.coach, aim_ai::CoachConfig::default());
        assert!(!old.coach.enabled, "the coach is opt-in");

        let on = Settings {
            coach: aim_ai::CoachConfig {
                enabled: true,
                base_url: Some(aim_ai::config::OLLAMA_BASE_URL.into()),
                model: "llama3.1".into(),
                api_key_env: Some("OPENAI_API_KEY".into()),
                ..aim_ai::CoachConfig::default()
            },
            ..Settings::default()
        };
        let back: Settings = toml::from_str(&toml::to_string_pretty(&on).unwrap()).unwrap();
        assert_eq!(back.coach, on.coach);
    }

    #[test]
    fn a_coach_or_breaks_value_that_is_not_a_table_resets_only_itself() {
        let settings = Settings::parse(
            "breaks = 45
coach = \"sk-secret\"
fullscreen = false
",
        );
        assert!(!settings.fullscreen, "the rest of the file still loads");
        assert_eq!(settings.coach, aim_ai::CoachConfig::default());
        assert_eq!(settings.breaks, BreakPolicy::default());
    }

    #[test]
    fn the_personal_warmup_dose_is_on_for_old_settings_files() {
        let old: Settings = toml::from_str("warmup_minutes = 4").unwrap();
        assert!(old.personal_warmup);
        assert_eq!(old.warmup_minutes, 4);
        let off = Settings::parse(
            "personal_warmup = false
",
        );
        assert!(!off.personal_warmup);
    }

    #[test]
    fn a_coach_table_holding_a_key_is_dropped_and_the_other_settings_survive() {
        let src = "fullscreen = false\n[coach]\nenabled = true\napi_key = \"sk-secret\"\n";
        let settings = Settings::parse(src);
        assert!(!settings.fullscreen, "the rest of the file still loads");
        assert_eq!(settings.coach, aim_ai::CoachConfig::default());
        let text = toml::to_string_pretty(&settings).unwrap();
        assert!(!text.contains("sk-secret"), "the next save forgets the key");
    }

    #[test]
    fn settings_never_serialize_an_api_key() {
        let settings = Settings {
            coach: aim_ai::CoachConfig {
                enabled: true,
                provider: aim_ai::ProviderKind::Anthropic,
                model: "claude-sonnet-5-5".into(),
                api_key_env: Some("ANTHROPIC_API_KEY".into()),
                ..aim_ai::CoachConfig::default()
            },
            ..Settings::default()
        };
        let text = toml::to_string_pretty(&settings).unwrap();
        let coach = text
            .split("[coach]")
            .nth(1)
            .expect("a coach table")
            .split("\n[")
            .next()
            .unwrap();
        let keys: Vec<&str> = coach
            .lines()
            .filter_map(|l| l.split_once(" = ").map(|(k, _)| k.trim()))
            .collect();
        assert!(!keys.is_empty(), "{coach}");
        for key in keys {
            assert!(
                !key.contains("key") || key == "api_key_env",
                "a key-like field {key:?} is written: {coach}"
            );
        }
    }
}
