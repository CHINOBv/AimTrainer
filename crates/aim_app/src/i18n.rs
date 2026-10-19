//! The player's UI language and the translation helpers the UI calls.
//!
//! The catalog and the pure lookups live in [`aim_core::i18n`]; this module only keeps the
//! current language, which [`LanguagePlugin`] copies from [`Settings::language`] whenever
//! the settings change (and `main` sets once at startup). It is a process-wide atomic
//! rather than a resource so that `'static` label tables, view functions and core
//! sentence builders can translate without threading the language through every call.
//!
//! Tests must not call [`set_language`]: tests run in parallel in one process and many
//! of them assert English text. Test translations with the `*_in` functions instead.
//! Text for the AI coach, MCP, files and logs stays English: never pass it through `tr`.

use crate::persistence::settings::Settings;
pub use aim_core::i18n::Lang;
use aim_core::i18n::{tr_fmt_in, tr_in, tr_named_in};
use bevy::prelude::*;
use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Makes `lang` the language every `tr*` call uses from now on.
pub fn set_language(lang: Lang) {
    let code = match lang {
        Lang::En => 0,
        Lang::Es => 1,
    };
    CURRENT.store(code, Ordering::Relaxed);
}

/// The current UI language.
pub fn language() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::Es,
        _ => Lang::En,
    }
}

/// `en` in the current language (English text when there is no translation).
pub fn tr(en: &str) -> &str {
    tr_in(language(), en)
}

/// `en_template` in the current language with its `{}` placeholders filled in order.
// Used by the translation sweep (docs/i18n.md); remove the allow once a caller exists.
#[allow(dead_code)]
pub fn tr_fmt(en_template: &str, args: &[&dyn Display]) -> String {
    tr_fmt_in(language(), en_template, args)
}

/// `en_template` in the current language with its `{name}` placeholders filled.
#[allow(dead_code)]
pub fn tr_named(en_template: &str, args: &[(&str, &dyn Display)]) -> String {
    tr_named_in(language(), en_template, args)
}

/// A `(value, English label)` table, such as tab lists, with its labels in `lang`.
pub fn labels_in<T: Copy, const N: usize>(
    lang: Lang,
    table: &[(T, &'static str); N],
) -> [(T, &'static str); N] {
    table.map(|(value, en)| (value, tr_in(lang, en)))
}

/// [`labels_in`] for the current language.
pub fn labels<T: Copy, const N: usize>(table: &[(T, &'static str); N]) -> [(T, &'static str); N] {
    labels_in(language(), table)
}

/// Keeps the current language equal to [`Settings::language`].
pub struct LanguagePlugin;

impl Plugin for LanguagePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            apply_language.run_if(resource_exists_and_changed::<Settings>),
        );
    }
}

fn apply_language(settings: Res<Settings>) {
    set_language(settings.language);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_language_is_a_valid_system() {
        bevy::ecs::system::assert_is_system(apply_language);
    }

    #[test]
    fn labels_translate_every_entry_and_keep_the_values() {
        let table = [(1, "Settings"), (2, "not translated")];
        assert_eq!(labels_in(Lang::En, &table), table);
        assert_eq!(
            labels_in(Lang::Es, &table),
            [(1, "Ajustes"), (2, "not translated")]
        );
    }
}
