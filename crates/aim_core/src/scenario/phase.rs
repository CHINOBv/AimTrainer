//! Multi-phase scenarios and run rules: an ordered list of phases (`[[phases]]`) that each
//! override part of `targets` and advance on a trigger, fail conditions (`[fail]`) that end
//! a run early, and how a run ends (`[end]`: time, kills or endless).
//!
//! The data types are parsed with the rest of the [`ScenarioDef`]; [`PhaseRun`] is the
//! session's bookkeeping for the phase being played. Nothing here draws random numbers.

use super::def::{
    BehaviorRule, FireMode, Motion, ReactionsDef, ScenarioDef, Shape, SpawnArea, TargetsDef,
    ValidationError,
};
use super::session::Stats;
use crate::math::Range;
use serde::{Deserialize, Serialize};

/// Most phases a scenario may list.
pub const MAX_PHASES: usize = 16;

/// Most targets one phase's wave (`count`) may hold.
pub const MAX_WAVE_COUNT: u32 = 1000;

/// How a run ends (`[end]`). Whatever the kind, `duration_secs` stays a hard cap, so no run
/// can go on forever.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RunEnd {
    /// The run ends after `duration_secs` (the default).
    #[default]
    Time,
    /// The run is completed at the `kills`-th kill; reaching `duration_secs` first fails it.
    Kills {
        /// Kills that complete the run.
        kills: u32,
    },
    /// The run goes on until a fail condition ends it; reaching the `duration_secs` cap
    /// completes it.
    Endless,
}

impl RunEnd {
    /// Whether this is the default, time-ended run.
    pub fn is_time(&self) -> bool {
        *self == RunEnd::Time
    }
}

/// Fail conditions (`[fail]`): the first one reached ends the run as failed. Counts cover the
/// whole run, across phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailDef {
    /// Fail at this many missed shots (click fire only; a shot on a decoy is a miss).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_misses: Option<u32>,
    /// Fail at this many expired targets (needs `lifetime_secs` on the targets or a phase).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_expired: Option<u32>,
}

impl FailDef {
    /// Whether no fail condition is set.
    pub fn is_empty(&self) -> bool {
        self.max_misses.is_none() && self.max_expired.is_none()
    }
}

/// One phase (`[[phases]]`): target overrides, an optional wave size, the trigger that
/// advances to the next phase, and an optional time limit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhaseDef {
    /// Name shown in the HUD and results ("Phase 2" when empty).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Settings that replace the matching `targets` fields while the phase runs.
    #[serde(default)]
    pub targets: TargetOverrides,
    /// Wave size: the phase spawns this many targets in total, then no more. Needs
    /// `advance = { kind = "cleared" }`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    /// What moves the run on to the next phase. Every phase but the last needs one; on the
    /// last phase it completes the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advance: Option<PhaseTrigger>,
    /// Fail the run when the phase has not advanced this many seconds after it started.
    /// Needs `advance`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_limit_secs: Option<f32>,
}

impl PhaseDef {
    /// The phase's display name: `name`, or "Phase N" (1-based) when it is empty.
    pub fn label(&self, index: usize) -> String {
        if self.name.trim().is_empty() {
            format!("Phase {}", index + 1)
        } else {
            self.name.clone()
        }
    }
}

/// What advances a phase (`advance`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PhaseTrigger {
    /// After this many kills in the phase.
    Kills {
        /// Kills in the phase.
        kills: u32,
    },
    /// When accuracy in the phase (hits / shots) reaches `accuracy` with at least
    /// `min_shots` shots fired in the phase. Click fire only.
    Accuracy {
        /// Hits / shots in the phase, in (0, 1].
        accuracy: f32,
        /// Shots the phase needs before its accuracy counts.
        min_shots: u32,
    },
    /// When the phase's wave (`count` targets) is over: every one of them was killed or
    /// expired.
    Cleared,
    /// After this many seconds in the phase.
    Time {
        /// Seconds in the phase.
        secs: f32,
    },
}

/// The `targets` fields a phase can override. Unset fields keep the scenario's `targets`
/// value. Set `reactions` or `behaviors` replace the scenario's blocks for the phase.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetOverrides {
    /// Target shape (same kind as `targets.shape`: a sphere radius, or `head_only`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<Shape>,
    /// Where targets spawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spawn: Option<SpawnArea>,
    /// How targets move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<Motion>,
    /// Targets alive at the same time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_alive: Option<u32>,
    /// Hits (click) or seconds on target (hold) to kill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<f32>,
    /// Seconds before an unkilled target expires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifetime_secs: Option<f32>,
    /// Random delay before a new target appears, seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appear_delay_secs: Option<Range>,
    /// Reactive behaviour for the phase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reactions: Option<ReactionsDef>,
    /// Behaviour rules for the phase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub behaviors: Option<Vec<BehaviorRule>>,
}

