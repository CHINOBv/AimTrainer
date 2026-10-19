//! DPI deviation test: a mouse's real DPI from a measured movement along a ruler.
//!
//! Mice often report more or fewer counts per inch than their nominal DPI. The player moves
//! the mouse a measured distance (at least [`MIN_DISTANCE_CM`]) while the trainer counts the
//! raw counts; [`measure`] turns that into the real DPI and its deviation from the nominal
//! value, and [`corrected_sensitivity`] keeps the cm/360 the player expects. The procedure
//! is described in `docs/training-science.md` ("DPI deviation test").

use super::{CM_PER_INCH, Sensitivity};

/// Shortest movement accepted: a ±1 mm reading error stays within ±2% at this distance.
pub const MIN_DISTANCE_CM: f64 = 5.0;

/// A measured physical distance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Distance {
    /// Centimeters.
    Cm(f64),
    /// Inches.
    Inches(f64),
}

impl Distance {
    /// The distance in centimeters.
    pub fn cm(self) -> f64 {
        match self {
            Distance::Cm(cm) => cm,
            Distance::Inches(inches) => inches * CM_PER_INCH,
        }
    }
}

/// Why a DPI measurement was rejected.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum DpiCheckError {
    /// No counts were recorded during the movement.
    #[error("no mouse counts were recorded; move the mouse along the ruler")]
    NoCounts,
    /// The distance is not a finite number.
    #[error("the measured distance is not a number")]
    InvalidDistance,
    /// The distance is shorter than [`MIN_DISTANCE_CM`]; carries it in centimeters.
    #[error("move at least {MIN_DISTANCE_CM} cm (measured {0:.1} cm)")]
    DistanceTooShort(f64),
    /// The nominal DPI is zero.
    #[error("the nominal DPI must be above 0")]
    InvalidNominal,
}

/// Result of a DPI deviation test.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DpiCheck {
    /// Counts per inch the mouse really reports.
    pub real_dpi: f64,
    /// DPI the mouse is set to (its software or label).
    pub nominal_dpi: u32,
    /// `(real - nominal) / nominal * 100`: positive when the mouse reports more counts.
    pub deviation_pct: f64,
}

/// The real DPI from `counts` moved over `distance`, compared with `nominal_dpi`.
pub fn measure(
    counts: u64,
    distance: Distance,
    nominal_dpi: u32,
) -> Result<DpiCheck, DpiCheckError> {
    if counts == 0 {
        return Err(DpiCheckError::NoCounts);
    }
    let cm = distance.cm();
    if !cm.is_finite() {
        return Err(DpiCheckError::InvalidDistance);
    }
    if cm < MIN_DISTANCE_CM {
        return Err(DpiCheckError::DistanceTooShort(cm));
    }
    if nominal_dpi == 0 {
        return Err(DpiCheckError::InvalidNominal);
    }
    let real_dpi = counts as f64 / (cm / CM_PER_INCH);
    let nominal = f64::from(nominal_dpi);
    Ok(DpiCheck {
        real_dpi,
        nominal_dpi,
        deviation_pct: (real_dpi - nominal) / nominal * 100.0,
    })
}

/// The sensitivity that gives the cm/360 the player expects from `current` (computed with
/// its nominal DPI) on a mouse that really reports `real_dpi`. The DPI becomes the rounded
/// real DPI and the value scales so `cm_per_360` is unchanged.
pub fn corrected_sensitivity(current: Sensitivity, real_dpi: f64) -> Sensitivity {
    // `with_dpi` keeps cm/360 for the DPI stored; at least 1 so a tiny reading never divides by 0.
    current.with_dpi((real_dpi.round() as u32).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn real_dpi_and_deviation_come_from_counts_over_the_distance() {
        // 4 inches at 850 counts per inch on an 800 DPI mouse.
        let c = measure(3400, Distance::Inches(4.0), 800).unwrap();
        assert!(close(c.real_dpi, 850.0, 1e-9), "{c:?}");
        assert_eq!(c.nominal_dpi, 800);
        assert!(close(c.deviation_pct, 6.25, 1e-9), "{c:?}");
        // The same movement in centimeters.
        let cm = measure(3400, Distance::Cm(4.0 * 2.54), 800).unwrap();
        assert!(close(cm.real_dpi, 850.0, 1e-9));
        // A mouse that reports fewer counts deviates negatively.
        let low = measure(1520, Distance::Inches(2.0), 800).unwrap();
        assert!(close(low.deviation_pct, -5.0, 1e-9), "{low:?}");
    }

    #[test]
    fn invalid_measurements_are_rejected() {
        assert_eq!(
            measure(0, Distance::Cm(10.0), 800),
            Err(DpiCheckError::NoCounts)
        );
        assert_eq!(
            measure(500, Distance::Cm(4.9), 800),
            Err(DpiCheckError::DistanceTooShort(4.9))
        );
        assert!(
            measure(500, Distance::Cm(5.0), 800).is_ok(),
            "5 cm is enough"
        );
        assert_eq!(
            measure(500, Distance::Inches(f64::NAN), 800),
            Err(DpiCheckError::InvalidDistance)
        );
        assert_eq!(
            measure(500, Distance::Cm(f64::INFINITY), 800),
            Err(DpiCheckError::InvalidDistance)
        );
        assert_eq!(
            measure(500, Distance::Cm(10.0), 0),
            Err(DpiCheckError::InvalidNominal)
        );
    }

    #[test]
    fn corrected_sensitivity_keeps_the_expected_cm_per_360() {
        let current = Sensitivity::valorant(0.4, 800);
        let expected = current.cm_per_360();
        let fixed = corrected_sensitivity(current, 850.0);
        assert_eq!(fixed.dpi, 850);
        assert!(close(fixed.value, 0.4 * 800.0 / 850.0, 1e-12), "{fixed:?}");
        assert!(close(fixed.cm_per_360(), expected, 1e-9));
        // Physically, at the true (unrounded) DPI the travel stays within rounding error.
        let fractional = corrected_sensitivity(current, 851.4);
        assert_eq!(fractional.dpi, 851);
        let physical = 360.0 * 2.54 / (fractional.deg_per_count() * 851.4);
        assert!((physical / expected - 1.0).abs() < 1e-3, "{physical}");
    }
}
