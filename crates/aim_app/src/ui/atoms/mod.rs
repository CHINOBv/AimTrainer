//! Smallest UI pieces: colors, text styles, the crosshair painter.

pub mod crosshair;

use bevy_egui::egui::{Color32, FontId, RichText};

pub const GOOD: Color32 = Color32::from_rgb(80, 220, 120);
pub const BAD: Color32 = Color32::from_rgb(240, 80, 80);
pub const WARN: Color32 = Color32::from_rgb(240, 200, 80);
pub const ERROR: Color32 = Color32::from_rgb(240, 120, 80);

/// Linear `[0, 1]` RGB from settings to an egui color.
pub fn color32(rgb: [f32; 3]) -> Color32 {
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;
    Color32::from_rgb(c(rgb[0]), c(rgb[1]), c(rgb[2]))
}

/// Large, bold text (timer, score, countdown).
pub fn big(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text)
        .font(FontId::proportional(size))
        .strong()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_conversion_clamps_and_scales() {
        assert_eq!(color32([1.0, 0.0, 2.0]), Color32::from_rgb(255, 0, 255));
        assert_eq!(color32([-1.0, 0.5, 0.0]), Color32::from_rgb(0, 127, 0));
    }
}
