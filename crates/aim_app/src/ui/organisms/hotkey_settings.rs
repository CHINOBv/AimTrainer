//! Global hotkey rows: current combination, "Record" (press the combination), "Clear",
//! and inline validation (unknown key, bare key, duplicates).

use crate::persistence::settings::Settings;
use crate::platform::hotkey;
use crate::platform::hotkey_spec::{self, Combo};
use crate::ui::atoms::{ERROR, WARN};
use bevy_egui::egui::{self, Key, Modifiers};

/// Names of the hotkey slots, in [`hotkey::specs`] order.
const SLOTS: [&str; 3] = ["Toggle overlay", "Queue warm-up", "Buy-phase micro drill"];

/// The three global hotkey rows. `recording` is the slot waiting for a key press; it is
/// shared with the hotkey adapter, which unregisters every hotkey meanwhile.
pub fn hotkey_settings(ui: &mut egui::Ui, settings: &mut Settings, recording: &mut Option<usize>) {
    if let Some(slot) = *recording {
        match take_key_press(ui) {
            Some(Captured::Cancel) => *recording = None,
            Some(Captured::Spec(spec)) => {
                *slot_spec(settings, slot) = spec;
                *recording = None;
            }
            Some(Captured::Ignore) | None => {}
        }
    }

    let specs = hotkey::specs(settings).map(|(s, _)| s.to_owned());
    let problems = hotkey_spec::problems(&specs.each_ref().map(String::as_str), hotkey::is_known);
    egui::Grid::new("hotkeys").num_columns(3).show(ui, |ui| {
        for (slot, name) in SLOTS.iter().enumerate() {
            ui.label(*name);
            if *recording == Some(slot) {
                ui.label(
                    egui::RichText::new(
                        "Press Ctrl/Alt/Shift + a key, or an F-key… (Esc to cancel)",
                    )
                    .color(WARN),
                );
            } else {
                ui.label(egui::RichText::new(hotkey_spec::label(&specs[slot])).strong());
            }
            ui.horizontal(|ui| {
                if *recording == Some(slot) {
                    if ui.button("Cancel").clicked() {
                        *recording = None;
                    }
                } else if ui.button("Record").clicked() {
                    *recording = Some(slot);
                }
                if ui
                    .add_enabled(!specs[slot].is_empty(), egui::Button::new("Clear"))
                    .on_hover_text("Disable this hotkey")
                    .clicked()
                {
                    slot_spec(settings, slot).clear();
                    if *recording == Some(slot) {
                        *recording = None;
                    }
                }
            });
            ui.end_row();
            if let Some(problem) = &problems[slot] {
                ui.label("");
                ui.small(egui::RichText::new(problem).color(ERROR));
                ui.end_row();
            }
        }
    });
}

fn slot_spec(settings: &mut Settings, slot: usize) -> &mut String {
    match slot {
        0 => &mut settings.overlay_hotkey,
        1 => &mut settings.warmup_hotkey,
        _ => &mut settings.micro_hotkey,
    }
}

/// The first meaningful key press of this frame. Every keyboard event is consumed so the
/// combination being recorded does not also drive other widgets.
fn take_key_press(ui: &egui::Ui) -> Option<Captured> {
    ui.input_mut(|input| {
        let captured = input.events.iter().find_map(|event| match event {
            egui::Event::Key {
                key,
                physical_key,
                pressed: true,
                modifiers,
                ..
            } => Some(capture(*key, *physical_key, *modifiers)).filter(|c| *c != Captured::Ignore),
            _ => None,
        });
        input
            .events
            .retain(|e| !matches!(e, egui::Event::Key { .. } | egui::Event::Text(_)));
        captured
    })
}

/// What one key press means while a hotkey is being recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Captured {
    /// Esc: stop recording, keep the old hotkey.
    Cancel,
    /// A lone modifier, a bare non-function key or a key hotkeys cannot use: keep waiting.
    Ignore,
    /// The new spec ("ctrl+shift+F10").
    Spec(String),
}

/// Interpret a key press (`physical` preferred: hotkeys bind physical keys).
pub fn capture(key: Key, physical: Option<Key>, mods: Modifiers) -> Captured {
    if key == Key::Escape {
        return Captured::Cancel;
    }
    let Some(token) = key_token(physical.unwrap_or(key)) else {
        return Captured::Ignore;
    };
    let combo = Combo {
        ctrl: mods.ctrl,
        alt: mods.alt,
        shift: mods.shift,
        meta: mods.mac_cmd,
        key: token,
    };
    // A bare letter, digit or Space would steal normal typing system-wide.
    if !combo.has_modifier() && !combo.is_function_key() {
        return Captured::Ignore;
    }
    Captured::Spec(combo.spec())
}

