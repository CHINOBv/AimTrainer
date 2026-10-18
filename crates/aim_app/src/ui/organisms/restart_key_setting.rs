//! The restart key row: current key, "Record" (press a key), "Reset", and inline
//! validation. Unlike the global hotkeys it is a bare in-window key, so keys gameplay
//! already uses (WASD, Shift, Esc, Space) are refused.

use crate::platform::game_key;
use crate::ui::atoms::{ERROR, WARN};
use bevy_egui::egui::{self, Key};

/// Recording state of the row, kept in egui memory.
#[derive(Debug, Clone, Default)]
struct RowState {
    recording: bool,
    problem: Option<&'static str>,
}

/// The restart key row, editing the stored key name (`settings.restart_key`).
pub fn restart_key_setting(ui: &mut egui::Ui, key_name: &mut String) {
    let id = ui.id().with("restart-key");
    let mut state = ui.data_mut(|d| d.get_temp::<RowState>(id).unwrap_or_default());
    if state.recording {
        match take_key_press(ui) {
            Some(Captured::Cancel) => state.recording = false,
            Some(Captured::Key(name)) => {
                *key_name = name;
                state = RowState::default();
            }
            Some(Captured::Rejected(problem)) => state.problem = Some(problem),
            None => {}
        }
    }
    ui.horizontal(|ui| {
        ui.label("Restart run");
        if state.recording {
            ui.label(egui::RichText::new("Press a key… (Esc to cancel)").color(WARN));
            if ui.button("Cancel").clicked() {
                state = RowState::default();
            }
        } else {
            ui.label(egui::RichText::new(game_key::label(key_name)).strong());
            if ui.button("Record").clicked() {
                state.recording = true;
                state.problem = None;
            }
        }
        if ui
            .add_enabled(
                key_name != game_key::DEFAULT_RESTART,
                egui::Button::new("Reset"),
            )
            .on_hover_text("Back to R")
            .clicked()
        {
            *key_name = game_key::DEFAULT_RESTART.to_owned();
            state = RowState::default();
        }
    });
    let problem = state.problem.or_else(|| game_key::problem(key_name));
    if let Some(problem) = problem {
        ui.small(egui::RichText::new(problem).color(ERROR));
    }
    ui.small(
        "Restarts the scenario or arena instantly, keeping its difficulty; nothing is recorded.",
    );
    ui.data_mut(|d| d.insert_temp(id, state));
}

/// The first key press of this frame. Every keyboard event is consumed so the key being
/// recorded does not also drive other widgets.
fn take_key_press(ui: &egui::Ui) -> Option<Captured> {
    ui.input_mut(|input| {
        let captured = input.events.iter().find_map(|event| match event {
            egui::Event::Key {
                key,
                physical_key,
                pressed: true,
                ..
            } => Some(capture(*key, *physical_key)),
            _ => None,
        });
        input
            .events
            .retain(|e| !matches!(e, egui::Event::Key { .. } | egui::Event::Text(_)));
        captured
    })
}

/// What one key press means while the restart key is being recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Captured {
    /// Esc: stop recording, keep the old key.
    Cancel,
    /// The new key name ("KeyR").
    Key(String),
    /// A key gameplay uses or that cannot be bound.
    Rejected(&'static str),
}

/// Interpret a key press (`physical` preferred: gameplay reads physical keys).
pub fn capture(key: Key, physical: Option<Key>) -> Captured {
    if key == Key::Escape {
        return Captured::Cancel;
    }
    let key = physical.unwrap_or(key);
    let name = key.name();
    let stored = match key {
        _ if name.len() == 1 && name.chars().all(|c| c.is_ascii_uppercase()) => {
            format!("Key{name}")
        }
        _ if name.len() == 1 && name.chars().all(|c| c.is_ascii_digit()) => {
            format!("Digit{name}")
        }
        Key::Backtick => "Backquote".to_owned(),
        _ => format!("{key:?}"),
    };
    match game_key::problem(&stored) {
        None => Captured::Key(stored),
        Some(problem) => Captured::Rejected(problem),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_letters_digits_and_function_keys_by_physical_key() {
        assert_eq!(capture(Key::T, None), Captured::Key("KeyT".into()));
        assert_eq!(capture(Key::Num4, None), Captured::Key("Digit4".into()));
        assert_eq!(capture(Key::F5, None), Captured::Key("F5".into()));
        assert_eq!(
            capture(Key::Backspace, None),
            Captured::Key("Backspace".into())
        );
        // AZERTY: logical A on the physical Q key.
        assert_eq!(capture(Key::A, Some(Key::Q)), Captured::Key("KeyQ".into()));
    }

    #[test]
    fn escape_cancels_and_gameplay_keys_are_refused() {
        assert_eq!(capture(Key::Escape, None), Captured::Cancel);
        for key in [Key::W, Key::A, Key::S, Key::D, Key::Space] {
            assert_eq!(
                capture(key, None),
                Captured::Rejected("Gameplay already uses this key."),
                "{key:?}"
            );
        }
        assert_eq!(
            capture(Key::Pipe, None),
            Captured::Rejected("This key cannot be bound.")
        );
    }
}
