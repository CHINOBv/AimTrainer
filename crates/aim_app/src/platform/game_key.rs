//! In-window gameplay keys as stored in the settings ("KeyR", "F5", "Backspace"): the
//! names Bevy gives its physical `KeyCode`s, a readable label and the keys gameplay already
//! owns. Unlike global hotkeys these only work while the trainer window has focus.

use bevy::input::keyboard::KeyCode;

/// Default restart key: free in gameplay (no reload key here) and next to WASD.
pub const DEFAULT_RESTART: &str = "KeyR";

/// Keys gameplay already uses: movement, walk, pause and the results screen's retry.
const RESERVED: [KeyCode; 8] = [
    KeyCode::KeyW,
    KeyCode::KeyA,
    KeyCode::KeyS,
    KeyCode::KeyD,
    KeyCode::ShiftLeft,
    KeyCode::ShiftRight,
    KeyCode::Escape,
    KeyCode::Space,
];

/// Every key that can be bound: (stored name, key, label).
const KEYS: [(&str, KeyCode, &str); 58] = [
    ("KeyA", KeyCode::KeyA, "A"),
    ("KeyB", KeyCode::KeyB, "B"),
    ("KeyC", KeyCode::KeyC, "C"),
    ("KeyD", KeyCode::KeyD, "D"),
    ("KeyE", KeyCode::KeyE, "E"),
    ("KeyF", KeyCode::KeyF, "F"),
    ("KeyG", KeyCode::KeyG, "G"),
    ("KeyH", KeyCode::KeyH, "H"),
    ("KeyI", KeyCode::KeyI, "I"),
    ("KeyJ", KeyCode::KeyJ, "J"),
    ("KeyK", KeyCode::KeyK, "K"),
    ("KeyL", KeyCode::KeyL, "L"),
    ("KeyM", KeyCode::KeyM, "M"),
    ("KeyN", KeyCode::KeyN, "N"),
    ("KeyO", KeyCode::KeyO, "O"),
    ("KeyP", KeyCode::KeyP, "P"),
    ("KeyQ", KeyCode::KeyQ, "Q"),
    ("KeyR", KeyCode::KeyR, "R"),
    ("KeyS", KeyCode::KeyS, "S"),
    ("KeyT", KeyCode::KeyT, "T"),
    ("KeyU", KeyCode::KeyU, "U"),
    ("KeyV", KeyCode::KeyV, "V"),
    ("KeyW", KeyCode::KeyW, "W"),
    ("KeyX", KeyCode::KeyX, "X"),
    ("KeyY", KeyCode::KeyY, "Y"),
    ("KeyZ", KeyCode::KeyZ, "Z"),
    ("Digit0", KeyCode::Digit0, "0"),
    ("Digit1", KeyCode::Digit1, "1"),
    ("Digit2", KeyCode::Digit2, "2"),
    ("Digit3", KeyCode::Digit3, "3"),
    ("Digit4", KeyCode::Digit4, "4"),
    ("Digit5", KeyCode::Digit5, "5"),
    ("Digit6", KeyCode::Digit6, "6"),
    ("Digit7", KeyCode::Digit7, "7"),
    ("Digit8", KeyCode::Digit8, "8"),
    ("Digit9", KeyCode::Digit9, "9"),
    ("F1", KeyCode::F1, "F1"),
    ("F2", KeyCode::F2, "F2"),
    ("F3", KeyCode::F3, "F3"),
    ("F4", KeyCode::F4, "F4"),
    ("F5", KeyCode::F5, "F5"),
    ("F6", KeyCode::F6, "F6"),
    ("F7", KeyCode::F7, "F7"),
    ("F8", KeyCode::F8, "F8"),
    ("F9", KeyCode::F9, "F9"),
    ("F10", KeyCode::F10, "F10"),
    ("F11", KeyCode::F11, "F11"),
    ("F12", KeyCode::F12, "F12"),
    ("Backspace", KeyCode::Backspace, "Backspace"),
    ("Tab", KeyCode::Tab, "Tab"),
    ("Enter", KeyCode::Enter, "Enter"),
    ("Backquote", KeyCode::Backquote, "`"),
    ("Escape", KeyCode::Escape, "Esc"),
    ("Space", KeyCode::Space, "Space"),
    ("ShiftLeft", KeyCode::ShiftLeft, "Left Shift"),
    ("ShiftRight", KeyCode::ShiftRight, "Right Shift"),
    ("Insert", KeyCode::Insert, "Insert"),
    ("Delete", KeyCode::Delete, "Delete"),
];

/// The key a stored name stands for, if it is a known key.
pub fn key_code(name: &str) -> Option<KeyCode> {
    KEYS.iter().find(|(n, ..)| *n == name).map(|&(_, k, _)| k)
}

/// Readable label of a stored name ("R", "F5"); unknown names read "unbound".
pub fn label(name: &str) -> &str {
    KEYS.iter()
        .find(|(n, ..)| *n == name)
        .map_or("unbound", |&(.., l)| l)
}

/// Why a key cannot be bound to a gameplay action, if it cannot.
pub fn problem(name: &str) -> Option<&'static str> {
    match key_code(name) {
        None => Some("This key cannot be bound."),
        Some(k) if RESERVED.contains(&k) => Some("Gameplay already uses this key."),
        Some(_) => None,
    }
}

/// The key a stored name stands for, when it may drive a gameplay action.
pub fn bindable(name: &str) -> Option<KeyCode> {
    problem(name).is_none().then(|| key_code(name)).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_restart_key_is_r_and_free_in_gameplay() {
        assert_eq!(bindable(DEFAULT_RESTART), Some(KeyCode::KeyR));
        assert_eq!(label(DEFAULT_RESTART), "R");
    }

    #[test]
    fn gameplay_keys_are_rejected() {
        for name in [
            "KeyW",
            "KeyA",
            "KeyS",
            "KeyD",
            "ShiftLeft",
            "Escape",
            "Space",
        ] {
            assert_eq!(
                problem(name),
                Some("Gameplay already uses this key."),
                "{name}"
            );
            assert_eq!(bindable(name), None, "{name}");
        }
    }

    #[test]
    fn unknown_names_are_unbound() {
        assert_eq!(key_code("KeyÑ"), None);
        assert_eq!(label("nope"), "unbound");
        assert_eq!(problem(""), Some("This key cannot be bound."));
        assert_eq!(bindable("F5"), Some(KeyCode::F5));
        assert_eq!(bindable("Backspace"), Some(KeyCode::Backspace));
    }
}
