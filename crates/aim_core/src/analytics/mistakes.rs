//! Mistake replays: the flicks of a click run that went wrong, one record per mistake.
//!
//! Each [`Mistake`] is tied to the run's trace samples (where its flick starts, the exact
//! frame it went wrong, and the shot), carries a tip specific to its kind and numbers, and
//! a corrected example: a realistic, human-like flick from the same start view to the same
//! target. The example has a reaction delay, a minimum-jerk primary movement that lands a
//! little short inside the target, and sometimes one small correction; it is never a
//! perfect straight snap. Everything is a pure function of the recording, so the same run
//! always gives the same mistakes and examples. See "Mistake replays" in
//! `docs/training-science.md`.
//!
//! Only click runs are covered: tracking runs have no single wrong frame to replay.

use super::flick::{self, Flick, FlickKind, FlickSpan, OVERSHOOT_RADII, along_error_deg, unit};
use super::rules::{Metric, Rule, Verdict, builtin::rules};
use super::trace::{self, Sample, tangent_offset_deg};
use crate::math::{ViewAngles, angle_between_deg, wrap_degrees};
use crate::rng::Rng;
use crate::scenario::{FireMode, Recording};
use crate::sim::{AimerProfile, minimum_jerk};

/// The slow-motion part of a replay starts this long before the wrong frame, so the player
/// sees the movement that led to it (a presentation choice).
pub const FOCUS_LEAD_SECS: f32 = 0.15;
/// A slow reaction's slow-motion part continues this long after the movement onset.
pub const FOCUS_TAIL_SECS: f32 = 0.15;
/// Skill of the corrected example: the simulated "veteran", good but clearly human.
pub const EXAMPLE_PROFILE: AimerProfile = AimerProfile::PRESETS[2];
/// The example's primary movement ends this far short of the target center, along the
/// movement, in target radii (uniform). Primary movements of real flicks fall slightly short
/// (Meyer et al. 1988); the range is an estimate that keeps the endpoint inside the target.
pub const EXAMPLE_SHORT_RADII: (f32, f32) = (0.3, 0.8);
/// Largest sideways endpoint error of the example's primary movement, target radii (estimate).
pub const EXAMPLE_SIDE_RADII: f32 = 0.35;
/// The example corrects its aim when the primary movement ended farther than this from the
/// center, in target radii (estimate).
pub const EXAMPLE_CORRECT_BEYOND_RADII: f32 = 0.5;
/// After a correction the remaining error is this share of the primary one (uniform, estimate).
pub const EXAMPLE_CORRECTION_KEEP: (f32, f32) = (0.2, 0.45);
/// Duration of the example's correction, seconds (uniform, estimate).
pub const EXAMPLE_CORRECTION_SECS: (f32, f32) = (0.06, 0.08);
/// Time from the end of the example's movement to its click, seconds (uniform, estimate).
pub const EXAMPLE_CLICK_AFTER_SECS: (f32, f32) = (0.02, 0.04);
/// The example skips its correction when it would push the click delay past this share of
/// the late-click threshold, so the example is never itself a late click.
pub const EXAMPLE_CLICK_BUDGET_SHARE: f32 = 0.9;
/// The example's target position is extrapolated at most this far past the player's shot.
const MAX_EXTRAPOLATION_SECS: f32 = 0.5;
/// Frame duration used when a trace frame has none.
const FALLBACK_DT_SECS: f32 = 1.0 / 240.0;

/// What went wrong in a flick. A flick gets at most one, the first that applies in this
/// order (an overflick's correction also slows its click, so only the cause is named).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MistakeKind {
    /// The primary movement ended past the target's far edge.
    Overflick,
    /// The primary movement ended short of the target's near edge.
    Underflick,
    /// More corrective sub-movements than the corrections rule allows.
    ExtraCorrections,
    /// Movement started later than the reaction-time rule allows (fresh flicks only).
    SlowReaction,
    /// The crosshair sat on the target longer than the click-delay rule allows.
    LateClick,
}

impl MistakeKind {
    /// Short human-readable name.
    pub fn label(self) -> &'static str {
        match self {
            MistakeKind::Overflick => "Overflick",
            MistakeKind::Underflick => "Underflick",
            MistakeKind::ExtraCorrections => "Extra corrections",
            MistakeKind::SlowReaction => "Slow reaction",
            MistakeKind::LateClick => "Late click",
        }
    }
}

/// One frame of a replayed path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathPoint {
    /// Time since the start of the flick's segment, seconds.
    pub time_secs: f32,
    /// Crosshair offset from the target's aim point at that time, degrees: `.0` horizontal
    /// (positive right), `.1` vertical (positive up).
    pub offset_deg: (f32, f32),
}

