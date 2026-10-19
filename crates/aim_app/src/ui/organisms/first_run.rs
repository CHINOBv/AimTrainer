//! First-run wizard: game, sensitivity, DPI and feedback preset, shown once to a new
//! player and skippable at any step.

use super::feedback_settings::{FEEL_PRESETS, feel_preset_picker};
use super::settings_panels::game_combo;
use crate::persistence::settings::Settings;
use bevy_egui::egui;

/// The wizard's pages, in order.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum WizardStep {
    #[default]
    Game,
    Sensitivity,
    Dpi,
    Feedback,
}

impl WizardStep {
    pub const ALL: [WizardStep; 4] = [
        WizardStep::Game,
        WizardStep::Sensitivity,
        WizardStep::Dpi,
        WizardStep::Feedback,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }

    /// The following page; `None` on the last one.
    pub fn next(self) -> Option<Self> {
        Self::ALL.get(self.index() + 1).copied()
    }

    /// The previous page; `None` on the first one.
    pub fn prev(self) -> Option<Self> {
        self.index().checked_sub(1).map(|i| Self::ALL[i])
    }
}

/// Draw the wizard while `settings.first_run_done` is false. Finishing or skipping sets it.
pub fn first_run_wizard(ctx: &egui::Context, step: &mut WizardStep, settings: &mut Settings) {
    if settings.first_run_done {
        return;
    }
    egui::Window::new("Welcome to CustomAimTrainer")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.small(format!(
                "Step {} of {}",
                step.index() + 1,
                WizardStep::ALL.len()
            ));
            ui.add_space(4.0);
            match step {
                WizardStep::Game => {
                    ui.label("Which game do you play? Your sensitivity is entered in its units.");
                    game_combo(ui, settings);
                }
                WizardStep::Sensitivity => {
                    ui.label(format!(
                        "Your in-game sensitivity in {}:",
                        settings.sensitivity.game.display_name()
                    ));
                    ui.add(
                        egui::DragValue::new(&mut settings.sensitivity.value)
                            .speed(0.001)
                            .range(0.001..=100.0)
                            .max_decimals(4),
                    );
                }
                WizardStep::Dpi => {
                    ui.label("Your mouse DPI (check your mouse software):");
                    ui.add(
                        egui::DragValue::new(&mut settings.sensitivity.dpi)
                            .speed(10)
                            .range(50..=64000),
                    );
                    let s = settings.sensitivity;
                    ui.small(format!(
                        "{:.1} cm/360 · eDPI {:.0}",
                        s.cm_per_360(),
                        s.edpi()
                    ));
                }
                WizardStep::Feedback => {
                    ui.label("How much feedback do you want while you play?");
                    feel_preset_picker(ui, &mut settings.feel);
                    if let Some((_, _, hint)) =
                        FEEL_PRESETS.iter().find(|p| p.0 == settings.feel.preset)
                    {
                        ui.small(*hint);
                    }
                }
            }
            ui.add_space(8.0);
            ui.small("Everything can be changed later in Settings.");
            ui.horizontal(|ui| {
                if ui.button("Skip").clicked() {
                    settings.first_run_done = true;
                }
                if let Some(prev) = step.prev()
                    && ui.button("Back").clicked()
                {
                    *step = prev;
                }
                match step.next() {
                    Some(next) => {
                        if ui.button("Next").clicked() {
                            *step = next;
                        }
                    }
                    None => {
                        if ui.button("Finish").clicked() {
                            settings.first_run_done = true;
                        }
                    }
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_go_forward_and_back_through_every_page_in_order() {
        let mut step = WizardStep::default();
        let mut seen = vec![step];
        while let Some(next) = step.next() {
            step = next;
            seen.push(step);
        }
        assert_eq!(seen, WizardStep::ALL);
        assert_eq!(WizardStep::Game.prev(), None);
        assert_eq!(WizardStep::Feedback.prev(), Some(WizardStep::Dpi));
    }
}
