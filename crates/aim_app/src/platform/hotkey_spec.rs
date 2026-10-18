//! Global hotkey specs as stored in the settings ("ctrl+shift+F10", the format the
//! `global-hotkey` crate parses): building, readable labels and validation. Pure string
//! logic, so it compiles and is tested on every platform.

/// A parsed spec: modifiers plus one main key token (e.g. "F10", "KeyK").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Combo {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
    pub key: String,
}

impl Combo {
    /// Parse the modifier/key structure of a spec (case-insensitive modifiers, the main
    /// key last). Whether the key name itself is known is the OS layer's call.
    pub fn parse(spec: &str) -> Result<Self, String> {
        let tokens: Vec<&str> = spec.split('+').map(str::trim).collect();
        let (key, mods) = tokens.split_last().ok_or_else(|| invalid(spec))?;
        if key.is_empty() || is_modifier(key) {
            return Err(invalid(spec));
        }
        let mut combo = Combo {
            ctrl: false,
            alt: false,
            shift: false,
            meta: false,
            key: (*key).to_owned(),
        };
        for m in mods {
            match m.to_ascii_uppercase().as_str() {
                "CTRL" | "CONTROL" => combo.ctrl = true,
                "ALT" | "OPTION" => combo.alt = true,
                "SHIFT" => combo.shift = true,
                "SUPER" | "META" | "CMD" | "COMMAND" => combo.meta = true,
                _ => return Err(invalid(spec)),
            }
        }
        Ok(combo)
    }

    /// Spec string in the `global-hotkey` format, modifiers first.
    pub fn spec(&self) -> String {
        let mut parts: Vec<&str> = self.modifiers().map(|(token, _)| token).collect();
        parts.push(&self.key);
        parts.join("+")
    }

    /// Readable label, e.g. "Ctrl + Shift + F10".
    pub fn label(&self) -> String {
        let key = self.canonical_key();
        let mut parts: Vec<&str> = self.modifiers().map(|(_, label)| label).collect();
        parts.push(&key);
        parts.join(" + ")
    }

    pub fn has_modifier(&self) -> bool {
        self.ctrl || self.alt || self.shift || self.meta
    }

    /// F1–F24: safe to bind without a modifier, nothing types them.
    pub fn is_function_key(&self) -> bool {
        self.key
            .strip_prefix(['F', 'f'])
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=24).contains(&n))
    }

    /// (spec token, label) of each held modifier, in spec order.
    fn modifiers(&self) -> impl Iterator<Item = (&'static str, &'static str)> {
        [
            (self.ctrl, "ctrl", "Ctrl"),
            (self.alt, "alt", "Alt"),
            (self.shift, "shift", "Shift"),
            (self.meta, "super", "Super"),
        ]
        .into_iter()
        .filter(|(held, _, _)| *held)
        .map(|(_, token, label)| (token, label))
    }

    /// The key without the crate's optional prefixes: "KeyK" and "k" are both "K",
    /// "Digit5" is "5". Function keys are upper-cased; other names keep their spelling.
    fn canonical_key(&self) -> String {
        let k = &self.key;
        for prefix in ["Key", "Digit"] {
            if k.len() == prefix.len() + 1
                && k.is_char_boundary(prefix.len())
                && k[..prefix.len()].eq_ignore_ascii_case(prefix)
            {
                return k[prefix.len()..].to_ascii_uppercase();
            }
        }
        if k.chars().count() == 1 || self.is_function_key() {
            k.to_ascii_uppercase()
        } else {
            k.clone()
        }
    }

    /// Identity for conflict checks: modifiers plus the case-folded canonical key.
    fn identity(&self) -> (bool, bool, bool, bool, String) {
        (
            self.ctrl,
            self.alt,
            self.shift,
            self.meta,
            self.canonical_key().to_ascii_uppercase(),
        )
    }
}

fn invalid(spec: &str) -> String {
    format!("\"{}\" is not a valid hotkey", spec.trim())
}

fn is_modifier(token: &str) -> bool {
    matches!(
        token.to_ascii_uppercase().as_str(),
        "CTRL" | "CONTROL" | "ALT" | "OPTION" | "SHIFT" | "SUPER" | "META" | "CMD" | "COMMAND"
    )
}

/// Readable label of a stored spec: "Not set" when empty, the raw text when unparsable.
pub fn label(spec: &str) -> String {
    if spec.trim().is_empty() {
        return "Not set".to_owned();
    }
    Combo::parse(spec).map_or_else(|_| spec.to_owned(), |c| c.label())
}

/// One problem per slot of `specs` (`None` when fine): unparsable or unknown key
/// (`known` is the OS layer's parser), a bare non-function key, or the same combination
/// as an earlier slot. Empty specs are disabled hotkeys and never a problem.
pub fn problems(specs: &[&str], known: impl Fn(&str) -> bool) -> Vec<Option<String>> {
    let parsed: Vec<Option<Result<Combo, String>>> = specs
        .iter()
        .map(|s| {
            (!s.trim().is_empty()).then(|| {
                Combo::parse(s).and_then(|c| if known(s) { Ok(c) } else { Err(invalid(s)) })
            })
        })
        .collect();
    parsed
        .iter()
        .enumerate()
        .map(|(i, p)| match p {
            None => None,
            Some(Err(e)) => Some(e.clone()),
            Some(Ok(c)) if !c.has_modifier() && !c.is_function_key() => Some(format!(
                "{} has no modifier: add Ctrl, Alt or Shift, or it fires while you type",
                c.label()
            )),
            Some(Ok(c)) => parsed[..i]
                .iter()
                .any(|q| matches!(q, Some(Ok(q)) if q.identity() == c.identity()))
                .then(|| format!("{} is already used by another hotkey", c.label())),
        })
        .collect()
}

