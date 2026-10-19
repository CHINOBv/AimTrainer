//! The AI coach on the results screen: "Coach's take" (one to three grounded points and an
//! optional drill for the run) and "Ask the coach why" (one mistake, in the mistake
//! player). Both are asked on demand only, run in a background [`Job`], and keep their
//! answer for the result they belong to, so reopening sends nothing again. A rejected or
//! failed reply shows its reason; the rule-based fix and tip stay on screen either way.

use crate::persistence::coach::{self, DebriefRequest, Job};
use aim_ai::CoachConfig;
use aim_core::coach::debrief::{Debrief, MistakeDigest, MistakeWhy, RunDigest};
use bevy_egui::egui;

const TAKE_TITLE: &str = "Coach's take";
const ASK_TAKE: &str = "Ask the coach";
const ASK_TAKE_HINT: &str = "Sends this run's numbers (see \"Preview what is sent\") to your \
     coach provider once. Nothing is sent until you click.";
const ASK_WHY: &str = "Ask the coach why";
const ASK_WHY_HINT: &str = "Sends this mistake's numbers (see \"Preview what is sent\") to \
     your coach provider once.";
const ASK_AGAIN: &str = "Ask again";
const ASKING: &str = "Asking the coach…";
const CANCEL: &str = "Cancel";
const CANCELLED: &str = "Cancelled.";
const FALLBACK: &str = "The rule-based advice above still applies.";
const PREVIEW: &str = "Preview what is sent";
const PLAY: &str = "▶ Play";
const CUE: &str = "Cue:";

/// The Coach's take for one result: its digest, the request and the answer.
pub struct CoachTake {
    digest: RunDigest,
    drill_ids: Vec<String>,
    job: Job<Debrief>,
    outcome: Option<Result<Debrief, String>>,
}

impl CoachTake {
    /// A take not asked yet, for the run in `digest`, offering `drill_ids`.
    pub fn new(digest: RunDigest, drill_ids: Vec<String>) -> Self {
        Self {
            digest,
            drill_ids,
            job: Job::default(),
            outcome: None,
        }
    }

    /// Collect the answer if it arrived. Never blocks.
    pub fn poll(&mut self) {
        self.job.poll();
        if let Some(outcome) = self.job.take() {
            self.outcome = Some(outcome);
        }
    }

    /// Send the digest once. Does nothing while a request runs or after a valid answer.
    pub fn ask(&mut self, config: &CoachConfig) {
        if !config.enabled || matches!(self.outcome, Some(Ok(_))) {
            return;
        }
        let request = DebriefRequest {
            config: config.clone(),
            digest: self.digest.clone(),
            drill_ids: self.drill_ids.clone(),
        };
        if self
            .job
            .spawn("coach-take", move || coach::run_debrief_request(request))
        {
            self.outcome = None;
        }
    }

    /// Stop waiting; the late answer is dropped.
    pub fn cancel(&mut self) {
        self.job.cancel();
    }
}

/// What the player clicked in [`coach_take`].
#[derive(Debug, Clone, PartialEq)]
pub enum TakeAction {
    Ask,
    Cancel,
    /// Play the suggested drill.
    Play(String),
}

/// The Coach's take card. `name_of` gives a scenario's display name.
pub fn coach_take(
    ui: &mut egui::Ui,
    take: &CoachTake,
    name_of: impl Fn(&str) -> Option<String>,
) -> Option<TakeAction> {
    let mut action = None;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.strong(TAKE_TITLE);
        match &take.outcome {
            Some(Ok(debrief)) => {
                for bullet in &debrief.bullets {
                    ui.label(format!("· {}", bullet.text));
                }
                if let Some(drill) = &debrief.drill {
                    let name = name_of(&drill.scenario_id).unwrap_or(drill.scenario_id.clone());
                    ui.horizontal(|ui| {
                        if ui.button(format!("{PLAY}: {name}")).clicked() {
                            action = Some(TakeAction::Play(drill.scenario_id.clone()));
                        }
                    });
                    ui.small(&drill.reason);
                }
            }
            outcome => {
                if let Some(Err(reason)) = outcome {
                    ui.small(reason);
                    ui.small(FALLBACK);
                }
                action = ask_row(
                    ui,
                    take.job.is_pending(),
                    take.job.request().phase() == &coach::Phase::Cancelled,
                    if outcome.is_some() {
                        ASK_AGAIN
                    } else {
                        ASK_TAKE
                    },
                    ASK_TAKE_HINT,
                )
                .map(|ask| match ask {
                    Ask::Ask => TakeAction::Ask,
                    Ask::Cancel => TakeAction::Cancel,
                });
            }
        }
        preview(ui, "coach-take-preview", &take.digest.to_prompt_json());
    });
    action
}

