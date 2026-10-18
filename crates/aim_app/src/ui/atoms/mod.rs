//! Smallest UI pieces: colors, fonts, text styles, the crosshair painter.

pub mod crosshair;
pub mod fonts;
pub mod hit_marker;
pub mod speed_ramp;

use bevy_egui::egui::{Color32, FontId, RichText};

pub const GOOD: Color32 = Color32::from_rgb(80, 220, 120);
pub const BAD: Color32 = Color32::from_rgb(240, 80, 80);
pub const WARN: Color32 = Color32::from_rgb(240, 200, 80);
pub const ERROR: Color32 = Color32::from_rgb(240, 120, 80);
/// Damage taken and kills: the red of the damage vignette and indicators.
pub const DAMAGE_RED: Color32 = Color32::from_rgb(235, 60, 60);
/// Hit marker on a headshot.
pub const HEAD_MARKER: Color32 = Color32::from_rgb(255, 210, 70);
/// Hit marker on a body shot.
pub const BODY_MARKER: Color32 = Color32::from_rgb(235, 235, 235);

/// Large, bold text (timer, score, countdown).
pub fn big(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text)
        .font(FontId::proportional(size))
        .strong()
}
