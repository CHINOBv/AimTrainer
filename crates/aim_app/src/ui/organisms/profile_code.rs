//! Profile code: sensitivity, DPI, ADS multiplier and crosshair in one shareable line.

use crate::i18n::tr;
use crate::persistence::settings::Settings;
use crate::ui::atoms::{ERROR, GOOD};
use aim_core::profile::Profile;
use bevy_egui::egui;

/// Text being imported and the outcome of the last import.
#[derive(Debug, Default)]
pub struct ProfileImport {
    pub text: String,
    pub outcome: Option<Result<(), String>>,
}

pub fn profile_code(ui: &mut egui::Ui, settings: &mut Settings, import: &mut ProfileImport) {
    ui.heading(tr("Profile code"));
    ui.label(tr(
        "Move your setup to another PC or share it: sensitivity, DPI, ADS multiplier and crosshair.",
    ));
    let mut code = Profile {
        sensitivity: settings.sensitivity,
        ads_multiplier: settings.ads_multiplier,
        crosshair: settings.crosshair,
    }
    .encode();
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut code).desired_width(420.0));
        if ui.button(tr("Copy")).clicked() {
            ui.ctx().copy_text(code.clone());
        }
    });
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut import.text)
                .hint_text(tr("Paste a CAT1|… code"))
                .desired_width(420.0),
        );
        if ui.button(tr("Import")).clicked() {
            import.outcome = Some(match Profile::decode(&import.text) {
                Ok(p) => {
                    settings.sensitivity = p.sensitivity;
                    settings.ads_multiplier = p.ads_multiplier;
                    settings.crosshair = p.crosshair;
                    Ok(())
                }
                Err(e) => Err(e.to_string()),
            });
        }
    });
    match &import.outcome {
        Some(Ok(())) => {
            ui.colored_label(GOOD, tr("Profile applied."));
        }
        Some(Err(e)) => {
            ui.colored_label(ERROR, e);
        }
        None => {}
    }
}
