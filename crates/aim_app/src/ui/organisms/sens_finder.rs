//! Guided sensitivity finder: short wide-switching tests at a faster and a slower
//! sensitivity, narrowing toward the one you score best with.

use crate::i18n::{tr, tr_fmt, tr_named};
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
    ui.heading(tr("Find my sensitivity"));
    ui.label(tr(
        "Plays short wide-switching tests at a faster and a slower sensitivity, keeps the one you score \
         better with, and narrows the gap each round. Scores are noisy: run it twice and trust a result \
         that repeats. Tests do not count toward your history.",
    ));
    match finder {
        None => {
            let cm = settings.sensitivity.cm_per_360();
            if ui
                .button(tr_fmt(
                    "▶ Start from your current {} cm/360",
                    &[&format!("{cm:.1}")],
                ))
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
                    ui.strong(tr("Round"));
                    ui.strong(tr("Fast"));
                    ui.strong(tr("Slow"));
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
                    // Whole sentences, so each language keeps its own word order.
                    let template = match slot {
                        Slot::Fast => "▶ Test the faster sensitivity ({} cm/360)",
                        Slot::Slow => "▶ Test the slower sensitivity ({} cm/360)",
                    };
                    ui.label(tr_fmt("Round {}/{}", &[&(f.round + 1), &MAX_ROUNDS]));
                    if ui
                        .button(tr_fmt(template, &[&format!("{cm:.1}")]))
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
                        tr_named(
                            "Best for you: {cm} cm/360 = {game} {value} at {dpi} DPI",
                            &[
                                ("cm", &format!("{:.1}", f.center_cm)),
                                ("game", &rec.game.display_name()),
                                ("value", &format!("{:.3}", rec.value)),
                                ("dpi", &rec.dpi),
                            ],
                        ),
                    );
                    if ui.button(tr("Apply as my sensitivity")).clicked() {
                        settings.sensitivity = rec;
                    }
                    ui.colored_label(
                        WARN,
                        tr("Change your in-game sensitivity to match before playing ranked."),
                    );
                }
            }
            if ui.small_button(tr("Reset finder")).clicked() {
                *finder = None;
            }
        }
    }
    action
}
