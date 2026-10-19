//! Arena damage feedback: arcs around the crosshair pointing to where each hit came from,
//! the red screen-edge vignette, and the optional screen punch. Everything here is an
//! overlay; nothing moves the camera.

use bevy::math::Vec3;
use bevy_egui::egui;
use std::f32::consts::{PI, TAU};

/// How long a damage direction arc stays on screen, seconds.
pub const INDICATOR_SECS: f32 = 1.0;
/// Damage that counts as a typical hit: about one Vandal body shot (39). A design
/// reference for scaling the effects, not a game value.
const TYPICAL_HIT_DAMAGE: f32 = 40.0;
/// Weight range of [`damage_weight`]: a chip hit still shows, a huge hit is capped.
const MIN_WEIGHT: f32 = 0.35;
const MAX_WEIGHT: f32 = 1.5;
/// Peak opacity and length of the vignette for a typical hit at 100%: today's flash.
const VIGNETTE_PEAK_ALPHA: f32 = 0.35;
const VIGNETTE_SECS: f32 = 0.25;
/// The vignette never gets more opaque than this, whatever the damage and intensity.
const VIGNETTE_MAX_ALPHA: f32 = 0.6;
/// Screen punch: length, peak offset (points) and oscillation period of the jolt.
pub const PUNCH_SECS: f32 = 0.18;
const PUNCH_POINTS: f32 = 10.0;
const PUNCH_PERIOD_SECS: f32 = 0.09;
/// Distance of the arcs from the crosshair, points.
const ARC_RADIUS: f32 = 90.0;

/// A hit the player took, `age` seconds ago, from a bot whose eye was at `from`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecentHit {
    pub from: Vec3,
    pub damage: f32,
    pub age: f32,
}

/// The hits of the last [`INDICATOR_SECS`], newest last. Several can stack.
#[derive(Debug, Default, Clone)]
pub struct RecentHits {
    hits: Vec<RecentHit>,
}

impl RecentHits {
    pub fn push(&mut self, from: Vec3, damage: f32) {
        self.hits.push(RecentHit {
            from,
            damage,
            age: 0.0,
        });
    }

    /// Age every hit by `dt` seconds and forget those older than [`INDICATOR_SECS`].
    pub fn advance(&mut self, dt: f32) {
        for h in &mut self.hits {
            h.age += dt;
        }
        self.hits.retain(|h| h.age < INDICATOR_SECS);
    }

    pub fn clear(&mut self) {
        self.hits.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &RecentHit> {
        self.hits.iter()
    }

    pub fn newest(&self) -> Option<&RecentHit> {
        self.hits.last()
    }
}

/// How strong a hit of `damage` looks: 1 for a typical hit, clamped to
/// [`MIN_WEIGHT`]..[`MAX_WEIGHT`].
pub fn damage_weight(damage: f32) -> f32 {
    (damage / TYPICAL_HIT_DAMAGE).clamp(MIN_WEIGHT, MAX_WEIGHT)
}

/// Where `from` lies around the player at `eye` looking along `yaw_deg`, as a screen
/// bearing in radians: 0 straight ahead (up on the screen), positive to the right,
/// ±π behind. Height is ignored. A source at the eye reads as straight ahead.
pub fn screen_bearing(eye: Vec3, yaw_deg: f32, from: Vec3) -> f32 {
    let d = from - eye;
    if d.x.abs() < 1e-6 && d.z.abs() < 1e-6 {
        return 0.0;
    }
    // Same convention as `ViewAngles::direction`: yaw 0 looks along −Z, +yaw turns right.
    let world = d.x.atan2(-d.z);
    wrap(world - yaw_deg.to_radians())
}

/// Opacity of a direction arc `age` seconds after a hit of `damage`, at `intensity`
/// (1 = 100%). It fades out linearly over [`INDICATOR_SECS`].
pub fn indicator_alpha(age: f32, damage: f32, intensity: f32) -> f32 {
    let fade = (1.0 - age / INDICATOR_SECS).clamp(0.0, 1.0);
    let strength = 0.45 + 0.55 * damage_weight(damage).min(1.0);
    (fade * strength * intensity.max(0.0)).min(1.0)
}

/// Half the angular length (degrees) and the thickness (points) of a direction arc for a
/// hit of `damage` at `intensity`.
pub fn indicator_shape(damage: f32, intensity: f32) -> (f32, f32) {
    let w = damage_weight(damage);
    let k = intensity.clamp(0.25, 2.0);
    (8.0 + 14.0 * w, (3.0 + 4.0 * w) * k)
}

/// Opacity of the red vignette `age` seconds after a hit of `damage` at `intensity`. A
/// typical hit at 100% matches the old fixed flash: 0.35, fading over 0.25 s.
pub fn vignette_alpha(age: f32, damage: f32, intensity: f32) -> f32 {
    let w = damage_weight(damage);
    let secs = VIGNETTE_SECS * w.max(0.6);
    let peak = (VIGNETTE_PEAK_ALPHA * w * intensity.max(0.0)).min(VIGNETTE_MAX_ALPHA);
    peak * (1.0 - age / secs).max(0.0)
}

/// Signed offset (points) of the screen punch overlay `age` seconds after a hit of
/// `damage` at `intensity`: a damped oscillation that starts at its peak and is 0 from
/// [`PUNCH_SECS`] on.
pub fn punch_offset(age: f32, damage: f32, intensity: f32) -> f32 {
    let envelope = (1.0 - age / PUNCH_SECS).max(0.0);
    let swing = (age * TAU / PUNCH_PERIOD_SECS).cos();
    PUNCH_POINTS * damage_weight(damage).min(1.0) * intensity.max(0.0) * envelope * envelope * swing
}

fn red(alpha: f32) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(220, 30, 30, (alpha.clamp(0.0, 1.0) * 255.0) as u8)
}

