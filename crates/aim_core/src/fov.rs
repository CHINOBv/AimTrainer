//! Field-of-view math. VALORANT locks the vertical FOV (≈70.53°) and derives the
//! horizontal FOV from the aspect ratio (Hor+): 103° at 16:9.

/// Horizontal FOV VALORANT uses on a 16:9 display.
pub const VALORANT_HFOV_16_9_DEG: f64 = 103.0;

/// Vertical FOV for a given horizontal FOV and aspect ratio (width / height).
pub fn vertical_from_horizontal(hfov_deg: f64, aspect: f64) -> f64 {
    2.0 * ((hfov_deg.to_radians() / 2.0).tan() / aspect)
        .atan()
        .to_degrees()
}

/// Horizontal FOV for a given vertical FOV and aspect ratio (width / height).
pub fn horizontal_from_vertical(vfov_deg: f64, aspect: f64) -> f64 {
    2.0 * ((vfov_deg.to_radians() / 2.0).tan() * aspect)
        .atan()
        .to_degrees()
}

/// VALORANT's fixed vertical FOV (≈70.5328°).
pub fn valorant_vertical_fov_deg() -> f64 {
    vertical_from_horizontal(VALORANT_HFOV_16_9_DEG, 16.0 / 9.0)
}

/// Widest aspect ratio VALORANT renders; wider displays get a pillarboxed 16:9 image.
pub const VALORANT_MAX_ASPECT: f64 = 16.0 / 9.0;

/// Horizontal FOV VALORANT renders at for the given aspect ratio: Hor+ up to 16:9
/// (narrower displays lose horizontal view), capped at 103° on ultrawide displays, which
/// the game pillarboxes to 16:9.
pub fn valorant_horizontal_fov_deg(aspect: f64) -> f64 {
    horizontal_from_vertical(valorant_vertical_fov_deg(), aspect.min(VALORANT_MAX_ASPECT))
}

/// Viewport size (width, height) VALORANT would use inside a `width`×`height` window:
/// the full window up to 16:9, a centered 16:9 region on wider windows.
pub fn valorant_viewport(width: f64, height: f64) -> (f64, f64) {
    if height > 0.0 && width / height > VALORANT_MAX_ASPECT {
        (height * VALORANT_MAX_ASPECT, height)
    } else {
        (width, height)
    }
}

/// Vertical FOV while zoomed, assuming the horizontal FOV scales linearly with zoom
/// (the model used by sensitivity calculators; Riot does not document it).
pub fn valorant_zoomed_vertical_fov_deg(zoom: f64) -> f64 {
    vertical_from_horizontal(VALORANT_HFOV_16_9_DEG / zoom, 16.0 / 9.0)
}

/// Angular size (degrees) of an object of `size_m` seen from `distance_m`.
pub fn angular_size_deg(size_m: f64, distance_m: f64) -> f64 {
    2.0 * (size_m / (2.0 * distance_m)).atan().to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn valorant_vertical_fov_is_70_53() {
        assert!(close(valorant_vertical_fov_deg(), 70.53));
    }

    #[test]
    fn valorant_is_hor_plus_across_aspect_ratios() {
        assert!(close(valorant_horizontal_fov_deg(16.0 / 9.0), 103.0));
        assert!(close(valorant_horizontal_fov_deg(4.0 / 3.0), 86.63));
        assert!(close(valorant_horizontal_fov_deg(16.0 / 10.0), 97.05));
        // Ultrawide is pillarboxed to 16:9, so the horizontal FOV stays 103°.
        assert!(close(valorant_horizontal_fov_deg(21.0 / 9.0), 103.0));
        assert!(close(valorant_horizontal_fov_deg(32.0 / 9.0), 103.0));
    }

    #[test]
    fn ultrawide_windows_get_a_centered_16_9_viewport() {
        assert_eq!(valorant_viewport(3440.0, 1440.0), (2560.0, 1440.0));
        assert_eq!(valorant_viewport(1920.0, 1080.0), (1920.0, 1080.0));
        assert_eq!(valorant_viewport(1024.0, 768.0), (1024.0, 768.0));
    }

    #[test]
    fn head_at_20m_spans_about_0_72_degrees() {
        assert!(close(angular_size_deg(0.25, 20.0), 0.716));
    }
}
