//! UI languages and the translation catalog (English and Spanish).
//!
//! The English text in the source code is the key: [`tr_in`] looks it up in the Spanish
//! catalog and falls back to the English text when there is no entry, so untranslated
//! strings keep working and English output never changes.
//!
//! The Spanish catalog is split into one TOML file per area (`locales/es/<area>.toml`,
//! see [`ES_SOURCES`]), embedded at compile time and merged on first use. Each file is a
//! flat table of `"English text" = "Spanish text"` entries. [`check`] reports every
//! catalog problem (bad TOML, empty values, placeholder mismatches, one English key with
//! two different translations); the tests require it to report none.
//!
//! Templates with values use placeholders: `{}` is filled in order ([`fill`]) and `{name}`
//! by name ([`fill_named`]), so translators can reorder named values. `{{` and `}}` are
//! literal braces, as in `format!`. Format specs such as `{:.1}` are not supported: format
//! the value first (`format!("{:.1}", x)`) and pass the string.
//!
//! Everything here is pure: the caller passes the [`Lang`]. The app keeps the player's
//! current language (see `aim_app::i18n`); the core never reads a global. Text sent to
//! the AI coach or over MCP stays English and must not go through this module.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Display;
use std::sync::OnceLock;

/// A UI language.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    /// English, the source language and the default.
    #[default]
    En,
    /// Spanish.
    Es,
}

impl Lang {
    /// Every language, in the order the language selector lists them.
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Es];

    /// The language's name in that language ("English", "Español"), never translated.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Es => "Español",
        }
    }
}

/// The Spanish catalog files: `(area, TOML source)`. One file per area, so translators
/// working on different areas never edit the same file.
pub const ES_SOURCES: &[(&str, &str)] = &[
    ("menu", include_str!("../../../locales/es/menu.toml")),
    (
        "settings",
        include_str!("../../../locales/es/settings.toml"),
    ),
    ("results", include_str!("../../../locales/es/results.toml")),
    ("replays", include_str!("../../../locales/es/replays.toml")),
    ("coach", include_str!("../../../locales/es/coach.toml")),
    ("hud", include_str!("../../../locales/es/hud.toml")),
    (
        "progress",
        include_str!("../../../locales/es/progress.toml"),
    ),
    ("tools", include_str!("../../../locales/es/tools.toml")),
    ("content", include_str!("../../../locales/es/content.toml")),
    ("rules", include_str!("../../../locales/es/rules.toml")),
    ("core", include_str!("../../../locales/es/core.toml")),
];

/// `en` in `lang`: the catalog entry, or `en` itself when there is none (always for
/// [`Lang::En`]). Works for `&'static` literals and for runtime text such as content names.
/// Catalog keys use LF line breaks; text with CRLF breaks (content read from a CRLF
/// checkout) is looked up with LF breaks.
pub fn tr_in(lang: Lang, en: &str) -> &str {
    match lang {
        Lang::En => en,
        Lang::Es => match spanish().get(en) {
            Some(es) => es,
            None if en.contains("\r\n") => spanish()
                .get(en.replace("\r\n", "\n").as_str())
                .copied()
                .unwrap_or(en),
            None => en,
        },
    }
}

/// Translates `en_template` with [`tr_in`], then fills its `{}` placeholders in order.
pub fn tr_fmt_in(lang: Lang, en_template: &str, args: &[&dyn Display]) -> String {
    fill(tr_in(lang, en_template), args)
}

/// Translates `en_template` with [`tr_in`], then fills its `{name}` placeholders.
pub fn tr_named_in(lang: Lang, en_template: &str, args: &[(&str, &dyn Display)]) -> String {
    fill_named(tr_in(lang, en_template), args)
}

/// Fills each `{}` with the next value of `args`, as `format!` does with `Display`.
/// `{{` and `}}` become literal braces. A `{}` without a value, or any other `{...}`,
/// is kept as written so a mistake is visible instead of silently dropped.
pub fn fill(template: &str, args: &[&dyn Display]) -> String {
    let mut next = args.iter();
    render(template, |name| match name {
        "" => next.next().map(|v| v.to_string()),
        _ => None,
    })
}

/// Fills each `{name}` with the value named `name` in `args`. `{{` and `}}` become
/// literal braces; an unknown name is kept as written.
pub fn fill_named(template: &str, args: &[(&str, &dyn Display)]) -> String {
    render(template, |name| {
        args.iter()
            .find(|(n, _)| !name.is_empty() && *n == name)
            .map(|(_, v)| v.to_string())
    })
}

