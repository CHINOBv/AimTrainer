//! Arena health bar with a "ghost" of the damage just taken and a low-health pulse.

use crate::ui::atoms::{BAD, GOOD, WARN};
use bevy_egui::egui::{self, RichText};

/// The ghost holds still this long after a hit before draining, seconds.
pub const GHOST_DELAY_SECS: f32 = 0.5;
/// How fast the ghost drains towards the health, HP per second.
pub const GHOST_DRAIN_PER_SEC: f32 = 120.0;
/// At or under this health the bar pulses. A design choice: about one more body hit.
pub const LOW_HP_THRESHOLD: f32 = 30.0;
/// Pulses per second at low health.
const LOW_HP_PULSE_HZ: f32 = 2.0;
/// Size of the bar, points.
const BAR_SIZE: egui::Vec2 = egui::vec2(220.0, 18.0);

/// The trailing "ghost" of the health bar: it marks the health before the latest hits,
/// waits [`GHOST_DELAY_SECS`], then drains down to the health. Healing or a respawn
/// snaps it to the health.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct HealthGhost {
    ghost: Option<f32>,
    last_health: f32,
    hold: f32,
}

impl HealthGhost {
    /// Follow `health` over a frame of `dt` seconds.
    pub fn update(&mut self, health: f32, dt: f32) {
        let ghost = self.ghost.unwrap_or(health);
        let ghost = if health >= ghost {
            self.hold = 0.0;
            health
        } else if health < self.last_health {
            self.hold = GHOST_DELAY_SECS;
            ghost
        } else if self.hold > 0.0 {
            self.hold -= dt;
            ghost
        } else {
            (ghost - GHOST_DRAIN_PER_SEC * dt).max(health)
        };
        self.ghost = Some(ghost);
        self.last_health = health;
    }

    /// The health the ghost shows (the health itself before the first update).
    pub fn value(&self) -> f32 {
        self.ghost.unwrap_or(self.last_health)
    }
}

/// Strength (0..1) of the low-health pulse at `health`, `clock` seconds into the run: 0
/// above [`LOW_HP_THRESHOLD`], otherwise a sine at [`LOW_HP_PULSE_HZ`].
pub fn low_hp_pulse(health: f32, clock: f32) -> f32 {
    if health > LOW_HP_THRESHOLD {
        return 0.0;
    }
    0.5 - 0.5 * (clock * LOW_HP_PULSE_HZ * std::f32::consts::TAU).cos()
}

/// What the bar shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HealthBar {
    pub health: f32,
    pub ghost: f32,
    pub max: f32,
    /// Low-health pulse strength, 0..1.
    pub pulse: f32,
}

/// The bar, its ghost, and the HP number on it.
pub fn draw(ui: &mut egui::Ui, bar: &HealthBar) {
    let (rect, _) = ui.allocate_exact_size(BAR_SIZE, egui::Sense::hover());
    let painter = ui.painter_at(rect.expand(8.0));
    let fraction = |hp: f32| (hp / bar.max).clamp(0.0, 1.0);
    let width = |hp: f32| rect.width() * fraction(hp);
    painter.rect_filled(rect, 3.0, egui::Color32::from_black_alpha(140));
    let ghost = egui::Rect::from_min_size(rect.min, egui::vec2(width(bar.ghost), rect.height()));
    painter.rect_filled(ghost, 3.0, WARN.gamma_multiply(0.8));
    let color = if bar.health > 50.0 { GOOD } else { BAD };
    let fill = egui::Rect::from_min_size(rect.min, egui::vec2(width(bar.health), rect.height()));
    painter.rect_filled(fill, 3.0, color);
    if bar.pulse > 0.0 {
        let glow = BAD.gamma_multiply(0.8 * bar.pulse);
        painter.rect_stroke(
            rect.expand(2.0 + 3.0 * bar.pulse),
            4.0,
            egui::Stroke::new(2.0 + 2.0 * bar.pulse, glow),
            egui::StrokeKind::Outside,
        );
    }
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{:.0}", bar.health),
        egui::FontId::proportional(15.0),
        egui::Color32::WHITE,
    );
    ui.label(RichText::new("HP").small());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn the_ghost_waits_then_drains_to_the_health() {
        let mut g = HealthGhost::default();
        g.update(100.0, 0.016);
        assert!(close(g.value(), 100.0));
        g.update(61.0, 0.016);
        assert!(
            close(g.value(), 100.0),
            "holds at the health before the hit"
        );
        g.update(61.0, GHOST_DELAY_SECS - 0.05);
        assert!(close(g.value(), 100.0), "still waiting");
        g.update(61.0, 0.1);
        g.update(61.0, 0.1);
        assert!(
            g.value() < 100.0 && g.value() > 61.0,
            "draining: {}",
            g.value()
        );
        g.update(61.0, 1.0);
        assert!(close(g.value(), 61.0), "never under the health");
    }

    #[test]
    fn a_new_hit_restarts_the_wait_and_healing_snaps() {
        let mut g = HealthGhost::default();
        g.update(100.0, 0.016);
        g.update(70.0, 0.016);
        g.update(70.0, GHOST_DELAY_SECS - 0.1);
        g.update(40.0, 0.016);
        g.update(40.0, GHOST_DELAY_SECS - 0.1);
        assert!(close(g.value(), 100.0), "the second hit restarted the wait");
        // A respawn refills the health: the ghost follows at once.
        g.update(100.0, 0.016);
        assert!(close(g.value(), 100.0));
        g.update(100.0, 1.0);
        assert!(close(g.value(), 100.0));
    }

    #[test]
    fn the_bar_pulses_only_at_low_health() {
        assert!(close(low_hp_pulse(31.0, 0.3), 0.0));
        assert!(close(low_hp_pulse(100.0, 0.125), 0.0));
        let samples: Vec<f32> = (0..20)
            .map(|i| low_hp_pulse(20.0, i as f32 * 0.05))
            .collect();
        assert!(samples.iter().all(|p| (0.0..=1.0).contains(p)));
        assert!(samples.iter().any(|&p| p > 0.9) && samples.iter().any(|&p| p < 0.1));
        assert!(low_hp_pulse(LOW_HP_THRESHOLD, 0.125) > 0.0);
    }
}
