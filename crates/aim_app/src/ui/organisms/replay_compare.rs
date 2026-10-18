//! Replay comparison: pick an earlier run of the same scenario and list how this run differs.
//!
//! Presentational: the caller passes the earlier runs (already filtered to the same base
//! scenario) and the `load` function; [`ReplayCompareState`] caches the loaded earlier run.
//! The figures come from the core's [`PathSummary`]; this module only formats them.

use super::replay_viewer::timestamp_label;
use crate::persistence::replays::ReplayEntry;
use crate::ui::atoms::{BAD, GOOD};
use aim_core::analytics::{PathSummary, ReplayPath};
use bevy_egui::egui;
use std::path::PathBuf;

/// Shown for a value one of the runs does not have.
const MISSING: &str = "–";

/// The earlier run picked for comparison.
#[derive(Debug, Default)]
pub struct ReplayCompareState {
    selected: Option<PathBuf>,
    /// `None` when nothing is picked or loading failed.
    path: Option<ReplayPath>,
}

impl ReplayCompareState {
    /// Forget the picked run.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// The loaded earlier run, if one is picked and could be read.
    pub fn path(&self) -> Option<&ReplayPath> {
        self.path.as_ref()
    }

    /// Pick `entry` and load it once.
    pub fn select(
        &mut self,
        entry: &ReplayEntry,
        load: &impl Fn(&ReplayEntry) -> Option<ReplayPath>,
    ) {
        self.selected = Some(entry.path.clone());
        self.path = load(entry);
    }
}

/// One line of the comparison table.
#[derive(Debug, Clone, PartialEq)]
pub struct CompareRow {
    pub label: &'static str,
    /// This run's value.
    pub current: String,
    /// The earlier run's value.
    pub earlier: String,
    /// This run minus the earlier run.
    pub change: String,
    /// `Some(true)` when the change is an improvement, `Some(false)` when it is worse,
    /// `None` when there is no change, no data, or no better direction.
    pub better: Option<bool>,
}

/// Which way a metric improves.
#[derive(Clone, Copy)]
enum Better {
    Higher,
    Lower,
}

fn row(
    label: &'static str,
    current: Option<f32>,
    earlier: Option<f32>,
    value: impl Fn(f32) -> String,
    change: impl Fn(f32) -> String,
    better: Better,
) -> CompareRow {
    let text = |v: Option<f32>| v.map_or_else(|| MISSING.to_owned(), &value);
    let delta = current.zip(earlier).map(|(c, e)| c - e);
    CompareRow {
        label,
        current: text(current),
        earlier: text(earlier),
        change: delta.map_or_else(|| MISSING.to_owned(), change),
        better: delta.filter(|d| *d != 0.0).map(|d| match better {
            Better::Higher => d > 0.0,
            Better::Lower => d < 0.0,
        }),
    }
}

fn percent(v: f32) -> String {
    format!("{:.0}%", v * 100.0)
}

fn points(d: f32) -> String {
    format!("{:+.0} pts", d * 100.0)
}

/// The comparison table of this run against an earlier one. Shares are per engagement so
/// runs of different lengths compare fairly.
pub fn compare_rows(current: &PathSummary, earlier: &PathSummary) -> Vec<CompareRow> {
    let ms = |s: Option<f32>| s.map(|s| s * 1000.0);
    vec![
        CompareRow {
            label: "Engagements",
            current: current.engagements.to_string(),
            earlier: earlier.engagements.to_string(),
            change: format!(
                "{:+}",
                current.engagements as i64 - earlier.engagements as i64
            ),
            better: None,
        },
        row(
            "Hit rate",
            current.hit_rate(),
            earlier.hit_rate(),
            percent,
            points,
            Better::Higher,
        ),
        row(
            "Overshoot",
            current.overshoot_share(),
            earlier.overshoot_share(),
            percent,
            points,
            Better::Lower,
        ),
        row(
            "Undershoot",
            current.undershoot_share(),
            earlier.undershoot_share(),
            percent,
            points,
            Better::Lower,
        ),
        row(
            "Corrections / engagement",
            current.corrections_per_engagement(),
            earlier.corrections_per_engagement(),
            |v| format!("{v:.1}"),
            |d| format!("{d:+.1}"),
            Better::Lower,
        ),
        row(
            "Median reaction",
            ms(current.median_reaction_secs),
            ms(earlier.median_reaction_secs),
            |v| format!("{v:.0} ms"),
            |d| format!("{d:+.0} ms"),
            Better::Lower,
        ),
    ]
}

