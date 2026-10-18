//! Sound localisation on the results card: a 3 × 3 grid around the player with the
//! angular error and turn time for sounds from each of the eight directions.
//!
//! [`AudioCueReport`] is plain text built from the core's
//! [`AudioCueAnalysis`](aim_core::analytics::audio_cue::AudioCueAnalysis); the view only lays
//! it out.

use crate::ui::atoms::WARN;
use crate::ui::molecules::stat_row;
use crate::ui::organisms::analysis_panel::ms;
use aim_core::analytics::audio_cue::{
    AudioCueAnalysis, Bearing, ERROR_AT_SECS, TURN_ONSET_DEG_PER_SEC,
};
use bevy_egui::egui;

/// Grid layout, row by row, front at the top; `None` is the player in the middle.
const LAYOUT: [[Option<Bearing>; 3]; 3] = [
    [
        Some(Bearing::FrontLeft),
        Some(Bearing::Front),
        Some(Bearing::FrontRight),
    ],
    [Some(Bearing::Left), None, Some(Bearing::Right)],
    [
        Some(Bearing::BackLeft),
        Some(Bearing::Back),
        Some(Bearing::BackRight),
    ],
];

const TITLE: &str = "Sound localisation";
const CENTER: &str = "you";
const NO_CUES: &str = "–";

#[derive(Debug, Clone, PartialEq)]
pub struct AudioCueReport {
    pub metrics: Vec<(&'static str, String)>,
    /// Three rows of three cells: "error · turn time (cues)" per direction, the player in
    /// the middle.
    pub rows: [[String; 3]; 3],
    /// The cell with the largest median error, as `(row, column)`, if any cue was measured.
    pub worst: Option<(usize, usize)>,
    /// How the numbers are measured.
    pub note: String,
}

fn degrees(v: f32) -> String {
    format!("{v:.0}°")
}

impl AudioCueReport {
    pub fn new(a: &AudioCueAnalysis) -> Self {
        let mut metrics = vec![("Sound cues", a.responses.len().to_string())];
        if let Some(e) = a.median_error_deg {
            metrics.push(("Angular error (median)", degrees(e)));
        }
        if let Some(rt) = a.median_reaction_secs {
            metrics.push(("Turn reaction (median)", ms(rt)));
        }
        let mut worst: Option<((usize, usize), f32)> = None;
        let rows = std::array::from_fn(|r| {
            std::array::from_fn(|c| match LAYOUT[r][c] {
                None => CENTER.to_owned(),
                Some(b) => {
                    let Some((s, error)) = a
                        .by_bearing
                        .iter()
                        .find(|s| s.bearing == b)
                        .and_then(|s| Some((s, s.median_error_deg?)))
                    else {
                        return NO_CUES.to_owned();
                    };
                    if worst.is_none_or(|(_, e)| error > e) {
                        worst = Some(((r, c), error));
                    }
                    let turn = s.median_reaction_secs.map_or(NO_CUES.to_owned(), ms);
                    format!("{} · {turn} ({})", degrees(error), s.cues)
                }
            })
        });
        Self {
            metrics,
            rows,
            worst: worst.map(|(cell, _)| cell),
            note: format!(
                "Error {:.1} s after each sound (or at your first shot); turn time until the \
                 crosshair moves towards it at {TURN_ONSET_DEG_PER_SEC:.0}°/s. Estimates.",
                ERROR_AT_SECS
            ),
        }
    }
}

pub fn audio_cue_panel(ui: &mut egui::Ui, report: &AudioCueReport) {
    egui::CollapsingHeader::new(TITLE)
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("audio_cue_metrics")
                .num_columns(2)
                .show(ui, |ui| {
                    for (label, value) in &report.metrics {
                        stat_row(ui, label, value.as_str());
                    }
                });
            egui::Grid::new("audio_cue_grid")
                .num_columns(3)
                .min_col_width(28.0)
                .show(ui, |ui| {
                    for (r, row) in report.rows.iter().enumerate() {
                        for (c, cell) in row.iter().enumerate() {
                            if report.worst == Some((r, c)) {
                                ui.colored_label(WARN, cell.as_str());
                            } else {
                                ui.label(cell.as_str());
                            }
                        }
                        ui.end_row();
                    }
                });
            ui.small(report.note.as_str());
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::analytics::audio_cue::{BearingStats, CueResponse};

    fn analysis() -> AudioCueAnalysis {
        let mut by_bearing = Bearing::ALL.map(|bearing| BearingStats {
            bearing,
            cues: 0,
            median_error_deg: None,
            median_reaction_secs: None,
        });
        by_bearing[4] = BearingStats {
            bearing: Bearing::Back,
            cues: 3,
            median_error_deg: Some(40.0),
            median_reaction_secs: Some(0.42),
        };
        by_bearing[2] = BearingStats {
            bearing: Bearing::Right,
            cues: 2,
            median_error_deg: Some(8.0),
            median_reaction_secs: None,
        };
        let response = CueResponse {
            id: 1,
            offset_deg: 180.0,
            bearing: Bearing::Back,
            error_deg: 40.0,
            reaction_secs: Some(0.42),
        };
        AudioCueAnalysis {
            responses: vec![response; 5],
            by_bearing,
            median_error_deg: Some(30.0),
            median_reaction_secs: Some(0.4),
        }
    }

    #[test]
    fn the_grid_puts_each_direction_around_the_player_and_marks_the_worst() {
        let r = AudioCueReport::new(&analysis());
        assert_eq!(r.rows[0], [NO_CUES, NO_CUES, NO_CUES]);
        assert_eq!(r.rows[1], [NO_CUES, CENTER, "8° · – (2)"]);
        assert_eq!(r.rows[2], [NO_CUES, "40° · 420 ms (3)", NO_CUES]);
        assert_eq!(r.worst, Some((2, 1)));
        assert_eq!(
            r.metrics,
            [
                ("Sound cues", "5".to_owned()),
                ("Angular error (median)", "30°".to_owned()),
                ("Turn reaction (median)", "400 ms".to_owned()),
            ]
        );
        assert!(r.note.contains("Estimates"), "{}", r.note);
    }
}
