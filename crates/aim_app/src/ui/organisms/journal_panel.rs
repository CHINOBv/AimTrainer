//! Ranked journal on the Progress page: a one-tap entry after each match, the losing
//! streak, results split by readiness and fatigue, and the tilt reset (a paced-breathing
//! timer, then easy smooth tracking) after two losses in a row.

use crate::i18n::{tr, tr_fmt, tr_named};
use crate::ui::UiAction;
use crate::ui::atoms::{ERROR, WARN, big};
use aim_core::training::journal::{
    BREATHING_SECS, Journal, JournalTag, MAX_SELF_RATING, MIN_SELF_RATING, Outcome, Split,
    tilt_reset,
};
use aim_core::training::progress::{Readiness, Warmth};
use bevy_egui::egui;

/// What the journal shows beyond the entries.
pub struct JournalView<'a> {
    pub journal: &'a Journal,
    /// A load or save problem with the journal file.
    pub problem: Option<String>,
    /// Today's readiness, stored with a new entry.
    pub readiness: Option<Warmth>,
    /// Whether a fatigue signal is showing now, stored with a new entry.
    pub fatigued: bool,
}

/// The entry form and the breathing timer, kept between frames.
#[derive(Debug, Default)]
pub struct JournalState {
    rr: String,
    /// 0 until the player picks one; entries use 3 then.
    rating: u8,
    tag: Option<JournalTag>,
    error: Option<String>,
    /// UI time the breathing timer started.
    breathing_since: Option<f64>,
}

/// One match to log, from the form.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub outcome: Outcome,
    pub rr_change: Option<i32>,
    pub self_rating: u8,
    pub tag: Option<JournalTag>,
}

/// What the player asked for in the journal.
#[derive(Debug, Clone, PartialEq)]
pub enum JournalAction {
    Log(Draft),
    Run(UiAction),
}

const DEFAULT_RATING: u8 = 3;

/// The RR field: empty is "not entered", otherwise a whole number such as `+18` or `-12`.
pub fn parse_rr(text: &str) -> Result<Option<i32>, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(None);
    }
    t.trim_start_matches('+')
        .parse::<i32>()
        .map(Some)
        .map_err(|_| {
            tr_fmt(
                "RR change must be a whole number such as +18 or -12 (got {})",
                &[&format!("{t:?}")],
            )
        })
}

/// The readiness stored with an entry: the coldest of today's reference scenarios, or
/// `None` when none has a baseline.
pub fn journal_readiness(readiness: &[(String, Readiness)]) -> Option<Warmth> {
    readiness
        .iter()
        .min_by(|a, b| a.1.ratio.total_cmp(&b.1.ratio))
        .map(|(_, r)| r.warmth)
}

/// Size of the breathing cue (0 = empty lungs, 1 = full) and the instruction, `elapsed`
/// seconds into the timer at `breath_secs` per breath: in for the first half, out after.
pub fn breath_cue(elapsed: f32, breath_secs: f32) -> (f32, &'static str) {
    let phase = (elapsed / breath_secs).rem_euclid(1.0);
    let size = 0.5 - 0.5 * (phase * std::f32::consts::TAU).cos();
    (
        size,
        if phase < 0.5 {
            "Breathe in"
        } else {
            "Breathe out"
        },
    )
}

/// One line for a group of matches.
pub fn split_text(s: &Split) -> String {
    if s.matches == 0 {
        return tr("no matches yet").to_owned();
    }
    let won = s.win_rate.map(|w| format!("{:.0}", w * 100.0));
    let rating = s.avg_self_rating.map(|r| format!("{r:.1}"));
    let template = match (&won, &rating) {
        (Some(_), Some(_)) => {
            "{matches} matches · {wins}W {losses}L {draws}D · {won}% won · rating {rating}"
        }
        (Some(_), None) => "{matches} matches · {wins}W {losses}L {draws}D · {won}% won",
        (None, Some(_)) => "{matches} matches · {wins}W {losses}L {draws}D · rating {rating}",
        (None, None) => "{matches} matches · {wins}W {losses}L {draws}D",
    };
    let won = won.unwrap_or_default();
    let rating = rating.unwrap_or_default();
    tr_named(
        template,
        &[
            ("matches", &s.matches),
            ("wins", &s.wins),
            ("losses", &s.losses),
            ("draws", &s.draws),
            ("won", &won),
            ("rating", &rating),
        ],
    )
}

fn tag_label(t: JournalTag) -> &'static str {
    match t {
        JournalTag::Tilted => "Tilted",
        JournalTag::Tired => "Tired",
        JournalTag::WarmedUp => "Warmed up",
        JournalTag::Cold => "Cold",
        JournalTag::AimOff => "Aim off",
        JournalTag::AimOn => "Aim on",
        JournalTag::TeamDiff => "Team diff",
        JournalTag::Other => "Other",
    }
}

fn warmth_label(w: Option<Warmth>) -> &'static str {
    match w {
        Some(Warmth::Ready) => "ready",
        Some(Warmth::Warming) => "warming up",
        Some(Warmth::Cold) => "cold",
        None => "unknown",
    }
}

