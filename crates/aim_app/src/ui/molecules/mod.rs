//! Small composed widgets.

use crate::persistence::settings::Toggle;
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

/// True when `text` contains every word of `query`, ignoring case. An empty query
/// matches everything.
pub fn matches_query(query: &str, text: &str) -> bool {
    let text = text.to_lowercase();
    query
        .split_whitespace()
        .all(|word| text.contains(&word.to_lowercase()))
}

/// What [`SettingsRows`] does with one row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowVisibility {
    Hidden,
    /// Draw the row, after this section heading when it is the section's first match.
    Shown(Option<&'static str>),
}

/// Settings rows filtered by a search. While searching, a section heading appears just
/// before the section's first matching row, so sections without matches disappear.
pub struct SettingsRows<'q> {
    query: &'q str,
    heading: Option<&'static str>,
}

impl<'q> SettingsRows<'q> {
    pub fn new(query: &'q str) -> Self {
        Self {
            query: query.trim(),
            heading: None,
        }
    }

    pub fn searching(&self) -> bool {
        !self.query.is_empty()
    }

    /// Whether a row with `text` (its label plus any keywords) is drawn.
    pub fn admit(&mut self, text: &str) -> RowVisibility {
        if matches_query(self.query, text) {
            RowVisibility::Shown(self.heading.take())
        } else {
            RowVisibility::Hidden
        }
    }

    /// Start a section: its heading now, or before its first match while searching.
    pub fn section(&mut self, ui: &mut egui::Ui, heading: &'static str) {
        if self.searching() {
            self.heading = Some(heading);
        } else {
            ui.add_space(6.0);
            ui.heading(heading);
        }
    }

    fn show(&mut self, ui: &mut egui::Ui, text: &str) -> bool {
        match self.admit(text) {
            RowVisibility::Hidden => false,
            RowVisibility::Shown(heading) => {
                if let Some(h) = heading {
                    ui.add_space(6.0);
                    ui.heading(h);
                }
                true
            }
        }
    }

    /// A labelled row; `add` draws the controls after the label.
    pub fn row(&mut self, ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
        if self.show(ui, label) {
            ui.horizontal(|ui| {
                ui.label(label);
                add(ui);
            });
        }
    }

    /// A checkbox row.
    pub fn check(&mut self, ui: &mut egui::Ui, label: &str, value: &mut bool, hint: &str) {
        if self.show(ui, label) {
            let response = ui.checkbox(value, label);
            if !hint.is_empty() {
                response.on_hover_text(hint);
            }
        }
    }

    /// An effect toggle: a checkbox plus its intensity slider.
    pub fn toggle(&mut self, ui: &mut egui::Ui, label: &str, toggle: &mut Toggle, hint: &str) {
        if self.show(ui, label) {
            ui.horizontal(|ui| {
                let response = ui.checkbox(&mut toggle.enabled, label);
                if !hint.is_empty() {
                    response.on_hover_text(hint);
                }
                ui.add_enabled(
                    toggle.enabled,
                    egui::Slider::new(&mut toggle.intensity_percent, INTENSITY_PERCENT)
                        .suffix(" %")
                        .text("intensity"),
                );
            });
        }
    }

    /// A free-form block found by `keywords` (for editors with many controls).
    pub fn block(&mut self, ui: &mut egui::Ui, keywords: &str, add: impl FnOnce(&mut egui::Ui)) {
        if self.show(ui, keywords) {
            add(ui);
        }
    }
}

/// Range of an effect's intensity slider, percent of its designed strength.
pub const INTENSITY_PERCENT: std::ops::RangeInclusive<u8> = 0..=200;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_matches_every_word_ignoring_case() {
        assert!(matches_query("", "Mouse DPI"));
        assert!(matches_query("   ", "Mouse DPI"));
        assert!(matches_query("dpi", "Mouse DPI"));
        assert!(matches_query("DPI mouse", "Mouse DPI"));
        assert!(!matches_query("dpi ads", "Mouse DPI"));
        assert!(!matches_query("volume", "Mouse DPI"));
    }

    #[test]
    fn section_headings_appear_only_before_the_first_match_while_searching() {
        let mut rows = SettingsRows::new("vol");
        assert!(rows.searching());
        rows.heading = Some("Sensitivity");
        assert_eq!(rows.admit("Mouse DPI"), RowVisibility::Hidden);
        rows.heading = Some("Audio");
        assert_eq!(rows.admit("Volume"), RowVisibility::Shown(Some("Audio")));
        assert_eq!(rows.admit("World volume"), RowVisibility::Shown(None));
        assert_eq!(rows.admit("Footsteps"), RowVisibility::Hidden);

        let mut all = SettingsRows::new("");
        assert!(!all.searching());
        assert_eq!(all.admit("Footsteps"), RowVisibility::Shown(None));
    }

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
