//! Potential score on the results card: the one fix with the largest estimated gain,
//! shown above the advice list, with the assumption behind the estimate.
//!
//! [`PotentialLine`] is plain text built from the core's
//! [`PotentialTip`](aim_core::analytics::potential::PotentialTip); the view only draws it.

use crate::i18n::tr_fmt;
use crate::ui::atoms::GOOD;
use aim_core::analytics::potential::PotentialTip;
use bevy_egui::egui;

#[derive(Debug, Clone, PartialEq)]
pub struct PotentialLine {
    /// "Fix overshoot → about 104 points (+4%)".
    pub headline: String,
    /// "Estimate: if your 4 overshoots settled like your clean flicks (median 100 ms)."
    pub assumption: String,
}

impl PotentialLine {
    pub fn new(tip: &PotentialTip) -> Self {
        Self {
            headline: tr_fmt(
                "Fix {} → about {} points (+{}%)",
                &[
                    &tip.gain.fix.label(),
                    &format!("{:.0}", tip.potential_score()),
                    &format!("{:.0}", tip.gain_share() * 100.0),
                ],
            ),
            assumption: tr_fmt("Estimate: {}.", &[&tip.gain.assumption()]),
        }
    }
}

pub fn potential_tip(ui: &mut egui::Ui, line: &PotentialLine) {
    ui.colored_label(GOOD, line.headline.as_str());
    ui.small(line.assumption.as_str());
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::analytics::potential::{Fix, Gain};

    #[test]
    fn the_line_states_the_fix_the_potential_score_and_the_assumption() {
        let tip = PotentialTip {
            score: 100.0,
            gain: Gain {
                fix: Fix::Overshoot,
                events: 4,
                saved_secs: 2.0,
                penalty_points: 0.0,
                points: 4.0,
                reference_secs: Some(0.1),
            },
        };
        let line = PotentialLine::new(&tip);
        assert_eq!(line.headline, "Fix overshoot → about 104 points (+4%)");
        assert_eq!(
            line.assumption,
            "Estimate: if your 4 overshoots settled like your clean flicks (median 100 ms)."
        );
    }
}