/// A problem in the catalog files, found by [`check`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogIssue {
    /// A file is not a flat TOML table of strings.
    Invalid {
        /// The catalog area.
        area: String,
        /// The parser's message.
        message: String,
    },
    /// An entry translates to an empty string.
    Empty {
        /// The catalog area.
        area: String,
        /// The English key.
        key: String,
    },
    /// The translation's placeholders differ from the English key's.
    Placeholders {
        /// The catalog area.
        area: String,
        /// The English key.
        key: String,
    },
    /// The same English key has different translations in two areas.
    Conflict {
        /// The English key.
        key: String,
        /// The area whose translation is used.
        kept: String,
        /// The area whose different translation is ignored.
        ignored: String,
    },
}

/// Every problem in `sources` (`(area, TOML)` pairs, such as [`ES_SOURCES`]).
pub fn check(sources: &[(&str, &str)]) -> Vec<CatalogIssue> {
    merge(sources).1
}

/// Merges the files into one map. The first area wins a conflict; entries with a
/// problem are left out. Returns the map and every problem found.
fn merge(sources: &[(&str, &str)]) -> (HashMap<String, String>, Vec<CatalogIssue>) {
    let mut map: HashMap<String, String> = HashMap::new();
    let mut owner: HashMap<String, &str> = HashMap::new();
    let mut issues = Vec::new();
    for &(area, text) in sources {
        let entries = match toml::from_str::<BTreeMap<String, String>>(text) {
            Ok(entries) => entries,
            Err(e) => {
                issues.push(CatalogIssue::Invalid {
                    area: area.to_owned(),
                    message: e.to_string(),
                });
                continue;
            }
        };
        for (key, value) in entries {
            if value.trim().is_empty() {
                issues.push(CatalogIssue::Empty {
                    area: area.to_owned(),
                    key,
                });
            } else if placeholders(&key) != placeholders(&value) {
                issues.push(CatalogIssue::Placeholders {
                    area: area.to_owned(),
                    key,
                });
            } else if let Some(existing) = map.get(&key) {
                if *existing != value {
                    issues.push(CatalogIssue::Conflict {
                        kept: owner[&key].to_owned(),
                        ignored: area.to_owned(),
                        key,
                    });
                }
            } else {
                owner.insert(key.clone(), area);
                map.insert(key, value);
            }
        }
    }
    (map, issues)
}

/// The merged Spanish catalog, built once. The strings are leaked on purpose: the
/// catalog lives for the whole program, and that is what lets [`tr_in`] return
/// `&'static str` for `&'static` keys.
fn spanish() -> &'static HashMap<&'static str, &'static str> {
    static CATALOG: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        merge(ES_SOURCES)
            .0
            .into_iter()
            .map(|(k, v)| (&*k.leak(), &*v.leak()))
            .collect()
    })
}

/// The placeholders of a template, sorted: `{}` as an empty name, `{name}` by name.
/// Positional placeholders are counted, so `{} {}` differs from `{}`.
fn placeholders(template: &str) -> Vec<String> {
    let mut found = Vec::new();
    render(template, |name| {
        found.push(name.to_owned());
        None
    });
    found.sort();
    found
}