fn painter(ctx: &egui::Context, id: &str) -> egui::Painter {
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new(id),
    ))
}

/// An arc around the screen center for every recent hit, pointing at its source as seen
/// from `eye` looking along `yaw_deg`. Bearings follow the view as the player turns.
pub fn draw_indicators(
    ctx: &egui::Context,
    hits: &RecentHits,
    eye: Vec3,
    yaw_deg: f32,
    intensity: f32,
) {
    let painter = painter(ctx, "damage-indicator");
    let center = ctx.content_rect().center();
    for h in hits.iter() {
        let alpha = indicator_alpha(h.age, h.damage, intensity);
        if alpha <= 0.0 {
            continue;
        }
        let bearing = screen_bearing(eye, yaw_deg, h.from);
        let (half_deg, thickness) = indicator_shape(h.damage, intensity);
        let half = half_deg.to_radians();
        let steps = 12;
        let points: Vec<egui::Pos2> = (0..=steps)
            .map(|i| {
                let a = bearing - half + 2.0 * half * i as f32 / steps as f32;
                center + egui::vec2(a.sin(), -a.cos()) * ARC_RADIUS
            })
            .collect();
        painter.add(egui::Shape::line(
            points,
            egui::Stroke::new(thickness, red(alpha)),
        ));
        // A small chevron at the arc's middle makes the direction readable at a glance.
        let dir = egui::vec2(bearing.sin(), -bearing.cos());
        let side = egui::vec2(-dir.y, dir.x);
        let tip = center + dir * (ARC_RADIUS + thickness + 8.0);
        let base = center + dir * (ARC_RADIUS + thickness);
        painter.add(egui::Shape::convex_polygon(
            vec![tip, base + side * 7.0, base - side * 7.0],
            red(alpha),
            egui::Stroke::NONE,
        ));
    }
}

/// The strongest vignette among the recent hits at `intensity`.
pub fn strongest_vignette(hits: &RecentHits, intensity: f32) -> f32 {
    hits.iter()
        .map(|h| vignette_alpha(h.age, h.damage, intensity))
        .fold(0.0, f32::max)
}

/// The red screen-edge vignette at `alpha`.
pub fn draw_vignette(ctx: &egui::Context, alpha: f32) {
    if alpha <= 0.0 {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("damage"),
    ));
    painter.rect_stroke(
        ctx.content_rect().shrink(20.0),
        0.0,
        egui::Stroke::new(40.0, red(alpha)),
        egui::StrokeKind::Inside,
    );
}

/// The screen punch of the newest hit: an edge flash shifted away from the shooter by a
/// damped jolt. Only this overlay moves, never the camera.
pub fn draw_punch(ctx: &egui::Context, hits: &RecentHits, eye: Vec3, yaw_deg: f32, intensity: f32) {
    let Some(h) = hits.newest().filter(|h| h.age < PUNCH_SECS) else {
        return;
    };
    let offset = punch_offset(h.age, h.damage, intensity);
    let bearing = screen_bearing(eye, yaw_deg, h.from);
    // Away from the shooter: the screen "recoils" from the hit.
    let shift = -egui::vec2(bearing.sin(), -bearing.cos()) * offset;
    let envelope = 1.0 - h.age / PUNCH_SECS;
    let rect = ctx.content_rect().translate(shift).shrink(4.0);
    let flash = egui::Color32::from_rgba_unmultiplied(255, 220, 220, (envelope * 120.0) as u8);
    let painter = painter(ctx, "screen-punch");
    painter.rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(6.0, flash),
        egui::StrokeKind::Inside,
    );
    painter.rect_stroke(
        rect.shrink(6.0),
        0.0,
        egui::Stroke::new(28.0, red(0.25 * envelope * intensity.min(1.5))),
        egui::StrokeKind::Inside,
    );
}