pub fn journal_panel(
    ui: &mut egui::Ui,
    state: &mut JournalState,
    v: &JournalView,
) -> Option<JournalAction> {
    let mut action = None;
    ui.label(big(tr("Ranked journal"), 20.0));
    ui.small(tr_fmt(
        "Log each match in one tap. Today's readiness ({}) and fatigue ({}) are stored with it.",
        &[
            &tr(warmth_label(v.readiness)),
            &tr(if v.fatigued { "showing" } else { "none" }),
        ],
    ));
    if let Some(p) = &v.problem {
        ui.colored_label(ERROR, p);
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("RR");
        ui.add(
            egui::TextEdit::singleline(&mut state.rr)
                .hint_text("+18 / -12")
                .desired_width(60.0),
        );
        ui.label(tr("Rating"));
        let current = if state.rating == 0 {
            DEFAULT_RATING
        } else {
            state.rating
        };
        for r in MIN_SELF_RATING..=MAX_SELF_RATING {
            if ui.selectable_label(current == r, r.to_string()).clicked() {
                state.rating = r;
            }
        }
        ui.label(tr("Tag"));
        egui::ComboBox::from_id_salt("journal_tag")
            .selected_text(tr(state.tag.map_or("none", tag_label)))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.tag, None, tr("none"));
                for t in JournalTag::ALL {
                    ui.selectable_value(&mut state.tag, Some(t), tr(tag_label(t)));
                }
            });
    });
    ui.horizontal(|ui| {
        for (outcome, label) in [
            (Outcome::Win, "Win"),
            (Outcome::Loss, "Loss"),
            (Outcome::Draw, "Draw"),
        ] {
            if ui.button(tr(label)).clicked() {
                match parse_rr(&state.rr) {
                    Ok(rr_change) => {
                        action = Some(JournalAction::Log(Draft {
                            outcome,
                            rr_change,
                            self_rating: if state.rating == 0 {
                                DEFAULT_RATING
                            } else {
                                state.rating
                            },
                            tag: state.tag,
                        }));
                        state.rr.clear();
                        state.tag = None;
                        state.error = None;
                    }
                    Err(e) => state.error = Some(e),
                }
            }
        }
    });
    if let Some(e) = &state.error {
        ui.colored_label(ERROR, e);
    }

    let j = v.journal;
    let streak = j.losing_streak();
    if streak > 0 {
        let template = if streak == 1 {
            "Losing streak: {} match"
        } else {
            "Losing streak: {} matches"
        };
        ui.label(tr_fmt(template, &[&streak]));
    }
    if let Some(started) = state.breathing_since {
        if let Some(a) = breathing_timer(ui, state, started) {
            action = Some(JournalAction::Run(a));
        }
    } else if j.tilt_reset_suggested() {
        let reset = tilt_reset();
        ui.colored_label(
            WARN,
            tr_named(
                "{streak} losses in a row. Try a {minutes}-minute reset before the next queue: {secs} s of slow breathing, then easy smooth tracking.",
                &[
                    ("streak", &streak),
                    ("minutes", &format!("{:.0}", reset.minutes())),
                    ("secs", &reset.breathing_secs),
                ],
            ),
        );
        if ui.button(tr("Tilt reset (3 min)")).clicked() {
            state.breathing_since = Some(ui.input(|i| i.time));
        }
    }

    if !j.entries.is_empty() {
        egui::CollapsingHeader::new(tr_fmt(
            "Results by readiness and fatigue ({} matches)",
            &[&j.entries.len()],
        ))
        .default_open(false)
        .show(ui, |ui| {
            let r = j.by_readiness();
            let f = j.by_fatigue();
            egui::Grid::new("journal_splits")
                .num_columns(2)
                .show(ui, |ui| {
                    for (label, s) in [
                        ("Ready", &r.ready),
                        ("Warming up", &r.warming),
                        ("Cold", &r.cold),
                        ("Readiness unknown", &r.unknown),
                        ("No fatigue signal", &f.fresh),
                        ("Fatigue signal", &f.fatigued),
                    ] {
                        ui.label(tr(label));
                        ui.label(split_text(s));
                        ui.end_row();
                    }
                });
            ui.small(tr("A pattern needs many matches; a handful says little."));
        });
    }
    action
}

