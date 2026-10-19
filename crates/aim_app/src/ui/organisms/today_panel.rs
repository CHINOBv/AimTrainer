//! Today's session of the weekly method: skill of the day, tier, progress, start/resume.

use crate::i18n::{tr, tr_fmt, tr_named};
use crate::ui::atoms::{GOOD, WARN, big};
use aim_core::training::resume::RoutinePosition;
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
    pub missions: MissionsModel,
}

/// Today's compound missions.
pub struct MissionsModel {
    pub commitment: &'static str,
    pub missions: Vec<MissionModel>,
}

pub struct MissionModel {
    pub scenario_id: String,
    pub scenario_name: String,
    /// Each condition, for example "accuracy ≥ 75%".
    pub conditions: Vec<String>,
    pub mastery: bool,
    pub done: bool,
    /// Most conditions met in a single run today.
    pub best_met: usize,
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

/// The Today page's shortcuts, shown even without a weekly method.
pub struct QuickModel {
    /// `(id, name)` of the scenario played last.
    pub last_scenario: Option<(String, String)>,
    /// An unfinished routine from today, with its name.
    pub routine: Option<(RoutinePosition, String)>,
}

pub enum TodayAction {
    /// Play the last scenario again.
    QuickPlay(String),
    /// Continue a routine started today where it stopped.
    Continue(RoutinePosition),
    Challenge,
    SelectTier(String),
    Start(String),
    Resume(String),
    WarmUp,
    /// Play a mission's scenario.
    PlayMission(String),
}

fn missions_section(ui: &mut egui::Ui, m: &MissionsModel, action: &mut Option<TodayAction>) {
    ui.heading(tr("Daily missions"));
    ui.small(tr_fmt(
        "Commitment: {} (change it in Settings). Every condition must hold in the same run; targets come from your recent bests.",
        &[&m.commitment],
    ));
    if m.missions.is_empty() {
        ui.label(tr("Play any scenario at its normal difficulty to unlock missions built from your own scores."));
        return;
    }
    for (i, mission) in m.missions.iter().enumerate() {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let name = tr(&mission.scenario_name);
            let title = if mission.mastery {
                tr_fmt("★ Mastery · {}", &[&name])
            } else {
                name.to_owned()
            };
            ui.strong(title);
            if mission.done {
                ui.colored_label(GOOD, tr("✔ done"));
            } else {
                ui.small(tr_fmt(
                    "best run today: {}/{} conditions",
                    &[&mission.best_met, &mission.conditions.len()],
                ));
                if ui
                    .push_id(i, |ui| {
                        crate::audio::ui_click::clicked(ui.button(tr("▶  Play")))
                    })
                    .inner
                    .clicked()
                {
                    *action = Some(TodayAction::PlayMission(mission.scenario_id.clone()));
                }
            }
        });
        ui.label(mission.conditions.join(tr("  AND  ")));
    }
}

fn routine_card(ui: &mut egui::Ui, r: &RoutineModel, action: &mut Option<TodayAction>) {
    ui.heading(tr(&r.name));
    ui.label(tr(&r.description));
    ui.label(tr_fmt(
        "{} runs · ≈{} min",
        &[&r.runs, &format!("{:.0}", r.minutes)],
    ));
    ui.add_space(6.0);
    match r.progress {
        Some((next, total)) if next >= total => {
            ui.colored_label(
                GOOD,
                tr("Done for today. Rest, or play a quick warm-up before ranked."),
            );
            if crate::audio::ui_click::clicked(ui.button(tr("↻ Play again from the start")))
                .clicked()
            {
                *action = Some(TodayAction::Start(r.id.clone()));
            }
        }
        Some((next, total)) => {
            ui.add(
                egui::ProgressBar::new(next as f32 / total as f32)
                    .text(tr_fmt("run {}/{}", &[&next, &total])),
            );
            ui.horizontal(|ui| {
                if ui
                    .add_sized([180.0, 34.0], egui::Button::new(tr("▶  Resume")))
                    .clicked()
                {
                    *action = Some(TodayAction::Resume(r.id.clone()));
                }
                if ui.button(tr("Restart")).clicked() {
                    *action = Some(TodayAction::Start(r.id.clone()));
                }
            });
            ui.small(tr(
                "Splitting a long day into 2–3 sessions is fine; progress is kept until midnight.",
            ));
        }
        None => {
            if ui
                .add_sized(
                    [180.0, 34.0],
                    egui::Button::new(tr("▶  Start today's session")),
                )
                .clicked()
            {
                *action = Some(TodayAction::Start(r.id.clone()));
            }
        }
    }
}

