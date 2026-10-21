//! Hand care tab: guided mobility, stretching and posture routines, followed one timed
//! exercise at a time, and posture tips (`aim_core::training::care`).

use crate::i18n::{tr, tr_fmt};
use crate::ui::atoms::big;
use aim_core::training::care::{CREDIT, CareSession, POSTURE_TIPS, ROUTINES};
use aim_core::training::ergonomics::{EVIDENCE_NOTE, STRETCH_DISCLAIMER};
use bevy_egui::egui;

/// Longest frame step the routine clock takes, seconds, so a hidden or stalled window does
/// not skip exercises.
const MAX_STEP_SECS: f64 = 0.5;

/// The routine being followed, if any. Kept for the session, not saved.
#[derive(Debug, Default)]
pub struct CareState {
    session: Option<CareSession>,
    /// `egui` time of the last frame that ran the clock.
    last_time: Option<f64>,
}

/// `secs` as "m:ss", rounded up so the clock shows 0:00 only at the end.
pub fn clock(secs: f32) -> String {
    let whole = secs.max(0.0).ceil() as u32;
    format!("{}:{:02}", whole / 60, whole % 60)
}

/// Whole minutes a routine of `secs` takes, at least 1.
pub fn minutes(secs: f32) -> u32 {
    ((secs / 60.0).round() as u32).max(1)
}

/// The Hand care tab.
pub fn care_panel(ui: &mut egui::Ui, state: &mut CareState) {
    if state.session.is_some() {
        guided(ui, state);
    } else {
        overview(ui, state);
    }
}

fn overview(ui: &mut egui::Ui, state: &mut CareState) {
    ui.heading(tr("Hand care"));
    ui.label(tr(
        "Short guided routines for your wrists, hands and posture. Follow along: each exercise has its own timer.",
    ));
    ui.small(tr(STRETCH_DISCLAIMER));
    ui.add_space(8.0);
    for routine in &ROUTINES {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(big(tr(routine.name), 18.0));
                ui.weak(tr_fmt(
                    "{} · about {} min",
                    &[&tr(routine.when.label()), &minutes(routine.total_secs())],
                ));
            });
            ui.label(tr(routine.purpose));
            if ui.button(tr("Start")).clicked() {
                state.session = Some(CareSession::new(routine));
                state.last_time = None;
            }
        });
        ui.add_space(4.0);
    }
    ui.add_space(8.0);
    ui.heading(tr("Posture while playing"));
    egui::Grid::new("posture_tips")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            for (part, tip) in POSTURE_TIPS {
                ui.strong(tr(part));
                ui.label(tr(tip));
                ui.end_row();
            }
        });
    ui.label(tr(
        "Treat the cause, not just the symptom: if something hurts, look at how long you play, your grip and your posture.",
    ));
    ui.add_space(8.0);
    ui.small(tr(CREDIT));
    ui.small(tr(EVIDENCE_NOTE));
}

fn guided(ui: &mut egui::Ui, state: &mut CareState) {
    let now = ui.input(|i| i.time);
    let dt = state
        .last_time
        .map_or(0.0, |t| (now - t).clamp(0.0, MAX_STEP_SECS));
    state.last_time = Some(now);
    let Some(session) = state.session.as_mut() else {
        return;
    };
    session.tick(dt as f32);
    let routine = session.routine();
    ui.heading(tr(routine.name));
    ui.add(egui::ProgressBar::new(session.progress()).show_percentage());
    ui.add_space(8.0);
    let mut stop = false;
    if session.is_finished() {
        ui.label(big(tr("Done. Well done!"), 22.0));
        ui.label(tr(routine.purpose));
        stop = ui.button(tr("Back to hand care")).clicked();
    } else {
        let step = session.step();
        ui.weak(tr_fmt(
            "Exercise {} of {}",
            &[&(session.step_index() + 1), &routine.steps.len()],
        ));
        ui.label(big(tr(step.name), 24.0));
        if let Some(side) = session.side() {
            ui.label(tr_fmt("Side {} of 2", &[&side]));
        }
        ui.label(tr(step.how));
        ui.add_space(6.0);
        ui.label(big(clock(session.left_secs()), 40.0));
        ui.horizontal(|ui| {
            if ui.button(tr("Previous")).clicked() {
                session.previous();
            }
            let pause = if session.is_paused() {
                "Resume"
            } else {
                "Pause"
            };
            if ui.button(tr(pause)).clicked() {
                session.toggle_pause();
            }
            if ui.button(tr("Next")).clicked() {
                session.skip();
            }
            stop = ui.button(tr("Stop")).clicked();
        });
        ui.add_space(6.0);
        ui.small(tr(STRETCH_DISCLAIMER));
        ui.ctx().request_repaint();
    }
    if stop {
        state.session = None;
        state.last_time = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::i18n::{Lang, tr_in};
    use aim_core::training::care::CareWhen;

    #[test]
    fn the_clock_shows_minutes_and_seconds_rounded_up() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(0.2), "0:01");
        assert_eq!(clock(45.0), "0:45");
        assert_eq!(clock(90.5), "1:31");
        assert_eq!(clock(-3.0), "0:00");
        assert_eq!(minutes(270.0), 5);
        assert_eq!(minutes(10.0), 1);
    }

    #[test]
    fn every_care_text_has_a_spanish_translation() {
        let mut texts = vec![CREDIT];
        for when in [
            CareWhen::BeforePlaying,
            CareWhen::AfterPlaying,
            CareWhen::Daily,
        ] {
            texts.push(when.label());
        }
        for r in &ROUTINES {
            texts.extend([r.name, r.purpose]);
            for s in r.steps {
                texts.extend([s.name, s.how]);
            }
        }
        for (part, tip) in POSTURE_TIPS {
            texts.extend([part, tip]);
        }
        let ui = [
            "Hand care",
            "Short guided routines for your wrists, hands and posture. Follow along: each exercise has its own timer.",
            "{} · about {} min",
            "Start",
            "Posture while playing",
            "Treat the cause, not just the symptom: if something hurts, look at how long you play, your grip and your posture.",
            "Done. Well done!",
            "Back to hand care",
            "Exercise {} of {}",
            "Side {} of 2",
            "Previous",
            "Resume",
            "Pause",
            "Next",
            "Stop",
        ];
        texts.extend(ui);
        let missing: Vec<&str> = texts
            .into_iter()
            .filter(|t| tr_in(Lang::Es, t) == *t)
            .collect();
        assert!(missing.is_empty(), "no Spanish entry: {missing:#?}");
    }
}
