//! Smallest UI pieces: colors, fonts, text styles, the crosshair painter.

pub mod crosshair;
pub mod fonts;
pub mod hit_marker;

use bevy_egui::egui::{Color32, FontId, RichText};

pub const GOOD: Color32 = Color32::from_rgb(80, 220, 120);
pub const BAD: Color32 = Color32::from_rgb(240, 80, 80);
pub const WARN: Color32 = Color32::from_rgb(240, 200, 80);
pub const ERROR: Color32 = Color32::from_rgb(240, 120, 80);

/// Large, bold text (timer, score, countdown).
pub fn big(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text)
        .font(FontId::proportional(size))
        .strong()
}
