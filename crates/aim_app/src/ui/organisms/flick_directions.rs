//! Per-direction flick grid on the results card: hit rate and reaction time for each screen
//! quadrant, near and far, plus the weakest direction and the weak axis with their numbers.
//!
//! [`DirectionGrid`] is plain text built from the core's
//! [`DirectionSummary`](aim_core::analytics::DirectionSummary); the view only lays it out.

use crate::i18n::{tr, tr_fmt, tr_named};
use crate::ui::atoms::WARN;
use aim_core::analytics::RunAnalysis;
use aim_core::analytics::direction::{
    DirectionBucket, DistanceClass, MIN_QUADRANT_FLICKS, Quadrant, RangeSource, WEAK_AXIS_MARGIN,
};
use bevy_egui::egui;

/// One quadrant row: its name, then the near and far cells.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectionRow {
    pub quadrant: &'static str,
    pub near: String,
    pub far: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DirectionGrid {
    /// Up-right, up-left, down-left, down-right.
    pub rows: Vec<DirectionRow>,
    /// How near and far were split, with the threshold.
    pub split_note: String,
    /// The weakest direction with its hit rates, when enough flicks were measured.
    pub weakest_note: Option<String>,
    /// Per-axis over/undershoot rates and the weak axis, when both axes were measured.
    pub axis_note: Option<String>,
    /// Near and far hit rates per quadrant, in the order of `rows` (for the radar chart);
    /// `None` for a quadrant without flicks.
    pub near_rates: [Option<f32>; 4],
    pub far_rates: [Option<f32>; 4],
}

fn pct(ratio: f32) -> String {
    format!("{:.0}%", ratio * 100.0)
}

fn cell(b: &DirectionBucket) -> String {
    match b.hit_rate() {
        None => "—".to_owned(),
        Some(rate) => {
            let reaction = b
                .median_reaction_secs
                .map_or(String::new(), |s| format!(" · {:.0} ms", s * 1000.0));
            format!(
                "{}{reaction}",
                tr_fmt("{} hit of {}", &[&pct(rate), &b.flicks])
            )
        }
    }
}

impl DirectionGrid {
    /// The grid for a flick run, or `None` for a tracking run.
    pub fn new(analysis: &RunAnalysis) -> Option<Self> {
        let split = analysis.distance_split?;
        let summary = analysis.direction_summary()?;
        let rows = Quadrant::ALL
            .iter()
            .map(|&q| DirectionRow {
                quadrant: q.label(),
                near: cell(summary.bucket(q, DistanceClass::Near)),
                far: cell(summary.bucket(q, DistanceClass::Far)),
            })
            .collect();
        let source = match split.source {
            RangeSource::SpawnArea => {
                "Far = above {far}° ((min + max) / 1.9 of the scenario's spawn range, {min}°–{max}°)."
            }
            RangeSource::Observed => {
                "Far = above {far}° ((min + max) / 1.9 of this run's flick distances, {min}°–{max}°)."
            }
        };
        let split_note = tr_named(
            source,
            &[
                ("far", &format!("{:.1}", split.far_above_deg())),
                ("min", &format!("{:.1}", split.min_deg)),
                ("max", &format!("{:.1}", split.max_deg)),
            ],
        );
        let weakest_note = summary.weakest.map(|w| {
            tr_fmt(
                "Weakest direction: {} hits {} vs {} elsewhere ({} points lower).",
                &[
                    &w.quadrant.label(),
                    &pct(w.hit_rate),
                    &pct(w.others_hit_rate),
                    &(w.gap() * 100.0).round(),
                ],
            )
        });
        let axis_note = summary.horizontal.zip(summary.vertical).map(|(h, v)| {
            let verdict = match summary.weak_axis {
                Some(axis) => tr_fmt(
                    "{} is your weak axis (more than {} points worse).",
                    &[&axis.label(), &(WEAK_AXIS_MARGIN * 100.0).round()],
                ),
                None => tr("No clear weak axis.").to_owned(),
            };
            let measured = tr_fmt(
                "First movement over/undershoots by more than one radius: horizontal {} of {}, vertical {} of {}.",
                &[
                    &pct(h.miss_rate()),
                    &h.samples,
                    &pct(v.miss_rate()),
                    &v.samples,
                ],
            );
            format!("{measured} {verdict}")
        });
        let rates = |d| std::array::from_fn(|i| summary.bucket(Quadrant::ALL[i], d).hit_rate());
        Some(Self {
            rows,
            split_note,
            weakest_note,
            axis_note,
            near_rates: rates(DistanceClass::Near),
            far_rates: rates(DistanceClass::Far),
        })
    }
}

