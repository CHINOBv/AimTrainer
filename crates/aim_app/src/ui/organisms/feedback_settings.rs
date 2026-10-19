//! Settings for what a hit and the arena feel like (Feedback sub-tab) and what they sound
//! like (Audio sub-tab). The audio panel returns the clip to preview, if one was asked for.

use crate::audio::Clip;
use crate::audio::synth::{HitSound, KillSound};
use crate::game::feedback::{HitMarkerStyle, MarkerColor, TargetEffect};
use crate::i18n::tr;
use crate::persistence::settings::{
    FeelPreset, FeelSettings, HitFeedbackSettings, Settings, SoundSettings,
};
use crate::ui::atoms::{BODY_MARKER, DAMAGE_RED, HEAD_MARKER, hit_marker};
use crate::ui::molecules::SettingsRows;
use aim_core::crosshair::Rgba;
use bevy_egui::egui;

const PREVIEW: &str = "▶ Preview";

/// The feel presets in the order the picker shows them.
pub const FEEL_PRESETS: [(FeelPreset, &str, &str); 3] = [
    (
        FeelPreset::Minimal,
        "Minimal",
        "Information only: damage direction, health bar and death screen.",
    ),
    (
        FeelPreset::Standard,
        "Standard",
        "The defaults: indicators, kill feed, transitions and markers.",
    ),
    (
        FeelPreset::Juicy,
        "Juicy",
        "Everything, including screen punch and crosshair bloom.",
    ),
];

/// Apply a preset picked in the UI: every feel toggle takes the preset's value.
pub fn apply_feel_preset(feel: &mut FeelSettings, preset: FeelPreset) {
    feel.apply_preset(preset);
}

/// Preset buttons; returns true when one was clicked.
pub fn feel_preset_picker(ui: &mut egui::Ui, feel: &mut FeelSettings) -> bool {
    let mut applied = false;
    ui.horizontal(|ui| {
        for (preset, label, hint) in FEEL_PRESETS {
            if ui
                .selectable_label(feel.preset == preset, tr(label))
                .on_hover_text(tr(hint))
                .clicked()
            {
                apply_feel_preset(feel, preset);
                applied = true;
            }
        }
    });
    applied
}

pub fn audio_panel(
    ui: &mut egui::Ui,
    rows: &mut SettingsRows,
    sound: &mut SoundSettings,
) -> Option<Clip> {
    let mut preview = None;
    rows.section(ui, "Sound");
    rows.check(ui, "Sound effects", &mut sound.enabled, "");
    ui.add_enabled_ui(sound.enabled, |ui| {
        rows.row(ui, "Master volume", |ui| {
            ui.add(egui::Slider::new(&mut sound.volume_percent, 0..=100).suffix(" %"));
        });
        rows.row(ui, "Hit sound", |ui| {
            style_combo(ui, "hit_sound", &mut sound.hit, &HitSound::ALL, |s| {
                s.label()
            });
            let clip = |head| Clip::Hit {
                sound: sound.hit,
                head,
            };
            ui.add_enabled_ui(sound.hit != HitSound::Off, |ui| {
                if ui.button(tr(PREVIEW)).clicked() {
                    preview = Some(clip(false));
                }
                if ui.button(tr("Headshot")).clicked() {
                    preview = Some(clip(true));
                }
            });
        });
        rows.row(ui, "Kill sound", |ui| {
            style_combo(ui, "kill_sound", &mut sound.kill, &KillSound::ALL, |s| {
                s.label()
            });
            ui.add_enabled_ui(sound.kill != KillSound::Off, |ui| {
                if ui.button(tr(PREVIEW)).clicked() {
                    preview = Some(Clip::Kill(sound.kill));
                }
            });
        });
        rows.block(ui, "Miss sound", |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut sound.miss, tr("Miss sound"));
                if ui
                    .add_enabled(sound.miss, egui::Button::new(tr(PREVIEW)))
                    .clicked()
                {
                    preview = Some(Clip::Miss);
                }
            });
        });

        rows.section(ui, "Volume per category");
        for (label, value, hint) in [
            (
                "Shots volume",
                &mut sound.shots_percent,
                "Your hit, kill and miss sounds.",
            ),
            (
                "World volume",
                &mut sound.world_percent,
                "Arena bot gunfire and footsteps.",
            ),
            ("Menu volume", &mut sound.ui_percent, "Menu click sounds."),
            (
                "Cues volume",
                &mut sound.cues_percent,
                "Damage taken, death, streaks, round and timer cues.",
            ),
        ] {
            rows.row(ui, label, |ui| {
                ui.add(egui::Slider::new(value, 0..=100).suffix(tr(" % of master")))
                    .on_hover_text(tr(hint));
            });
        }

        rows.section(ui, "Sounds");
        rows.check(
            ui,
            "Bot gunfire (arenas)",
            &mut sound.bot_gunfire,
            "Positional: you hear where the shot came from.",
        );
        rows.check(
            ui,
            "Footsteps (arenas)",
            &mut sound.footsteps,
            "Positional bot footsteps.",
        );
        rows.check(ui, "Damage taken sound", &mut sound.damage_taken, "");
        rows.check(ui, "Death sound", &mut sound.death, "");
        rows.check(ui, "Multi-kill and streak sounds", &mut sound.streak, "");
        rows.check(
            ui,
            "Round cues",
            &mut sound.round_cues,
            "Countdown beeps and round start and end stings.",
        );
        rows.check(
            ui,
            "Low time warning",
            &mut sound.low_time_warning,
            "Ticks in the last seconds of a timed run.",
        );
        rows.check(
            ui,
            "Menu click sounds",
            &mut sound.ui_clicks,
            "Off by default.",
        );
    });
    preview
}

