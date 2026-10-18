//! Sequential colour ramp for the replay viewer: crosshair speed on the trail and shot
//! density on the heatmap.
//!
//! The stops are the upper three quarters of matplotlib's viridis map (blue, teal, green,
//! yellow): perceptually uniform, readable with the common colour-vision deficiencies, and
//! bright enough on the dark theme. Its darkest purple is left out because it vanishes on a
//! dark background. The input is a `0..=1` position computed in the core
//! (`aim_core::analytics::path::speed_scale`, or a count over the largest count).

use bevy_egui::egui::Color32;

/// Ramp stops, evenly spaced from 0 to 1 (viridis at 0.25, 0.5, 0.75 and 1.0).
const STOPS: [(u8, u8, u8); 4] = [(59, 82, 139), (33, 145, 140), (94, 201, 98), (253, 231, 37)];

/// Colour at `t` along the ramp; `t` is clamped to `0..=1` and NaN reads as 0.
pub fn ramp_color(t: f32) -> Color32 {
    let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
    let scaled = t * (STOPS.len() - 1) as f32;
    let i = (scaled as usize).min(STOPS.len() - 2);
    let f = scaled - i as f32;
    let (a, b) = (STOPS[i], STOPS[i + 1]);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f).round() as u8;
    Color32::from_rgb(mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(c: Color32) -> f32 {
        0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32
    }

    #[test]
    fn the_ramp_runs_from_blue_to_yellow_and_clamps() {
        assert_eq!(ramp_color(0.0), Color32::from_rgb(59, 82, 139));
        assert_eq!(ramp_color(1.0), Color32::from_rgb(253, 231, 37));
        assert_eq!(ramp_color(-3.0), ramp_color(0.0));
        assert_eq!(ramp_color(9.0), ramp_color(1.0));
        assert_eq!(ramp_color(f32::NAN), ramp_color(0.0));
    }

    #[test]
    fn brightness_rises_along_the_ramp() {
        let steps: Vec<f32> = (0..=20)
            .map(|k| luminance(ramp_color(k as f32 / 20.0)))
            .collect();
        assert!(steps.windows(2).all(|w| w[1] > w[0]), "{steps:?}");
    }
}