/// Draw the grid: one row per quadrant, near and far columns.
pub fn flick_directions(ui: &mut egui::Ui, grid: &DirectionGrid) {
    ui.add_space(4.0);
    ui.strong(tr("Flicks by direction"));
    egui::Grid::new("flick_directions")
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            ui.label("");
            ui.label(tr("Near"));
            ui.label(tr("Far"));
            ui.end_row();
            for row in &grid.rows {
                ui.label(row.quadrant);
                ui.label(row.near.as_str());
                ui.label(row.far.as_str());
                ui.end_row();
            }
        });
    ui.small(grid.split_note.as_str());
    match &grid.weakest_note {
        Some(note) => {
            ui.colored_label(WARN, note);
        }
        None => {
            ui.small(tr_fmt(
                "A weakest direction needs {} flicks in a quadrant.",
                &[&MIN_QUADRANT_FLICKS],
            ));
        }
    }
    if let Some(note) = &grid.axis_note {
        ui.small(note.as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::organisms::analysis_panel::RunReport;
    use aim_core::analytics::analyze;
    use aim_core::scenario::{Recording, builtin};
    use aim_core::sim::{AimerProfile, SimAimer};

    fn analysis(id: &str) -> RunAnalysis {
        let def = builtin::all().into_iter().find(|d| d.id == id).unwrap();
        let mut rec = Recording::new(def, 3);
        let mut live = rec.start();
        let mut aimer = SimAimer::new(AimerProfile::PRESETS[2], 3);
        while !live.is_finished() {
            let i = aimer.input(&live, 1.0 / 240.0);
            rec.inputs.push(i);
            live.update(i);
        }
        analyze(&rec)
    }

    #[test]
    fn a_flick_run_gets_a_four_by_two_grid_with_its_far_threshold() {
        let a = analysis("flick-wide");
        let grid = DirectionGrid::new(&a).unwrap();
        let names: Vec<&str> = grid.rows.iter().map(|r| r.quadrant).collect();
        assert_eq!(names, ["up-right", "up-left", "down-left", "down-right"]);
        assert_eq!(
            grid.split_note,
            "Far = above 35.3° ((min + max) / 1.9 of the scenario's spawn range, 20.0°–47.0°)."
        );
        assert!(
            grid.rows
                .iter()
                .flat_map(|r| [&r.near, &r.far])
                .all(|c| c == "—" || c.contains(" hit of ")),
            "{grid:?}"
        );
        // The radar's rates agree with the text cells: a rate exactly where a cell has one.
        for (row, (near, far)) in grid
            .rows
            .iter()
            .zip(grid.near_rates.iter().zip(&grid.far_rates))
        {
            assert_eq!(near.is_some(), row.near != "—", "{row:?}");
            assert_eq!(far.is_some(), row.far != "—", "{row:?}");
            assert!(near.iter().chain(far).all(|r| (0.0..=1.0).contains(r)));
        }
        let report = RunReport::new(&a, |_| None);
        assert_eq!(report.directions, Some(grid));
    }

    #[test]
    fn tracking_runs_have_no_direction_grid() {
        let a = analysis("track-precise");
        assert_eq!(DirectionGrid::new(&a), None);
        assert_eq!(RunReport::new(&a, |_| None).directions, None);
    }
}