/// Side of the hit-marker preview square, points.
const MARKER_PREVIEW_SIZE: f32 = 40.0;

/// Hit feedback and game-feel effects.
pub fn feedback_panel(ui: &mut egui::Ui, rows: &mut SettingsRows, settings: &mut Settings) {
    let crosshair = settings.crosshair.color();
    hit_feedback(ui, rows, &mut settings.hit_feedback, crosshair);
    feel(ui, rows, &mut settings.feel);
}

/// `crosshair`: the crosshair color, for markers drawn in it.
fn hit_feedback(
    ui: &mut egui::Ui,
    rows: &mut SettingsRows,
    fb: &mut HitFeedbackSettings,
    crosshair: Rgba,
) {
    rows.section(ui, "Hit feedback");
    rows.row(ui, "Hit marker", |ui| {
        style_combo(
            ui,
            "hit_marker",
            &mut fb.marker,
            &HitMarkerStyle::ALL,
            |s| s.label(),
        );
        marker_preview(ui, fb, crosshair);
    });
    let shaped = !matches!(fb.marker, HitMarkerStyle::Off | HitMarkerStyle::Flash);
    rows.row(ui, "Marker color", |ui| {
        ui.add_enabled_ui(shaped, |ui| {
            style_combo(
                ui,
                "marker_color",
                &mut fb.marker_color,
                &MarkerColor::ALL,
                |c| c.label(),
            );
        });
    });
    let marker_on = fb.marker != HitMarkerStyle::Off;
    rows.row(ui, "Marker duration", |ui| {
        ui.add_enabled(
            marker_on,
            egui::Slider::new(&mut fb.marker_ms, HitFeedbackSettings::MARKER_MS).suffix(" ms"),
        );
    });
    rows.row(ui, "Target effect", |ui| {
        style_combo(
            ui,
            "target_effect",
            &mut fb.target_effect,
            &TargetEffect::ALL,
            |e| e.label(),
        );
    });
    if !rows.searching() {
        ui.small(tr(
            "Effects are short: a killed target lingers for the effect, at most 0.2 s.",
        ));
    }
}

