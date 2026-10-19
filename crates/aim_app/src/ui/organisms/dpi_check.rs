//! "Measure DPI": move the mouse a measured distance along a ruler while holding Shift; the
//! raw counts give the mouse's real DPI (`aim_core::sensitivity::dpi_check`), its deviation
//! from the configured DPI, and an optional corrected sensitivity.

use crate::i18n::{tr, tr_fmt, tr_named};
use crate::persistence::settings::Settings;
use crate::ui::atoms::{ERROR, GOOD, WARN};
use aim_core::sensitivity::dpi_check::{
    Distance, DpiCheck, DpiCheckError, MIN_DISTANCE_CM, corrected_sensitivity, measure,
};
use bevy_egui::egui;

/// Name of the key held while moving, as shown to the player.
pub const MEASURE_KEY: &str = "Shift";
/// Ruler distance suggested at first, cm.
pub const DEFAULT_DISTANCE_CM: f64 = 10.0;

/// Raw x counts counted while the measure key is held, plus the helper's form.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DpiMeter {
    /// The helper is open and counting while the key is held.
    pub armed: bool,
    /// Signed raw x counts of the current or last hold.
    pub x: f64,
    /// The key was held on the previous frame.
    held: bool,
    /// Distance moved along the ruler, cm.
    pub distance_cm: f64,
    /// "Apply" was clicked once and waits for confirmation.
    pub confirm_apply: bool,
}

impl Default for DpiMeter {
    fn default() -> Self {
        Self {
            armed: false,
            x: 0.0,
            held: false,
            distance_cm: DEFAULT_DISTANCE_CM,
            confirm_apply: false,
        }
    }
}

impl DpiMeter {
    /// Feed one frame: `held` is the measure key's state and `dx` the frame's raw x counts.
    /// A new hold starts a new count; releasing the key keeps the count.
    pub fn update(&mut self, held: bool, dx: f64) {
        if !self.armed {
            self.held = false;
            return;
        }
        if held && !self.held {
            self.x = 0.0;
            self.confirm_apply = false;
        }
        if held {
            self.x += dx;
        }
        self.held = held;
    }

    /// Start or Stop: Start begins a new reading; Stop keeps the last one so it can be
    /// applied.
    pub fn toggle(&mut self) {
        if self.armed {
            self.armed = false;
            self.held = false;
        } else {
            *self = DpiMeter {
                armed: true,
                distance_cm: self.distance_cm,
                ..DpiMeter::default()
            };
        }
    }

    /// Whether the key is held right now (counting).
    pub fn holding(&self) -> bool {
        self.held
    }

    /// Counts moved, in either direction.
    pub fn counts(&self) -> u64 {
        self.x.abs().round() as u64
    }

    /// The measurement against the configured `nominal_dpi`.
    pub fn check(&self, nominal_dpi: u32) -> Result<DpiCheck, DpiCheckError> {
        measure(self.counts(), Distance::Cm(self.distance_cm), nominal_dpi)
    }
}

/// One line for a measurement.
pub fn check_text(c: &DpiCheck) -> String {
    tr_named(
        "Real DPI ≈ {real} (set to {set}): {deviation}% deviation",
        &[
            ("real", &format!("{:.0}", c.real_dpi)),
            ("set", &c.nominal_dpi),
            ("deviation", &format!("{:+.1}", c.deviation_pct)),
        ],
    )
}

