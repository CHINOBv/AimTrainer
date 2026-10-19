//! Progress page: daily goal and streak, readiness before ranked, fatigue, benchmark trend,
//! personal bests and per-skill standings with their trends. Every verdict shows the
//! numbers behind it.

use crate::ui::UiAction;
use crate::ui::atoms::{BAD, GOOD, WARN, big};
use crate::ui::organisms::history_chart::history_chart;
use aim_core::analytics::BenchmarkProfile;
use aim_core::analytics::profile::Verdict;
use aim_core::analytics::skills::SubcategoryStanding;
use aim_core::training::progress::{
    DailyGoal, FatigueSignal, PLATEAU_WEEKS, Plateau, Readiness, Warmth,
};
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

/// What the history export row shows.
pub struct ExportView<'a> {
    /// Placeholder of the path field: where the export goes when it is left empty.
    pub default_path: &'a str,
    pub running: bool,
    /// Outcome of the last export.
    pub status: Option<&'a str>,
}

/// "Export history (CSV)": a path field (empty = the default path) and the button. Returns
/// true when the button is clicked.
pub fn export_section(ui: &mut egui::Ui, path: &mut String, view: &ExportView) -> bool {
    ui.add_space(10.0);
    ui.label(big("History", 20.0));
    ui.small("One row per run with its settings snapshot; columns are listed in the README.");
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(path)
                .hint_text(view.default_path)
                .desired_width(360.0),
        );
        let button = ui.add_enabled(!view.running, egui::Button::new("Export history (CSV)"));
        clicked = button.clicked();
        if view.running {
            ui.spinner();
        }
    });
    if let Some(status) = view.status {
        let color = if status.starts_with("Export failed") {
            BAD
        } else {
            GOOD
        };
        ui.colored_label(color, status);
    }
    clicked
}

/// The headline of one plateau.
pub fn plateau_text(p: &Plateau) -> String {
    format!(
        "{}: no new personal best in {} week{}, although you kept playing it.",
        p.subcategory,
        p.weeks,
        if p.weeks == 1 { "" } else { "s" }
    )
}

/// Benchmark subcategories without a new personal best for [`PLATEAU_WEEKS`] weeks, each
/// with the changes of stimulus to try.
pub fn plateau_section(ui: &mut egui::Ui, plateaus: &[Plateau]) {
    ui.add_space(10.0);
    ui.label(big("Plateaus", 20.0));
    ui.small(format!(
        "A subcategory you still play with no new personal best in {PLATEAU_WEEKS} weeks (an estimate, not a sourced threshold)."
    ));
    if plateaus.is_empty() {
        ui.label("No plateau right now.");
        return;
    }
    for p in plateaus {
        ui.colored_label(WARN, plateau_text(p));
        for s in &p.suggestions {
            ui.label(format!("  • {}", s.text()));
        }
    }
}

/// Played weekly energies of a subcategory, oldest first (weeks before its first run are
/// skipped).
pub fn trend_points(s: &SubcategoryStanding) -> Vec<f32> {
    s.weekly.iter().flatten().copied().collect()
}

/// "+120 energy over the last 3 weeks": change between the first and last played weeks and
/// the weeks between them, or `None` with fewer than two played weeks.
pub fn trend_text(s: &SubcategoryStanding) -> Option<String> {
    let played = || {
        s.weekly
            .iter()
            .enumerate()
            .filter_map(|(i, e)| Some((i, (*e)?)))
    };
    let (i0, first) = played().next()?;
    let (i1, last) = played().next_back()?;
    let weeks = i1 - i0;
    let unit = if weeks == 1 { "week" } else { "weeks" };
    (weeks >= 1).then(|| format!("{:+.0} energy over the last {weeks} {unit}", last - first))
}

/// Skills section: each benchmark subcategory's strength or weakness, its weekly energy
/// trend, and a button to practice a weak one. `name_of` maps a scenario id to its name.
pub fn skills_section(
    ui: &mut egui::Ui,
    profile: &BenchmarkProfile,
    name_of: impl Fn(&str) -> String,
) -> Option<UiAction> {
    let mut action = None;
    ui.add_space(10.0);
    ui.label(big("Skills", 20.0));
    ui.small(
        "Each benchmark subcategory compared with your average over the ones you played (provisional 15% margin).",
    );
    for s in &profile.subcategories {
        ui.horizontal(|ui| {
            match s.verdict {
                Some(Verdict::Strength) => {
                    ui.colored_label(GOOD, "STRENGTH");
                }
                Some(Verdict::Weakness) => {
                    ui.colored_label(BAD, "WEAKNESS");
                }
                None => {}
            }
            ui.label(&s.explanation);
            if s.verdict == Some(Verdict::Weakness)
                && let Some(id) = s.scenario_ids.first()
                && ui
                    .small_button("▶")
                    .on_hover_text(format!("Practice {}", name_of(id)))
                    .clicked()
            {
                action = Some(UiAction::StartScenario(id.clone()));
            }
        });
        if let Some(text) = trend_text(s) {
            ui.small(format!("{} ({}) · {text}", s.name, s.skill));
            history_chart(ui, &trend_points(s));
        }
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plateau_lines_name_the_subcategory_and_the_weeks() {
        use aim_core::training::progress::PlateauSuggestion;
        let p = Plateau {
            subcategory: "Static clicking".into(),
            skill: "clicking".into(),
            weeks: 4,
            last_best_at: 0,
            suggestions: PlateauSuggestion::ALL.to_vec(),
        };
        assert_eq!(
            plateau_text(&p),
            "Static clicking: no new personal best in 4 weeks, although you kept playing it."
        );
        let one = Plateau { weeks: 1, ..p };
        assert!(plateau_text(&one).contains("in 1 week,"));
    }

    fn standing(weekly: Vec<Option<f32>>) -> SubcategoryStanding {
        SubcategoryStanding {
            name: "Static clicking".into(),
            skill: "clicking".into(),
            energy: weekly.last().copied().flatten(),
            verdict: None,
            explanation: String::new(),
            weekly,
            scenario_ids: vec!["micro-adjust".into()],
        }
    }

    #[test]
    fn skill_trend_skips_unplayed_weeks_and_needs_two_points() {
        let s = standing(vec![None, Some(100.0), None, Some(220.0)]);
        assert_eq!(trend_points(&s), vec![100.0, 220.0]);
        assert_eq!(
            trend_text(&s).as_deref(),
            Some("+120 energy over the last 2 weeks")
        );
        assert_eq!(trend_text(&standing(vec![None, Some(100.0)])), None);
        assert_eq!(
            trend_text(&standing(vec![Some(300.0), Some(250.0)])).as_deref(),
            Some("-50 energy over the last 1 week"),
            "two consecutive weekly points span one week"
        );
    }

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
