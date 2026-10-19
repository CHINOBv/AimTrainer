//! Font families for the whole UI.

use bevy_egui::egui::{FontDefinitions, FontFamily};

/// egui's default fonts with `Hack` as the last proportional fallback.
///
/// The default proportional fonts lack common symbols such as `→`, which then render as an
/// empty box. `Hack` already ships with egui (it is the monospace font) and covers them.
pub fn font_definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .push("Hack".to_owned());
    fonts
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_egui::egui::{Color32, Context, FontId};

    /// Every non-ASCII symbol the UI strings use.
    const UI_SYMBOLS: &str = "→▶↻·≈±°—…–✔⏵≤−×▲▼⚠↗↖↙↘÷";

    /// Every Spanish letter and punctuation mark outside ASCII.
    const SPANISH_CHARS: &str = "áéíóúñ¿¡üÁÉÍÓÚÑÜ";

    /// What egui draws for a character no font in the chain has (an empty box).
    const REPLACEMENT: char = '◻';

    /// Symbols that would render as the replacement box instead of their own glyph.
    ///
    /// `has_glyph` cannot be used: it reports `false` for any character that lives in the
    /// same face as the replacement glyph (NotoEmoji), such as `▶`.
    fn missing_glyphs(fonts: FontDefinitions, chars: &str) -> Vec<char> {
        let ctx = Context::default();
        ctx.set_fonts(fonts);
        // Fonts load on the first pass; the texture delta is irrelevant without a renderer.
        ctx.run_ui(Default::default(), |_| {})
            .textures_delta
            .clear();
        ctx.fonts_mut(|f| {
            let mut uv = |c: char| {
                let galley =
                    f.layout_no_wrap(c.to_string(), FontId::proportional(14.0), Color32::WHITE);
                galley.rows[0].glyphs[0].uv_rect
            };
            let replacement = uv(REPLACEMENT);
            chars.chars().filter(|&c| uv(c) == replacement).collect()
        })
    }

    #[test]
    fn default_fonts_lack_the_arrow() {
        assert!(missing_glyphs(FontDefinitions::default(), UI_SYMBOLS).contains(&'→'));
    }

    #[test]
    fn proportional_font_renders_every_ui_symbol() {
        assert_eq!(
            missing_glyphs(font_definitions(), UI_SYMBOLS),
            Vec::<char>::new()
        );
    }

    #[test]
    fn proportional_font_renders_every_spanish_character() {
        assert_eq!(
            missing_glyphs(font_definitions(), SPANISH_CHARS),
            Vec::<char>::new()
        );
    }
}
