//! Settings for what a shot sounds like. Returns the clip to preview, if one was asked for.

use crate::audio::Clip;
use crate::audio::synth::{HitSound, KillSound};
use crate::persistence::settings::SoundSettings;
use bevy_egui::egui;

const PREVIEW: &str = "▶ Preview";

pub fn sound_panel(ui: &mut egui::Ui, sound: &mut SoundSettings) -> Option<Clip> {
    let mut preview = None;
    ui.heading("Sound");
    ui.checkbox(&mut sound.enabled, "Sound effects");
    ui.add_enabled_ui(sound.enabled, |ui| {
        egui::Grid::new("sound").num_columns(2).show(ui, |ui| {
            ui.label("Volume");
            ui.add(egui::Slider::new(&mut sound.volume_percent, 0..=100).suffix(" %"));
            ui.end_row();

            ui.label("Hit sound");
            ui.horizontal(|ui| {
                style_combo(ui, "hit_sound", &mut sound.hit, &HitSound::ALL, |s| {
                    s.label()
                });
                let clip = |head| Clip::Hit {
                    sound: sound.hit,
                    head,
                };
                ui.add_enabled_ui(sound.hit != HitSound::Off, |ui| {
                    if ui.button(PREVIEW).clicked() {
                        preview = Some(clip(false));
                    }
                    if ui.button("Headshot").clicked() {
                        preview = Some(clip(true));
                    }
                });
            });
            ui.end_row();

            ui.label("Kill sound");
            ui.horizontal(|ui| {
                style_combo(ui, "kill_sound", &mut sound.kill, &KillSound::ALL, |s| {
                    s.label()
                });
                ui.add_enabled_ui(sound.kill != KillSound::Off, |ui| {
                    if ui.button(PREVIEW).clicked() {
                        preview = Some(Clip::Kill(sound.kill));
                    }
                });
            });
            ui.end_row();

            ui.checkbox(&mut sound.miss, "Miss sound");
            if ui
                .add_enabled(sound.miss, egui::Button::new(PREVIEW))
                .clicked()
            {
                preview = Some(Clip::Miss);
            }
            ui.end_row();
        });
    });
    preview
}

fn style_combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    salt: &str,
    value: &mut T,
    all: &[T],
    label: impl Fn(T) -> &'static str,
) {
    egui::ComboBox::from_id_salt(salt)
        .selected_text(label(*value))
        .show_ui(ui, |ui| {
            for &option in all {
                ui.selectable_value(value, option, label(option));
            }
        });
}