/// The paced-breathing timer: an expanding and contracting circle, then the easy tracking
/// run. Returns the run to start when the timer ends or is skipped.
fn breathing_timer(ui: &mut egui::Ui, state: &mut JournalState, started: f64) -> Option<UiAction> {
    let reset = tilt_reset();
    let elapsed = (ui.input(|i| i.time) - started) as f32;
    let start_run = || UiAction::StartRoutineDef(Box::new(tilt_reset().routine));
    if elapsed >= BREATHING_SECS as f32 {
        state.breathing_since = None;
        return Some(start_run());
    }
    let (size, cue) = breath_cue(elapsed, reset.breath_secs());
    ui.label(tr(&reset.breathing_note));
    let (rect, _) = ui.allocate_exact_size(egui::vec2(140.0, 140.0), egui::Sense::hover());
    let radius = 18.0 + 50.0 * size;
    ui.painter().circle_filled(
        rect.center(),
        radius,
        egui::Color32::from_rgba_unmultiplied(110, 170, 255, 90),
    );
    ui.painter().circle_stroke(
        rect.center(),
        68.0,
        egui::Stroke::new(1.0, egui::Color32::GRAY),
    );
    ui.label(tr_fmt(
        "{} · {} s left",
        &[
            &tr(cue),
            &format!("{:.0}", (BREATHING_SECS as f32 - elapsed).ceil()),
        ],
    ));
    ui.ctx().request_repaint();
    let mut out = None;
    ui.horizontal(|ui| {
        if ui.button(tr("Skip to tracking")).clicked() {
            state.breathing_since = None;
            out = Some(start_run());
        }
        if ui.button(tr("Cancel")).clicked() {
            state.breathing_since = None;
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn readiness(ratio: f32, warmth: Warmth) -> (String, Readiness) {
        (
            "x".into(),
            Readiness {
                scenario: "x".into(),
                baseline: 100.0,
                today_best: 100.0 * ratio,
                ratio,
                warmth,
            },
        )
    }

    #[test]
    fn rr_is_optional_and_signed() {
        assert_eq!(parse_rr(""), Ok(None));
        assert_eq!(parse_rr("  "), Ok(None));
        assert_eq!(parse_rr("+18"), Ok(Some(18)));
        assert_eq!(parse_rr("-12"), Ok(Some(-12)));
        assert_eq!(parse_rr("7"), Ok(Some(7)));
        assert!(parse_rr("abc").unwrap_err().contains("whole number"));
        assert!(parse_rr("1.5").is_err());
    }

    #[test]
    fn the_entry_readiness_is_the_coldest_reference_scenario() {
        assert_eq!(journal_readiness(&[]), None);
        let r = [
            readiness(0.97, Warmth::Ready),
            readiness(0.80, Warmth::Cold),
            readiness(0.90, Warmth::Warming),
        ];
        assert_eq!(journal_readiness(&r), Some(Warmth::Cold));
    }

    #[test]
    fn the_breathing_cue_expands_on_the_inhale_and_contracts_on_the_exhale() {
        let breath = 10.0; // 6 breaths a minute
        let (start, cue) = breath_cue(0.0, breath);
        assert!(start.abs() < 1e-6);
        assert_eq!(cue, "Breathe in");
        let (full, _) = breath_cue(5.0, breath);
        assert!((full - 1.0).abs() < 1e-6);
        let (mid_out, cue) = breath_cue(7.5, breath);
        assert!((mid_out - 0.5).abs() < 1e-5);
        assert_eq!(cue, "Breathe out");
        assert!(
            breath_cue(10.0, breath).0.abs() < 1e-5,
            "the next breath starts empty"
        );
        assert!(
            breath_cue(2.0, breath).0 < breath_cue(4.0, breath).0,
            "expands"
        );
        assert!(
            breath_cue(6.0, breath).0 > breath_cue(8.0, breath).0,
            "contracts"
        );
    }

    #[test]
    fn split_lines_show_counts_rate_and_rating() {
        assert_eq!(split_text(&Split::default()), "no matches yet");
        let s = Split {
            matches: 3,
            wins: 2,
            losses: 1,
            draws: 0,
            win_rate: Some(2.0 / 3.0),
            avg_self_rating: Some(3.333),
        };
        assert_eq!(
            split_text(&s),
            "3 matches · 2W 1L 0D · 67% won · rating 3.3"
        );
    }

    #[test]
    fn the_tilt_reset_lasts_three_minutes_and_starts_easy_tracking() {
        let r = tilt_reset();
        assert!((r.minutes() - 3.0).abs() < 1e-6);
        assert_eq!(r.routine.blocks[0].scenario, "track-precise");
    }

    #[test]
    fn journal_lines_read_in_spanish() {
        use aim_core::i18n::{Lang, tr_fmt_in, tr_in, tr_named_in};
        assert_eq!(
            tr_named_in(
                Lang::Es,
                "{matches} matches · {wins}W {losses}L {draws}D",
                &[("matches", &3), ("wins", &2), ("losses", &1), ("draws", &0)]
            ),
            "3 partidas · 2V 1D 0E"
        );
        assert_eq!(
            tr_fmt_in(Lang::Es, "Losing streak: {} match", &[&1]),
            "Racha de derrotas: 1 partida"
        );
        assert_eq!(tr_in(Lang::Es, "Breathe in"), "Inhala");
        let s = Split {
            matches: 2,
            wins: 1,
            losses: 1,
            draws: 0,
            win_rate: Some(0.5),
            avg_self_rating: None,
        };
        assert_eq!(split_text(&s), "2 matches · 1W 1L 0D · 50% won");
    }
}
