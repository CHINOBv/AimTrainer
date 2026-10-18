//! Peripheral perception on the results card: per-area detection of a flash drill, as a
//! 3 × 3 grid around the crosshair, and the picks of a radar-recall drill.
//!
//! [`DetectionReport`] and [`RadarRecallReport`] are plain text built from the core's
//! [`DetectionAnalysis`] and [`RadarRecall`]; the views only lay them out.

use crate::ui::atoms::WARN;
use crate::ui::molecules::stat_row;
use crate::ui::organisms::analysis_panel::{ms, pct};
use aim_core::analytics::miss_compass::Compass;
use aim_core::analytics::perception::{DetectionAnalysis, RadarRecall};
use bevy_egui::egui;

/// Grid layout, row by row; `None` is the crosshair in the middle.
const LAYOUT: [[Option<Compass>; 3]; 3] = [
    [
        Some(Compass::UpLeft),
        Some(Compass::Up),
        Some(Compass::UpRight),
    ],
    [Some(Compass::Left), None, Some(Compass::Right)],
    [
        Some(Compass::DownLeft),
        Some(Compass::Down),
        Some(Compass::DownRight),
    ],
];

const DETECTION_TITLE: &str = "Peripheral detection";
const RADAR_TITLE: &str = "Radar recall";
const CENTER: &str = "+";
const NO_FLASHES: &str = "–";
const GRID_NOTE: &str = "Per area: flashes seen · reaction time (flashes shown).";

#[derive(Debug, Clone, PartialEq)]
pub struct DetectionReport {
    pub metrics: Vec<(&'static str, String)>,
    /// Three rows of three cells: "hit rate · reaction (flashes)" per area, the crosshair
    /// in the middle.
    pub rows: [[String; 3]; 3],
    /// The cell with the lowest hit rate, as `(row, column)`, if any flash was shown.
    pub weakest: Option<(usize, usize)>,
}

impl DetectionReport {
    pub fn new(d: &DetectionAnalysis) -> Self {
        let mut metrics = vec![
            (
                "Flashes seen",
                format!("{} of {}", d.detected(), d.flashes()),
            ),
            ("False alarms (click, no flash)", d.false_alarms.to_string()),
            ("Blank trials", d.blank_trials.to_string()),
        ];
        if let Some(rt) = d.median_reaction_secs {
            metrics.push(("Reaction (median)", ms(rt)));
        }
        let mut weakest: Option<((usize, usize), f32)> = None;
        let rows = std::array::from_fn(|r| {
            std::array::from_fn(|c| match LAYOUT[r][c] {
                None => CENTER.to_owned(),
                Some(area) => {
                    let Some((a, rate)) = d
                        .areas
                        .iter()
                        .find(|a| a.area == area)
                        .and_then(|a| Some((a, a.hit_rate()?)))
                    else {
                        return NO_FLASHES.to_owned();
                    };
                    if weakest.is_none_or(|(_, w)| rate < w) {
                        weakest = Some(((r, c), rate));
                    }
                    let rt = a.median_reaction_secs.map_or(NO_FLASHES.to_owned(), ms);
                    format!("{} · {rt} ({})", pct(rate), a.flashes)
                }
            })
        });
        Self {
            metrics,
            rows,
            weakest: weakest.map(|(cell, _)| cell),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RadarRecallReport {
    pub metrics: Vec<(&'static str, String)>,
}

impl RadarRecallReport {
    pub fn new(r: &RadarRecall) -> Self {
        let mut metrics = vec![
            ("Correct picks", r.correct_picks.to_string()),
            ("Incorrect picks", r.incorrect_picks.to_string()),
            ("Radar enemies missed", r.missed.to_string()),
            ("Others left alone", r.left_alone.to_string()),
        ];
        if let Some(a) = r.pick_accuracy() {
            metrics.push(("Pick accuracy", pct(a)));
        }
        Self { metrics }
    }
}

pub fn detection_panel(ui: &mut egui::Ui, report: &DetectionReport) {
    egui::CollapsingHeader::new(DETECTION_TITLE)
        .default_open(true)
        .show(ui, |ui| {
            metrics_grid(ui, "detection_metrics", &report.metrics);
            egui::Grid::new("detection_grid")
                .num_columns(3)
                .min_col_width(28.0)
                .show(ui, |ui| {
                    for (r, row) in report.rows.iter().enumerate() {
                        for (c, cell) in row.iter().enumerate() {
                            if report.weakest == Some((r, c)) {
                                ui.colored_label(WARN, cell.as_str());
                            } else {
                                ui.label(cell.as_str());
                            }
                        }
                        ui.end_row();
                    }
                });
            ui.small(GRID_NOTE);
        });
}

pub fn radar_recall_panel(ui: &mut egui::Ui, report: &RadarRecallReport) {
    egui::CollapsingHeader::new(RADAR_TITLE)
        .default_open(true)
        .show(ui, |ui| metrics_grid(ui, "radar_metrics", &report.metrics));
}

fn metrics_grid(ui: &mut egui::Ui, id: &str, metrics: &[(&'static str, String)]) {
    egui::Grid::new(id).num_columns(2).show(ui, |ui| {
        for (label, value) in metrics {
            stat_row(ui, label, value.as_str());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::analytics::perception::AreaDetection;

    fn area(area: Compass, flashes: u32, detected: u32, rt: Option<f32>) -> AreaDetection {
        AreaDetection {
            area,
            flashes,
            detected,
            median_reaction_secs: rt,
        }
    }

    #[test]
    fn the_detection_grid_places_each_area_around_the_crosshair_and_marks_the_weakest() {
        let mut areas: Vec<_> = Compass::ALL.iter().map(|&c| area(c, 0, 0, None)).collect();
        areas[0] = area(Compass::Up, 4, 3, Some(0.35));
        areas[6] = area(Compass::Left, 2, 0, None);
        let d = DetectionAnalysis {
            areas,
            false_alarms: 2,
            blank_trials: 3,
            median_reaction_secs: Some(0.35),
        };
        let r = DetectionReport::new(&d);
        assert_eq!(r.rows[0], [NO_FLASHES, "75% · 350 ms (4)", NO_FLASHES]);
        assert_eq!(r.rows[1], ["0% · – (2)", CENTER, NO_FLASHES]);
        assert_eq!(r.weakest, Some((1, 0)));
        assert_eq!(
            r.metrics,
            [
                ("Flashes seen", "3 of 6".to_owned()),
                ("False alarms (click, no flash)", "2".to_owned()),
                ("Blank trials", "3".to_owned()),
                ("Reaction (median)", "350 ms".to_owned()),
            ]
        );
    }

    #[test]
    fn the_radar_report_lists_correct_and_incorrect_picks() {
        let r = RadarRecallReport::new(&RadarRecall {
            correct_picks: 9,
            incorrect_picks: 3,
            missed: 1,
            left_alone: 7,
        });
        assert_eq!(r.metrics[0], ("Correct picks", "9".to_owned()));
        assert_eq!(r.metrics[1], ("Incorrect picks", "3".to_owned()));
        assert_eq!(r.metrics[4], ("Pick accuracy", "75%".to_owned()));
    }
}
