//! Converting a sensitivity between games: 360° distance or monitor-distance matching.
//!
//! Monitor distance p % keeps the mouse travel equal for a flick to a point p % of the
//! way from the screen center to the edge (MDH: horizontal edge, MDV: vertical edge).
//! 0 % is the focal-length limit. Formulas and sources are in
//! `docs/research/sensitivity-games.md`.

use super::{FovAxis, FovModel, Game, Sensitivity};
use crate::fov;
use thiserror::Error;

/// How to carry a sensitivity from one game to another.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Matching {
    /// Same cm/360 in both games.
    Distance360,
    /// Same mouse travel to a point `percent` % of the way to the screen edge on `axis`.
    Monitor { axis: FovAxis, percent: f64 },
}

impl Matching {
    pub const MDH_0: Matching = Matching::Monitor {
        axis: FovAxis::Horizontal,
        percent: 0.0,
    };
    /// A common compromise between 0 % (micro adjustments) and 100 % (flicks to the edge).
    pub const MDH_75: Matching = Matching::Monitor {
        axis: FovAxis::Horizontal,
        percent: 75.0,
    };
    pub const MDH_100: Matching = Matching::Monitor {
        axis: FovAxis::Horizontal,
        percent: 100.0,
    };
    pub const MDV_0: Matching = Matching::Monitor {
        axis: FovAxis::Vertical,
        percent: 0.0,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Error)]
pub enum MatchError {
    #[error("{} has no documented FOV model; only 360° matching is available", .0.display_name())]
    NoFovModel(Game),
    #[error("monitor-distance percentage must be finite and ≥ 0, got {0}")]
    InvalidPercent(f64),
    #[error("FOV must be between 0° and 180°, got {0}")]
    InvalidFov(f64),
    #[error("aspect ratio must be positive, got {0}")]
    InvalidAspect(f64),
    #[error("sensitivity must be positive with a positive DPI")]
    InvalidSensitivity,
}

fn check_fov(fov_deg: f64) -> Result<f64, MatchError> {
    if fov_deg.is_finite() && fov_deg > 0.0 && fov_deg < 180.0 {
        Ok(fov_deg)
    } else {
        Err(MatchError::InvalidFov(fov_deg))
    }
}

/// FOV in degrees on `axis` that a game with `model` shows on a `monitor_aspect`
/// (width / height) display, for the in-game value `setting_deg`.
pub fn effective_fov_deg(
    model: &FovModel,
    setting_deg: f64,
    monitor_aspect: f64,
    axis: FovAxis,
) -> f64 {
    let vertical_deg = match model.axis {
        FovAxis::Horizontal => fov::vertical_from_horizontal(setting_deg, model.reference_aspect),
        FovAxis::Vertical => setting_deg,
    };
    let aspect = model
        .max_aspect
        .map_or(monitor_aspect, |max| monitor_aspect.min(max));
    match axis {
        FovAxis::Horizontal => fov::horizontal_from_vertical(vertical_deg, aspect),
        FovAxis::Vertical => vertical_deg,
    }
}

/// Factor by which degrees per count must scale when the FOV changes from
/// `from_fov_deg` to `to_fov_deg` (both on the same axis), keeping a `percent` %
/// monitor-distance match.
///
/// `ratio = atan(p·tan(to/2)) / atan(p·tan(from/2))`, and `tan(to/2) / tan(from/2)` at 0 %.
pub fn monitor_distance_ratio(from_fov_deg: f64, to_fov_deg: f64, percent: f64) -> f64 {
    let half_from = (from_fov_deg / 2.0).to_radians().tan();
    let half_to = (to_fov_deg / 2.0).to_radians().tan();
    let p = percent / 100.0;
    if p == 0.0 {
        half_to / half_from
    } else {
        (p * half_to).atan() / (p * half_from).atan()
    }
}