/// A drop-down of `candidates` (earlier runs of the same scenario, newest first), plus
/// "None". Loads the picked run once.
pub fn compare_picker(
    ui: &mut egui::Ui,
    state: &mut ReplayCompareState,
    candidates: &[&ReplayEntry],
    load: &impl Fn(&ReplayEntry) -> Option<ReplayPath>,
) {
    if state
        .selected
        .as_ref()
        .is_some_and(|p| !candidates.iter().any(|c| &c.path == p))
    {
        state.clear();
    }
    let shown = state
        .selected
        .as_ref()
        .and_then(|p| candidates.iter().find(|c| &c.path == p))
        .map_or_else(
            || "None".to_owned(),
            |c| timestamp_label(c.timestamp_millis),
        );
    ui.horizontal(|ui| {
        ui.label("Compare with an earlier run:");
        if candidates.is_empty() {
            ui.small("none saved for this scenario yet");
            return;
        }
        let mut pick: Option<Option<&ReplayEntry>> = None;
        egui::ComboBox::from_id_salt("replay_compare_pick")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(state.selected.is_none(), "None")
                    .clicked()
                {
                    pick = Some(None);
                }
                for c in candidates {
                    let selected = state.selected.as_ref() == Some(&c.path);
                    let label =
                        format!("{}  {}", timestamp_label(c.timestamp_millis), c.scenario_id);
                    if ui.selectable_label(selected, label).clicked() && !selected {
                        pick = Some(Some(c));
                    }
                }
            });
        match pick {
            Some(Some(c)) => state.select(c, load),
            Some(None) => state.clear(),
            None => {}
        }
    });
    if state.selected.is_some() && state.path.is_none() {
        ui.colored_label(BAD, "The earlier replay could not be read.");
    }
}

/// The comparison table: metric, this run, the earlier run and the change, with
/// improvements in green and regressions in red.
pub fn differences_table(ui: &mut egui::Ui, current: &PathSummary, earlier: &PathSummary) {
    egui::Grid::new("replay_compare_table")
        .num_columns(4)
        .striped(true)
        .show(ui, |ui| {
            ui.strong("Whole run");
            ui.strong("This run");
            ui.strong("Earlier");
            ui.strong("Change");
            ui.end_row();
            for r in compare_rows(current, earlier) {
                ui.label(r.label);
                ui.label(&r.current);
                ui.label(&r.earlier);
                match r.better {
                    Some(true) => ui.colored_label(GOOD, format!("{} ▲ better", r.change)),
                    Some(false) => ui.colored_label(BAD, format!("{} ▼ worse", r.change)),
                    None => ui.label(&r.change),
                };
                ui.end_row();
            }
        });
    ui.small(
        "Overshoot and undershoot: share of engagements with at least one. \
         Reaction: median time to start the first flick on each target.",
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::analytics::PathSummary;

    fn summary() -> PathSummary {
        PathSummary {
            engagements: 10,
            kills: 8,
            shots: 20,
            hits: 15,
            overshoots: 4,
            undershoots: 2,
            corrections: 12,
            median_reaction_secs: Some(0.300),
        }
    }

    #[test]
    fn rows_show_both_runs_the_change_and_whether_it_is_better() {
        let now = summary();
        let before = PathSummary {
            engagements: 8,
            hits: 10,
            overshoots: 4,
            undershoots: 1,
            corrections: 16,
            median_reaction_secs: Some(0.340),
            ..summary()
        };
        let rows = compare_rows(&now, &before);
        let table: Vec<(&str, &str, &str, &str, Option<bool>)> = rows
            .iter()
            .map(|r| {
                (
                    r.label,
                    r.current.as_str(),
                    r.earlier.as_str(),
                    r.change.as_str(),
                    r.better,
                )
            })
            .collect();
        assert_eq!(
            table,
            vec![
                ("Engagements", "10", "8", "+2", None),
                ("Hit rate", "75%", "50%", "+25 pts", Some(true)),
                ("Overshoot", "40%", "50%", "-10 pts", Some(true)),
                ("Undershoot", "20%", "12%", "+8 pts", Some(false)),
                ("Corrections / engagement", "1.2", "2.0", "-0.8", Some(true)),
                ("Median reaction", "300 ms", "340 ms", "-40 ms", Some(true)),
            ]
        );
    }

    #[test]
    fn missing_values_show_a_dash_and_no_verdict() {
        let empty = PathSummary {
            engagements: 0,
            kills: 0,
            shots: 0,
            hits: 0,
            overshoots: 0,
            undershoots: 0,
            corrections: 0,
            median_reaction_secs: None,
        };
        let rows = compare_rows(&summary(), &empty);
        assert_eq!(rows[0].change, "+10");
        for r in &rows[1..] {
            assert_eq!(
                (r.earlier.as_str(), r.change.as_str(), r.better),
                ("–", "–", None)
            );
        }
        let same = compare_rows(&summary(), &summary());
        assert!(
            same.iter().all(|r| r.better.is_none()),
            "no change, no verdict"
        );
        assert_eq!(same[1].change, "+0 pts");
    }
}