/// `a` wrapped to (-π, π].
fn wrap(a: f32) -> f32 {
    let w = (a + PI).rem_euclid(TAU) - PI;
    if w <= -PI { w + TAU } else { w }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn bearings_are_relative_to_the_view_with_right_positive() {
        let eye = Vec3::new(0.0, 1.6, 0.0);
        // Forward is −Z at yaw 0; +X is to the right.
        assert!(close(
            screen_bearing(eye, 0.0, Vec3::new(0.0, 1.6, -5.0)),
            0.0
        ));
        assert!(close(
            screen_bearing(eye, 0.0, Vec3::new(5.0, 1.6, 0.0)),
            FRAC_PI_2
        ));
        assert!(close(
            screen_bearing(eye, 0.0, Vec3::new(-5.0, 3.0, 0.0)),
            -FRAC_PI_2
        ));
        assert!(close(
            screen_bearing(eye, 0.0, Vec3::new(0.0, 1.6, 5.0)).abs(),
            PI
        ));
        // Turned 90° right, the source that was on the right is now ahead.
        assert!(close(
            screen_bearing(eye, 90.0, Vec3::new(5.0, 1.6, 0.0)),
            0.0
        ));
        // Turned 90° left, the player faces −X, so the source at +X is behind.
        assert!(close(
            screen_bearing(eye, -90.0, Vec3::new(5.0, 1.6, 0.0)).abs(),
            PI
        ));
        // Results stay in (−π, π] for any yaw.
        let b = screen_bearing(eye, 720.0 + 45.0, Vec3::new(0.0, 1.6, -5.0));
        assert!(close(b, -PI / 4.0), "{b}");
        // A source at the eye reads as ahead.
        assert!(close(screen_bearing(eye, 30.0, eye), 0.0));
    }

    #[test]
    fn weight_is_one_for_a_typical_hit_and_clamped() {
        assert!(close(damage_weight(40.0), 1.0));
        assert!(close(damage_weight(1.0), MIN_WEIGHT));
        assert!(close(damage_weight(500.0), MAX_WEIGHT));
        assert!(damage_weight(20.0) < damage_weight(30.0));
    }

    #[test]
    fn arcs_fade_over_a_second_and_grow_with_damage_and_intensity() {
        assert!(indicator_alpha(0.0, 40.0, 1.0) > indicator_alpha(0.5, 40.0, 1.0));
        assert!(close(indicator_alpha(INDICATOR_SECS, 40.0, 1.0), 0.0));
        assert!(indicator_alpha(0.0, 80.0, 1.0) > indicator_alpha(0.0, 10.0, 1.0));
        assert!(indicator_alpha(0.0, 40.0, 1.0) > indicator_alpha(0.0, 40.0, 0.5));
        assert!(close(indicator_alpha(0.0, 40.0, 0.0), 0.0));
        assert!(indicator_alpha(0.0, 500.0, 2.0) <= 1.0);
        let (small_len, small_w) = indicator_shape(10.0, 1.0);
        let (big_len, big_w) = indicator_shape(80.0, 1.0);
        assert!(big_len > small_len && big_w > small_w);
        assert!(indicator_shape(40.0, 1.5).1 > indicator_shape(40.0, 0.5).1);
    }

    #[test]
    fn a_typical_hit_keeps_the_old_vignette_and_damage_scales_it() {
        // The old flash: 0.35 at the hit, falling linearly to 0 over 0.25 s.
        assert!(close(vignette_alpha(0.0, 40.0, 1.0), 0.35));
        assert!(close(vignette_alpha(0.125, 40.0, 1.0), 0.175));
        assert!(close(vignette_alpha(0.25, 40.0, 1.0), 0.0));
        assert!(vignette_alpha(0.0, 10.0, 1.0) < 0.35);
        assert!(vignette_alpha(0.0, 150.0, 1.0) > 0.35);
        assert!(vignette_alpha(0.0, 150.0, 1.0) <= VIGNETTE_MAX_ALPHA);
        assert!(close(vignette_alpha(0.0, 40.0, 0.5), 0.175));
        assert!(close(vignette_alpha(0.0, 40.0, 0.0), 0.0));
    }

    #[test]
    fn the_punch_starts_at_its_peak_and_settles() {
        let peak = punch_offset(0.0, 40.0, 1.0);
        assert!(close(peak, PUNCH_POINTS));
        assert!(punch_offset(0.1, 40.0, 1.0).abs() < peak);
        assert!(close(punch_offset(PUNCH_SECS, 40.0, 1.0), 0.0));
        assert!(close(punch_offset(1.0, 40.0, 1.0), 0.0));
        assert!(punch_offset(0.0, 10.0, 1.0) < peak);
        assert!(close(punch_offset(0.0, 40.0, 0.0), 0.0));
    }

    #[test]
    fn hits_stack_and_expire() {
        let mut hits = RecentHits::default();
        hits.push(Vec3::X, 40.0);
        hits.advance(0.6);
        hits.push(Vec3::Z, 20.0);
        assert_eq!(hits.iter().count(), 2);
        assert!(close(
            strongest_vignette(&hits, 1.0),
            vignette_alpha(0.0, 20.0, 1.0)
        ));
        hits.advance(0.5);
        assert_eq!(hits.iter().count(), 1);
        assert_eq!(hits.newest().map(|h| h.damage), Some(20.0));
        hits.clear();
        assert!(hits.newest().is_none());
    }
}
