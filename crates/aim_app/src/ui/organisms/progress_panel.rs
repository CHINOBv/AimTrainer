//! Progress page: daily goal and streak, readiness before ranked, fatigue, benchmark trend
//! and personal bests. Every verdict shows the numbers behind it.

use crate::ui::UiAction;
use crate::ui::atoms::{BAD, GOOD, WARN, big};
use crate::ui::organisms::history_chart::history_chart;
use aim_core::training::progress::{DailyGoal, FatigueSignal, Readiness, Warmth};
use bevy_egui::egui;

pub struct ProgressModel {
    pub goal: DailyGoal,
    /// `(scenario name, readiness)` for the warm-up reference scenarios with enough history.
    pub readiness: Vec<(String, Readiness)>,
    /// Reference scenarios without a baseline yet: `(id, name)`.
    pub readiness_missing: Vec<(String, String)>,
    pub fatigue: Vec<FatigueSignal>,
    /// Overall benchmark energy at the end of each of the last weeks (oldest first).
    pub trend: Vec<Option<f32>>,
    /// `(subcategory, scenario name, best score)`.
    pub bests: Vec<(String, String, Option<f32>)>,
}

/// One line explaining a fatigue signal.
pub fn fatigue_text(f: &FatigueSignal) -> String {
    match f {
        FatigueSignal::LongSession { minutes } => format!(
            "You have trained {minutes:.0} min this session. Gains flatten after about an hour; stopping now keeps today's progress."
        ),
        FatigueSignal::ScoreDrop {
            scenario,
            peak,
            recent,
            drop,
        } => format!(
            "{scenario}: your last runs average {recent:.0}, {:.0}% below this session's peak of {peak:.0}. That usually means fatigue; take a break.",
            drop * 100.0
        ),
    }
}

/// One line for the daily goal (also used on the results screen).
pub fn goal_text(g: &DailyGoal) -> String {
    let streak = if g.streak_days > 0 {
        format!(" · {} day streak", g.streak_days)
    } else {
        String::new()
    };
    format!(
        "Today {:.0}/{:.0} min{streak}",
        g.minutes_today, g.goal_minutes
    )
}

pub fn progress_panel(ui: &mut egui::Ui, m: &ProgressModel) -> Option<UiAction> {
    let mut action = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.label(big("Today", 22.0));
        let g = &m.goal;
        ui.add(
            egui::ProgressBar::new((g.minutes_today / g.goal_minutes).clamp(0.0, 1.0))
                .text(goal_text(g)),
        );
        if g.met_today() {
            ui.colored_label(
                GOOD,
                "Daily goal reached. More time today adds little; rest or play.",
            );
        }
        for f in &m.fatigue {
            ui.colored_label(WARN, fatigue_text(f));
        }

        ui.add_space(10.0);
        ui.label(big("Ready for ranked?", 20.0));
        ui.small(
            "Today's best compared with the median of your daily bests over the last 30 days.",
        );
        for (name, r) in &m.readiness {
            let (label, color) = match r.warmth {
                Warmth::Ready => ("READY", GOOD),
                Warmth::Warming => ("WARMING UP", WARN),
                Warmth::Cold => ("COLD", BAD),
            };
            ui.horizontal(|ui| {
                ui.colored_label(color, label);
                ui.label(format!(
                    "{name}: today {:.0} vs usual {:.0} ({:.0}%)",
                    r.today_best,
                    r.baseline,
                    r.ratio * 100.0
                ));
            });
        }
        for (id, name) in &m.readiness_missing {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{name}: play it today (and on 3 different days) to get a readiness check."
                ));
                if ui.small_button("▶").clicked() {
                    action = Some(UiAction::StartScenario(id.clone()));
                }
            });
        }

        ui.add_space(10.0);
        ui.label(big("Benchmark trend", 20.0));
        let points: Vec<f32> = m.trend.iter().flatten().copied().collect();
        if points.len() >= 2 {
            ui.small(format!("Overall energy, last {} weeks", m.trend.len()));
            history_chart(ui, &points);
        } else {
            ui.small("Score every benchmark subcategory to start a trend.");
        }

        ui.add_space(10.0);
        ui.label(big("Personal bests", 20.0));
        egui::Grid::new("bests")
            .striped(true)
            .num_columns(3)
            .show(ui, |ui| {
                for (sub, name, best) in &m.bests {
                    ui.label(sub);
                    ui.label(name);
                    ui.label(best.map_or("—".to_owned(), |b| format!("{b:.0}")));
                    ui.end_row();
                }
            });
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goal_and_fatigue_texts_carry_their_numbers() {
        let g = DailyGoal {
            minutes_today: 12.4,
            goal_minutes: 30.0,
            streak_days: 3,
        };
        assert_eq!(goal_text(&g), "Today 12/30 min · 3 day streak");
        let f = FatigueSignal::ScoreDrop {
            scenario: "Micro Adjust".into(),
            peak: 1000.0,
            recent: 850.0,
            drop: 0.15,
        };
        let t = fatigue_text(&f);
        assert!(
            t.contains("850") && t.contains("15%") && t.contains("1000"),
            "{t}"
        );
    }
}
