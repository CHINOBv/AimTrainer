//! Guided sensitivity finder: short wide-switching tests at a faster and a slower
//! sensitivity, narrowing toward the one you score best with.

use crate::persistence::settings::Settings;
use crate::ui::UiAction;
use crate::ui::atoms::{GOOD, WARN};
use aim_core::sensitivity::Sensitivity;
use aim_core::sensitivity::finder::{MAX_ROUNDS, SensFinder, Slot};
use bevy_egui::egui;

fn sens_at(settings: &Settings, cm: f64) -> Sensitivity {
    let s = settings.sensitivity;
    Sensitivity::from_cm_per_360(s.game, cm, s.dpi)
}

pub fn sens_finder(
    ui: &mut egui::Ui,
    finder: &mut Option<SensFinder>,
    settings: &mut Settings,
) -> Option<UiAction> {
    let mut action = None;
    ui.heading("Find my sensitivity");
    ui.label(
        "Plays short wide-switching tests at a faster and a slower sensitivity, keeps the one you score \
         better with, and narrows the gap each round. Scores are noisy: run it twice and trust a result \
         that repeats. Tests do not count toward your history.",
    );
    match finder {
        None => {
            let cm = settings.sensitivity.cm_per_360();
            if ui
                .button(format!("▶ Start from your current {cm:.1} cm/360"))
                .clicked()
            {
                *finder = Some(SensFinder::new(cm));
            }
        }
        Some(f) => {
            egui::Grid::new("finder-rounds")
                .striped(true)
                .num_columns(3)
                .show(ui, |ui| {
                    ui.strong("Round");
                    ui.strong("Fast");
                    ui.strong("Slow");
                    ui.end_row();
                    for (i, (fast, slow, fs, ss)) in f.rounds.iter().enumerate() {
                        ui.label(format!("{}", i + 1));
                        ui.label(format!("{fast:.1} cm → {fs:.0}"));
                        ui.label(format!("{slow:.1} cm → {ss:.0}"));
                        ui.end_row();
                    }
                });
            match f.next_slot() {
                Some(slot) => {
                    let cm = f.cm_for(slot);
                    let label = match slot {
                        Slot::Fast => "faster",
                        Slot::Slow => "slower",
                    };
                    ui.label(format!("Round {}/{MAX_ROUNDS}", f.round + 1));
                    if ui
                        .button(format!("▶ Test the {label} sensitivity ({cm:.1} cm/360)"))
                        .clicked()
                    {
                        action = Some(UiAction::StartFinderTest {
                            slot,
                            sens: sens_at(settings, cm),
                        });
                    }
                }
                None => {
                    let rec = sens_at(settings, f.center_cm);
                    ui.colored_label(
                        GOOD,
                        format!(
                            "Best for you: {:.1} cm/360 = {} {:.3} at {} DPI",
                            f.center_cm,
                            rec.game.display_name(),
                            rec.value,
                            rec.dpi
                        ),
                    );
                    if ui.button("Apply as my sensitivity").clicked() {
                        settings.sensitivity = rec;
                    }
                    ui.colored_label(
                        WARN,
                        "Change your in-game sensitivity to match before playing ranked.",
                    );
                }
            }
            if ui.small_button("Reset finder").clicked() {
                *finder = None;
            }
        }
    }
    action
}