impl TargetOverrides {
    /// `base` with every set override applied.
    pub fn apply(&self, base: &TargetsDef) -> TargetsDef {
        let mut t = base.clone();
        if let Some(v) = &self.shape {
            t.shape = v.clone();
        }
        if let Some(v) = &self.spawn {
            t.spawn = v.clone();
        }
        if let Some(v) = &self.motion {
            t.motion = v.clone();
        }
        if let Some(v) = self.max_alive {
            t.max_alive = v;
        }
        if let Some(v) = self.health {
            t.health = v;
        }
        if let Some(v) = self.lifetime_secs {
            t.lifetime_secs = Some(v);
        }
        if let Some(v) = self.appear_delay_secs {
            t.appear_delay_secs = v;
        }
        if let Some(v) = &self.reactions {
            t.reactions = Some(v.clone());
        }
        if let Some(v) = &self.behaviors {
            t.behaviors = v.clone();
        }
        t
    }

    /// Rewrites every set override with its value in `resolved`, so a transform applied to
    /// the resolved phase targets (difficulty, tuning) reaches the overrides too.
    pub fn take_from(&mut self, resolved: &TargetsDef) {
        if let Some(v) = &mut self.shape {
            *v = resolved.shape.clone();
        }
        if let Some(v) = &mut self.spawn {
            *v = resolved.spawn.clone();
        }
        if let Some(v) = &mut self.motion {
            *v = resolved.motion.clone();
        }
        if let Some(v) = &mut self.max_alive {
            *v = resolved.max_alive;
        }
        if let Some(v) = &mut self.health {
            *v = resolved.health;
        }
        if let Some(v) = &mut self.lifetime_secs {
            *v = resolved.lifetime_secs.unwrap_or(*v);
        }
        if let Some(v) = &mut self.appear_delay_secs {
            *v = resolved.appear_delay_secs;
        }
        if let Some(v) = &mut self.reactions {
            *v = resolved.reactions.clone().unwrap_or_else(|| v.clone());
        }
        if let Some(v) = &mut self.behaviors {
            *v = resolved.behaviors.clone();
        }
    }
}

impl ScenarioDef {
    /// Applies `transform` to `targets` and to every phase's overrides, each resolved against
    /// the untransformed `targets`, so phases scale exactly like the base targets do.
    pub(super) fn transform_targets(&mut self, transform: impl Fn(&mut TargetsDef)) {
        let base = self.targets.clone();
        for phase in self.phases.iter_mut().flatten() {
            let mut resolved = phase.targets.apply(&base);
            transform(&mut resolved);
            phase.targets.take_from(&resolved);
        }
        transform(&mut self.targets);
    }

    /// Problems with `[[phases]]`, `[fail]` and `[end]`; part of [`ScenarioDef::validate`].
    pub(super) fn run_rule_problems(&self) -> Vec<ValidationError> {
        let mut errs = Vec::new();
        let mut check = |ok: bool, field: &'static str, msg: String| {
            if !ok {
                errs.push(ValidationError {
                    field,
                    message: msg,
                });
            }
        };
        let click = self.weapon.fire == FireMode::Click;
        let phases = self.phases.as_deref().unwrap_or_default();

        match self.end {
            RunEnd::Time => {}
            RunEnd::Kills { kills } => {
                check(kills >= 1, "end.kills", "must be >= 1".into());
            }
            RunEnd::Endless => check(
                !self.fail.is_empty() || phases.iter().any(|p| p.time_limit_secs.is_some()),
                "end",
                "endless needs a fail condition: [fail] or a phase time_limit_secs".into(),
            ),
        }

        if let Some(m) = self.fail.max_misses {
            check(
                m >= 1 && click,
                "fail.max_misses",
                "must be >= 1 and needs fire = \"click\"".into(),
            );
        }
        if let Some(e) = self.fail.max_expired {
            // Ballistic targets expire when they land, so they need no lifetime.
            let lifetimes = self.targets.lifetime_secs.is_some()
                || matches!(self.targets.motion, Motion::Ballistic { .. })
                || phases.iter().any(|p| p.targets.lifetime_secs.is_some());
            check(
                e >= 1 && lifetimes,
                "fail.max_expired",
                "must be >= 1 and needs lifetime_secs on the targets or a phase, or a \
                 ballistic motion"
                    .into(),
            );
        }