/// One mistake of a run, ready to replay step by step.
#[derive(Debug, Clone, PartialEq)]
pub struct Mistake {
    /// What went wrong.
    pub kind: MistakeKind,
    /// Index of the flick in the run's flicks.
    pub flick_index: usize,
    /// Id of the target the flick was aimed at.
    pub target_id: u64,
    /// Angular radius of the target's aim zone at the shot, degrees.
    pub radius_deg: f32,
    /// Angle from the crosshair at the start to the target, degrees.
    pub distance_deg: f32,
    /// Screen vector from the start to the target, degrees (`.0` right, `.1` up).
    pub direction_deg: (f32, f32),
    /// Trace sample where the flick's segment starts.
    pub segment_start: usize,
    /// Trace sample where the slow-motion part (the wrong moment and its lead-in) starts.
    pub focus_start: usize,
    /// Trace sample of the wrong moment: where an overflick passed the far edge, an
    /// underflick or extra corrections ended the primary movement, a slow reaction started
    /// moving, or a late click first reached the target.
    pub error_at: usize,
    /// Trace sample where the slow-motion part ends.
    pub focus_end: usize,
    /// Trace sample of the shot, the last of the segment.
    pub segment_end: usize,
    /// Signed primary endpoint error, target radii (positive past the target).
    pub error_radii: Option<f32>,
    /// Distance beyond the far edge (overflick) or before the near edge (underflick), degrees.
    pub edge_error_deg: Option<f32>,
    /// Reaction time (slow reaction) or click delay (late click), seconds.
    pub delay_secs: Option<f32>,
    /// Corrective sub-movements after the primary one.
    pub corrections: u32,
    /// The shot hit.
    pub hit: bool,
    /// What to change, naming the mistake, its amount and where it happened.
    pub tip: String,
    /// The player's crosshair, one point per sample from `segment_start` to `segment_end`.
    pub player: Vec<PathPoint>,
    /// A realistic corrected flick from the same start to the same target, sampled at the
    /// trace's frame rate; its last point is the click.
    pub corrected: Vec<PathPoint>,
}

impl Mistake {
    /// Index into [`Mistake::player`] of trace sample `sample`.
    pub fn local(&self, sample: usize) -> usize {
        sample
            .saturating_sub(self.segment_start)
            .min(self.player.len().saturating_sub(1))
    }

    /// Time of trace sample `sample` since the segment start, seconds.
    pub fn time_of(&self, sample: usize) -> f32 {
        self.player
            .get(self.local(sample))
            .map_or(0.0, |p| p.time_secs)
    }

    /// Time of the wrong moment since the segment start, seconds.
    pub fn error_secs(&self) -> f32 {
        self.time_of(self.error_at)
    }

    /// The slow-motion window, seconds since the segment start.
    pub fn focus_secs(&self) -> (f32, f32) {
        (self.time_of(self.focus_start), self.time_of(self.focus_end))
    }
}

/// The mistakes of a recorded run, in the order they happened. Empty for hold runs and
/// detection drills, which have no aimed flicks.
pub fn from_recording(recording: &Recording) -> Vec<Mistake> {
    let detection = recording
        .def
        .targets
        .perception
        .as_ref()
        .is_some_and(|p| p.is_detection());
    if recording.def.weapon.fire != FireMode::Click || detection {
        return Vec::new();
    }
    mistakes(&trace::trace(recording), recording.seed)
}

/// The mistakes in `samples`, in the order they happened. `seed` makes the corrected
/// examples deterministic.
pub fn mistakes(samples: &[Sample], seed: u64) -> Vec<Mistake> {
    let limits = Limits::from_rules();
    flick::flick_spans(samples)
        .iter()
        .enumerate()
        .filter_map(|(index, (f, span))| {
            let (kind, error_at) = classify(samples, f, span, &limits)?;
            Some(build(
                samples, index, f, span, kind, error_at, &limits, seed,
            ))
        })
        .collect()
}

/// Per-flick thresholds, taken from the weakness rules so a mistake means the same as the
/// run-level weakness it feeds.
struct Limits {
    reaction: Option<&'static Rule>,
    corrections: Option<&'static Rule>,
    click_delay: Option<&'static Rule>,
}

impl Limits {
    fn from_rules() -> Self {
        let weakness = |metric| {
            rules()
                .rules
                .iter()
                .find(|r| r.metric == metric && r.verdict == Verdict::Weakness)
        };
        Self {
            reaction: weakness(Metric::ReactionTime),
            corrections: weakness(Metric::Corrections),
            click_delay: weakness(Metric::ClickDelay),
        }
    }
}

