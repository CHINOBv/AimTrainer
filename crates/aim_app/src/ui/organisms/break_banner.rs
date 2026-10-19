//! Non-blocking break banner: a due break with the stretch list, or the weekly volume cap.
//! Shown in the menu and on the results screen, never during a run.

use crate::i18n::{tr, tr_fmt, tr_named};
use crate::ui::atoms::WARN;
use aim_core::training::ergonomics::{EVIDENCE_NOTE, STRETCH_DISCLAIMER, STRETCHES};
use bevy_egui::egui::{self, Align2};

/// What the banner shows.
pub struct BannerModel {
    /// Minutes of continuous play, when a break is due.
    pub break_due: Option<f32>,
    /// Suggested break length, minutes.
    pub break_minutes: u32,
    /// `(minutes this week, weekly cap)` when the cap is reached and not dismissed.
    pub weekly: Option<(f32, u32)>,
}

/// What the player chose in the banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerAction {
    /// Hide the break prompt until another full interval of play.
    Dismiss,
    /// Hide the break prompt for a few minutes of play.
    Snooze,
    /// Hide the weekly-cap note for this session.
    DismissWeekly,
}

/// The break line for `minutes` of continuous play.
pub fn break_text(minutes: f32, break_minutes: u32) -> String {
    tr_named(
        "Break time: {minutes} min of continuous play. Take about {break_minutes} min away from the mouse.",
        &[
            ("minutes", &format!("{minutes:.0}")),
            ("break_minutes", &break_minutes),
        ],
    )
}

/// The weekly-cap line.
pub fn weekly_text(minutes: f32, cap: u32) -> String {
    tr_named(
        "{minutes} min of aim training in the last 7 days, at or above your weekly cap of {cap} min. More volume this week adds little; consider lighter days.",
        &[("minutes", &format!("{minutes:.0}")), ("cap", &cap)],
    )
}

pub fn break_banner(ctx: &egui::Context, m: &BannerModel) -> Option<BannerAction> {
    let mut action = None;
    egui::Window::new(tr("Break reminder"))
        .id(egui::Id::new("break_reminder"))
        .anchor(Align2::CENTER_TOP, [0.0, 36.0])
        .collapsible(true)
        .resizable(false)
        .default_width(460.0)
        .show(ctx, |ui| {
            if let Some(minutes) = m.break_due {
                ui.colored_label(WARN, break_text(minutes, m.break_minutes));
                egui::CollapsingHeader::new(tr("Stretches"))
                    .id_salt("break_stretches")
                    .default_open(false)
                    .show(ui, |ui| {
                        for s in &STRETCHES {
                            ui.label(format!("• {} ({} s): {}", tr(s.name), s.secs, tr(s.how)));
                        }
                        ui.small(tr(STRETCH_DISCLAIMER));
                    });
                ui.small(tr(EVIDENCE_NOTE));
                ui.horizontal(|ui| {
                    if ui.button(tr("Dismiss")).clicked() {
                        action = Some(BannerAction::Dismiss);
                    }
                    if ui
                        .button(tr_fmt(
                            "Snooze {} min",
                            &[&format!("{:.0}", crate::ui::pages::breaks::SNOOZE_MINUTES)],
                        ))
                        .clicked()
                    {
                        action = Some(BannerAction::Snooze);
                    }
                });
            }
            if let Some((minutes, cap)) = m.weekly {
                if m.break_due.is_some() {
                    ui.separator();
                }
                ui.label(weekly_text(minutes, cap));
                if ui.button(tr("Dismiss weekly note")).clicked() {
                    action = Some(BannerAction::DismissWeekly);
                }
            }
        });
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_lines_show_the_numbers_behind_them() {
        assert_eq!(
            break_text(47.4, 5),
            "Break time: 47 min of continuous play. Take about 5 min away from the mouse."
        );
        assert!(weekly_text(431.0, 420).starts_with("431 min of aim training in the last 7 days"));
    }

    #[test]
    fn the_disclaimer_makes_no_medical_claim() {
        assert!(STRETCH_DISCLAIMER.contains("not medical advice"));
        assert!(EVIDENCE_NOTE.contains("no trial has shown"));
    }

    #[test]
    fn banner_lines_read_in_spanish() {
        use aim_core::i18n::{Lang, tr_fmt_in, tr_in};
        assert_eq!(
            tr_in(Lang::Es, "Break reminder"),
            "Recordatorio de descanso"
        );
        assert_eq!(
            tr_fmt_in(Lang::Es, "Snooze {} min", &[&10]),
            "Posponer 10 min"
        );
        assert_eq!(
            tr_in(Lang::Es, STRETCHES[0].name),
            "Estiramiento de flexores de muñeca"
        );
    }
}