        let Some(phases) = &self.phases else {
            return errs;
        };
        check(
            (1..=MAX_PHASES).contains(&phases.len()),
            "phases",
            format!("must list between 1 and {MAX_PHASES} phases"),
        );
        let base_errors = self.base_problems();
        for (i, p) in phases.iter().enumerate() {
            let n = i + 1;
            let last = i + 1 == phases.len();
            check(
                last || p.advance.is_some(),
                "phases.advance",
                format!("phase {n}: every phase but the last needs a trigger"),
            );
            check(
                p.count.is_some() == matches!(p.advance, Some(PhaseTrigger::Cleared)),
                "phases.count",
                format!("phase {n}: count and advance = {{ kind = \"cleared\" }} go together"),
            );
            check(
                p.count.is_none_or(|c| (1..=MAX_WAVE_COUNT).contains(&c)),
                "phases.count",
                format!("phase {n}: must be between 1 and {MAX_WAVE_COUNT}"),
            );
            if let Some(limit) = p.time_limit_secs {
                let after = match p.advance {
                    Some(PhaseTrigger::Time { secs }) => secs,
                    _ => 0.0,
                };
                check(
                    limit.is_finite() && limit > after && p.advance.is_some(),
                    "phases.time_limit_secs",
                    format!(
                        "phase {n}: must be > 0, longer than a time trigger, and needs advance"
                    ),
                );
            }
            match p.advance {
                Some(PhaseTrigger::Kills { kills }) => check(
                    kills >= 1,
                    "phases.advance.kills",
                    format!("phase {n}: must be >= 1"),
                ),
                Some(PhaseTrigger::Accuracy {
                    accuracy,
                    min_shots,
                }) => check(
                    accuracy > 0.0 && accuracy <= 1.0 && min_shots >= 1 && click,
                    "phases.advance.accuracy",
                    format!(
                        "phase {n}: needs accuracy in (0, 1], min_shots >= 1 and fire = \"click\""
                    ),
                ),
                Some(PhaseTrigger::Time { secs }) => check(
                    secs.is_finite() && secs > 0.0,
                    "phases.advance.secs",
                    format!("phase {n}: must be > 0"),
                ),
                Some(PhaseTrigger::Cleared) | None => {}
            }
            check(
                p.targets.shape.as_ref().is_none_or(|s| {
                    std::mem::discriminant(s) == std::mem::discriminant(&self.targets.shape)
                }),
                "phases.targets.shape",
                format!("phase {n}: must keep the kind of targets.shape"),
            );
            // The phase's resolved targets go through the same checks as `targets`; problems
            // the base targets already have are reported once, by the base.
            let mut resolved = self.clone();
            resolved.targets = p.targets.apply(&self.targets);
            for e in resolved.base_problems() {
                if !base_errors.contains(&e) {
                    check(false, "phases.targets", format!("phase {n}: {e}"));
                }
            }
        }
        errs
    }

    /// [`ScenarioDef::validate`] of this definition without its run rules.
    fn base_problems(&self) -> Vec<ValidationError> {
        let mut plain = self.clone();
        plain.phases = None;
        plain.fail = FailDef::default();
        plain.end = RunEnd::Time;
        plain.validate()
    }
}

/// Why a run failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailReason {
    /// `fail.max_misses` missed shots.
    Misses,
    /// `fail.max_expired` expired targets.
    Expired,
    /// A phase ran past its `time_limit_secs`.
    PhaseTimeLimit,
    /// A kills-ended run reached `duration_secs` before its kills.
    OutOfTime,
    /// A ballistic target with `on_floor = "fail"` landed.
    Dropped,
}

impl FailReason {
    /// Short description for the results screen.
    pub fn label(self) -> &'static str {
        match self {
            FailReason::Misses => "too many misses",
            FailReason::Expired => "too many targets expired",
            FailReason::PhaseTimeLimit => "phase time limit",
            FailReason::OutOfTime => "ran out of time",
            FailReason::Dropped => "a target hit the floor",
        }
    }
}

/// How a finished run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunOutcome {
    /// The run reached its end: the time, the kills, the endless cap or the last phase's
    /// trigger.
    Completed,
    /// A fail condition ended the run early.
    Failed(FailReason),
}

/// The session's bookkeeping for the phase being played.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct PhaseRun {
    /// Index of the phase in `[[phases]]`.
    pub(super) index: usize,
    /// Session time the phase started, seconds.
    pub(super) started_at: f32,
    /// Targets spawned in the phase.
    pub(super) spawned: u32,
    /// Run counters when the phase started: `(shots, hits, kills)`.
    marks: (u32, u32, u32),
}

impl PhaseRun {
    /// Bookkeeping for phase `index`, starting at `now` with the run's `stats`.
    pub(super) fn start(index: usize, now: f32, stats: &Stats) -> Self {
        Self {
            index,
            started_at: now,
            spawned: 0,
            marks: (stats.shots, stats.hits, stats.kills),
        }
    }

    /// Whether the phase may spawn another target.
    pub(super) fn has_room(&self, phase: &PhaseDef) -> bool {
        phase.count.is_none_or(|c| self.spawned < c)
    }