fn feel(ui: &mut egui::Ui, rows: &mut SettingsRows, feel: &mut FeelSettings) {
    rows.section(ui, "Game feel");
    rows.block(ui, "Feel preset Minimal Standard Juicy", |ui| {
        ui.horizontal(|ui| {
            ui.label(tr("Preset"));
            feel_preset_picker(ui, feel);
        });
        ui.small(tr(
            "A preset sets every effect below; you can then change single effects.",
        ));
    });
    rows.check(
        ui,
        "Distinct head / body / kill markers",
        &mut feel.distinct_markers,
        "",
    );
    if feel.distinct_markers && !rows.searching() {
        ui.horizontal(|ui| {
            ui.colored_label(HEAD_MARKER, tr("■ head"));
            ui.colored_label(BODY_MARKER, tr("■ body"));
            ui.colored_label(DAMAGE_RED, tr("■ kill"));
        });
    }
    rows.check(
        ui,
        "Crosshair bloom",
        &mut feel.crosshair_bloom,
        "The crosshair opens up with recoil and movement inaccuracy. Off by default.",
    );
    rows.check(
        ui,
        "Round transitions",
        &mut feel.round_transitions,
        "Animated countdown, a GO pop and an end-of-round banner.",
    );

    rows.section(ui, "Arena indicators");
    rows.toggle(
        ui,
        "Damage direction indicator",
        &mut feel.damage_indicator,
        "An arrow around the crosshair towards the shooter, sized by the damage.",
    );
    rows.toggle(
        ui,
        "Damage vignette",
        &mut feel.damage_vignette,
        "The red screen-edge flash when you take damage.",
    );
    rows.toggle(
        ui,
        "Screen punch",
        &mut feel.screen_punch,
        "A short screen-space jolt on damage. It never moves the camera. Off by default.",
    );
    rows.check(
        ui,
        "Health bar",
        &mut feel.health_bar,
        "With a ghost of the damage just taken.",
    );
    rows.check(ui, "Low health pulse", &mut feel.low_hp_pulse, "");
    rows.check(ui, "Kill feed", &mut feel.kill_feed, "");
    rows.check(
        ui,
        "Streak banner",
        &mut feel.streak_banner,
        "Multi-kills and kill streaks.",
    );
    rows.check(
        ui,
        "Death screen",
        &mut feel.death_screen,
        "The direction of the killing shot and the respawn timer.",
    );
    rows.check(ui, "Kill flash", &mut feel.kill_flash_arena, "");
    rows.check(
        ui,
        "Arrows towards off-screen enemies",
        &mut feel.arena_arrows,
        "Enemies in sight but outside the view.",
    );
}

/// A small square showing the marker; clicking it replays the fade.
fn marker_preview(ui: &mut egui::Ui, fb: &HitFeedbackSettings, crosshair: Rgba) {
    let id = egui::Id::new("hit_marker_preview");
    let size = egui::Vec2::splat(MARKER_PREVIEW_SIZE);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let now = ui.input(|i| i.time);
    if response.clicked() {
        ui.data_mut(|d| d.insert_temp(id, now));
    }
    let started = ui.data(|d| d.get_temp::<f64>(id));
    let t = started.map_or(0.0, |s| ((now - s) as f32 / fb.marker_secs()).min(1.0));
    if t < 1.0 && started.is_some() {
        ui.ctx().request_repaint();
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, egui::Color32::from_gray(25));
    hit_marker::paint(
        &painter,
        rect.center(),
        fb.marker,
        fb.marker_color,
        crosshair,
        t,
    );
    response.on_hover_text(tr("Click to replay"));
}

fn style_combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    salt: &str,
    value: &mut T,
    all: &[T],
    label: impl Fn(T) -> &'static str,
) {
    egui::ComboBox::from_id_salt(salt)
        .selected_text(tr(label(*value)))
        .show_ui(ui, |ui| {
            for &option in all {
                ui.selectable_value(value, option, tr(label(option)));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picking_a_preset_sets_every_feel_toggle() {
        let mut feel = FeelSettings {
            kill_feed: false,
            ..FeelSettings::default()
        };
        apply_feel_preset(&mut feel, FeelPreset::Juicy);
        assert_eq!(feel, FeelSettings::for_preset(FeelPreset::Juicy));
        apply_feel_preset(&mut feel, FeelPreset::Minimal);
        assert_eq!(feel.preset, FeelPreset::Minimal);
        assert!(!feel.kill_feed && !feel.screen_punch.enabled);
    }

    #[test]
    fn the_picker_offers_every_preset_once() {
        let presets: Vec<FeelPreset> = FEEL_PRESETS.iter().map(|p| p.0).collect();
        assert_eq!(
            presets,
            [FeelPreset::Minimal, FeelPreset::Standard, FeelPreset::Juicy]
        );
    }
}