fn fires(rule: Option<&Rule>, value: Option<f32>) -> bool {
    matches!((rule, value), (Some(r), Some(v)) if r.fires(v))
}

/// The target's aim point as seen in `s`, or at the shot when it is gone.
fn goal_in(s: &Sample, span: &FlickSpan) -> (ViewAngles, f32) {
    s.target(span.target.id)
        .map_or((span.target.aim_point, span.target.radius_deg), |t| {
            (t.aim_point, t.radius_deg)
        })
}

/// The flick's mistake and its wrong frame, if any.
fn classify(
    samples: &[Sample],
    f: &Flick,
    span: &FlickSpan,
    limits: &Limits,
) -> Option<(MistakeKind, usize)> {
    let primary_end = span.primary_end.unwrap_or(span.shot);
    if f.overshot() {
        let radius = span.target.radius_deg.max(1e-3);
        let dir =
            unit(tangent_offset_deg(span.start_view, span.target.aim_point)).unwrap_or((1.0, 0.0));
        let from = span.onset.unwrap_or(span.first);
        let passed = (from..=primary_end).find(|&i| {
            let (goal, _) = goal_in(&samples[i], span);
            along_error_deg(samples[i].view, goal, dir) / radius > OVERSHOOT_RADII
        });
        return Some((MistakeKind::Overflick, passed.unwrap_or(primary_end)));
    }
    if f.undershot() {
        return Some((MistakeKind::Underflick, primary_end));
    }
    if fires(limits.corrections, Some(f.corrections as f32)) {
        return Some((MistakeKind::ExtraCorrections, primary_end));
    }
    if f.kind == FlickKind::Fresh && fires(limits.reaction, f.reaction_secs) {
        return Some((MistakeKind::SlowReaction, span.onset.unwrap_or(span.first)));
    }
    if fires(limits.click_delay, f.click_delay_secs) {
        let entry = (span.first..=span.shot).find(|&i| {
            let (goal, radius) = goal_in(&samples[i], span);
            angle_between_deg(samples[i].view.direction(), goal.direction()) <= radius
        });
        return Some((MistakeKind::LateClick, entry.unwrap_or(span.shot)));
    }
    None
}

/// First sample in `range` at or after `secs` (session time), or the range's end.
fn first_at(samples: &[Sample], range: std::ops::RangeInclusive<usize>, secs: f32) -> usize {
    let end = *range.end();
    range
        .clone()
        .find(|&i| samples[i].time_secs >= secs)
        .unwrap_or(end)
}

#[allow(clippy::too_many_arguments)]
fn build(
    samples: &[Sample],
    flick_index: usize,
    f: &Flick,
    span: &FlickSpan,
    kind: MistakeKind,
    error_at: usize,
    limits: &Limits,
    seed: u64,
) -> Mistake {
    let error_secs = samples[error_at].time_secs;
    let (focus_start, focus_end) = match kind {
        MistakeKind::SlowReaction => (
            span.first,
            first_at(samples, error_at..=span.shot, error_secs + FOCUS_TAIL_SECS),
        ),
        _ => (
            first_at(samples, span.first..=error_at, error_secs - FOCUS_LEAD_SECS),
            span.shot,
        ),
    };
    let first = &samples[span.first];
    let dt = if first.dt_secs > 0.0 {
        first.dt_secs
    } else {
        FALLBACK_DT_SECS
    };
    let origin = first.time_secs - dt;
    let player: Vec<PathPoint> = samples[span.first..=span.shot]
        .iter()
        .map(|s| PathPoint {
            time_secs: s.time_secs - origin,
            offset_deg: tangent_offset_deg(goal_in(s, span).0, s.view),
        })
        .collect();
    let track = Track {
        times: player.iter().map(|p| p.time_secs).collect(),
        points: samples[span.first..=span.shot]
            .iter()
            .map(|s| goal_in(s, span).0)
            .collect(),
    };
    let reaction = match (kind, f.reaction_secs) {
        (MistakeKind::SlowReaction, _) | (_, None) => EXAMPLE_PROFILE.reaction_secs,
        (_, Some(r)) => r,
    };
    let example = Example {
        start: span.start_view,
        radius_deg: span.target.radius_deg.max(1e-3),
        move_start_secs: (span.start_secs - origin).max(0.0) + reaction,
        dt_secs: dt,
        click_delay_limit_secs: limits.click_delay.map(|r| r.threshold),
    };
    let mixed = seed ^ (flick_index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let corrected = example.path(&track, &mut Rng::new(mixed));
    let error_radii = matches!(kind, MistakeKind::Overflick | MistakeKind::Underflick)
        .then_some(f.primary_error_radii)
        .flatten();
    let edge_error_deg = error_radii.map(|r| (r.abs() - OVERSHOOT_RADII) * f.target_radius_deg);
    let delay_secs = match kind {
        MistakeKind::SlowReaction => f.reaction_secs,
        MistakeKind::LateClick => f.click_delay_secs,
        _ => None,
    };
    let mut m = Mistake {
        kind,
        flick_index,
        target_id: f.target_id,
        radius_deg: f.target_radius_deg,
        distance_deg: f.distance_deg,
        direction_deg: f.direction_deg,
        segment_start: span.first,
        focus_start,
        error_at,
        focus_end,
        segment_end: span.shot,
        error_radii,
        edge_error_deg,
        delay_secs,
        corrections: f.corrections,
        hit: f.hit,
        tip: String::new(),
        player,
        corrected,
    };
    m.tip = tip(&m, limits);
    m
}

/// Screen direction of a flick as a word ("right", "up-left", ...).
fn direction_word((x, y): (f32, f32)) -> &'static str {
    const WORDS: [&str; 8] = [
        "right",
        "up-right",
        "up",
        "up-left",
        "left",
        "down-left",
        "down",
        "down-right",
    ];
    let angle = y.atan2(x).to_degrees().rem_euclid(360.0);
    WORDS[((angle + 22.5) / 45.0) as usize % 8]
}

