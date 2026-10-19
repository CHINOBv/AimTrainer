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
/// Favourite star.
pub const FAVORITE: Color32 = Color32::from_rgb(250, 200, 60);

/// Large, bold text (timer, score, countdown).
pub fn big(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text)
        .font(FontId::proportional(size))
        .strong()
}

/// Growth of a pop at its first frame (0.6 = 160% size), and how long it takes to settle
/// back to full size, seconds.
const POP_GROWTH: f32 = 0.6;
const POP_SETTLE_SECS: f32 = 0.18;
/// Share of a pop's life spent fading out at its end.
const POP_FADE_SHARE: f32 = 0.3;

/// Scale and opacity of something that pops in and lasts `secs`, `age` seconds after it
/// appeared: it starts large, settles to scale 1, and fades out over the last part of its
/// life. `None` before it appears and once it is over.
pub fn pop(age: f32, secs: f32) -> Option<(f32, f32)> {
    if !(0.0..secs).contains(&age) {
        return None;
    }
    let settle = (1.0 - age / POP_SETTLE_SECS).max(0.0);
    let scale = 1.0 + POP_GROWTH * settle * settle;
    let alpha = ((secs - age) / (POP_FADE_SHARE * secs)).min(1.0);
    Some((scale, alpha))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pop_starts_big_settles_and_fades_out() {
        let (s0, a0) = pop(0.0, 1.0).unwrap();
        assert!((s0 - 1.0 - POP_GROWTH).abs() < 1e-4 && (a0 - 1.0).abs() < 1e-4);
        let (s1, a1) = pop(0.5, 1.0).unwrap();
        assert!((s1 - 1.0).abs() < 1e-4 && (a1 - 1.0).abs() < 1e-4);
        let (_, late) = pop(0.9, 1.0).unwrap();
        assert!(late > 0.0 && late < 0.5);
        assert_eq!(pop(1.0, 1.0), None);
        assert_eq!(pop(-0.1, 1.0), None);
    }
}
