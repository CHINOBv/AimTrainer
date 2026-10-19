//! Simulated aimer used to test that scenarios are playable and to generate provisional
//! benchmark thresholds before real player data exists.
//!
//! Model (see `docs/training-science.md`):
//! - Clicking: after a reaction delay, a minimum-jerk movement towards the target whose
//!   duration follows Fitts' law, `MT = a + b·log2(D/W + 1)`, landing with Gaussian endpoint
//!   error proportional to the target's angular size. Misses trigger a corrective sub-movement.
//! - Tracking: a first-order lag on the target's direction (`gain` per second), which yields
//!   the velocity-dependent following error real players show.

use crate::math::{Vec3, ViewAngles, angle_between_deg, wrap_degrees};
use crate::rng::Rng;
use crate::scenario::{FireMode, FrameInput, ScenarioDef, Session, Shape, Target};

/// Skill parameters of a simulated aimer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AimerProfile {
    /// Preset name.
    pub name: &'static str,
    /// Delay in seconds before starting a movement towards a new target.
    pub reaction_secs: f32,
    /// Fitts' law intercept, seconds.
    pub fitts_a: f32,
    /// Fitts' law slope, seconds per bit.
    pub fitts_b: f32,
    /// Endpoint error standard deviation as a fraction of the target's angular radius.
    pub endpoint_noise: f32,
    /// Tracking gain in 1/s (higher follows tighter).
    pub tracking_gain: f32,
}

impl AimerProfile {
    /// Built-in skill levels, from weakest to strongest.
    pub const PRESETS: [AimerProfile; 4] = [
        AimerProfile {
            name: "recruit",
            reaction_secs: 0.32,
            fitts_a: 0.15,
            fitts_b: 0.20,
            endpoint_noise: 0.9,
            tracking_gain: 6.0,
        },
        AimerProfile {
            name: "contender",
            reaction_secs: 0.27,
            fitts_a: 0.12,
            fitts_b: 0.16,
            endpoint_noise: 0.7,
            tracking_gain: 9.0,
        },
        AimerProfile {
            name: "veteran",
            reaction_secs: 0.23,
            fitts_a: 0.10,
            fitts_b: 0.13,
            endpoint_noise: 0.5,
            tracking_gain: 13.0,
        },
        AimerProfile {
            name: "elite",
            reaction_secs: 0.20,
            fitts_a: 0.08,
            fitts_b: 0.10,
            endpoint_noise: 0.35,
            tracking_gain: 18.0,
        },
    ];

    /// Go/no-go: chance of clicking a decoy instead of recognizing it. An estimate that
    /// grows with endpoint noise (recruit ≈ 22%, elite ≈ 9%); no player data backs it yet.
    pub fn decoy_mistake_chance(&self) -> f32 {
        0.25 * self.endpoint_noise
    }

    /// Go/no-go: extra delay to tell a match target from a decoy before moving, half the
    /// simple reaction time (an estimate in the spirit of Hick's law: a choice is slower
    /// than a simple reaction).
    pub fn recognition_secs(&self) -> f32 {
        0.5 * self.reaction_secs
    }

    /// Fitts' law movement time, seconds, for a flick of `distance_deg` to a target of
    /// angular radius `radius_deg`: `a + b·log2(D/W + 1)` with `W` the target's width.
    pub fn fitts_secs(&self, distance_deg: f32, radius_deg: f32) -> f32 {
        self.fitts_a + self.fitts_b * (distance_deg / (2.0 * radius_deg) + 1.0).log2()
    }
}

/// Minimum-jerk position profile (Flash & Hogan 1985): the share of a movement done at
/// normalized time `tau` in [0, 1], `s = τ³(10 − 15τ + 6τ²)`.
pub fn minimum_jerk(tau: f32) -> f32 {
    tau * tau * tau * (10.0 - 15.0 * tau + 6.0 * tau * tau)
}

struct Plan {
    target: u64,
    start_at: f32,
    duration: f32,
    from: ViewAngles,
    error: (f32, f32),
    shot: bool,
}

/// A simulated player that produces `FrameInput`s for a `Session`.
pub struct SimAimer {
    profile: AimerProfile,
    rng: Rng,
    view: ViewAngles,
    plan: Option<Plan>,
    /// Go/no-go decoys the aimer recognized and decided to leave alone.
    ignored: Vec<u64>,
    /// Detection drills: the last flash the aimer clicked.
    answered: Option<u64>,
}

impl SimAimer {
    /// Creates an aimer with the given skill and RNG seed.
    pub fn new(profile: AimerProfile, seed: u64) -> Self {
        Self {
            profile,
            rng: Rng::new(seed ^ 0xA1A1),
            view: ViewAngles::default(),
            plan: None,
            ignored: Vec::new(),
            answered: None,
        }
    }