fn plural(n: u32, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// The tip for `m`: names the mistake, its amount and where it happened, then what to change.
fn tip(m: &Mistake, limits: &Limits) -> String {
    let flick = format!(
        "this {:.0}° flick {}",
        m.distance_deg,
        direction_word(m.direction_deg)
    );
    let back = if m.corrections > 0 {
        format!(
            ", then needed {} to come back",
            plural(m.corrections, "correction")
        )
    } else {
        String::new()
    };
    let ms = |s: Option<f32>| s.map_or(0.0, |s| s * 1000.0);
    let limit_ms = |r: Option<&Rule>| r.map_or(0.0, |r| r.threshold * 1000.0);
    match m.kind {
        MistakeKind::Overflick => format!(
            "Overflick: on {flick} your first movement ended {:.1} target radii past the \
             center, {:.2}° beyond the far edge{back}. Start braking earlier: aim to stop on \
             the near half of the target and let one small correction finish the shot. If \
             most of your flicks overshoot, try a slightly lower sensitivity.",
            m.error_radii.unwrap_or(0.0),
            m.edge_error_deg.unwrap_or(0.0),
        ),
        MistakeKind::Underflick => format!(
            "Underflick: on {flick} your first movement stopped {:.1} target radii short of \
             the center, {:.2}° before the near edge{back}. Commit to the full distance: move \
             through to the target in one motion instead of creeping up on it.",
            -m.error_radii.unwrap_or(0.0),
            m.edge_error_deg.unwrap_or(0.0),
        ),
        MistakeKind::ExtraCorrections => format!(
            "Extra corrections: on {flick} you needed {} after the first movement (more than \
             {:.0} is a lot). Make the first movement land on the target: slow its end \
             slightly and keep it to one small correction at most.",
            plural(m.corrections, "correction"),
            limits.corrections.map_or(0.0, |r| r.threshold),
        ),
        MistakeKind::SlowReaction => format!(
            "Slow reaction: you started {flick} {:.0} ms after the target appeared (more than \
             {:.0} ms is slow). Keep your crosshair at head height and your grip relaxed, and \
             start moving the moment you see the target; small errors can be corrected.",
            ms(m.delay_secs),
            limit_ms(limits.reaction),
        ),
        MistakeKind::LateClick => format!(
            "Late click: on {flick} your crosshair was on the target for {:.0} ms before you \
             clicked (more than {:.0} ms is hesitation). Click as soon as the crosshair reaches \
             the target; trust the flick instead of checking it.",
            ms(m.delay_secs),
            limit_ms(limits.click_delay),
        ),
    }
}

/// The target's aim point over time, from the player's segment: interpolated between
/// frames, held before the first and extrapolated a little past the last.
struct Track {
    times: Vec<f32>,
    points: Vec<ViewAngles>,
}

impl Track {
    fn at(&self, t: f32) -> ViewAngles {
        let n = self.points.len();
        let lerp = |a: ViewAngles, b: ViewAngles, s: f32| {
            ViewAngles::new(
                a.yaw_deg + wrap_degrees(b.yaw_deg - a.yaw_deg) * s,
                a.pitch_deg + (b.pitch_deg - a.pitch_deg) * s,
            )
        };
        if n < 2 || t <= self.times[0] {
            return self.points[0];
        }
        match self.times.iter().position(|&x| x >= t) {
            Some(j) => {
                let span = (self.times[j] - self.times[j - 1]).max(1e-6);
                lerp(
                    self.points[j - 1],
                    self.points[j],
                    (t - self.times[j - 1]) / span,
                )
            }
            None => {
                let span = (self.times[n - 1] - self.times[n - 2]).max(1e-6);
                let ahead = (t - self.times[n - 1]).min(MAX_EXTRAPOLATION_SECS);
                lerp(self.points[n - 2], self.points[n - 1], 1.0 + ahead / span)
            }
        }
    }
}

/// The aim point displaced by a screen offset (degrees, `.0` right, `.1` up).
fn displaced(goal: ViewAngles, off: (f32, f32)) -> ViewAngles {
    let cos = goal.pitch_deg.to_radians().cos().max(0.1);
    ViewAngles::new(goal.yaw_deg + off.0 / cos, goal.pitch_deg + off.1)
}

/// `from` moved the share `s` of the way to `to`.
fn toward(from: ViewAngles, to: ViewAngles, s: f32) -> ViewAngles {
    from.rotated(
        wrap_degrees(to.yaw_deg - from.yaw_deg) * s,
        (to.pitch_deg - from.pitch_deg) * s,
    )
}

/// Inputs of a corrected example.
struct Example {
    start: ViewAngles,
    radius_deg: f32,
    /// When the movement starts, seconds since the segment start.
    move_start_secs: f32,
    dt_secs: f32,
    click_delay_limit_secs: Option<f32>,
}

impl Example {
    /// The example's crosshair, one point per frame from the segment start to the click.
    fn path(&self, track: &Track, rng: &mut Rng) -> Vec<PathPoint> {
        let mut uniform = |(lo, hi): (f32, f32)| lo + (hi - lo) * rng.next_f32();
        let r = self.radius_deg;
        let aim = track.at(self.move_start_secs);
        let along =
            unit(tangent_offset_deg(self.start, aim)).map_or((-1.0, 0.0), |(x, y)| (-x, -y));
        // `along` points from the target back towards the start; `side` is perpendicular.
        let side = (-along.1, along.0);
        let short = r * uniform(EXAMPLE_SHORT_RADII);
        let aside = r * uniform((-EXAMPLE_SIDE_RADII, EXAMPLE_SIDE_RADII));
        let primary = (
            along.0 * short + side.0 * aside,
            along.1 * short + side.1 * aside,
        );
        let keep = uniform(EXAMPLE_CORRECTION_KEEP);
        let correction_secs = uniform(EXAMPLE_CORRECTION_SECS);
        let click_after = uniform(EXAMPLE_CLICK_AFTER_SECS);
        let distance = angle_between_deg(self.start.direction(), aim.direction());
        let duration = EXAMPLE_PROFILE.fitts_secs(distance, r);
        let primary_end = self.move_start_secs + duration;

        let primary_view = |t: f32| {
            let tau = ((t - self.move_start_secs) / duration).clamp(0.0, 1.0);
            toward(
                self.start,
                displaced(track.at(t), primary),
                minimum_jerk(tau),
            )
        };
        // When the primary movement enters the target, to keep the click delay in budget.
        let frames = (primary_end / self.dt_secs).ceil() as usize;
        let entry = (0..=frames)
            .map(|k| k as f32 * self.dt_secs)
            .find(|&t| {
                let off = tangent_offset_deg(track.at(t), primary_view(t));
                (off.0 * off.0 + off.1 * off.1).sqrt() <= r
            })
            .unwrap_or(primary_end);
        let budget = self
            .click_delay_limit_secs
            .map_or(f32::INFINITY, |l| l * EXAMPLE_CLICK_BUDGET_SHARE);
        let correct = short.hypot(aside) > EXAMPLE_CORRECT_BEYOND_RADII * r
            && (primary_end - entry) + correction_secs + click_after <= budget;
        let settled = if correct {
            (primary.0 * keep, primary.1 * keep)
        } else {
            primary
        };
        let correction_end = primary_end + if correct { correction_secs } else { 0.0 };
        let click = correction_end + click_after;

        let last = (click / self.dt_secs).ceil() as usize;
        (0..=last)
            .map(|k| {
                let t = k as f32 * self.dt_secs;
                let goal = track.at(t);
                let view = if t <= primary_end {
                    primary_view(t)
                } else if t < correction_end {
                    let tau = (t - primary_end) / correction_secs;
                    toward(
                        displaced(goal, primary),
                        displaced(goal, settled),
                        minimum_jerk(tau),
                    )
                } else {
                    displaced(goal, settled)
                };
                PathPoint {
                    time_secs: t,
                    offset_deg: tangent_offset_deg(goal, view),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::test_support::record;
    use crate::analytics::trace::TargetSample;
    use crate::scenario::FrameInput;
    use crate::sim::AimerProfile;

    const DT: f32 = 1.0 / 240.0;

    fn len(v: (f32, f32)) -> f32 {
        (v.0 * v.0 + v.1 * v.1).sqrt()
    }

    /// Drive flick-wide with a scripted view: react, flick 25% past the first target, stop,
    /// come back and click.
    fn overflick_recording() -> Recording {
        let def = crate::scenario::builtin::all()
            .into_iter()
            .find(|d| d.id == "flick-wide")
            .unwrap();
        let mut rec = Recording::new(def, 9);
        let mut live = rec.start();
        let idle = |view| FrameInput {
            dt: DT,
            view,
            ..Default::default()
        };
        let push = |rec: &mut Recording, live: &mut crate::scenario::Session, i: FrameInput| {
            rec.inputs.push(i);
            live.update(i);
        };
        while live.targets().next().is_none() {
            push(&mut rec, &mut live, idle(ViewAngles::default()));
        }
        let target = live.targets().next().unwrap().clone();
        let goal = ViewAngles::from_direction(target.position - live.eye_position());
        let from = ViewAngles::default();
        let past = ViewAngles::new(goal.yaw_deg * 1.25, goal.pitch_deg * 1.25);
        let lerp = |a: ViewAngles, b: ViewAngles, t: f32| {
            let s = minimum_jerk(t);
            ViewAngles::new(
                a.yaw_deg + (b.yaw_deg - a.yaw_deg) * s,
                a.pitch_deg + (b.pitch_deg - a.pitch_deg) * s,
            )
        };
        for _ in 0..48 {
            push(&mut rec, &mut live, idle(from));
        }
        for k in 1..=48 {
            push(&mut rec, &mut live, idle(lerp(from, past, k as f32 / 48.0)));
        }
        for _ in 0..24 {
            push(&mut rec, &mut live, idle(past));
        }
        for k in 1..=24 {
            push(&mut rec, &mut live, idle(lerp(past, goal, k as f32 / 24.0)));
        }
        push(
            &mut rec,
            &mut live,
            FrameInput {
                dt: DT,
                view: goal,
                fire_pressed: true,
                fire_held: true,
                ..Default::default()
            },
        );
        rec
    }

    #[test]
    fn an_overflick_is_one_mistake_marked_where_the_crosshair_passes_the_far_edge() {
        let rec = overflick_recording();
        let found = from_recording(&rec);
        assert_eq!(found.len(), 1, "{found:?}");
        let m = &found[0];
        assert_eq!(m.kind, MistakeKind::Overflick);
        assert!(m.hit);
        assert_eq!(m.corrections, 1);

        // Independently: the first frame whose angle from the start exceeds the target's
        // distance plus its radius (the path is a straight line from the start).
        let samples = trace::trace(&rec);
        let start = m
            .segment_start
            .checked_sub(1)
            .map_or(ViewAngles::default(), |p| samples[p].view);
        let expected = (m.segment_start..=m.segment_end)
            .find(|&i| {
                angle_between_deg(start.direction(), samples[i].view.direction())
                    > m.distance_deg + m.radius_deg
            })
            .unwrap();
        assert_eq!(m.error_at, expected);
        assert!(m.segment_start <= m.focus_start && m.focus_start < m.error_at);
        assert_eq!(m.focus_end, m.segment_end);
        assert_eq!(m.player.len(), m.segment_end - m.segment_start + 1);
        let past_edge = len(m.player[m.local(m.error_at)].offset_deg);
        assert!(past_edge > m.radius_deg, "{past_edge}");

        let radii = m.error_radii.unwrap();
        assert!(radii > OVERSHOOT_RADII, "{radii}");
        assert!(m.tip.contains("Overflick"), "{}", m.tip);
        assert!(
            m.tip.contains(&format!("{radii:.1} target radii")),
            "{}",
            m.tip
        );
        let edge = m.edge_error_deg.unwrap();
        assert!(m.tip.contains(&format!("{edge:.2}°")), "{}", m.tip);
    }

    #[test]
    fn the_corrected_example_is_human_like_lands_inside_and_is_deterministic() {
        let rec = overflick_recording();
        let m = &from_recording(&rec)[0];
        let c = &m.corrected;
        let end = c.last().unwrap();
        let d = len(end.offset_deg);
        assert!(d < m.radius_deg, "lands inside: {d} vs {}", m.radius_deg);
        assert!(d > 0.05 * m.radius_deg, "not a perfect center hit: {d}");
        // A reaction delay: the first frames do not move.
        assert_eq!(c[0].offset_deg, c[10].offset_deg);
        assert!((c[1].time_secs - c[0].time_secs - DT).abs() < 1e-6);
        // Never past the far edge along the movement, unlike the player.
        let dir = unit((-c[0].offset_deg.0, -c[0].offset_deg.1)).unwrap();
        let worst = c
            .iter()
            .map(|p| flick::dot(p.offset_deg, dir))
            .fold(f32::MIN, f32::max);
        assert!(worst < m.radius_deg, "{worst}");
        // Not a straight snap: it differs from a minimum-jerk move straight to the center
        // with no endpoint error.
        assert_ne!(c, &m.player);
        assert_eq!(from_recording(&rec), from_recording(&rec));
        let samples = trace::trace(&rec);
        assert_ne!(
            mistakes(&samples, 1)[0].corrected,
            mistakes(&samples, 2)[0].corrected
        );
    }

    /// Samples of a fixed target at yaw 10°, radius 1°, visible from the start; the crosshair
    /// follows `yaws` (pitch 0) and the last frame fires.
    fn scripted(yaws: &[f32]) -> Vec<Sample> {
        let target = TargetSample {
            id: 1,
            aim_point: ViewAngles::new(10.0, 0.0),
            radius_deg: 1.0,
            visible_at_secs: 0.0,
            decoy: false,
        };
        path_samples(target, yaws.iter().map(|&y| ViewAngles::new(y, 0.0)))
    }

    fn path_samples(target: TargetSample, views: impl Iterator<Item = ViewAngles>) -> Vec<Sample> {
        let views: Vec<ViewAngles> = views.collect();
        let mut previous = ViewAngles::default();
        views
            .iter()
            .enumerate()
            .map(|(i, &view)| {
                let last = i + 1 == views.len();
                // From the tangent offset, which stays precise for tiny per-frame steps.
                let speed = len(tangent_offset_deg(previous, view)) / DT;
                previous = view;
                Sample {
                    time_secs: (i + 1) as f32 * DT,
                    dt_secs: DT,
                    view,
                    angular_speed_deg_per_sec: speed,
                    targets: vec![target],
                    aimed: Some(0),
                    fire_held: last,
                    shot: last,
                    hit: last
                        && angle_between_deg(view.direction(), target.aim_point.direction())
                            <= target.radius_deg,
                    ..Default::default()
                }
            })
            .collect()
    }

    fn ramp(from: f32, to: f32, frames: usize) -> Vec<f32> {
        (1..=frames)
            .map(|k| from + (to - from) * minimum_jerk(k as f32 / frames as f32))
            .collect()
    }

    fn hold(at: f32, frames: usize) -> Vec<f32> {
        vec![at; frames]
    }

    fn script(parts: &[Vec<f32>]) -> Vec<Sample> {
        scripted(&parts.concat())
    }

    #[test]
    fn a_clean_flick_has_no_mistake() {
        let s = script(&[hold(0.0, 50), ramp(0.0, 9.8, 40), hold(9.8, 5)]);
        assert!(mistakes(&s, 1).is_empty());
    }

    #[test]
    fn a_slow_reaction_is_marked_at_the_movement_onset() {
        // 0.5 s before moving, over the 300 ms reaction rule.
        let s = script(&[hold(0.0, 120), ramp(0.0, 9.8, 40), hold(9.8, 5)]);
        let found = mistakes(&s, 1);
        assert_eq!(found.len(), 1, "{found:?}");
        let m = &found[0];
        assert_eq!(m.kind, MistakeKind::SlowReaction);
        let (f, span) = flick::flick_spans(&s).remove(0);
        assert_eq!(m.error_at, span.onset.unwrap());
        assert_eq!(m.focus_start, m.segment_start);
        assert!(m.focus_end > m.error_at && m.focus_end <= m.segment_end);
        let ms = (f.reaction_secs.unwrap() * 1000.0).round();
        assert!(m.tip.contains(&format!("{ms:.0} ms")), "{}", m.tip);
        // The example reacts like the example aimer, within the rule.
        let moved = m
            .corrected
            .iter()
            .position(|p| p.offset_deg != m.corrected[0].offset_deg)
            .unwrap();
        let reaction = m.corrected[moved].time_secs;
        assert!(reaction < 0.3, "{reaction}");
    }

    #[test]
    fn a_late_click_is_marked_where_the_crosshair_first_reached_the_target() {
        // On target from frame 89 and clicks 72 frames (300 ms) later.
        let s = script(&[hold(0.0, 50), ramp(0.0, 9.8, 40), hold(9.8, 72)]);
        let found = mistakes(&s, 1);
        assert_eq!(found.len(), 1, "{found:?}");
        let m = &found[0];
        assert_eq!(m.kind, MistakeKind::LateClick);
        let entry = (0..s.len()).find(|&i| (s[i].view.yaw_deg - 10.0).abs() <= 1.0);
        assert_eq!(Some(m.error_at), entry);
        let delay = m.delay_secs.unwrap();
        assert!(delay > 0.2, "{delay}");
        assert!(
            m.tip.contains(&format!("{:.0} ms", delay * 1000.0)),
            "{}",
            m.tip
        );
    }

    #[test]
    fn an_underflick_is_marked_where_the_primary_movement_stopped_short() {
        // Stops at 7.5, 2.5 radii short, then creeps on.
        let s = script(&[
            hold(0.0, 50),
            ramp(0.0, 7.5, 40),
            hold(7.5, 20),
            ramp(7.5, 9.9, 24),
        ]);
        let found = mistakes(&s, 1);
        assert_eq!(found.len(), 1, "{found:?}");
        let m = &found[0];
        assert_eq!(m.kind, MistakeKind::Underflick);
        let (_, span) = flick::flick_spans(&s).remove(0);
        assert_eq!(m.error_at, span.primary_end.unwrap());
        let r = m.error_radii.unwrap();
        assert!(r < -OVERSHOOT_RADII, "{r}");
        assert!(
            m.tip.contains(&format!("{:.1} target radii", -r)),
            "{}",
            m.tip
        );
    }

    #[test]
    fn extra_corrections_are_marked_where_the_primary_movement_ended() {
        // Lands inside, then two more separate adjustments.
        let s = script(&[
            hold(0.0, 50),
            ramp(0.0, 9.3, 40),
            hold(9.3, 10),
            ramp(9.3, 10.6, 10),
            hold(10.6, 10),
            ramp(10.6, 10.0, 10),
        ]);
        let found = mistakes(&s, 1);
        assert_eq!(found.len(), 1, "{found:?}");
        let m = &found[0];
        assert_eq!(m.kind, MistakeKind::ExtraCorrections);
        assert_eq!(m.corrections, 2);
        let (_, span) = flick::flick_spans(&s).remove(0);
        assert_eq!(m.error_at, span.primary_end.unwrap());
        assert!(m.tip.contains("2 corrections"), "{}", m.tip);
    }

    #[test]
    fn the_corrected_example_would_not_be_flagged_again() {
        let s = script(&[
            hold(0.0, 50),
            ramp(0.0, 12.5, 40),
            hold(12.5, 20),
            ramp(12.5, 10.0, 20),
        ]);
        for seed in 0..20 {
            let m = &mistakes(&s, seed)[0];
            assert_eq!(m.kind, MistakeKind::Overflick);
            let target = s[0].targets[0];
            // Rebuild the example's views from its target-relative offsets (pitch 0).
            let views = m
                .corrected
                .iter()
                .skip(1)
                .map(|p| ViewAngles::new(10.0 + p.offset_deg.0, p.offset_deg.1));
            let again = path_samples(target, views);
            let again = mistakes(&again, seed);
            assert!(again.is_empty(), "seed {seed}: {again:?}");
        }
    }

    #[test]
    fn simulated_runs_give_ordered_consistent_mistakes() {
        let rec = record("flick-wide", AimerProfile::PRESETS[0], 3);
        let found = from_recording(&rec);
        assert!(!found.is_empty(), "the recruit makes mistakes");
        assert_eq!(found, from_recording(&rec));
        for pair in found.windows(2) {
            assert!(pair[0].segment_end < pair[1].segment_start, "in order");
        }
        for m in &found {
            assert!(m.segment_start <= m.focus_start);
            assert!(m.focus_start <= m.error_at && m.error_at <= m.focus_end);
            assert!(m.focus_end <= m.segment_end);
            assert!(!m.tip.is_empty());
            let end = m.corrected.last().unwrap();
            assert!(len(end.offset_deg) < m.radius_deg, "{m:?}");
        }
        let hold = record("track-precise", AimerProfile::PRESETS[0], 3);
        assert!(from_recording(&hold).is_empty());
    }
}
