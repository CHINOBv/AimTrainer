//! Small composed widgets.

use bevy_egui::egui;

/// Horizontal tab strip; returns nothing, edits `current` in place.
pub fn tab_bar<T: PartialEq + Copy>(ui: &mut egui::Ui, current: &mut T, tabs: &[(T, &str)]) {
    for (tab, label) in tabs {
        ui.selectable_value(current, *tab, *label);
    }
}

/// One row of a two-column stats grid.
pub fn stat_row(ui: &mut egui::Ui, label: &str, value: impl Into<egui::WidgetText>) {
    ui.label(label);
    ui.label(value);
    ui.end_row();
}

/// Scenario entry with its personal best. Returns true when clicked.
pub fn scenario_item(ui: &mut egui::Ui, name: &str, best: Option<f32>, selected: bool) -> bool {
    let best = best.map_or(String::new(), |b| format!("  (PB {b:.0})"));
    ui.selectable_label(selected, format!("{name}{best}"))
        .clicked()
}

/// "123 · Rank" or "—" for missing energy. `unit` is appended to the number when non-empty
/// ("123 energy · Rank").
pub fn energy_text(
    energy: Option<f32>,
    unit: &str,
    rank: impl Fn(f32) -> Option<String>,
) -> String {
    energy.map_or("—".to_owned(), |e| {
        format!(
            "{e:.0}{}{unit} · {}",
            if unit.is_empty() { "" } else { " " },
            rank(e).unwrap_or_else(|| "Unranked".to_owned())
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn energy_text_formats_rank_or_placeholder() {
        let rank = |e: f32| (e >= 100.0).then(|| "Recruit".to_owned());
        assert_eq!(energy_text(None, "", rank), "—");
        assert_eq!(energy_text(Some(50.0), "", rank), "50 · Unranked");
        assert_eq!(energy_text(Some(150.4), "", rank), "150 · Recruit");
        assert_eq!(
            energy_text(Some(150.4), "energy", rank),
            "150 energy · Recruit"
        );
    }
}