/// Minimum size of the shortcut buttons, points.
const QUICK_BUTTON: egui::Vec2 = egui::vec2(0.0, 30.0);

/// "Quick play" and "Continue routine" buttons; nothing when neither applies.
pub fn quick_bar(ui: &mut egui::Ui, m: &QuickModel) -> Option<TodayAction> {
    let mut action = None;
    if m.last_scenario.is_none() && m.routine.is_none() {
        return None;
    }
    ui.horizontal_wrapped(|ui| {
        if let Some((id, name)) = &m.last_scenario
            && ui
                .add(
                    egui::Button::new(tr_fmt("▶  Quick play: {}", &[&tr(name)]))
                        .min_size(QUICK_BUTTON),
                )
                .on_hover_text(tr("Play the scenario you played last."))
                .clicked()
        {
            action = Some(TodayAction::QuickPlay(id.clone()));
        }
        if let Some((pos, name)) = &m.routine
            && ui
                .add(
                    egui::Button::new(tr_named(
                        "⏵ Continue {name} (run {next}/{total})",
                        &[
                            ("name", &tr(name)),
                            ("next", &(pos.next_index + 1)),
                            ("total", &pos.total_runs),
                        ],
                    ))
                    .min_size(QUICK_BUTTON),
                )
                .clicked()
        {
            action = Some(TodayAction::Continue(pos.clone()));
        }
    });
    ui.separator();
    action
}

pub fn today_panel(ui: &mut egui::Ui, m: &TodayModel) -> Option<TodayAction> {
    let mut action = None;
    egui::ScrollArea::vertical().id_salt("today").show(ui, |ui| {
        ui.label(big(
            format!("{} · {}", m.weekday, tr(&m.method_name)),
            24.0,
        ));
        ui.label(tr(&m.method_description));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(tr("Tier"));
            let selected = m
                .tiers
                .iter()
                .find(|t| t.0 == m.selected_tier)
                .map_or("", |t| tr(&t.1));
            egui::ComboBox::from_id_salt("tier")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (id, name) in &m.tiers {
                        if ui.selectable_label(*id == m.selected_tier, tr(name)).clicked() {
                            action = Some(TodayAction::SelectTier(id.clone()));
                        }
                    }
                });
            let suggested = m
                .tiers
                .iter()
                .find(|t| t.0 == m.suggested_tier)
                .map_or("", |t| tr(&t.1));
            let energy = m
                .benchmark_energy
                .map_or(tr("not benchmarked yet").to_owned(), |e| {
                    tr_fmt("{} energy", &[&format!("{e:.0}")])
                });
            ui.small(tr_named(
                "Suggested: {tier} ({energy})",
                &[("tier", &suggested), ("energy", &energy)],
            ));
        });
        if m.selected_tier != m.suggested_tier {
            ui.colored_label(
                WARN,
                tr("You picked a different tier than suggested; move back if it feels too hard."),
            );
        }
        ui.separator();
        match &m.day {
            DayModel::Routine(r) => routine_card(ui, r, &mut action),
            DayModel::Flex { weakest } => {
                ui.heading(tr("Flex day"));
                ui.label(tr("Rest is training too: sleep and spacing consolidate skills."));
                if let Some(r) = weakest {
                    ui.add_space(6.0);
                    ui.label(tr("Or replay your weakest skill day:"));
                    routine_card(ui, r, &mut action);
                }
                ui.add_space(6.0);
                if ui.button(tr("Quick warm-up (10 min)")).clicked() {
                    action = Some(TodayAction::WarmUp);
                }
            }
        }
        if let Some(c) = &m.challenge {
            ui.separator();
            ui.heading(tr("Daily challenge"));
            ui.label(tr_fmt(
                "{} with today's fixed seed: everyone gets the same targets, so you can compare scores with friends.",
                &[&tr(&c.scenario_name)],
            ));
            let best = c.best_today.map_or(tr("not played yet").to_owned(), |b| {
                tr_fmt("your best today {}", &[&format!("{b:.0}")])
            });
            ui.horizontal(|ui| {
                if crate::audio::ui_click::clicked(ui.button(tr("▶  Play the daily challenge"))).clicked() {
                    action = Some(TodayAction::Challenge);
                }
                ui.small(best);
            });
        }
        ui.separator();
        missions_section(ui, &m.missions, &mut action);
        ui.add_space(10.0);
        ui.small(tr(&m.credit));
    });
    action
}