/// The hotkeys to register: none while one is being recorded (the OS must hand the
/// combination to the window), otherwise every non-empty spec `problems` accepts. Of
/// duplicates only the first slot is kept; a bare non-function key is never registered.
pub fn registrable<T: Copy>(
    specs: &[(&str, T)],
    recording: bool,
    known: impl Fn(&str) -> bool,
) -> Vec<(String, T)> {
    if recording {
        return Vec::new();
    }
    let texts: Vec<&str> = specs.iter().map(|(s, _)| *s).collect();
    specs
        .iter()
        .zip(problems(&texts, known))
        .filter(|((s, _), problem)| !s.trim().is_empty() && problem.is_none())
        .map(|((s, value), _)| ((*s).to_owned(), *value))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrable_drops_empty_bare_invalid_and_later_duplicates() {
        let specs = [
            ("ctrl+shift+F10", 1),
            ("", 2),
            ("KeyK", 3),
            ("F9", 4),
            ("CTRL+SHIFT+f10", 5),
            ("ctrl+Bogus", 6),
            ("alt+KeyK", 7),
        ];
        let known = |s: &str| !s.contains("Bogus");
        assert_eq!(
            registrable(&specs, false, known),
            vec![
                ("ctrl+shift+F10".to_owned(), 1),
                ("F9".to_owned(), 4),
                ("alt+KeyK".to_owned(), 7),
            ]
        );
    }

    #[test]
    fn registrable_is_empty_while_recording() {
        assert!(registrable(&[("ctrl+shift+F10", 1)], true, |_| true).is_empty());
    }

    fn combo(ctrl: bool, alt: bool, shift: bool, key: &str) -> Combo {
        Combo {
            ctrl,
            alt,
            shift,
            meta: false,
            key: key.to_owned(),
        }
    }

    #[test]
    fn parses_modifiers_case_insensitively_with_key_last() {
        assert_eq!(
            Combo::parse("ctrl+shift+F10"),
            Ok(combo(true, false, true, "F10"))
        );
        assert_eq!(
            Combo::parse(" Control + ALT + KeyK "),
            Ok(combo(true, true, false, "KeyK"))
        );
        assert_eq!(Combo::parse("F9"), Ok(combo(false, false, false, "F9")));
        assert!(Combo::parse("").is_err());
        assert!(Combo::parse("ctrl+shift").is_err());
        assert!(Combo::parse("ctrl+F1+shift").is_err());
        assert!(Combo::parse("ctrl++F1").is_err());
    }

    #[test]
    fn spec_round_trips_in_crate_format() {
        let c = combo(true, true, true, "F8");
        assert_eq!(c.spec(), "ctrl+alt+shift+F8");
        assert_eq!(Combo::parse(&c.spec()), Ok(c));
        assert_eq!(combo(false, false, false, "F7").spec(), "F7");
    }

    #[test]
    fn labels_are_readable() {
        assert_eq!(label("ctrl+shift+F10"), "Ctrl + Shift + F10");
        assert_eq!(label("alt+KeyK"), "Alt + K");
        assert_eq!(label("ctrl+Digit5"), "Ctrl + 5");
        assert_eq!(label("super+ArrowUp"), "Super + ArrowUp");
        assert_eq!(label("  "), "Not set");
        assert_eq!(label("ctrl+"), "ctrl+");
    }

    #[test]
    fn function_keys_are_recognized() {
        assert!(combo(false, false, false, "F1").is_function_key());
        assert!(combo(false, false, false, "f24").is_function_key());
        assert!(!combo(false, false, false, "F25").is_function_key());
        assert!(!combo(false, false, false, "KeyF").is_function_key());
        assert!(!combo(false, false, false, "F").is_function_key());
        assert!(combo(false, true, false, "KeyF").has_modifier());
    }

    #[test]
    fn reports_invalid_bare_and_duplicate_hotkeys() {
        let known = |s: &str| !s.contains("Bogus");
        let p = problems(
            &[
                "ctrl+shift+F10",
                "CTRL+SHIFT+f10",
                "KeyK",
                "F9",
                "",
                "",
                "ctrl+Bogus",
                "ctrl+",
            ],
            known,
        );
        assert_eq!(p[0], None);
        assert!(p[1].as_deref().unwrap().contains("Ctrl + Shift + F10"));
        assert!(p[2].as_deref().unwrap().contains("modifier"));
        assert_eq!(p[3], None);
        assert_eq!(p[4], None);
        assert_eq!(p[5], None, "two disabled hotkeys do not conflict");
        assert!(p[6].as_deref().unwrap().contains("not a valid"));
        assert!(p[7].as_deref().unwrap().contains("not a valid"));
    }

    #[test]
    fn key_aliases_count_as_the_same_hotkey() {
        let p = problems(&["alt+KeyK", "ALT+k"], |_| true);
        assert_eq!(p[0], None);
        assert!(p[1].is_some());
    }
}
