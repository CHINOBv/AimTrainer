//! Arena death screen: a fade, the direction of the killing shot and the respawn timer.

use crate::ui::atoms::{BAD, big};
use bevy_egui::egui::{self, Align2};
use std::f32::consts::FRAC_PI_4;

/// How long the dark fade takes to clear after a death, seconds.
pub const DEATH_FADE_SECS: f32 = 0.6;
/// Opacity of the fade at the moment of death.
const DEATH_FADE_ALPHA: f32 = 0.45;

/// Where the killing shot came from, relative to where the player was looking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShotSide {
    Ahead,
    Left,
    Right,
    Behind,
}

impl ShotSide {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ahead => "Shot from ahead ↑",
            Self::Left => "Shot from ← left",
            Self::Right => "Shot from right →",
            Self::Behind => "Shot from behind ↓",
        }
    }
}

/// The side of a screen bearing (radians, 0 ahead, positive right): ahead within 45°,
/// behind beyond 135°, left or right otherwise.
pub fn shot_side(bearing: f32) -> ShotSide {
    let off = bearing.abs();
    if off <= FRAC_PI_4 {
        ShotSide::Ahead
    } else if off >= 3.0 * FRAC_PI_4 {
        ShotSide::Behind
    } else if bearing > 0.0 {
        ShotSide::Right
    } else {
        ShotSide::Left
    }
}

/// Opacity of the death fade `age` seconds after the death.
pub fn death_fade(age: f32) -> f32 {
    DEATH_FADE_ALPHA * (1.0 - age / DEATH_FADE_SECS).max(0.0)
}

/// What the death screen shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeathView {
    /// Where the killing shot came from, when a bot dealt it.
    pub side: Option<ShotSide>,
    /// Seconds since the death.
    pub age: f32,
    /// Seconds of spawn protection left.
    pub respawn_secs: f32,
}

pub fn draw(ctx: &egui::Context, view: &DeathView) {
    let fade = death_fade(view.age);
    if fade > 0.0 {
        ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("death-fade"),
        ))
        .rect_filled(
            ctx.content_rect(),
            0.0,
            egui::Color32::from_rgba_unmultiplied(40, 0, 0, (fade * 255.0) as u8),
        );
    }
    egui::Area::new(egui::Id::new("death-screen"))
        .anchor(Align2::CENTER_CENTER, [0.0, 110.0])
        .interactable(false)
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(big("ELIMINATED", 30.0).color(BAD));
                if let Some(side) = view.side {
                    ui.label(big(side.label(), 20.0));
                }
                ui.label(format!("Respawned · protected {:.1} s", view.respawn_secs));
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::atoms::fonts::font_definitions;
    use bevy_egui::egui::{Color32, Context, FontId};
    use std::f32::consts::{FRAC_PI_2, PI};

    #[test]
    fn bearings_map_to_four_sides() {
        assert_eq!(shot_side(0.0), ShotSide::Ahead);
        assert_eq!(shot_side(FRAC_PI_4 - 0.01), ShotSide::Ahead);
        assert_eq!(shot_side(-FRAC_PI_4 + 0.01), ShotSide::Ahead);
        assert_eq!(shot_side(FRAC_PI_2), ShotSide::Right);
        assert_eq!(shot_side(-FRAC_PI_2), ShotSide::Left);
        assert_eq!(shot_side(PI), ShotSide::Behind);
        assert_eq!(shot_side(-3.0 * FRAC_PI_4 - 0.01), ShotSide::Behind);
        assert_eq!(shot_side(3.0 * FRAC_PI_4 - 0.01), ShotSide::Right);
    }

    #[test]
    fn the_fade_clears_after_the_death() {
        assert!((death_fade(0.0) - DEATH_FADE_ALPHA).abs() < 1e-4);
        assert!(death_fade(0.3) < death_fade(0.1));
        assert_eq!(death_fade(DEATH_FADE_SECS), 0.0);
        assert_eq!(death_fade(5.0), 0.0);
    }

    /// The arrows and the kill feed's ▶ render with the UI fonts, not as empty boxes.
    #[test]
    fn hud_symbols_have_glyphs() {
        let ctx = Context::default();
        ctx.set_fonts(font_definitions());
        ctx.run_ui(Default::default(), |_| {})
            .textures_delta
            .clear();
        ctx.fonts_mut(|f| {
            let mut uv = |c: char| {
                let galley =
                    f.layout_no_wrap(c.to_string(), FontId::proportional(14.0), Color32::WHITE);
                galley.rows[0].glyphs[0].uv_rect
            };
            let replacement = uv('◻');
            for c in "←↑→↓▶×".chars() {
                assert_ne!(uv(c), replacement, "{c} has no glyph");
            }
        });
    }
}
