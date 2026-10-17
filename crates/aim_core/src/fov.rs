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

/// Horizontal FOV VALORANT renders at for the given aspect ratio.
pub fn valorant_horizontal_fov_deg(aspect: f64) -> f64 {
    horizontal_from_vertical(valorant_vertical_fov_deg(), aspect)
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
        assert!(close(valorant_horizontal_fov_deg(21.0 / 9.0), 117.57));
    }

    #[test]
    fn head_at_20m_spans_about_0_72_degrees() {
        assert!(close(angular_size_deg(0.25, 20.0), 0.716));
    }
}
