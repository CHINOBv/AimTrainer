//! Miss compass on the results card: a 3 × 3 grid with the misses that went in each of the
//! eight directions around the aim point, plus the above/below split.
//!
//! [`MissCompassGrid`] is plain text built from the core's
//! [`MissCompass`](aim_core::analytics::miss_compass::MissCompass); the view only lays it out.

use crate::i18n::{tr, tr_fmt};
use crate::ui::atoms::WARN;
use aim_core::analytics::miss_compass::{Compass, MissCompass};
use bevy_egui::egui;

/// Grid layout, row by row; `None` is the aim point in the middle.
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

const TITLE: &str = "Miss compass";
const CENTER: &str = "⊕";
const HINT: &str = "Where your misses went, from the head (humanoids) or the center (spheres).";

#[derive(Debug, Clone, PartialEq)]
pub struct MissCompassGrid {
    /// Three rows of three cells: the count per direction, the center marker in the middle.
    pub rows: [[String; 3]; 3],
    /// Cells holding the most misses, as `(row, column)`.
    pub busiest: Vec<(usize, usize)>,
    /// "Above 6 · below 3 of 10 misses".
    pub split_note: String,
    /// Misses per direction, clockwise from up (for the radial chart).
    pub counts: [u32; 8],
    /// Indexes into `counts` holding the most misses.
    pub busiest_wedges: Vec<usize>,
}

impl MissCompassGrid {
    pub fn new(compass: &MissCompass) -> Self {
        let most = Compass::ALL
            .iter()
            .map(|&d| compass.count(d))
            .max()
            .unwrap_or(0);
        let mut busiest = Vec::new();
        let rows = std::array::from_fn(|r| {
            std::array::from_fn(|c| match LAYOUT[r][c] {
                None => CENTER.to_owned(),
                Some(d) => {
                    let n = compass.count(d);
                    if n > 0 && n == most {
                        busiest.push((r, c));
                    }
                    n.to_string()
                }
            })
        });
        let counts: [u32; 8] = std::array::from_fn(|i| compass.count(Compass::ALL[i]));
        let busiest_wedges = (0..counts.len())
            .filter(|&i| counts[i] > 0 && counts[i] == most)
            .collect();
        Self {
            rows,
            busiest,
            counts,
            busiest_wedges,
            split_note: tr_fmt(
                "Above {} · below {} of {} misses",
                &[&compass.above(), &compass.below(), &compass.misses()],
            ),
        }
    }
}

pub fn miss_compass(ui: &mut egui::Ui, grid: &MissCompassGrid) {
    egui::CollapsingHeader::new(tr(TITLE))
        .default_open(true)
        .show(ui, |ui| {
            egui::Grid::new("miss_compass")
                .num_columns(3)
                .min_col_width(28.0)
                .show(ui, |ui| {
                    for (r, row) in grid.rows.iter().enumerate() {
                        for (c, cell) in row.iter().enumerate() {
                            if grid.busiest.contains(&(r, c)) {
                                ui.colored_label(WARN, cell.as_str());
                            } else {
                                ui.label(cell.as_str());
                            }
                        }
                        ui.end_row();
                    }
                });
            ui.label(grid.split_note.as_str());
            ui.small(tr(HINT));
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_places_each_direction_around_the_center_and_marks_the_busiest() {
        let c = MissCompass::from_offsets([(0.0, -1.0), (0.0, -1.0), (-1.0, 1.0), (1.0, 0.0)]);
        let g = MissCompassGrid::new(&c);
        assert_eq!(g.rows[0], ["1", "0", "0"]);
        assert_eq!(g.rows[1], ["0", CENTER, "1"]);
        assert_eq!(g.rows[2], ["0", "2", "0"]);
        assert_eq!(g.busiest, [(2, 1)]);
        assert_eq!(g.split_note, "Above 1 · below 2 of 4 misses");
    }

    #[test]
    fn the_radial_chart_counts_run_clockwise_from_up() {
        let c = MissCompass::from_offsets([(0.0, -1.0), (0.0, -1.0), (-1.0, 1.0), (1.0, 0.0)]);
        let g = MissCompassGrid::new(&c);
        // Up, up-right, right, down-right, down, down-left, left, up-left.
        assert_eq!(g.counts, [0, 0, 1, 0, 2, 0, 0, 1]);
        assert_eq!(g.busiest_wedges, [4]);
    }
}