    /// Whether `phase`'s trigger has fired at `now`, with `alive` targets left.
    pub(super) fn triggered(
        &self,
        phase: &PhaseDef,
        stats: &Stats,
        now: f32,
        alive: usize,
    ) -> bool {
        let (shots0, hits0, kills0) = self.marks;
        match phase.advance {
            None => false,
            Some(PhaseTrigger::Kills { kills }) => stats.kills - kills0 >= kills,
            Some(PhaseTrigger::Accuracy {
                accuracy,
                min_shots,
            }) => {
                let shots = stats.shots - shots0;
                // The tolerance keeps "3 of 4" at exactly 0.75 from failing on rounding.
                shots >= min_shots && (stats.hits - hits0) as f32 / shots as f32 >= accuracy - 1e-6
            }
            Some(PhaseTrigger::Cleared) => !self.has_room(phase) && alive == 0,
            Some(PhaseTrigger::Time { secs }) => now - self.started_at >= secs,
        }
    }

    /// Whether `phase` ran past its time limit at `now`.
    pub(super) fn timed_out(&self, phase: &PhaseDef, now: f32) -> bool {
        phase
            .time_limit_secs
            .is_some_and(|l| now - self.started_at >= l)
    }
}

/// The fail condition of `fail` that `stats` reached, if any.
pub(super) fn failed(fail: &FailDef, stats: &Stats) -> Option<FailReason> {
    let misses = stats.shots - stats.hits;
    if fail.max_misses.is_some_and(|m| misses >= m) {
        Some(FailReason::Misses)
    } else if fail.max_expired.is_some_and(|m| stats.expired >= m) {
        Some(FailReason::Expired)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::ViewAngles;
    use crate::scenario::session::{FrameInput, Session, SessionEvent};

    const BASE: &str = r#"
        id = "p"
        name = "P"
        category = "custom"
        duration_secs = 10
        [targets]
        shape = { kind = "sphere", radius = 0.3 }
        spawn = { kind = "angular", yaw_deg = [-30, 30], pitch_deg = [-5, 10], distance = [10, 10] }
        max_alive = 3
        min_separation_deg = 5
    "#;

    fn parse(extra: &str) -> ScenarioDef {
        ScenarioDef::from_toml(&format!("{BASE}\n{extra}")).expect("test TOML parses")
    }

    fn def(extra: &str) -> ScenarioDef {
        let d = parse(extra);
        assert!(d.validate().is_empty(), "{:?}", d.validate());
        d
    }

    fn fields(extra: &str) -> Vec<&'static str> {
        parse(extra)
            .validate()
            .into_iter()
            .map(|e| e.field)
            .collect()
    }

    /// One 0.2 s frame (longer than the Vandal's fire interval) looking along `view`.
    fn frame(view: ViewAngles, fire: bool) -> FrameInput {
        FrameInput {
            dt: 0.2,
            view,
            fire_pressed: fire,
            fire_held: fire,
            ..Default::default()
        }
    }

    /// Shoots the first visible target.
    fn hit(s: &mut Session) -> Vec<SessionEvent> {
        let target = s.targets().next().expect("a visible target").position;
        let view = ViewAngles::from_direction(target - s.eye_position());
        s.update(frame(view, true))
    }

    /// Fires straight behind the player, where nothing spawns.
    fn miss(s: &mut Session) -> Vec<SessionEvent> {
        s.update(frame(ViewAngles::new(180.0, 0.0), true))
    }

    /// Waits `frames` frames of 0.2 s without firing.
    fn wait(s: &mut Session, frames: u32) -> Vec<SessionEvent> {
        (0..frames)
            .flat_map(|_| s.update(frame(ViewAngles::default(), false)))
            .collect()
    }

    fn started(events: &[SessionEvent]) -> Vec<usize> {
        events
            .iter()
            .filter_map(|e| match e {
                SessionEvent::PhaseStarted { index } => Some(*index),
                _ => None,
            })
            .collect()
    }

    fn failure(events: &[SessionEvent]) -> Option<FailReason> {
        events.iter().find_map(|e| match e {
            SessionEvent::RunFailed { reason } => Some(*reason),
            _ => None,
        })
    }

    fn radius(t: &TargetsDef) -> f32 {
        match t.shape {
            Shape::Sphere { radius } => radius,
            Shape::Humanoid { .. } => panic!("sphere expected"),
        }
    }

    #[test]
    fn a_kills_trigger_advances_to_the_next_phase_and_its_overrides_apply() {
        let mut s = Session::new(
            def(r#"
                [[phases]]
                targets = { max_alive = 1 }
                advance = { kind = "kills", kills = 2 }
                [[phases]]
                targets = { max_alive = 2, shape = { kind = "sphere", radius = 0.5 } }
            "#),
            1,
        );
        assert_eq!((s.phase_index(), s.phase_count()), (Some(0), 2));
        assert_eq!(s.targets().count(), 1, "phase 1 overrides max_alive");
        assert!(started(&hit(&mut s)).is_empty());
        assert_eq!(s.phase_index(), Some(0));
        assert_eq!(started(&hit(&mut s)), vec![1]);
        assert_eq!(s.phase_index(), Some(1));
        assert_eq!(s.targets().count(), 2);
        assert_eq!(radius(s.active_targets()), 0.5);
        assert_eq!(
            radius(&s.def().targets),
            0.3,
            "def() keeps the scenario as given"
        );
        assert_eq!(s.def().targets.max_alive, 3);
        assert!(!s.is_finished());
    }

    #[test]
    fn an_accuracy_trigger_needs_its_minimum_shots_and_rate_within_the_phase() {
        let mut s = Session::new(
            def(r#"
                [[phases]]
                advance = { kind = "accuracy", accuracy = 0.75, min_shots = 4 }
                [[phases]]
                targets = { max_alive = 1 }
            "#),
            2,
        );
        miss(&mut s);
        hit(&mut s);
        assert!(started(&hit(&mut s)).is_empty(), "3 shots < min_shots");
        assert_eq!(started(&hit(&mut s)), vec![1], "3 of 4 = 75%");
        assert_eq!(s.targets().count(), 1);
    }

    #[test]
    fn accuracy_counts_only_the_phase_own_shots() {
        let mut s = Session::new(
            def(r#"
                [[phases]]
                advance = { kind = "kills", kills = 1 }
                [[phases]]
                advance = { kind = "accuracy", accuracy = 1.0, min_shots = 2 }
                [[phases]]
            "#),
            3,
        );
        miss(&mut s);
        assert_eq!(started(&hit(&mut s)), vec![1]);
        hit(&mut s);
        assert_eq!(
            started(&hit(&mut s)),
            vec![2],
            "the phase-1 miss does not count"
        );
    }

    #[test]
    fn a_wave_spawns_its_count_and_advances_once_cleared() {
        let mut s = Session::new(
            def(r#"
                [[phases]]
                count = 2
                advance = { kind = "cleared" }
                [[phases]]
            "#),
            4,
        );
        assert_eq!(
            s.targets().count(),
            2,
            "the wave holds 2 even with max_alive 3"
        );
        assert!(started(&hit(&mut s)).is_empty());
        assert_eq!(s.targets().count(), 1, "no refill past the wave count");
        assert_eq!(started(&hit(&mut s)), vec![1]);
        assert_eq!(s.targets().count(), 3);
    }

    #[test]
    fn an_expired_target_also_ends_its_wave() {
        let mut s = Session::new(
            def(r#"
                [[phases]]
                count = 1
                advance = { kind = "cleared" }
                targets = { lifetime_secs = 0.5 }
                [[phases]]
            "#),
            5,
        );
        let events = wait(&mut s, 3);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, SessionEvent::Expired { .. }))
        );
        assert_eq!(started(&events), vec![1]);
        assert_eq!(s.stats().expired, 1);
    }

    #[test]
    fn a_time_trigger_advances_after_its_seconds() {
        let mut s = Session::new(
            def(r#"
                [[phases]]
                advance = { kind = "time", secs = 1.0 }
                [[phases]]
            "#),
            6,
        );
        assert!(started(&wait(&mut s, 4)).is_empty(), "0.8 s");
        assert_eq!(started(&wait(&mut s, 1)), vec![1], "1.0 s");
    }

    #[test]
    fn the_last_phase_trigger_completes_the_run() {
        let mut s = Session::new(
            def(r#"
                [[phases]]
                advance = { kind = "kills", kills = 1 }
            "#),
            7,
        );
        assert_eq!(s.outcome(), None);
        let events = hit(&mut s);
        assert_eq!(events.last(), Some(&SessionEvent::Finished));
        assert!(s.is_finished());
        assert_eq!(s.outcome(), Some(RunOutcome::Completed));
        assert_eq!(s.phase_index(), Some(0));
        assert!(s.result().duration_secs < 1.0);
    }

    #[test]
    fn too_many_misses_fail_the_run() {
        let mut s = Session::new(def("[fail]\nmax_misses = 2"), 8);
        assert_eq!(failure(&miss(&mut s)), None);
        hit(&mut s);
        let events = miss(&mut s);
        assert_eq!(failure(&events), Some(FailReason::Misses));
        assert_eq!(events.last(), Some(&SessionEvent::Finished));
        assert!(s.is_finished());
        assert_eq!(s.outcome(), Some(RunOutcome::Failed(FailReason::Misses)));
        assert!((s.result().duration_secs - 0.6).abs() < 1e-4);
        assert!(s.update(frame(ViewAngles::default(), true)).is_empty());
    }

    #[test]
    fn too_many_expired_targets_fail_the_run() {
        let mut d = parse("[fail]\nmax_expired = 2");
        d.targets.lifetime_secs = Some(0.5);
        d.targets.max_alive = 1;
        assert!(d.validate().is_empty(), "{:?}", d.validate());
        let mut s = Session::new(d, 9);
        let events = wait(&mut s, 10);
        assert_eq!(failure(&events), Some(FailReason::Expired));
        assert_eq!(s.stats().expired, 2);
        assert_eq!(s.outcome(), Some(RunOutcome::Failed(FailReason::Expired)));
    }

    #[test]
    fn a_phase_past_its_time_limit_fails_the_run() {
        let mut s = Session::new(
            def(r#"
                [[phases]]
                advance = { kind = "kills", kills = 5 }
                time_limit_secs = 1.0
                [[phases]]
            "#),
            10,
        );
        assert_eq!(failure(&wait(&mut s, 4)), None);
        assert_eq!(failure(&wait(&mut s, 1)), Some(FailReason::PhaseTimeLimit));
        assert!((s.time() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn a_trigger_reached_on_the_time_limit_frame_still_advances() {
        let mut s = Session::new(
            def(r#"
                [[phases]]
                advance = { kind = "kills", kills = 1 }
                time_limit_secs = 0.2
                [[phases]]
            "#),
            11,
        );
        let events = hit(&mut s);
        assert_eq!(started(&events), vec![1]);
        assert_eq!(failure(&events), None);
    }

    #[test]
    fn a_kills_ended_run_completes_at_its_kills_or_fails_at_the_cap() {
        let kills = "[end]\nkind = \"kills\"\nkills = 2";
        let mut s = Session::new(def(kills), 12);
        hit(&mut s);
        assert!(!s.is_finished());
        assert_eq!(hit(&mut s).last(), Some(&SessionEvent::Finished));
        assert_eq!(s.outcome(), Some(RunOutcome::Completed));
        assert!((s.result().duration_secs - 0.4).abs() < 1e-4);

        let mut s = Session::new(def(kills), 12);
        let events = wait(&mut s, 60);
        assert_eq!(failure(&events), Some(FailReason::OutOfTime));
        assert!((s.time() - 10.0).abs() < 1e-4, "duration_secs is the cap");
    }

    #[test]
    fn an_endless_run_lasts_until_a_fail_condition_or_its_cap() {
        let endless = "[end]\nkind = \"endless\"\n[fail]\nmax_misses = 1";
        let mut s = Session::new(def(endless), 13);
        wait(&mut s, 25);
        assert!(!s.is_finished(), "5 s in, no fail yet");
        assert_eq!(failure(&miss(&mut s)), Some(FailReason::Misses));

        let mut s = Session::new(def(endless), 13);
        let events = wait(&mut s, 60);
        assert_eq!(failure(&events), None);
        assert_eq!(s.outcome(), Some(RunOutcome::Completed));
        assert!((s.time() - 10.0).abs() < 1e-4);
    }

    #[test]
    fn a_time_ended_run_is_completed_at_its_duration() {
        let mut s = Session::new(def(""), 14);
        wait(&mut s, 45);
        assert_eq!(s.outcome(), None);
        assert_eq!(wait(&mut s, 10).last(), Some(&SessionEvent::Finished));
        assert!((s.time() - 10.0).abs() < 1e-4);
        assert_eq!(s.outcome(), Some(RunOutcome::Completed));
        assert_eq!(s.phase_index(), None);
    }

    #[test]
    fn phased_runs_are_deterministic_per_seed() {
        let d = def(r#"
            [fail]
            max_misses = 6
            [[phases]]
            count = 3
            advance = { kind = "cleared" }
            [[phases]]
            targets = { max_alive = 1, spawn = { kind = "angular", yaw_deg = [-60, 60], pitch_deg = [0, 5], distance = [20, 30] } }
            advance = { kind = "time", secs = 2.0 }
            [[phases]]
            targets = { motion = { kind = "drift", speed = [1, 2], change_secs = [0.3, 0.6] } }
        "#);
        let play = |seed| {
            let mut s = Session::new(d.clone(), seed);
            let (mut events, mut positions) = (Vec::new(), Vec::new());
            for i in 0..40 {
                events.extend(if i % 3 == 2 {
                    miss(&mut s)
                } else {
                    hit(&mut s)
                });
                positions.extend(s.targets().map(|t| t.position));
                if s.is_finished() {
                    break;
                }
            }
            (events, positions, s.result(), s.outcome())
        };
        assert_eq!(play(21), play(21));
        assert!(started(&play(21).0).contains(&2), "reaches the last phase");
    }

    #[test]
    fn run_rules_default_off_and_stay_out_of_serialized_definitions() {
        let d = def("");
        assert_eq!(
            (d.end, d.fail, d.phases.is_none()),
            (RunEnd::Time, FailDef::default(), true)
        );
        let toml = toml::to_string(&d).expect("serializes");
        for key in ["end", "fail", "phases"] {
            assert!(!toml.contains(&format!("[{key}")), "{toml}");
        }
    }

    #[test]
    fn misspelled_phase_fields_are_rejected() {
        let src = format!("{BASE}\n[[phases]]\nadvanse = {{ kind = \"cleared\" }}");
        assert!(ScenarioDef::from_toml(&src).is_err());
        let src = format!("{BASE}\n[[phases]]\ntargets = {{ spawn_rules = {{}} }}");
        assert!(ScenarioDef::from_toml(&src).is_err(), "not an override");
        let src = format!("{BASE}\n[end]\nkind = \"kills\"\nkils = 3");
        assert!(ScenarioDef::from_toml(&src).is_err());
    }

    #[test]
    fn validation_rejects_empty_and_impossible_phase_lists() {
        let mut empty = parse("");
        empty.phases = Some(Vec::new());
        let empty: Vec<_> = empty.validate().into_iter().map(|e| e.field).collect();
        assert_eq!(empty, vec!["phases"]);
        assert_eq!(
            fields("[[phases]]\n[[phases]]"),
            vec!["phases.advance"],
            "the first phase could never end"
        );
        assert_eq!(
            fields("[[phases]]\ncount = 3\nadvance = { kind = \"kills\", kills = 3 }"),
            vec!["phases.count"]
        );
        assert_eq!(
            fields("[[phases]]\nadvance = { kind = \"cleared\" }"),
            vec!["phases.count"],
            "a wave without a size never clears"
        );
        assert_eq!(
            fields("[[phases]]\ncount = 0\nadvance = { kind = \"cleared\" }"),
            vec!["phases.count"]
        );
        assert_eq!(
            fields("[[phases]]\ntime_limit_secs = 5"),
            vec!["phases.time_limit_secs"],
            "a limit without a trigger always fails"
        );
        assert_eq!(
            fields("[[phases]]\nadvance = { kind = \"time\", secs = 5 }\ntime_limit_secs = 5"),
            vec!["phases.time_limit_secs"],
            "the limit would fail the phase before its time trigger"
        );
    }

    #[test]
    fn validation_checks_trigger_values() {
        let trigger = |t: &str| fields(&format!("[[phases]]\nadvance = {t}"));
        assert_eq!(
            trigger("{ kind = \"kills\", kills = 0 }"),
            vec!["phases.advance.kills"]
        );
        assert_eq!(
            trigger("{ kind = \"accuracy\", accuracy = 0, min_shots = 5 }"),
            vec!["phases.advance.accuracy"]
        );
        assert_eq!(
            trigger("{ kind = \"accuracy\", accuracy = 1.2, min_shots = 5 }"),
            vec!["phases.advance.accuracy"]
        );
        assert_eq!(
            trigger("{ kind = \"accuracy\", accuracy = 0.8, min_shots = 0 }"),
            vec!["phases.advance.accuracy"]
        );
        assert_eq!(
            trigger("{ kind = \"time\", secs = 0 }"),
            vec!["phases.advance.secs"]
        );
        let mut hold =
            parse("[[phases]]\nadvance = { kind = \"accuracy\", accuracy = 0.8, min_shots = 5 }");
        hold.weapon.fire = FireMode::Hold;
        let hold: Vec<_> = hold.validate().into_iter().map(|e| e.field).collect();
        assert_eq!(hold, vec!["phases.advance.accuracy"]);
    }

    #[test]
    fn phase_targets_go_through_the_target_checks_once() {
        let errs = parse(
            "[[phases]]\nadvance = { kind = \"kills\", kills = 1 }\n\
             targets = { max_alive = 0 }\n[[phases]]\ntargets = { lifetime_secs = -1 }",
        )
        .validate();
        let messages: Vec<_> = errs.iter().map(|e| (e.field, e.message.as_str())).collect();
        assert_eq!(
            messages,
            vec![
                (
                    "phases.targets",
                    "phase 1: targets.max_alive: must be between 1 and 32"
                ),
                (
                    "phases.targets",
                    "phase 2: targets.lifetime_secs: must be > 0"
                ),
            ]
        );
        assert_eq!(
            fields("[[phases]]\ntargets = { shape = { kind = \"humanoid\" } }"),
            vec!["phases.targets.shape"]
        );
        // A problem of the base targets is reported by the base only.
        let mut d = parse("[[phases]]\n[[phases]]\n");
        d.phases.as_mut().unwrap()[0].advance = Some(PhaseTrigger::Kills { kills: 1 });
        d.targets.max_alive = 0;
        let fields: Vec<_> = d.validate().into_iter().map(|e| e.field).collect();
        assert_eq!(fields, vec!["targets.max_alive"]);
    }

    #[test]
    fn validation_checks_end_and_fail_conditions() {
        assert_eq!(
            fields("[end]\nkind = \"kills\"\nkills = 0"),
            vec!["end.kills"]
        );
        assert_eq!(
            fields("[end]\nkind = \"endless\""),
            vec!["end"],
            "an endless run needs a way to fail"
        );
        assert!(
            fields(
                "[end]\nkind = \"endless\"\n[[phases]]\n\
                 advance = { kind = \"kills\", kills = 5 }\ntime_limit_secs = 9"
            )
            .is_empty()
        );
        assert_eq!(fields("[fail]\nmax_misses = 0"), vec!["fail.max_misses"]);
        assert_eq!(
            fields("[fail]\nmax_expired = 2"),
            vec!["fail.max_expired"],
            "nothing can expire without a lifetime"
        );
        assert!(
            fields("[fail]\nmax_expired = 2\n[[phases]]\ntargets = { lifetime_secs = 1 }")
                .is_empty()
        );
        let mut hold = parse("[fail]\nmax_misses = 3");
        hold.weapon.fire = FireMode::Hold;
        hold.targets.health = 1.0;
        let hold: Vec<_> = hold.validate().into_iter().map(|e| e.field).collect();
        assert_eq!(hold, vec!["fail.max_misses"]);
    }

    #[test]
    fn transforms_reach_phase_overrides_resolved_against_the_base() {
        let mut d = def(r#"
            [[phases]]
            targets = { shape = { kind = "sphere", radius = 0.5 }, lifetime_secs = 2.0 }
            advance = { kind = "kills", kills = 1 }
            [[phases]]
            targets = { max_alive = 1 }
        "#);
        d.transform_targets(|t| {
            if let Shape::Sphere { radius } = &mut t.shape {
                *radius *= 0.5;
            }
            t.max_alive += 1;
        });
        let phases = d.phases.as_ref().unwrap();
        assert_eq!(radius(&d.targets), 0.15);
        assert_eq!(
            phases[0].targets.shape,
            Some(Shape::Sphere { radius: 0.25 })
        );
        assert_eq!(
            phases[0].targets.lifetime_secs,
            Some(2.0),
            "untouched by this transform"
        );
        assert_eq!(
            phases[0].targets.max_alive, None,
            "unset overrides stay unset"
        );
        assert_eq!(phases[1].targets.max_alive, Some(2));
        assert_eq!(phases[1].targets.shape, None);
    }

    fn builtin(id: &str) -> ScenarioDef {
        crate::scenario::builtin::all()
            .into_iter()
            .find(|d| d.id == id)
            .expect("builtin exists")
    }

    /// Phase reached (0-based) and outcome of a simulated run.
    fn simulated(def: &ScenarioDef, profile: usize, seed: u64) -> (usize, Option<RunOutcome>) {
        let mut s = Session::new(def.clone(), seed);
        let mut aimer = crate::sim::SimAimer::new(crate::sim::AimerProfile::PRESETS[profile], seed);
        while !s.is_finished() {
            let input = aimer.input(&s, 1.0 / 240.0);
            s.update(input);
        }
        (s.phase_index().unwrap_or(0), s.outcome())
    }

    #[test]
    fn the_peek_ladder_chains_waves_with_triggers_and_fails_on_misses() {
        let d = builtin("phase-peek-ladder");
        let phases = d.phases.as_ref().unwrap();
        assert!(phases.len() >= 2);
        assert!(
            phases
                .iter()
                .any(|p| p.advance == Some(PhaseTrigger::Cleared))
        );
        assert!(phases.iter().any(|p| p.time_limit_secs.is_some()));
        assert!(d.fail.max_misses.is_some());
        let (contender, _) = simulated(&d, 1, 3);
        assert!(contender >= 1, "a contender gets past the first wave");
        let (elite, outcome) = simulated(&d, 3, 3);
        assert_eq!(elite, phases.len() - 1, "an elite reaches the finale");
        assert!(outcome.is_some());
    }

    #[test]
    fn the_kill_race_ends_at_its_kills_after_an_accuracy_gated_warm_up() {
        let d = builtin("phase-kill-race");
        assert!(matches!(d.end, RunEnd::Kills { .. }));
        let first = &d.phases.as_ref().unwrap()[0];
        assert!(matches!(first.advance, Some(PhaseTrigger::Accuracy { .. })));
        let (phase, outcome) = simulated(&d, 3, 3);
        assert_eq!((phase, outcome), (1, Some(RunOutcome::Completed)));
    }
}
