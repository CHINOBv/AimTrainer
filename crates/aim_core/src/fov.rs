//! Field-of-view math. VALORANT locks the vertical FOV (≈70.53°) and derives the
//! horizontal FOV from the aspect ratio (Hor+): 103° at 16:9.

use crate::math::{Vec3, ViewAngles};

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

/// Margin (degrees) kept between an "on-screen" spawn and the edge of VALORANT's view, so
/// the target is clearly visible and not clipped by the screen border. A design choice,
/// not a VALORANT constant.
pub const ON_SCREEN_MARGIN_DEG: f64 = 4.0;

/// Largest horizontal angle (degrees, measured on screen from the crosshair) at which an
/// on-screen spawn may appear: half of VALORANT's 103° minus [`ON_SCREEN_MARGIN_DEG`].
pub fn valorant_on_screen_half_h_deg() -> f64 {
    VALORANT_HFOV_16_9_DEG / 2.0 - ON_SCREEN_MARGIN_DEG
}

/// Largest vertical angle (degrees, measured on screen from the crosshair) at which an
/// on-screen spawn may appear: half of VALORANT's ≈70.53° minus [`ON_SCREEN_MARGIN_DEG`].
pub fn valorant_on_screen_half_v_deg() -> f64 {
    valorant_vertical_fov_deg() / 2.0 - ON_SCREEN_MARGIN_DEG
}

/// Where `point` lies relative to a camera at `eye` looking along `view`, whose view spans
/// `half_h_deg` to each side and `half_v_deg` up and down (half-angles of a rectilinear
/// projection).
///
/// Returns `None` when the point is inside the view. Otherwise returns the screen-space
/// direction from the crosshair towards the point, in radians: 0 points right and the
/// angle grows counter-clockwise (π/2 is up). Points behind the camera point the way the
/// shorter turn goes; a point straight behind reports 0 (right).
pub fn offscreen_bearing(
    view: ViewAngles,
    eye: Vec3,
    point: Vec3,
    half_h_deg: f32,
    half_v_deg: f32,
) -> Option<f32> {
    let forward = view.direction();
    let (sy, cy) = view.yaw_deg.to_radians().sin_cos();
    let right = Vec3::new(cy, 0.0, sy);
    let up = right.cross(forward);
    let d = point - eye;
    let (x, y, z) = (d.dot(right), d.dot(up), d.dot(forward));
    let inside = z > 0.0
        && x.abs() <= z * half_h_deg.to_radians().tan()
        && y.abs() <= z * half_v_deg.to_radians().tan();
    (!inside).then(|| y.atan2(x))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Vec3, ViewAngles};

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
    fn on_screen_limits_are_valorant_half_angles_minus_the_margin() {
        assert!(close(ON_SCREEN_MARGIN_DEG, 4.0));
        assert!(close(valorant_on_screen_half_h_deg(), 51.5 - 4.0));
        assert!(close(valorant_on_screen_half_v_deg(), 70.5328 / 2.0 - 4.0));
    }

    fn bearing(view: ViewAngles, d_yaw: f32, d_pitch: f32) -> Option<f32> {
        let eye = Vec3::new(1.0, 1.6, -2.0);
        let point = eye + view.rotated(d_yaw, d_pitch).direction() * 10.0;
        offscreen_bearing(view, eye, point, 51.5, 35.27)
    }

    #[test]
    fn points_inside_the_view_have_no_bearing() {
        let view = ViewAngles::new(30.0, 10.0);
        assert_eq!(bearing(view, 0.0, 0.0), None);
        assert_eq!(bearing(view, 45.0, 0.0), None);
        assert_eq!(bearing(view, -20.0, -25.0), None);
    }

    #[test]
    fn off_screen_bearing_points_towards_the_target() {
        use std::f32::consts::{FRAC_PI_2, PI};
        let view = ViewAngles::new(-60.0, 0.0);
        let right = bearing(view, 70.0, 0.0).expect("right is off-screen");
        assert!(right.abs() < 1e-3, "{right}");
        let left = bearing(view, -70.0, 0.0).expect("left is off-screen");
        assert!((left.abs() - PI).abs() < 1e-3, "{left}");
        let up = bearing(view, 0.0, 50.0).expect("up is off-screen");
        assert!((up - FRAC_PI_2).abs() < 1e-3, "{up}");
        let down = bearing(view, 0.0, -50.0).expect("down is off-screen");
        assert!((down + FRAC_PI_2).abs() < 1e-3, "{down}");
        // Behind and a bit to the right: turn right.
        let behind = bearing(view, 160.0, 0.0).expect("behind is off-screen");
        assert!(behind.abs() < 1e-3, "{behind}");
    }

    #[test]
    fn a_point_beyond_the_horizontal_edge_is_off_screen_even_when_level() {
        // 52° right is just past the 51.5° half-angle.
        assert!(bearing(ViewAngles::new(0.0, 0.0), 52.0, 0.0).is_some());
        assert!(bearing(ViewAngles::new(0.0, 0.0), 51.0, 0.0).is_none());
    }

    #[test]
    fn head_at_20m_spans_about_0_72_degrees() {
        assert!(close(angular_size_deg(0.25, 20.0), 0.716));
    }
}
