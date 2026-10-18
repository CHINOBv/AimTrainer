//! Settings for what a hit sounds and looks like. The sound panel returns the clip to
//! preview, if one was asked for.

use crate::audio::Clip;
use crate::audio::synth::{HitSound, KillSound};
use crate::game::feedback::{HitMarkerStyle, MarkerColor, TargetEffect};
use crate::persistence::settings::{HitFeedbackSettings, SoundSettings};
use crate::ui::atoms::hit_marker;
use aim_core::crosshair::Rgba;
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

/// Side of the hit-marker preview square, points.
const MARKER_PREVIEW_SIZE: f32 = 40.0;

/// `crosshair`: the crosshair color, for markers drawn in it.
pub fn hit_feedback_panel(ui: &mut egui::Ui, fb: &mut HitFeedbackSettings, crosshair: Rgba) {
    ui.heading("Hit feedback");
    egui::Grid::new("hit_feedback")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("Hit marker");
            ui.horizontal(|ui| {
                style_combo(
                    ui,
                    "hit_marker",
                    &mut fb.marker,
                    &HitMarkerStyle::ALL,
                    |s| s.label(),
                );
                marker_preview(ui, fb, crosshair);
            });
            ui.end_row();

            let shaped = !matches!(fb.marker, HitMarkerStyle::Off | HitMarkerStyle::Flash);
            ui.label("Marker color");
            ui.add_enabled_ui(shaped, |ui| {
                style_combo(
                    ui,
                    "marker_color",
                    &mut fb.marker_color,
                    &MarkerColor::ALL,
                    |c| c.label(),
                );
            });
            ui.end_row();

            ui.label("Marker duration");
            ui.add_enabled(
                fb.marker != HitMarkerStyle::Off,
                egui::Slider::new(&mut fb.marker_ms, HitFeedbackSettings::MARKER_MS).suffix(" ms"),
            );
            ui.end_row();

            ui.label("Target effect");
            style_combo(
                ui,
                "target_effect",
                &mut fb.target_effect,
                &TargetEffect::ALL,
                |e| e.label(),
            );
            ui.end_row();
        });
    ui.small("Effects are short: a killed target lingers for the effect, at most 0.2 s.");
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
    response.on_hover_text("Click to replay");
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