/// Walks `template`, replacing each `{name}` (`{}` has an empty name) with `value(name)`,
/// or keeping it as written when that returns `None`. `{{` and `}}` are literal braces.
fn render(template: &str, mut value: impl FnMut(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if tail.starts_with("{{") || tail.starts_with("}}") {
            out.push_str(&tail[..1]);
            rest = &tail[2..];
        } else if let (true, Some(end)) = (tail.starts_with('{'), tail.find('}')) {
            let placeholder = &tail[..=end];
            match value(&placeholder[1..end]) {
                Some(v) => out.push_str(&v),
                None => out.push_str(placeholder),
            }
            rest = &tail[end + 1..];
        } else {
            // A lone `}` or an unclosed `{`: plain text.
            out.push_str(&tail[..1]);
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_the_default_and_serializes_as_a_lowercase_code() {
        assert_eq!(Lang::default(), Lang::En);
        #[derive(Serialize, Deserialize)]
        struct S {
            language: Lang,
        }
        let es = toml::to_string(&S { language: Lang::Es }).unwrap();
        assert_eq!(es.trim(), "language = \"es\"");
        let en: S = toml::from_str("language = \"en\"").unwrap();
        assert_eq!(en.language, Lang::En);
        assert!(toml::from_str::<S>("language = \"fr\"").is_err());
    }

    #[test]
    fn language_names_are_written_in_their_own_language() {
        assert_eq!(Lang::En.native_name(), "English");
        assert_eq!(Lang::Es.native_name(), "Español");
    }

    #[test]
    fn english_returns_the_source_text_unchanged() {
        assert_eq!(tr_in(Lang::En, "Settings"), "Settings");
        assert_eq!(tr_in(Lang::En, "not in any catalog"), "not in any catalog");
    }

    #[test]
    fn spanish_uses_the_catalog_and_falls_back_to_english() {
        assert_eq!(tr_in(Lang::Es, "Settings"), "Ajustes");
        assert_eq!(tr_in(Lang::Es, "not in any catalog"), "not in any catalog");
    }

    #[test]
    fn text_with_crlf_line_breaks_finds_its_lf_entry() {
        let key = spanish()
            .keys()
            .find(|k| k.contains('\n'))
            .expect("a multi-line content description");
        let crlf = key.replace('\n', "\r\n");
        assert_eq!(tr_in(Lang::Es, &crlf), spanish()[key]);
        assert_eq!(tr_in(Lang::En, &crlf), crlf);
    }

    #[test]
    fn runtime_text_is_looked_up_too() {
        let name = String::from("Settings");
        assert_eq!(tr_in(Lang::Es, &name), "Ajustes");
    }

    #[test]
    fn fill_replaces_positional_placeholders_in_order_like_format() {
        let n = 3;
        let pct = format!("{:.1}", 45.678);
        assert_eq!(
            fill("{} hits, {}% accuracy {{raw}}", &[&n, &pct]),
            format!("{} hits, {}% accuracy {{raw}}", n, pct)
        );
    }

    #[test]
    fn fill_keeps_placeholders_it_cannot_fill() {
        assert_eq!(fill("{} of {}", &[&1]), "1 of {}");
        assert_eq!(fill("{name} {:.1} }{", &[&1]), "{name} {:.1} }{");
    }

    #[test]
    fn fill_named_lets_a_translation_reorder_values() {
        let args: [(&str, &dyn Display); 2] = [("hits", &7), ("total", &10)];
        assert_eq!(fill_named("{hits} of {total}", &args), "7 of 10");
        assert_eq!(fill_named("de {total}, {hits}", &args), "de 10, 7");
        assert_eq!(fill_named("{missing} {}", &args), "{missing} {}");
    }

    #[test]
    fn tr_fmt_translates_the_template_before_filling_it() {
        let src = [(
            "t",
            "\"{} of {}\" = \"{} de {}\"\n\"{a} then {b}\" = \"{b} y luego {a}\"",
        )];
        let (map, issues) = merge(&src);
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(fill(&map["{} of {}"], &[&1, &2]), "1 de 2");
        let args: [(&str, &dyn Display); 2] = [("a", &"x"), ("b", &"y")];
        assert_eq!(fill_named(&map["{a} then {b}"], &args), "y y luego x");
        // Untranslated templates fall back to English in both helpers.
        assert_eq!(tr_fmt_in(Lang::Es, "zz {} zz", &[&5]), "zz 5 zz");
        assert_eq!(tr_named_in(Lang::Es, "zz {n} zz", &[("n", &5)]), "zz 5 zz");
        assert_eq!(tr_fmt_in(Lang::En, "{} of {}", &[&1, &2]), "1 of 2");
    }

    #[test]
    fn check_reports_conflicts_empty_values_placeholder_mismatches_and_bad_files() {
        let sources = [
            (
                "a",
                "\"Play\" = \"Jugar\"\n\"Same\" = \"Igual\"\n\"{} hits\" = \"{} aciertos\"",
            ),
            (
                "b",
                "\"Play\" = \"Juega\"\n\"Same\" = \"Igual\"\n\"Blank\" = \" \"",
            ),
            (
                "c",
                "\"{} of {}\" = \"{} de\"\n\"{n} left\" = \"quedan {m}\"",
            ),
            ("d", "nested = { x = \"y\" }"),
        ];
        let issues = check(&sources);
        assert!(issues.contains(&CatalogIssue::Conflict {
            key: "Play".into(),
            kept: "a".into(),
            ignored: "b".into(),
        }));
        assert!(issues.contains(&CatalogIssue::Empty {
            area: "b".into(),
            key: "Blank".into(),
        }));
        for key in ["{} of {}", "{n} left"] {
            assert!(issues.contains(&CatalogIssue::Placeholders {
                area: "c".into(),
                key: key.into(),
            }));
        }
        assert!(matches!(&issues[..], [.., CatalogIssue::Invalid { area, .. }] if area == "d"));
        assert_eq!(
            issues.len(),
            5,
            "an identical duplicate is fine: {issues:?}"
        );
        // The first area wins a conflict.
        assert_eq!(merge(&sources).0["Play"], "Jugar");
    }

    #[test]
    fn the_spanish_catalog_files_are_consistent() {
        assert_eq!(check(ES_SOURCES), Vec::new());
    }
}