/// VALORANT ADS/scope multiplier that gives a `percent` % horizontal monitor match at
/// `zoom`, with the zoomed horizontal FOV modelled as 103° / zoom (see
/// [`fov::valorant_zoomed_vertical_fov_deg`]). 100 % gives 1.0.
pub fn valorant_ads_multiplier(zoom: f64, percent: f64) -> f64 {
    let hip_deg = fov::VALORANT_HFOV_16_9_DEG;
    monitor_distance_ratio(hip_deg, hip_deg / zoom, percent) * zoom
}

/// One side of a conversion: the game and its in-game FOV value (`None` = the default).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Side {
    pub game: Game,
    pub fov_deg: Option<f64>,
}

impl Side {
    pub fn new(game: Game, fov_deg: Option<f64>) -> Self {
        Self { game, fov_deg }
    }

    fn effective_fov_deg(&self, monitor_aspect: f64, axis: FovAxis) -> Result<f64, MatchError> {
        let model = self
            .game
            .fov_model()
            .ok_or(MatchError::NoFovModel(self.game))?;
        let setting = check_fov(self.fov_deg.unwrap_or(model.default_deg))?;
        check_fov(effective_fov_deg(&model, setting, monitor_aspect, axis))
    }
}

/// Converts `source` (played with `source_fov_deg`, `None` = default FOV) into `target`
/// at the same DPI, on a `monitor_aspect` (width / height) display.
pub fn convert(
    source: &Sensitivity,
    source_fov_deg: Option<f64>,
    target: Side,
    matching: Matching,
    monitor_aspect: f64,
) -> Result<Sensitivity, MatchError> {
    if !source.is_valid() {
        return Err(MatchError::InvalidSensitivity);
    }
    let base = source.convert_to(target.game);
    let (axis, percent) = match matching {
        Matching::Distance360 => return Ok(base),
        Matching::Monitor { axis, percent } => (axis, percent),
    };
    if !percent.is_finite() || percent < 0.0 {
        return Err(MatchError::InvalidPercent(percent));
    }
    if !monitor_aspect.is_finite() || monitor_aspect <= 0.0 {
        return Err(MatchError::InvalidAspect(monitor_aspect));
    }
    let from_deg =
        Side::new(source.game, source_fov_deg).effective_fov_deg(monitor_aspect, axis)?;
    let to_deg = target.effective_fov_deg(monitor_aspect, axis)?;
    Ok(Sensitivity {
        value: base.value * monitor_distance_ratio(from_deg, to_deg, percent),
        ..base
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASPECT_16_9: f64 = 16.0 / 9.0;

    fn cs2_to_valorant(matching: Matching) -> f64 {
        let cs2 = Sensitivity {
            game: Game::Cs2,
            value: 1.0,
            dpi: 800,
        };
        convert(
            &cs2,
            None,
            Side::new(Game::Valorant, None),
            matching,
            ASPECT_16_9,
        )
        .unwrap()
        .value
    }

    #[test]
    fn worked_example_cs2_to_valorant() {
        assert!((cs2_to_valorant(Matching::Distance360) - 0.3143).abs() < 1e-4);
        assert!((cs2_to_valorant(Matching::MDH_0) - 0.2963).abs() < 1e-4);
        assert!((cs2_to_valorant(Matching::MDH_100) - 0.3046).abs() < 1e-4);
        let mdh75 = cs2_to_valorant(Matching::MDH_75);
        assert!(
            mdh75 > cs2_to_valorant(Matching::MDH_0) && mdh75 < cs2_to_valorant(Matching::MDH_100),
            "{mdh75}"
        );
    }

    #[test]
    fn cs2_shows_106_26_degrees_on_16_9() {
        let model = Game::Cs2.fov_model().unwrap();
        let h = effective_fov_deg(&model, 90.0, ASPECT_16_9, FovAxis::Horizontal);
        assert!((h - 106.26).abs() < 0.01, "{h}");
    }

    #[test]
    fn valorant_ads_zero_percent_multiplier_for_the_vandal() {
        assert!((valorant_ads_multiplier(1.25, 0.0) - 0.870439).abs() < 1e-4);
        assert!((valorant_ads_multiplier(1.25, 100.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn distance_360_equals_convert_to() {
        let s = Sensitivity::valorant(0.4, 800);
        let out = convert(
            &s,
            None,
            Side::new(Game::Overwatch2, Some(90.0)),
            Matching::Distance360,
            ASPECT_16_9,
        )
        .unwrap();
        assert_eq!(out, s.convert_to(Game::Overwatch2));
    }

    #[test]
    fn same_game_and_fov_is_identity_for_every_method() {
        let s = Sensitivity::valorant(0.4, 800);
        for matching in [
            Matching::MDH_0,
            Matching::MDH_100,
            Matching::MDV_0,
            Matching::Monitor {
                axis: FovAxis::Horizontal,
                percent: 75.0,
            },
        ] {
            let out = convert(
                &s,
                None,
                Side::new(Game::Valorant, None),
                matching,
                ASPECT_16_9,
            )
            .unwrap();
            assert!((out.value - 0.4).abs() < 1e-12, "{matching:?}");
        }
    }

    #[test]
    fn zero_percent_is_the_limit_of_small_percentages() {
        let exact = monitor_distance_ratio(106.26, 103.0, 0.0);
        let near = monitor_distance_ratio(106.26, 103.0, 1e-6);
        assert!((exact - near).abs() < 1e-9);
    }

    #[test]
    fn mdv_177_78_percent_equals_mdh_100_percent_at_16_9() {
        let s = Sensitivity {
            game: Game::Cs2,
            value: 1.0,
            dpi: 800,
        };
        let target = Side::new(Game::Overwatch2, Some(90.0));
        let mdh = convert(&s, None, target, Matching::MDH_100, ASPECT_16_9).unwrap();
        let mdv = convert(
            &s,
            None,
            target,
            Matching::Monitor {
                axis: FovAxis::Vertical,
                percent: 100.0 * ASPECT_16_9,
            },
            ASPECT_16_9,
        )
        .unwrap();
        assert!((mdh.value - mdv.value).abs() < 1e-9);
    }

    #[test]
    fn rainbow_six_uses_a_vertical_fov() {
        let model = Game::RainbowSixSiege.fov_model().unwrap();
        assert_eq!(
            effective_fov_deg(&model, 60.0, ASPECT_16_9, FovAxis::Vertical),
            60.0
        );
    }

    #[test]
    fn valorant_is_pillarboxed_on_ultrawide() {
        let model = Game::Valorant.fov_model().unwrap();
        let h = effective_fov_deg(&model, 103.0, 21.0 / 9.0, FovAxis::Horizontal);
        assert!((h - 103.0).abs() < 1e-9);
    }

    #[test]
    fn games_without_a_fov_model_only_support_360() {
        let s = Sensitivity::valorant(0.4, 800);
        let deadlock = Side::new(Game::Deadlock, None);
        assert_eq!(
            convert(&s, None, deadlock, Matching::MDH_0, ASPECT_16_9),
            Err(MatchError::NoFovModel(Game::Deadlock))
        );
        assert!(convert(&s, None, deadlock, Matching::Distance360, ASPECT_16_9).is_ok());
    }

    #[test]
    fn invalid_inputs_are_errors() {
        let s = Sensitivity::valorant(0.4, 800);
        let target = Side::new(Game::Cs2, None);
        let bad_percent = Matching::Monitor {
            axis: FovAxis::Horizontal,
            percent: -1.0,
        };
        assert_eq!(
            convert(&s, None, target, bad_percent, ASPECT_16_9),
            Err(MatchError::InvalidPercent(-1.0))
        );
        assert_eq!(
            convert(&s, None, target, Matching::MDH_0, 0.0),
            Err(MatchError::InvalidAspect(0.0))
        );
        let ow = Sensitivity {
            game: Game::Overwatch2,
            value: 5.0,
            dpi: 800,
        };
        assert_eq!(
            convert(&ow, Some(200.0), target, Matching::MDH_0, ASPECT_16_9),
            Err(MatchError::InvalidFov(200.0))
        );
        let zero = Sensitivity::valorant(0.0, 800);
        assert_eq!(
            convert(&zero, None, target, Matching::MDH_0, ASPECT_16_9),
            Err(MatchError::InvalidSensitivity)
        );
    }
}