/// "Ask the coach why" for one mistake: the request and the answer.
#[derive(Default)]
pub struct CoachWhy {
    job: Job<MistakeWhy>,
    outcome: Option<Result<MistakeWhy, String>>,
}

impl CoachWhy {
    /// Collect the answer if it arrived. Never blocks.
    pub fn poll(&mut self) {
        self.job.poll();
        if let Some(outcome) = self.job.take() {
            self.outcome = Some(outcome);
        }
    }

    /// Send `digest` once. Does nothing while a request runs or after a valid answer.
    pub fn ask(&mut self, config: &CoachConfig, digest: MistakeDigest) {
        if !config.enabled || matches!(self.outcome, Some(Ok(_))) {
            return;
        }
        let config = config.clone();
        if self
            .job
            .spawn("coach-why", move || coach::run_why_request(config, digest))
        {
            self.outcome = None;
        }
    }

    /// Stop waiting; the late answer is dropped.
    pub fn cancel(&mut self) {
        self.job.cancel();
    }
}

/// What the player clicked in [`coach_why`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhyAction {
    Ask,
    Cancel,
}

/// The "Ask the coach why" section under a mistake's tip; `digest` is what would be sent.
pub fn coach_why(ui: &mut egui::Ui, why: &CoachWhy, digest: &MistakeDigest) -> Option<WhyAction> {
    let mut action = None;
    match &why.outcome {
        Some(Ok(answer)) => {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label(&answer.why);
                ui.horizontal_wrapped(|ui| {
                    ui.strong(CUE);
                    ui.label(&answer.cue);
                });
            });
        }
        outcome => {
            if let Some(Err(reason)) = outcome {
                ui.small(reason);
            }
            action = ask_row(
                ui,
                why.job.is_pending(),
                why.job.request().phase() == &coach::Phase::Cancelled,
                if outcome.is_some() {
                    ASK_AGAIN
                } else {
                    ASK_WHY
                },
                ASK_WHY_HINT,
            )
            .map(|ask| match ask {
                Ask::Ask => WhyAction::Ask,
                Ask::Cancel => WhyAction::Cancel,
            });
        }
    }
    preview(ui, "coach-why-preview", &digest.to_prompt_json());
    action
}

enum Ask {
    Ask,
    Cancel,
}

/// The ask button, or a spinner with Cancel while the request runs.
fn ask_row(
    ui: &mut egui::Ui,
    pending: bool,
    cancelled: bool,
    label: &str,
    hint: &str,
) -> Option<Ask> {
    let mut action = None;
    ui.horizontal(|ui| {
        if pending {
            ui.spinner();
            ui.label(ASKING);
            if ui.button(CANCEL).clicked() {
                action = Some(Ask::Cancel);
            }
        } else {
            if ui.button(label).on_hover_text(hint).clicked() {
                action = Some(Ask::Ask);
            }
            if cancelled {
                ui.small(CANCELLED);
            }
        }
    });
    action
}

/// The exact JSON that is sent, behind a collapsed header.
fn preview(ui: &mut egui::Ui, id: &str, json: &str) {
    egui::CollapsingHeader::new(PREVIEW)
        .id_salt(id)
        .default_open(false)
        .show(ui, |ui| {
            ui.small(format!("{} bytes", json.len()));
            ui.add(egui::Label::new(egui::RichText::new(json).monospace().small()).wrap());
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::coach::debrief::Metrics;
    use aim_core::scenario::Category;

    #[test]
    fn a_disabled_coach_never_starts_a_request() {
        let off = CoachConfig::default();
        assert!(!off.enabled);
        let digest = RunDigest::new("micro-adjust", Category::Flicking, Metrics::default());
        let mut take = CoachTake::new(digest, vec!["micro-adjust".into()]);
        take.ask(&off);
        assert!(
            !take.job.is_pending(),
            "nothing is sent while the coach is off"
        );
        let mut why = CoachWhy::default();
        why.ask(
            &off,
            MistakeDigest {
                scenario_id: "micro-adjust".into(),
                category: Category::Flicking,
                kind: "Overflick".into(),
                hit: false,
                metrics: Metrics::default(),
            },
        );
        assert!(!why.job.is_pending());
    }
}
