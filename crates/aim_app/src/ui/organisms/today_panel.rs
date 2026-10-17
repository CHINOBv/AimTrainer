//! Today's session of the weekly method: skill of the day, tier, progress, start/resume.

use crate::ui::atoms::{GOOD, WARN, big};
use bevy_egui::egui;

/// Plain data the Today page shows.
pub struct TodayModel {
    pub weekday: &'static str,
    pub method_name: String,
    pub method_description: String,
    pub credit: String,
    /// `(id, name)` of every tier.
    pub tiers: Vec<(String, String)>,
    pub selected_tier: String,
    pub suggested_tier: String,
    pub benchmark_energy: Option<f32>,
    pub day: DayModel,
    pub challenge: Option<ChallengeModel>,
}

/// Today's daily challenge: same scenario and targets for every player.
pub struct ChallengeModel {
    pub scenario_name: String,
    pub best_today: Option<f32>,
}

pub enum DayModel {
    Routine(RoutineModel),
    /// Free day: replay the weakest skill day, warm up, or rest.
    Flex {
        weakest: Option<RoutineModel>,
    },
}

pub struct RoutineModel {
    pub id: String,
    pub name: String,
    pub description: String,
    pub runs: usize,
    pub minutes: f32,
    /// `(next run, total runs)` when today's session was started earlier.
    pub progress: Option<(usize, usize)>,
}

pub enum TodayAction {
    Challenge,
    SelectTier(String),
    Start(String),
    Resume(String),
    WarmUp,
}

fn routine_card(ui: &mut egui::Ui, r: &RoutineModel, action: &mut Option<TodayAction>) {
    ui.heading(&r.name);
    ui.label(&r.description);
    ui.label(format!("{} runs · ≈{:.0} min", r.runs, r.minutes));
    ui.add_space(6.0);
    match r.progress {
        Some((next, total)) if next >= total => {
            ui.colored_label(
                GOOD,
                "Done for today. Rest, or play a quick warm-up before ranked.",
            );
            if ui.button("↻ Play again from the start").clicked() {
                *action = Some(TodayAction::Start(r.id.clone()));
            }
        }
        Some((next, total)) => {
            ui.add(
                egui::ProgressBar::new(next as f32 / total as f32)
                    .text(format!("run {next}/{total}")),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_sized([180.0, 34.0], egui::Button::new("▶  Resume"))
                    .clicked()
                {
                    *action = Some(TodayAction::Resume(r.id.clone()));
                }
                if ui.button("Restart").clicked() {
                    *action = Some(TodayAction::Start(r.id.clone()));
                }
            });
            ui.small(
                "Splitting a long day into 2–3 sessions is fine; progress is kept until midnight.",
            );
        }
        None => {
            if ui
                .add_sized([180.0, 34.0], egui::Button::new("▶  Start today's session"))
                .clicked()
            {
                *action = Some(TodayAction::Start(r.id.clone()));
            }
        }
    }
}

pub fn today_panel(ui: &mut egui::Ui, m: &TodayModel) -> Option<TodayAction> {
    let mut action = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.label(big(format!("{} · {}", m.weekday, m.method_name), 24.0));
        ui.label(&m.method_description);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Tier");
            let selected = m
                .tiers
                .iter()
                .find(|t| t.0 == m.selected_tier)
                .map_or("", |t| t.1.as_str());
            egui::ComboBox::from_id_salt("tier")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (id, name) in &m.tiers {
                        if ui.selectable_label(*id == m.selected_tier, name).clicked() {
                            action = Some(TodayAction::SelectTier(id.clone()));
                        }
                    }
                });
            let suggested = m
                .tiers
                .iter()
                .find(|t| t.0 == m.suggested_tier)
                .map_or("", |t| t.1.as_str());
            let energy = m
                .benchmark_energy
                .map_or("not benchmarked yet".to_owned(), |e| {
                    format!("{e:.0} energy")
                });
            ui.small(format!("Suggested: {suggested} ({energy})"));
        });
        if m.selected_tier != m.suggested_tier {
            ui.colored_label(
                WARN,
                "You picked a different tier than suggested; move back if it feels too hard.",
            );
        }
        ui.separator();
        match &m.day {
            DayModel::Routine(r) => routine_card(ui, r, &mut action),
            DayModel::Flex { weakest } => {
                ui.heading("Flex day");
                ui.label("Rest is training too: sleep and spacing consolidate skills.");
                if let Some(r) = weakest {
                    ui.add_space(6.0);
                    ui.label("Or replay your weakest skill day:");
                    routine_card(ui, r, &mut action);
                }
                ui.add_space(6.0);
                if ui.button("Quick warm-up (10 min)").clicked() {
                    action = Some(TodayAction::WarmUp);
                }
            }
        }
        if let Some(c) = &m.challenge {
            ui.separator();
            ui.heading("Daily challenge");
            ui.label(format!(
                "{} with today's fixed seed: everyone gets the same targets, so you can compare scores with friends.",
                c.scenario_name
            ));
            let best = c.best_today.map_or("not played yet".to_owned(), |b| format!("your best today {b:.0}"));
            ui.horizontal(|ui| {
                if ui.button("▶  Play the daily challenge").clicked() {
                    action = Some(TodayAction::Challenge);
                }
                ui.small(best);
            });
        }
        ui.add_space(10.0);
        ui.small(&m.credit);
    });
    action
}