/// The helper's body. Changes `settings.sensitivity` only after a confirmed "Apply".
pub fn dpi_check(ui: &mut egui::Ui, settings: &mut Settings, meter: &mut DpiMeter) {
    let s = settings.sensitivity;
    ui.label(tr_named(
        "1. Lay a ruler next to the mouse.  2. Press Start.  3. Hold {key}, move the mouse the distance below to the right along the ruler, then release {key}.",
        &[("key", &MEASURE_KEY)],
    ));
    ui.horizontal(|ui| {
        ui.label(tr("Distance moved"));
        ui.add(
            egui::DragValue::new(&mut meter.distance_cm)
                .speed(0.1)
                .range(MIN_DISTANCE_CM..=60.0)
                .suffix(" cm"),
        );
        if ui
            .button(tr(if meter.armed { "Stop" } else { "Start" }))
            .clicked()
        {
            meter.toggle();
        }
    });
    if meter.armed {
        let status = if meter.holding() {
            tr_fmt("Counting… {} counts", &[&meter.counts()])
        } else {
            tr_named(
                "Hold {key} and move. Last count: {count}",
                &[("key", &MEASURE_KEY), ("count", &meter.counts())],
            )
        };
        ui.colored_label(WARN, status);
    }
    if meter.counts() == 0 {
        ui.small(tr("Expect a match within a few %. A large mismatch means a wrong DPI value or OS acceleration."));
        return;
    }
    match meter.check(s.dpi) {
        Ok(c) => {
            let color = if c.deviation_pct.abs() < 3.0 {
                GOOD
            } else {
                WARN
            };
            ui.colored_label(color, check_text(&c));
            let fixed = corrected_sensitivity(s, c.real_dpi);
            if !meter.confirm_apply {
                if ui
                    .button(tr("Apply corrected sensitivity…"))
                    .on_hover_text(tr("Stores the real DPI and scales the sensitivity so your cm/360 stays the same."))
                    .clicked()
                {
                    meter.confirm_apply = true;
                }
            } else {
                ui.label(tr_named(
                    "Change to {value} @ {dpi} DPI (keeps {cm} cm/360)? Set the same sensitivity in VALORANT, or the trainer will no longer match the game.",
                    &[
                        ("value", &format!("{:.4}", fixed.value)),
                        ("dpi", &fixed.dpi),
                        ("cm", &format!("{:.2}", fixed.cm_per_360())),
                    ],
                ));
                ui.horizontal(|ui| {
                    if ui.button(tr("Apply")).clicked() {
                        settings.sensitivity = fixed;
                        meter.confirm_apply = false;
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        meter.confirm_apply = false;
                    }
                });
            }
        }
        Err(e) => {
            ui.colored_label(ERROR, e.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn armed() -> DpiMeter {
        DpiMeter {
            armed: true,
            ..DpiMeter::default()
        }
    }

    #[test]
    fn counts_only_while_armed_and_the_key_is_held() {
        let mut m = DpiMeter::default();
        m.update(true, 500.0);
        assert_eq!(m.counts(), 0, "not armed");
        let mut m = armed();
        m.update(false, 500.0);
        assert_eq!(m.counts(), 0, "key not held");
        m.update(true, 1000.0);
        m.update(true, 2149.6);
        assert!(m.holding());
        m.update(false, 999.0);
        assert!(!m.holding());
        assert_eq!(m.counts(), 3150, "released: the count is kept");
    }

    #[test]
    fn stop_keeps_the_reading_and_start_begins_a_new_one() {
        let mut m = DpiMeter::default();
        m.toggle();
        m.update(true, 800.0);
        m.update(false, 0.0);
        m.toggle();
        assert!(!m.armed);
        assert_eq!(m.counts(), 800, "the reading stays for Apply");
        m.update(true, 50.0);
        assert_eq!(m.counts(), 800, "a stopped meter does not count");
        m.toggle();
        assert!(m.armed);
        assert_eq!(m.counts(), 0, "Start begins a new reading");
    }

    #[test]
    fn a_new_hold_starts_a_new_count_and_direction_does_not_matter() {
        let mut m = armed();
        m.update(true, 400.0);
        m.update(false, 0.0);
        m.update(true, -100.0);
        m.update(true, -200.0);
        assert_eq!(m.counts(), 300);
    }

    #[test]
    fn ten_centimeters_at_800_dpi_reads_800() {
        // The previous meter's case: 800 DPI over 10 cm ≈ 3149.6 counts.
        let mut m = armed();
        m.update(true, -3149.6);
        let c = m.check(800).unwrap();
        assert!((c.real_dpi - 800.0).abs() < 0.2, "{c:?}");
        assert!(c.deviation_pct.abs() < 0.1);
    }

    #[test]
    fn tools_text_is_spanish_in_spanish_and_unchanged_in_english() {
        use aim_core::i18n::{Lang, tr_in, tr_named_in};
        let reading = |lang| {
            tr_named_in(
                lang,
                "Real DPI ≈ {real} (set to {set}): {deviation}% deviation",
                &[("real", &"850"), ("set", &800), ("deviation", &"+6.2")],
            )
        };
        assert_eq!(
            reading(Lang::Es),
            "DPI real ≈ 850 (configurado en 800): +6.2% de desviación"
        );
        assert_eq!(
            reading(Lang::En),
            "Real DPI ≈ 850 (set to 800): +6.2% deviation"
        );
        assert_eq!(
            tr_in(Lang::Es, "Sensitivity converter"),
            "Conversor de sensibilidad"
        );
        assert_eq!(tr_in(Lang::Es, "System check"), "Revisión del sistema");
        assert_eq!(tr_in(Lang::Es, "Profile code"), "Código de perfil");
        assert_eq!(tr_in(Lang::En, "Profile code"), "Profile code");
    }

    #[test]
    fn the_deviation_and_its_errors_are_reported() {
        let mut m = armed();
        m.update(true, 3346.0); // about 850 DPI over 10 cm
        let c = m.check(800).unwrap();
        assert_eq!(
            check_text(&c),
            "Real DPI ≈ 850 (set to 800): +6.2% deviation"
        );
        assert_eq!(armed().check(800), Err(DpiCheckError::NoCounts));
        m.distance_cm = 2.0;
        assert!(matches!(
            m.check(800),
            Err(DpiCheckError::DistanceTooShort(_))
        ));
    }
}