/// `global-hotkey` key name of an egui key; `None` for modifiers and keys it cannot bind.
fn key_token(key: Key) -> Option<String> {
    let name = key.name();
    let token = match key {
        Key::A
        | Key::B
        | Key::C
        | Key::D
        | Key::E
        | Key::F
        | Key::G
        | Key::H
        | Key::I
        | Key::J
        | Key::K
        | Key::L
        | Key::M
        | Key::N
        | Key::O
        | Key::P
        | Key::Q
        | Key::R
        | Key::S
        | Key::T
        | Key::U
        | Key::V
        | Key::W
        | Key::X
        | Key::Y
        | Key::Z => format!("Key{name}"),
        Key::Num0
        | Key::Num1
        | Key::Num2
        | Key::Num3
        | Key::Num4
        | Key::Num5
        | Key::Num6
        | Key::Num7
        | Key::Num8
        | Key::Num9 => format!("Digit{name}"),
        Key::Backtick => "Backquote".to_owned(),
        Key::Equals => "Equal".to_owned(),
        Key::OpenBracket => "BracketLeft".to_owned(),
        Key::CloseBracket => "BracketRight".to_owned(),
        Key::ArrowDown
        | Key::ArrowLeft
        | Key::ArrowRight
        | Key::ArrowUp
        | Key::Tab
        | Key::Backspace
        | Key::Enter
        | Key::Space
        | Key::Insert
        | Key::Delete
        | Key::Home
        | Key::End
        | Key::PageUp
        | Key::PageDown
        | Key::Comma
        | Key::Backslash
        | Key::Slash
        | Key::Minus
        | Key::Period
        | Key::Semicolon
        | Key::Quote
        | Key::IntlBackslash => format!("{key:?}"),
        _ => {
            // Function keys: the crate binds F1–F24.
            let name = format!("{key:?}");
            return Combo::parse(&name)
                .is_ok_and(|c| c.is_function_key())
                .then_some(name);
        }
    };
    Some(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL_SHIFT: Modifiers = Modifiers {
        alt: false,
        ctrl: true,
        shift: true,
        mac_cmd: false,
        command: true,
    };

    #[test]
    fn records_key_with_modifiers_in_crate_format() {
        assert_eq!(
            capture(Key::F10, Some(Key::F10), CTRL_SHIFT),
            Captured::Spec("ctrl+shift+F10".to_owned())
        );
        assert_eq!(
            capture(Key::K, None, Modifiers::ALT),
            Captured::Spec("alt+KeyK".to_owned())
        );
        assert_eq!(
            capture(Key::Num5, None, Modifiers::CTRL),
            Captured::Spec("ctrl+Digit5".to_owned())
        );
        assert_eq!(
            capture(Key::F8, None, Modifiers::NONE),
            Captured::Spec("F8".to_owned())
        );
    }

    #[test]
    fn prefers_the_physical_key() {
        // AZERTY: logical A on the physical Q key.
        assert_eq!(
            capture(Key::A, Some(Key::Q), Modifiers::CTRL),
            Captured::Spec("ctrl+KeyQ".to_owned())
        );
    }

    #[test]
    fn bare_non_function_keys_wait_for_a_modifier() {
        assert_eq!(capture(Key::K, None, Modifiers::NONE), Captured::Ignore);
        assert_eq!(capture(Key::Space, None, Modifiers::NONE), Captured::Ignore);
        assert_eq!(
            capture(Key::K, None, Modifiers::SHIFT),
            Captured::Spec("shift+KeyK".to_owned())
        );
    }

    #[test]
    fn escape_cancels_and_lone_modifiers_wait() {
        assert_eq!(
            capture(Key::Escape, None, Modifiers::NONE),
            Captured::Cancel
        );
        assert_eq!(
            capture(Key::ShiftLeft, None, Modifiers::SHIFT),
            Captured::Ignore
        );
        assert_eq!(
            capture(Key::ControlRight, None, Modifiers::CTRL),
            Captured::Ignore
        );
        assert_eq!(capture(Key::Pipe, None, Modifiers::CTRL), Captured::Ignore);
    }
}