    /// The next target to click. Without decoys it is the nearest target. With decoys the
    /// aimer looks at the nearest target it has not dismissed; a decoy is dismissed unless
    /// the aimer mistakes it for a match. Random draws happen only when decoys are on, so
    /// other scenarios keep their exact results.
    fn pick(&mut self, session: &Session) -> Option<Target> {
        if !has_decoys(session.def()) {
            return self.nearest(session);
        }
        self.ignored
            .retain(|id| session.targets().any(|t| t.id == *id));
        loop {
            let t = self.nearest_where(session, |t| !self.ignored.contains(&t.id))?;
            if !t.decoy || self.rng.next_f32() < self.profile.decoy_mistake_chance() {
                return Some(t);
            }
            self.ignored.push(t.id);
        }
    }

    fn gaussian(&mut self) -> f32 {
        let u1 = self.rng.next_f32().max(1e-6);
        let u2 = self.rng.next_f32();
        (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
    }

    /// Point the aimer wants to hit and its angular radius (degrees).
    fn aim(session: &Session, t: &Target) -> (Vec3, f32) {
        let eye = session.eye_position();
        // Shrinking targets are aimed at at their current size; others at the defined one.
        let shape = if session.def().targets.shrink_per_kill.is_some() {
            &session.active_targets().shape
        } else {
            &session.def().targets.shape
        };
        match *shape {
            Shape::Sphere { radius } => {
                let d = (t.position - eye).length();
                (t.position, (radius / d).atan().to_degrees())
            }
            Shape::Humanoid { .. } => {
                let dims = session.target_dims(t);
                let head = t.position + Vec3::new(0.0, dims.head_center_height(), 0.0);
                let d = (head - eye).length();
                (head, (dims.head_size / 2.0 / d).atan().to_degrees())
            }
            Shape::Capsule { .. } => {
                let (up, r) = shape.capsule_aim().expect("a capsule");
                let p = t.position + Vec3::UP * up;
                let d = (p - eye).length();
                (p, (r / d).atan().to_degrees())
            }
        }
    }

    fn angles_to(session: &Session, p: Vec3) -> ViewAngles {
        ViewAngles::from_direction(p - session.eye_position())
    }

    fn nearest(&self, session: &Session) -> Option<Target> {
        self.nearest_where(session, |_| true)
    }

    fn nearest_where(&self, session: &Session, keep: impl Fn(&Target) -> bool) -> Option<Target> {
        let dir = self.view.direction();
        session
            .targets()
            .filter(|t| keep(t))
            .min_by(|a, b| {
                let da = angle_between_deg(dir, Self::aim(session, a).0 - session.eye_position());
                let db = angle_between_deg(dir, Self::aim(session, b).0 - session.eye_position());
                da.total_cmp(&db)
            })
            .cloned()
    }

    /// Produce this frame's input.
    pub fn input(&mut self, session: &Session, dt: f32) -> FrameInput {
        match session.def().weapon.fire {
            FireMode::Hold => self.track(session, dt),
            FireMode::Click if is_detection(session.def()) => self.detect(session, dt),
            FireMode::Click => self.click(session, dt),
        }
    }

    fn track(&mut self, session: &Session, dt: f32) -> FrameInput {
        if let Some(t) = self.nearest(session) {
            let goal = Self::angles_to(session, Self::aim(session, &t).0);
            let k = 1.0 - (-self.profile.tracking_gain * dt).exp();
            let dy = wrap_degrees(goal.yaw_deg - self.view.yaw_deg);
            let dp = goal.pitch_deg - self.view.pitch_deg;
            self.view = self.view.rotated(dy * k, dp * k);
        }
        FrameInput {
            dt,
            view: self.view,
            fire_held: true,
            ..Default::default()
        }
    }

    /// Detection drills: keep the crosshair still and click once per flash, a reaction
    /// time after it appears. With no flash showing the aimer sometimes clicks anyway, at
    /// [`AimerProfile::decoy_mistake_chance`] times per second (an estimate). Random draws
    /// happen only here, so other scenarios keep their exact results.
    fn detect(&mut self, session: &Session, dt: f32) -> FrameInput {
        let now = session.time();
        let fire = match session.targets().next() {
            Some(t) => {
                let due = now >= t.visible_at + self.profile.reaction_secs;
                let fire = due && self.answered != Some(t.id);
                if fire {
                    self.answered = Some(t.id);
                }
                fire
            }
            None => self.rng.next_f32() < self.profile.decoy_mistake_chance() * dt,
        };
        FrameInput {
            dt,
            view: self.view,
            fire_pressed: fire,
            fire_held: fire,
            ..Default::default()
        }
    }

    fn click(&mut self, session: &Session, dt: f32) -> FrameInput {
        let now = session.time();
        let current = self
            .plan
            .as_ref()
            .and_then(|p| session.targets().find(|t| t.id == p.target).cloned());
        let target = match current {
            Some(t) => t,
            None => {
                self.plan = None;
                match self.pick(session) {
                    Some(t) => t,
                    None => {
                        return FrameInput {
                            dt,
                            view: self.view,
                            ..Default::default()
                        };
                    }
                }
            }
        };

        let (point, radius) = Self::aim(session, &target);
        let goal = Self::angles_to(session, point);
        if self.plan.as_ref().is_none_or(|p| p.shot) {
            // Plan a (corrective) sub-movement. A correction reacts faster than a fresh target.
            let corrective = self.plan.is_some();
            let distance = angle_between_deg(self.view.direction(), goal.direction());
            let fitts = self.profile.fitts_secs(distance, radius);
            let sd = self.profile.endpoint_noise * radius;
            let error = (self.gaussian() * sd, self.gaussian() * sd);
            let reaction = if corrective {
                self.profile.reaction_secs * 0.5
            } else if has_decoys(session.def()) {
                self.profile.reaction_secs + self.profile.recognition_secs()
            } else {
                self.profile.reaction_secs
            };
            let start_at = (target.visible_at.max(now - dt)) + reaction;
            self.plan = Some(Plan {
                target: target.id,
                start_at,
                duration: fitts,
                from: self.view,
                error,
                shot: false,
            });
        }

        let plan = self.plan.as_mut().expect("plan set above");
        let tau = ((now - plan.start_at) / plan.duration).clamp(0.0, 1.0);
        let s = minimum_jerk(tau);
        let dy = wrap_degrees(goal.yaw_deg + plan.error.0 - plan.from.yaw_deg);
        let dp = goal.pitch_deg + plan.error.1 - plan.from.pitch_deg;
        self.view = plan.from.rotated(dy * s, dp * s);
        // With recoil on, every profile except the recruit waits for the gun to recover so
        // the shot is a fresh first bullet (the habit the burst drills teach).
        let recovering = self.profile.name != "recruit"
            && session.recoil_recovery_remaining().is_some_and(|r| r > 0.0);
        let fire = tau >= 1.0 && !recovering;
        if fire {
            plan.shot = true;
        }
        FrameInput {
            dt,
            view: self.view,
            fire_pressed: fire,
            fire_held: fire,
            ..Default::default()
        }
    }
}

/// Whether `def` has decoys to recognize: go/no-go decoys, or the targets a radar did not
/// show (the aimer's memory of the radar errs like its recognition of a decoy).
fn has_decoys(def: &ScenarioDef) -> bool {
    def.targets.decoy_chance > 0.0
        || def
            .targets
            .perception
            .as_ref()
            .is_some_and(|p| p.is_radar())
}

fn is_detection(def: &ScenarioDef) -> bool {
    def.targets
        .perception
        .as_ref()
        .is_some_and(|p| p.is_detection())
}

/// Run a whole scenario with a simulated aimer at a fixed timestep.
pub fn simulate(def: &ScenarioDef, profile: AimerProfile, seed: u64) -> crate::scenario::RunResult {
    const DT: f32 = 1.0 / 240.0;
    let mut session = Session::new(def.clone(), seed);
    let mut aimer = SimAimer::new(profile, seed);
    while !session.is_finished() {
        let input = aimer.input(&session, DT);
        session.update(input);
    }
    session.result()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::builtin;

    #[test]
    fn every_builtin_is_playable_and_better_aimers_score_higher() {
        for def in builtin::all() {
            let scores: Vec<f32> = AimerProfile::PRESETS
                .iter()
                .map(|p| simulate(&def, *p, 11).score)
                .collect();
            assert!(
                scores[1] > 0.0,
                "{}: contender scored nothing ({scores:?})",
                def.id
            );
            assert!(
                scores[3] > scores[0],
                "{}: elite should beat recruit ({scores:?})",
                def.id
            );
        }
    }

    #[test]
    fn simulation_is_deterministic_per_seed() {
        let def = builtin::all()
            .into_iter()
            .find(|d| d.id == "micro-adjust")
            .unwrap();
        let p = AimerProfile::PRESETS[1];
        assert_eq!(simulate(&def, p, 5), simulate(&def, p, 5));
    }

    fn builtin(id: &str) -> ScenarioDef {
        builtin::all().into_iter().find(|d| d.id == id).unwrap()
    }

    /// Scores recorded before score rings and decoys existed: scenarios that use neither
    /// must keep their random sequences, and so their exact results.
    #[test]
    fn scenarios_without_decoys_simulate_exactly_as_before() {
        let contender = AimerProfile::PRESETS[1];
        for (id, score, kills, expired) in [
            ("micro-adjust", 5850.0, 67, 0),
            ("switch-heads", 5300.0, 57, 0),
            ("peek-reaction", 1900.0, 22, 2),
            ("dodge-peek", 2200.0, 24, 0),
            ("track-precise", 2681.6558, 0, 0),
        ] {
            let r = simulate(&builtin(id), contender, 9);
            assert!((r.score - score).abs() < 1e-2, "{id}: {}", r.score);
            assert_eq!((r.kills, r.expired), (kills, expired), "{id}");
        }
    }

    /// Results recorded before reactive targets, conditional behaviours, spawn rules,
    /// regeneration and score decay existed. One scenario per spawn area, motion, fire mode
    /// and weapon feature: scenarios that use none of the new blocks must keep their random
    /// sequences, and so their exact results.
    #[test]
    fn scenarios_without_the_target_behaviour_blocks_simulate_exactly_as_before() {
        let contender = AimerProfile::PRESETS[1];
        let mut changed = Vec::new();
        for (id, score, kills, shots, hits) in PINNED {
            let mut session = Session::new(builtin(id), 9);
            let mut aimer = SimAimer::new(contender, 9);
            while !session.is_finished() {
                let input = aimer.input(&session, 1.0 / 240.0);
                session.update(input);
            }
            let s = session.stats();
            let now = (s.score.to_bits(), s.kills, s.shots, s.hits);
            if now != (score.to_bits(), kills, shots, hits) {
                changed.push(format!(
                    "(\"{id}\", {:?}, {}, {}, {})",
                    s.score, s.kills, s.shots, s.hits
                ));
            }
        }
        assert!(changed.is_empty(), "{changed:#?}");
    }

    /// `(id, score, kills, shots, hits)` of a contender run with seed 9.
    const PINNED: [(&str, f32, u32, u32, u32); 9] = [
        ("flick-wide", 4460.0, 49, 71, 49),
        ("switch-wide", 5265.0, 57, 86, 57),
        ("counter-strafe", 4420.0, 47, 75, 47),
        ("track-orbit", 3189.722, 0, 0, 0),
        ("dodge-juke-heads", 4070.0, 51, 65, 51),
        ("go-no-go", 150.0, 15, 41, 15),
        ("precision-rings", 4525.0, 64, 93, 64),
        ("spray-transfer", 6950.0, 8, 85, 67),
        ("track-reactive-strafe", 1749.9491, 0, 0, 0),
    ];

    #[test]
    fn perception_drills_simulate_deterministically_and_better_aimers_false_alarm_less() {
        let p = AimerProfile::PRESETS[1];
        for id in ["sound-turn", "peripheral-detect", "radar-recall"] {
            assert_eq!(
                simulate(&builtin(id), p, 5),
                simulate(&builtin(id), p, 5),
                "{id}"
            );
        }
        let false_alarms = |profile: AimerProfile| {
            (1..=5)
                .map(|seed| {
                    let mut session = Session::new(builtin("peripheral-detect"), seed);
                    let mut aimer = SimAimer::new(profile, seed);
                    while !session.is_finished() {
                        let input = aimer.input(&session, 1.0 / 240.0);
                        session.update(input);
                    }
                    session.stats().false_alarms
                })
                .sum::<u32>()
        };
        let (recruit, elite) = (
            false_alarms(AimerProfile::PRESETS[0]),
            false_alarms(AimerProfile::PRESETS[3]),
        );
        assert!(recruit > elite && elite > 0, "{recruit} vs {elite}");
    }

    /// Decoys shot and decoys left alone over five seeds.
    fn decoy_choices(profile: AimerProfile) -> (u32, u32) {
        let def = builtin("go-no-go");
        let (mut shot, mut waited) = (0, 0);
        for seed in 1..=5 {
            let mut session = Session::new(def.clone(), seed);
            let mut aimer = SimAimer::new(profile, seed);
            while !session.is_finished() {
                let input = aimer.input(&session, 1.0 / 240.0);
                session.update(input);
            }
            shot += session.stats().decoy_hits;
            waited += session.stats().decoy_waits;
        }
        (shot, waited)
    }

    #[test]
    fn the_aimer_mostly_leaves_decoys_alone_and_better_aimers_err_less() {
        let (recruit_shot, recruit_waited) = decoy_choices(AimerProfile::PRESETS[0]);
        let (elite_shot, elite_waited) = decoy_choices(AimerProfile::PRESETS[3]);
        assert!(
            recruit_waited > recruit_shot,
            "{recruit_shot}/{recruit_waited}"
        );
        assert!(elite_waited > elite_shot, "{elite_shot}/{elite_waited}");
        assert!(recruit_shot > elite_shot, "{recruit_shot} vs {elite_shot}");
        assert!(
            elite_shot > 0,
            "even an elite aimer sometimes clicks a decoy"
        );
    }
}
