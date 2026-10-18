//! Runtime simulation of one scenario run. The engine feeds view angles, buttons and
//! movement keys every frame and renders whatever `targets()` returns.
//!
//! [`Session`] owns the run state and orchestrates each frame; spawning, target motion,
//! weapons, recoil and scoring live in the sibling `spawn`, `motion`, `weapon`, `recoil`
//! and `scoring` modules.

use super::def::{FireMode, ScenarioDef};
use super::hit::{HitZone, Ray};
use super::motion::MotionState;
use super::recoil::RecoilState;
use super::{scoring, spawn, weapon};
use crate::games::valorant::{HumanoidDims, RecoilProfile, WeaponStats};
use crate::math::{Vec3, ViewAngles};
use crate::movement::{self, LocalVelocity, MoveInput, MovementProfile};
use crate::rng::Rng;
use serde::{Deserialize, Serialize};

/// Player input for one frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct FrameInput {
    /// Frame duration in seconds.
    pub dt: f32,
    /// View orientation this frame.
    pub view: ViewAngles,
    /// Fire button went down this frame.
    pub fire_pressed: bool,
    /// Fire button is held.
    pub fire_held: bool,
    /// Movement keys pressed.
    pub movement: MoveInput,
}

/// Something that happened during a session update.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    /// A target appeared.
    Spawned {
        /// Id of the new target.
        id: u64,
    },
    /// A shot was fired.
    Shot {
        /// Target id and zone hit, if any.
        hit: Option<(u64, HitZone)>,
        /// Whether the player was moving above the accuracy threshold.
        moving: bool,
    },
    /// A target was killed.
    Killed {
        /// Id of the killed target.
        id: u64,
        /// Seconds from the target becoming visible to its death.
        time_to_kill: f32,
    },
    /// A target was removed because its lifetime ran out.
    Expired {
        /// Id of the expired target.
        id: u64,
    },
    /// Ran out of ammo; firing is blocked for `secs`.
    Reload {
        /// Reload duration in seconds.
        secs: f32,
    },
    /// The run reached its duration.
    Finished,
}

/// A live target.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    /// Unique id within the session.
    pub id: u64,
    /// Sphere center, or humanoid feet.
    pub position: Vec3,
    /// Yaw the humanoid faces (towards the player at spawn).
    pub facing_yaw_deg: f32,
    /// Remaining health.
    pub health: f32,
    /// Health at spawn.
    pub max_health: f32,
    /// Session time in seconds when the target becomes visible.
    pub visible_at: f32,
    expires_at: Option<f32>,
    motion: MotionState,
}

impl Target {
    /// Whether the target is visible at session time `now` (seconds).
    pub fn is_visible(&self, now: f32) -> bool {
        now >= self.visible_at
    }

    /// Current velocity in m/s.
    pub fn velocity(&self) -> Vec3 {
        self.motion.velocity()
    }

    /// How far into a crouch the target is: 0 standing, 1 fully crouched. Only
    /// [`super::Motion::Dodge`] targets crouch.
    pub fn crouch(&self) -> f32 {
        self.motion.crouch
    }
}

/// Counters accumulated during a run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    /// Points scored so far.
    pub score: f32,
    /// Shots fired.
    pub shots: u32,
    /// Shots that hit a target.
    pub hits: u32,
    /// Hits on the head.
    pub head_hits: u32,
    /// Hits on the body.
    pub body_hits: u32,
    /// Hits on the legs.
    pub leg_hits: u32,
    /// Targets killed.
    pub kills: u32,
    /// Targets that expired.
    pub expired: u32,
    /// Shots fired while moving above the accuracy threshold.
    pub moving_shots: u32,
    /// Reloads forced by running out of ammo.
    pub reloads: u32,
    /// Seconds the fire button was held (hold mode).
    pub hold_secs: f32,
    /// Seconds the crosshair was on a target while firing (hold mode).
    pub on_target_secs: f32,
    /// Seconds the crosshair was on a head while firing (hold mode).
    pub head_on_target_secs: f32,
    /// Seconds from a target becoming visible to its death.
    pub kill_times: Vec<f32>,
    /// Recoil scenarios only: bursts fired. A burst is a run of shots with no pause as
    /// long as the weapon's recovery time, so it always starts with a fully recovered
    /// first bullet.
    #[serde(default)]
    pub bursts: u32,
    /// Recoil scenarios only: bullets in the longest burst.
    #[serde(default)]
    pub longest_burst: u32,
    /// Recoil scenarios only: shots per bullet-index bucket (see [`Stats::BULLET_BUCKETS`]).
    #[serde(default)]
    pub shots_by_bullet: [u32; 4],
    /// Recoil scenarios only: hits per bullet-index bucket (see [`Stats::BULLET_BUCKETS`]).
    #[serde(default)]
    pub hits_by_bullet: [u32; 4],
}

impl Stats {
    /// Labels of the bullet-index buckets of `shots_by_bullet` and `hits_by_bullet`.
    pub const BULLET_BUCKETS: [&'static str; 4] = ["1", "2-3", "4-6", "7+"];

    /// Bucket of a 1-based bullet index: 1, 2–3, 4–6 or 7+.
    pub fn bullet_bucket(bullet: u32) -> usize {
        match bullet {
            0 | 1 => 0,
            2..=3 => 1,
            4..=6 => 2,
            _ => 3,
        }
    }

    /// Shots fired in recoil bursts.
    pub fn burst_shots(&self) -> u32 {
        self.shots_by_bullet.iter().sum()
    }

    /// Average bullets per burst; 0 without bursts.
    pub fn avg_burst_len(&self) -> f32 {
        if self.bursts == 0 {
            0.0
        } else {
            self.burst_shots() as f32 / self.bursts as f32
        }
    }

    /// Share of burst shots that were fully recovered first bullets; 0 without bursts.
    pub fn first_bullet_share(&self) -> f32 {
        let shots = self.burst_shots();
        if shots == 0 {
            0.0
        } else {
            self.shots_by_bullet[0] as f32 / shots as f32
        }
    }
}

/// Final numbers of a run, ready to display and persist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunResult {
    /// Id of the scenario played.
    pub scenario_id: String,
    /// Points scored.
    pub score: f32,
    /// Hits / shots in click mode, time on target / time firing in hold mode.
    pub accuracy: f32,
    /// Targets killed.
    pub kills: u32,
    /// Kills per minute of run time.
    pub kills_per_min: f32,
    /// Mean seconds to kill, if any target was killed.
    pub avg_time_to_kill: Option<f32>,
    /// Fraction of hits that landed on the head.
    pub headshot_rate: f32,
    /// Shots fired while moving above the accuracy threshold.
    pub moving_shots: u32,
    /// Targets that expired.
    pub expired: u32,
    /// Run length in seconds.
    pub duration_secs: f32,
}

/// A running scenario: owns targets, player state and scoring, and advances deterministically from `FrameInput`s.
pub struct Session {
    def: ScenarioDef,
    rng: Rng,
    time: f32,
    finished: bool,
    targets: Vec<Target>,
    next_id: u64,
    stats: Stats,
    player_pos: Vec3,
    player_vel: LocalVelocity,
    view: ViewAngles,
    profile: MovementProfile,
    weapon: WeaponStats,
    dims: HumanoidDims,
    last_shot_at: f32,
    ammo: f32,
    reload_until: f32,
    /// Spray state when the scenario enables `weapon.recoil`.
    recoil: Option<RecoilState>,
}

impl Session {
    /// Starts a session of `def` with a seeded random generator.
    pub fn new(def: ScenarioDef, seed: u64) -> Self {
        let weapon = def.weapon.kind.stats();
        let profile = def.weapon.kind.movement();
        let recoil = def
            .weapon
            .recoil
            .then(|| def.weapon.kind.recoil())
            .flatten()
            .map(|r| RecoilState::new(r, weapon.first_shot_spread_deg));
        let start = Vec3::new(0.0, def.player.elevation, 0.0);
        let mut s = Self {
            def,
            rng: Rng::new(seed),
            time: 0.0,
            finished: false,
            targets: Vec::new(),
            next_id: 1,
            stats: Stats::default(),
            player_pos: start,
            player_vel: LocalVelocity::default(),
            view: ViewAngles::default(),
            profile,
            weapon,
            dims: HumanoidDims::default(),
            last_shot_at: f32::NEG_INFINITY,
            ammo: 0.0,
            reload_until: f32::NEG_INFINITY,
            recoil,
        };
        s.ammo = s.def.weapon.ammo.map_or(0.0, |a| a.clip);
        s.refill(&mut Vec::new());
        s
    }

    /// Scenario being played.
    pub fn def(&self) -> &ScenarioDef {
        &self.def
    }
    /// Seconds elapsed.
    pub fn time(&self) -> f32 {
        self.time
    }
    /// Seconds left before the run ends.
    pub fn remaining(&self) -> f32 {
        (self.def.duration_secs - self.time).max(0.0)
    }
    /// Whether the run has ended.
    pub fn is_finished(&self) -> bool {
        self.finished
    }
    /// Counters so far.
    pub fn stats(&self) -> &Stats {
        &self.stats
    }
    /// Humanoid hitbox dimensions in use.
    pub fn dims(&self) -> &HumanoidDims {
        &self.dims
    }

    /// Hitbox of humanoid target `t` right now: the standing [`Self::dims`] lowered by
    /// its crouch.
    pub fn target_dims(&self, t: &Target) -> HumanoidDims {
        self.dims.crouched(t.crouch())
    }
    /// Player feet position in meters.
    pub fn player_position(&self) -> Vec3 {
        self.player_pos
    }
    /// Player eye position in meters.
    pub fn eye_position(&self) -> Vec3 {
        self.player_pos + Vec3::new(0.0, self.dims.eye_height, 0.0)
    }
    /// Horizontal speed in m/s.
    pub fn player_speed(&self) -> f32 {
        self.player_vel.speed()
    }
    /// Movement parameters of the equipped weapon.
    pub fn movement_profile(&self) -> &MovementProfile {
        &self.profile
    }
    /// Whether the current speed allows fully accurate shots.
    pub fn is_accurate(&self) -> bool {
        self.player_speed() <= self.profile.accuracy_speed_limit()
    }

    /// Targets currently visible to the player.
    pub fn targets(&self) -> impl Iterator<Item = &Target> {
        let now = self.time;
        self.targets.iter().filter(move |t| t.is_visible(now))
    }

    /// Advances the session by one frame and returns what happened.
    pub fn update(&mut self, input: FrameInput) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        if self.finished {
            return events;
        }
        let dt = input.dt.clamp(0.0, 0.25).min(self.remaining());
        self.time += dt;
        self.view = input.view;

        self.move_player(input.movement, dt);
        self.move_targets(dt);
        match self.def.weapon.fire {
            FireMode::Click if input.fire_pressed => self.shoot(&mut events),
            // With recoil the weapon is full-auto: holding the button keeps firing.
            FireMode::Click if input.fire_held && self.recoil.is_some() => self.shoot(&mut events),
            FireMode::Hold if input.fire_held => self.hold_fire(dt, &mut events),
            _ => {}
        }
        self.expire(&mut events);
        self.refill(&mut events);

        if self.time >= self.def.duration_secs {
            self.finished = true;
            events.push(SessionEvent::Finished);
        }
        events
    }

    /// Final numbers of the run so far.
    pub fn result(&self) -> RunResult {
        scoring::run_result(&self.def, &self.stats, self.time)
    }

    fn move_player(&mut self, input: MoveInput, dt: f32) {
        if !self.def.player.movement {
            return;
        }
        self.player_vel = movement::step(&self.profile, self.player_vel, input, dt);
        let yaw = self.view.yaw_deg;
        let mut world = self.player_vel.to_world(yaw);
        let next = self.player_pos + world * dt;
        let (bx, bz) = (self.def.player.bounds_x, self.def.player.bounds_z);
        let clamped = Vec3::new(
            next.x.clamp(bx.min, bx.max),
            self.def.player.elevation,
            next.z.clamp(bz.min, bz.max),
        );
        // Hitting an edge of the play area behaves like a wall: only the velocity pushing
        // into it is lost, so the player keeps sliding along the edge.
        if clamped.x != next.x {
            world.x = 0.0;
        }
        if clamped.z != next.z {
            world.z = 0.0;
        }
        self.player_vel = LocalVelocity::from_world(world, yaw);
        self.player_pos = clamped;
    }

    fn move_targets(&mut self, dt: f32) {
        let now = self.time;
        let targets = &self.def.targets;
        for t in &mut self.targets {
            if t.is_visible(now) {
                t.motion
                    .step(&mut t.position, targets, &mut self.rng, now, dt);
            }
        }
    }

    fn shot_direction(&mut self) -> (Vec3, bool) {
        weapon::shot_direction(
            self.view,
            self.player_speed(),
            &self.profile,
            &self.weapon,
            self.def.weapon.simulate_spread,
            &mut self.rng,
        )
    }

    /// The target index and counted zone that a shot along `dir` from the eye hits.
    fn hit_along(&self, dir: Vec3) -> Option<(usize, HitZone)> {
        let ray = Ray {
            origin: self.eye_position(),
            dir,
        };
        let shape = &self.def.targets.shape;
        weapon::raycast(ray, &self.targets, shape, &self.dims, self.time)
            .filter(|(_, z)| scoring::counts(shape, *z))
    }

    /// Score-ring multiplier of a shot along `dir` that hit `zone` of target `idx`; 1
    /// without rings, so scenarios without rings score exactly as before.
    fn ring_multiplier(&self, dir: Vec3, idx: usize, zone: HitZone) -> f32 {
        let Some(rings) = &self.def.scoring.rings else {
            return 1.0;
        };
        let ray = Ray {
            origin: self.eye_position(),
            dir,
        };
        let shape = &self.def.targets.shape;
        weapon::zone_center_fraction(ray, &self.targets[idx], shape, &self.dims, zone)
            .map_or(1.0, |f| rings.multiplier_at(f))
    }

    fn shoot(&mut self, events: &mut Vec<SessionEvent>) {
        if !weapon::can_fire(&self.weapon, self.time, self.last_shot_at) || self.is_reloading() {
            return;
        }
        if let Some(a) = self.def.weapon.ammo {
            self.ammo -= a.cost;
        }
        self.last_shot_at = self.shot_time();
        let (dir, moving, bullet) = self.fire_bullet();
        scoring::record_shot(&mut self.stats, &self.def.scoring, moving);
        if let Some(b) = bullet {
            scoring::record_bullet(&mut self.stats, b);
        }

        let Some((idx, zone)) = self.hit_along(dir) else {
            scoring::record_miss(&mut self.stats, &self.def.scoring);
            events.push(SessionEvent::Shot { hit: None, moving });
            self.reload_if_dry(events);
            return;
        };
        let multiplier = self.ring_multiplier(dir, idx, zone);
        scoring::record_hit(&mut self.stats, &self.def.scoring, zone, multiplier);
        if let (Some(b), Some(r)) = (bullet, &self.recoil) {
            let protected = r.profile().protected_bullets;
            scoring::record_bullet_hit(&mut self.stats, &self.def.scoring, b, protected);
        }
        if let Some(a) = self.def.weapon.ammo {
            self.ammo = (self.ammo + a.refund).min(a.clip);
        }
        self.reload_if_dry(events);
        let id = self.targets[idx].id;
        events.push(SessionEvent::Shot {
            hit: Some((id, zone)),
            moving,
        });
        self.targets[idx].health -= 1.0;
        if self.targets[idx].health <= 0.0 {
            self.kill(idx, events);
        }
    }

    /// Time stamp of a shot fired this frame. A held full-auto trigger fires on the
    /// weapon's fixed cadence even when frames do not line up with it; every other shot
    /// is stamped with the current time.
    fn shot_time(&self) -> f32 {
        let interval = 1.0 / self.weapon.fire_rate;
        if self.recoil.is_some() && self.time - self.last_shot_at < 2.0 * interval {
            self.last_shot_at + interval
        } else {
            self.time
        }
    }

    /// Direction and moving flag of the shot fired now, plus its bullet index within the
    /// burst when recoil is on.
    fn fire_bullet(&mut self) -> (Vec3, bool, Option<u32>) {
        let speed = self.player_speed();
        let Some(recoil) = self.recoil.as_mut() else {
            let (dir, moving) = self.shot_direction();
            return (dir, moving, None);
        };
        let kick = recoil.fire(self.time, &mut self.rng);
        let (dir, moving) = weapon::recoil_direction(
            self.view,
            kick,
            speed,
            &self.profile,
            &self.weapon,
            self.def.weapon.simulate_spread,
            &mut self.rng,
        );
        (dir, moving, Some(kick.bullet))
    }

    /// The recoil model in use, when the scenario enables `weapon.recoil`.
    pub fn recoil_profile(&self) -> Option<&RecoilProfile> {
        self.recoil.as_ref().map(RecoilState::profile)
    }

    /// Index of the last bullet fired in the burst in progress (1 = first bullet). It is 0
    /// when the next shot is a fully recovered first bullet, and always 0 without recoil.
    pub fn burst_index(&self) -> u32 {
        self.recoil
            .as_ref()
            .map_or(0, |r| r.bullet_index(self.time))
    }

    /// Seconds until recoil fully resets (0 once recovered); `None` without recoil.
    pub fn recoil_recovery_remaining(&self) -> Option<f32> {
        self.recoil
            .as_ref()
            .map(|r| r.recovery_remaining(self.time))
    }

    /// Start a reload when the next shot cannot be paid for.
    fn reload_if_dry(&mut self, events: &mut Vec<SessionEvent>) {
        let Some(a) = self.def.weapon.ammo else {
            return;
        };
        if self.ammo < a.cost {
            self.ammo = a.clip;
            self.reload_until = self.time + a.reload_secs;
            self.stats.reloads += 1;
            events.push(SessionEvent::Reload {
                secs: a.reload_secs,
            });
        }
    }

    /// `(current, clip)` when the scenario uses an ammo economy.
    pub fn ammo(&self) -> Option<(f32, f32)> {
        self.def.weapon.ammo.map(|a| (self.ammo, a.clip))
    }

    /// Whether firing is blocked by a reload.
    pub fn is_reloading(&self) -> bool {
        self.time < self.reload_until
    }

    /// Share of attempts that succeeded, the signal adaptive difficulty steers to ~80%:
    /// hits over shots plus expired targets in click mode, time on target in hold mode.
    pub fn success_rate(&self) -> f32 {
        let s = &self.stats;
        match self.def.weapon.fire {
            FireMode::Hold => {
                if s.hold_secs > 0.0 {
                    s.on_target_secs / s.hold_secs
                } else {
                    0.0
                }
            }
            FireMode::Click => {
                let attempts = s.shots + s.expired;
                if attempts > 0 {
                    s.hits as f32 / attempts as f32
                } else {
                    0.0
                }
            }
        }
    }

    fn hold_fire(&mut self, dt: f32, events: &mut Vec<SessionEvent>) {
        self.stats.hold_secs += dt;
        let (dir, _) = self.shot_direction();
        let Some((idx, zone)) = self.hit_along(dir) else {
            return;
        };
        let multiplier = self.ring_multiplier(dir, idx, zone);
        scoring::record_on_target(&mut self.stats, &self.def.scoring, zone, dt, multiplier);
        self.targets[idx].health -= dt;
        if self.targets[idx].health <= 0.0 {
            self.kill(idx, events);
        }
    }

    fn kill(&mut self, idx: usize, events: &mut Vec<SessionEvent>) {
        let t = self.targets.swap_remove(idx);
        let time_to_kill = self.time - t.visible_at;
        scoring::record_kill(&mut self.stats, time_to_kill);
        events.push(SessionEvent::Killed {
            id: t.id,
            time_to_kill,
        });
    }

    fn expire(&mut self, events: &mut Vec<SessionEvent>) {
        let now = self.time;
        let scoring_def = &self.def.scoring;
        let stats = &mut self.stats;
        self.targets.retain(|t| {
            let alive = t.expires_at.is_none_or(|e| now < e);
            if !alive {
                scoring::record_expired(stats, scoring_def);
                events.push(SessionEvent::Expired { id: t.id });
            }
            alive
        });
    }

    fn refill(&mut self, events: &mut Vec<SessionEvent>) {
        while self.targets.len() < self.def.targets.max_alive as usize {
            let t = self.spawn_target();
            events.push(SessionEvent::Spawned { id: t.id });
            self.targets.push(t);
        }
    }

    fn spawn_target(&mut self) -> Target {
        let point = spawn::separated_point(
            &self.def.targets,
            &self.targets,
            self.eye_position(),
            self.view,
            &self.dims,
            &mut self.rng,
        );
        let (facing_yaw_deg, lateral) = spawn::face_player(point, self.player_pos);
        let delay = self.rng.in_range(self.def.targets.appear_delay_secs);
        let visible_at = self.time + delay;
        let id = self.next_id;
        self.next_id += 1;
        Target {
            id,
            position: point,
            facing_yaw_deg,
            health: self.def.targets.health,
            max_health: self.def.targets.health,
            visible_at,
            expires_at: self.def.targets.lifetime_secs.map(|l| visible_at + l),
            motion: MotionState::new(point, lateral, visible_at),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Range, angle_between_deg};
    use crate::scenario::def::default_pitch_limit;
    use crate::scenario::def::*;

    fn sphere_def(fire: FireMode, health: f32) -> ScenarioDef {
        ScenarioDef {
            id: "t".into(),
            name: "t".into(),
            category: Category::Custom,
            description: String::new(),
            duration_secs: 10.0,
            weapon: WeaponDef {
                fire,
                ..Default::default()
            },
            player: PlayerDef::default(),
            targets: TargetsDef {
                shape: Shape::Sphere { radius: 0.3 },
                spawn: SpawnArea::Angular {
                    yaw_deg: Range::new(-30.0, 30.0),
                    pitch_deg: Range::new(-5.0, 10.0),
                    distance: Range::fixed(10.0),
                },
                motion: Motion::Static,
                max_alive: 3,
                health,
                lifetime_secs: None,
                appear_delay_secs: Range::fixed(0.0),
                min_separation_deg: 5.0,
                speed_scale: 1.0,
            },
            scoring: ScoringDef::default(),
            difficulty: Default::default(),
        }
    }

    fn aim_at(s: &Session, p: Vec3) -> ViewAngles {
        ViewAngles::from_direction(p - s.eye_position())
    }

    #[test]
    fn spawns_max_alive_targets_with_separation() {
        let s = Session::new(sphere_def(FireMode::Click, 1.0), 7);
        let ts: Vec<_> = s.targets().collect();
        assert_eq!(ts.len(), 3);
        let eye = s.eye_position();
        for a in &ts {
            for b in &ts {
                if a.id != b.id {
                    assert!(angle_between_deg(a.position - eye, b.position - eye) >= 5.0);
                }
            }
        }
    }

    #[test]
    fn clicking_on_a_target_kills_it_and_respawns() {
        let mut s = Session::new(sphere_def(FireMode::Click, 1.0), 1);
        let target = s.targets().next().unwrap().clone();
        let view = aim_at(&s, target.position);
        let ev = s.update(FrameInput {
            dt: 0.016,
            view,
            fire_pressed: true,
            fire_held: true,
            ..Default::default()
        });
        assert!(
            ev.iter()
                .any(|e| matches!(e, SessionEvent::Killed { id, .. } if *id == target.id))
        );
        assert_eq!(s.targets().count(), 3);
        assert_eq!(s.stats().kills, 1);
        assert!((s.stats().score - 100.0).abs() < 1e-3);
    }

    #[test]
    fn missing_counts_shot_and_applies_penalty() {
        let mut def = sphere_def(FireMode::Click, 1.0);
        def.scoring.miss_penalty = 10.0;
        let mut s = Session::new(def, 1);
        let view = ViewAngles::new(180.0, 0.0);
        s.update(FrameInput {
            dt: 0.016,
            view,
            fire_pressed: true,
            ..Default::default()
        });
        assert_eq!((s.stats().shots, s.stats().hits), (1, 0));
        assert_eq!(s.stats().score, -10.0);
        assert_eq!(s.result().score, 0.0, "displayed score never goes negative");
    }

    #[test]
    fn fire_rate_limits_clicks() {
        let mut s = Session::new(sphere_def(FireMode::Click, 100.0), 1);
        let view = ViewAngles::new(180.0, 0.0);
        for _ in 0..10 {
            s.update(FrameInput {
                dt: 0.01,
                view,
                fire_pressed: true,
                ..Default::default()
            });
        }
        // Vandal fires 9.75 rounds/s: 0.1 s of clicking allows one shot.
        assert_eq!(s.stats().shots, 1);
    }

    #[test]
    fn hold_mode_accumulates_time_on_target_and_kills_after_health_seconds() {
        let mut s = Session::new(sphere_def(FireMode::Hold, 0.5), 3);
        let target = s.targets().next().unwrap().clone();
        let view = aim_at(&s, target.position);
        let mut killed = false;
        for _ in 0..60 {
            let ev = s.update(FrameInput {
                dt: 0.01,
                view,
                fire_held: true,
                ..Default::default()
            });
            killed |= ev
                .iter()
                .any(|e| matches!(e, SessionEvent::Killed { id, .. } if *id == target.id));
            if killed {
                break;
            }
        }
        assert!(killed);
        assert!((s.stats().on_target_secs - 0.5).abs() < 0.011);
        assert!((s.result().accuracy - 1.0).abs() < 1e-3);
    }

    #[test]
    fn run_finishes_at_duration() {
        let mut s = Session::new(sphere_def(FireMode::Click, 1.0), 1);
        let mut finished = 0;
        for _ in 0..200 {
            let ev = s.update(FrameInput {
                dt: 0.1,
                ..Default::default()
            });
            finished += ev.iter().filter(|e| **e == SessionEvent::Finished).count();
        }
        assert_eq!(finished, 1);
        assert!(s.is_finished());
        assert!((s.time() - 10.0).abs() < 1e-4);
    }

    #[test]
    fn lifetime_expires_targets_and_appear_delay_hides_them() {
        let mut def = sphere_def(FireMode::Click, 1.0);
        def.targets.max_alive = 1;
        def.targets.lifetime_secs = Some(0.5);
        def.targets.appear_delay_secs = Range::fixed(0.2);
        let mut s = Session::new(def, 1);
        assert_eq!(s.targets().count(), 0);
        s.update(FrameInput {
            dt: 0.25,
            ..Default::default()
        });
        assert_eq!(s.targets().count(), 1);
        s.update(FrameInput {
            dt: 0.25,
            ..Default::default()
        });
        s.update(FrameInput {
            dt: 0.25,
            ..Default::default()
        });
        assert_eq!(s.stats().expired, 1);
    }

    #[test]
    fn moving_shots_are_flagged_and_spread_ruins_them() {
        let mut def = sphere_def(FireMode::Click, 1.0);
        def.player = PlayerDef {
            movement: true,
            bounds_x: Range::new(-50.0, 50.0),
            bounds_z: Range::new(-1.0, 1.0),
            ..PlayerDef::default()
        };
        def.weapon.simulate_spread = true;
        def.scoring.moving_shot_penalty = 5.0;
        let mut s = Session::new(def, 9);
        let run = MoveInput {
            right: 1.0,
            ..Default::default()
        };
        for _ in 0..50 {
            s.update(FrameInput {
                dt: 0.01,
                movement: run,
                ..Default::default()
            });
        }
        assert!(!s.is_accurate());
        s.update(FrameInput {
            dt: 0.01,
            movement: run,
            fire_pressed: true,
            ..Default::default()
        });
        assert_eq!(s.stats().moving_shots, 1);
    }

    #[test]
    fn strafing_targets_move_at_valorant_speed_within_bounds() {
        let mut def = sphere_def(FireMode::Click, 1.0);
        def.targets.max_alive = 1;
        def.targets.motion = Motion::Strafe {
            weapon: crate::games::valorant::Weapon::Vandal,
            change_secs: Range::new(0.5, 0.8),
            stop_chance: 0.0,
            max_offset: 2.0,
        };
        let mut s = Session::new(def, 4);
        let mut max_speed: f32 = 0.0;
        for _ in 0..500 {
            s.update(FrameInput {
                dt: 0.01,
                ..Default::default()
            });
            let t = s.targets().next().unwrap();
            max_speed = max_speed.max(t.velocity().length());
            assert!((t.position - t.motion.anchor).length() <= 2.0 + 0.1);
        }
        assert!(max_speed <= 5.4 + 1e-3 && max_speed > 4.0, "{max_speed}");
    }

    fn dodge(stop: f32, crouch: f32, jump: f32) -> Motion {
        Motion::Dodge {
            weapon: crate::games::valorant::Weapon::Vandal,
            change_secs: Range::new(0.3, 0.5),
            juke_secs: Range::new(0.1, 0.2),
            juke_chance: 0.5,
            stop_chance: stop,
            crouch_chance: crouch,
            jump_chance: jump,
            max_offset: 2.0,
        }
    }

    fn dodging_humanoid(motion: Motion) -> ScenarioDef {
        let mut def = sphere_def(FireMode::Click, 1.0);
        def.targets.shape = Shape::Humanoid { head_only: true };
        def.targets.spawn = SpawnArea::Floor {
            x: Range::new(-2.0, 2.0),
            z: Range::new(-15.0, -12.0),
            height: Range::fixed(0.0),
        };
        def.targets.max_alive = 1;
        def.targets.motion = motion;
        def
    }

    fn idle(s: &mut Session, frames: u32) {
        for _ in 0..frames {
            s.update(FrameInput {
                dt: 0.01,
                ..Default::default()
            });
        }
    }

    #[test]
    fn dodging_targets_juke_at_valorant_speed_within_their_lane() {
        let mut s = Session::new(dodging_humanoid(dodge(0.0, 0.0, 0.0)), 4);
        let mut max_speed: f32 = 0.0;
        let mut reversals = 0;
        let mut last_dir = 0.0;
        for _ in 0..600 {
            idle(&mut s, 1);
            let t = s.targets().next().unwrap();
            let v = t.velocity().dot(t.motion.lateral);
            max_speed = max_speed.max(v.abs());
            if v.abs() > 0.5 && v.signum() != last_dir {
                reversals += 1;
                last_dir = v.signum();
            }
            assert!((t.position - t.motion.anchor).length() <= 2.0 + 0.1);
            assert_eq!(t.position.y, 0.0, "grounded without jumps");
            assert_eq!(t.crouch(), 0.0, "never crouches without crouch_chance");
        }
        assert!(max_speed <= 5.4 + 1e-3 && max_speed > 3.0, "{max_speed}");
        assert!(reversals >= 10, "jukes reverse often: {reversals}");
    }

    #[test]
    fn crouching_lowers_the_head_hitbox() {
        let mut s = Session::new(dodging_humanoid(dodge(0.0, 1.0, 0.0)), 3);
        idle(&mut s, 50);
        let t = s.targets().next().unwrap().clone();
        assert!(
            (t.crouch() - 1.0).abs() < 1e-6,
            "fully crouched: {}",
            t.crouch()
        );
        let standing = s.dims().head_center_height();
        let crouched = s.target_dims(&t).head_center_height();
        let drop = crate::games::valorant::CROUCH_HEIGHT_DROP_M;
        assert!((standing - crouched - drop).abs() < 1e-5);
        assert_eq!(s.target_dims(&t).head_size, s.dims().head_size);

        // The standing head line now passes over the target; the crouched head is hit.
        let over = aim_at(&s, t.position + Vec3::new(0.0, standing, 0.0));
        let ev = s.update(FrameInput {
            dt: 0.0,
            view: over,
            fire_pressed: true,
            fire_held: true,
            ..Default::default()
        });
        assert!(ev.contains(&SessionEvent::Shot {
            hit: None,
            moving: false
        }));
        s.update(FrameInput::default());
        s.update(FrameInput {
            dt: 1.0,
            ..Default::default()
        });
        let t = s.targets().next().unwrap().clone();
        let head = t.position + Vec3::new(0.0, s.target_dims(&t).head_center_height(), 0.0);
        let ev = s.update(FrameInput {
            dt: 0.0,
            view: aim_at(&s, head),
            fire_pressed: true,
            fire_held: true,
            ..Default::default()
        });
        assert!(
            ev.iter().any(|e| matches!(
                e,
                SessionEvent::Shot {
                    hit: Some((_, HitZone::Head)),
                    ..
                }
            )),
            "{ev:?}"
        );
    }

    #[test]
    fn jumping_targets_rise_to_the_valorant_apex_and_land() {
        use crate::games::valorant::{JUMP_AIR_SECS, JUMP_APEX_M};
        let mut s = Session::new(dodging_humanoid(dodge(0.0, 0.0, 1.0)), 9);
        let mut apex: f32 = 0.0;
        let mut landings = 0;
        let mut airborne = false;
        for _ in 0..400 {
            idle(&mut s, 1);
            let y = s.targets().next().unwrap().position.y;
            assert!(y >= 0.0, "never sinks below the floor: {y}");
            apex = apex.max(y);
            if airborne && y == 0.0 {
                landings += 1;
            }
            airborne = y > 0.0;
        }
        assert!((apex - JUMP_APEX_M).abs() < 0.02, "apex {apex}");
        // 4 s of back-to-back jumps, each ≈JUMP_AIR_SECS long.
        assert!(landings >= (3.0 / JUMP_AIR_SECS) as i32, "{landings}");
    }

    #[test]
    fn dodging_is_deterministic_per_seed() {
        let run = |seed| {
            let mut s = Session::new(dodging_humanoid(dodge(0.2, 0.2, 0.2)), seed);
            let mut trace = Vec::new();
            for _ in 0..300 {
                idle(&mut s, 1);
                let t = s.targets().next().unwrap();
                trace.push((t.position, t.crouch()));
            }
            trace
        };
        assert_eq!(run(5), run(5));
        assert_ne!(run(5), run(6));
    }

    fn movement_def() -> ScenarioDef {
        let mut def = sphere_def(FireMode::Click, 1.0);
        def.player = PlayerDef {
            movement: true,
            bounds_x: Range::new(-6.0, 6.0),
            bounds_z: Range::new(-1.0, 1.0),
            ..PlayerDef::default()
        };
        def
    }

    #[test]
    fn pushing_into_a_bound_still_lets_the_player_slide_along_it() {
        let mut s = Session::new(movement_def(), 2);
        let input = MoveInput {
            right: 1.0,
            forward: 1.0,
            walk: false,
        };
        // Long enough to reach the front edge (z = -1) and keep strafing right.
        for _ in 0..150 {
            s.update(FrameInput {
                dt: 0.01,
                movement: input,
                ..Default::default()
            });
        }
        let p = s.player_position();
        assert!(
            (p.z + 1.0).abs() < 1e-4,
            "should be held at the front edge: {p:?}"
        );
        assert!(
            p.x > 3.0,
            "strafing right must not be blocked by the front edge: {p:?}"
        );
    }

    #[test]
    fn elevation_raises_the_player_floor_and_movement_stays_on_it() {
        let flat = Session::new(movement_def(), 2);
        assert_eq!(
            flat.player_position().y,
            0.0,
            "default floor is the targets' floor"
        );
        let mut def = movement_def();
        def.player.elevation = 2.5;
        let mut s = Session::new(def, 2);
        assert_eq!(s.player_position().y, 2.5);
        assert!((s.eye_position().y - (2.5 + s.dims().eye_height)).abs() < 1e-6);
        let input = MoveInput {
            right: 1.0,
            forward: -1.0,
            walk: false,
        };
        for _ in 0..150 {
            s.update(FrameInput {
                dt: 0.01,
                movement: input,
                ..Default::default()
            });
            assert_eq!(
                s.player_position().y,
                2.5,
                "moving keeps the feet on the floor"
            );
        }
        assert!(s.player_position().x > 1.0, "the player still moves");
    }

    #[test]
    fn elevation_leaves_target_floors_in_place() {
        let mut def = sphere_def(FireMode::Click, 1.0);
        def.targets.shape = Shape::Humanoid { head_only: true };
        def.targets.spawn = SpawnArea::Floor {
            x: Range::new(-3.0, 3.0),
            z: Range::new(-15.0, -12.0),
            height: Range::fixed(0.0),
        };
        def.player.elevation = -2.0;
        let s = Session::new(def, 5);
        assert!(s.targets().count() > 0);
        for t in s.targets() {
            assert_eq!(t.position.y, 0.0, "humanoids stand on the targets' floor");
        }
    }

    #[test]
    fn hitting_a_bound_only_stops_the_blocked_direction_when_the_camera_is_turned() {
        let mut s = Session::new(movement_def(), 2);
        // Looking 45° right, D moves diagonally into the back edge (+z) and to the right.
        let view = ViewAngles::new(45.0, 0.0);
        let input = MoveInput {
            right: 1.0,
            ..Default::default()
        };
        for _ in 0..150 {
            s.update(FrameInput {
                dt: 0.01,
                view,
                movement: input,
                ..Default::default()
            });
        }
        let p = s.player_position();
        assert!((p.z - 1.0).abs() < 1e-4, "{p:?}");
        assert!(p.x > 2.0, "must keep sliding right along the edge: {p:?}");
    }

    fn crosshair_def(offset_deg: Range) -> ScenarioDef {
        let mut def = sphere_def(FireMode::Click, 1.0);
        def.duration_secs = 1000.0;
        def.targets.max_alive = 1;
        def.targets.spawn = SpawnArea::AroundCrosshair {
            offset_deg,
            distance: Range::fixed(12.0),
            pitch_limit_deg: default_pitch_limit(),
            on_screen: false,
        };
        def
    }

    /// Kill the current target with a perfect flick and return the pitch (from the eye) of
    /// every target that spawns afterwards.
    fn chained_flick_pitches(def: ScenarioDef, kills: usize) -> Vec<f32> {
        let mut s = Session::new(def, 21);
        let mut pitches = Vec::new();
        for _ in 0..kills {
            let t = s
                .targets()
                .next()
                .expect("a target is always alive")
                .clone();
            let view = aim_at(&s, t.position);
            s.update(FrameInput {
                dt: 0.2,
                view,
                fire_pressed: true,
                ..Default::default()
            });
            let next = s.targets().next().expect("respawned").position;
            pitches.push(aim_at(&s, next).pitch_deg);
        }
        pitches
    }

    #[test]
    fn chained_wide_flicks_never_climb_into_the_sky() {
        let limit = default_pitch_limit();
        let pitches = chained_flick_pitches(crosshair_def(Range::new(20.0, 70.0)), 300);
        let (lo, hi) = pitches
            .iter()
            .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(*p), b.max(*p)));
        assert!(
            hi <= limit.max + 1e-3,
            "a target spawned at pitch {hi}° (limit {})",
            limit.max
        );
        assert!(
            lo >= limit.min - 1e-3,
            "a target spawned at pitch {lo}° (limit {})",
            limit.min
        );
    }

    #[test]
    fn custom_pitch_limit_is_respected_and_flicks_still_go_up_and_down() {
        let mut def = crosshair_def(Range::new(5.0, 20.0));
        if let SpawnArea::AroundCrosshair {
            pitch_limit_deg, ..
        } = &mut def.targets.spawn
        {
            *pitch_limit_deg = Range::new(-5.0, 10.0);
        }
        let pitches = chained_flick_pitches(def, 200);
        assert!(
            pitches
                .iter()
                .all(|p| (-5.0 - 1e-3..=10.0 + 1e-3).contains(p)),
            "{pitches:?}"
        );
        // The band must not collapse every flick to the horizon.
        assert!(pitches.iter().any(|p| *p > 3.0) && pitches.iter().any(|p| *p < 0.0));
    }

    #[test]
    fn pitch_limit_defaults_when_omitted_in_toml() {
        let def = ScenarioDef::from_toml(
            r#"
            id = "x"
            name = "x"
            category = "flicking"
            [targets]
            shape = { kind = "sphere", radius = 0.2 }
            spawn = { kind = "around_crosshair", offset_deg = [10, 40], distance = [10, 10] }
            "#,
        )
        .unwrap();
        let SpawnArea::AroundCrosshair {
            pitch_limit_deg, ..
        } = def.targets.spawn
        else {
            panic!("wrong spawn kind")
        };
        assert_eq!(pitch_limit_deg, default_pitch_limit());
    }

    fn ammo_def() -> ScenarioDef {
        let mut def = sphere_def(FireMode::Click, 1.0);
        def.weapon.ammo = Some(crate::scenario::def::AmmoDef::default());
        def
    }

    #[test]
    fn misses_drain_ammo_hits_refund_and_running_dry_reloads() {
        let mut s = Session::new(ammo_def(), 3);
        assert_eq!(s.ammo(), Some((100.0, 100.0)));
        let away = ViewAngles::new(180.0, 0.0);
        let shoot = |s: &mut Session, view| {
            s.update(FrameInput {
                dt: 0.2,
                view,
                fire_pressed: true,
                ..Default::default()
            })
        };
        shoot(&mut s, away);
        shoot(&mut s, away);
        assert_eq!(s.ammo().unwrap().0, 40.0, "two misses cost 60");
        let t = s.targets().next().unwrap().position;
        let view = aim_at(&s, t);
        shoot(&mut s, view);
        assert_eq!(s.ammo().unwrap().0, 47.0, "a hit costs 30 and refunds 37");
        // 47 - 30 = 17 cannot pay for the next shot, so this miss starts a reload.
        let ev = shoot(&mut s, away);
        assert!(
            ev.iter().any(|e| matches!(e, SessionEvent::Reload { .. })),
            "{ev:?}"
        );
        assert!(s.is_reloading());
        assert_eq!(s.stats().reloads, 1);
        let shots = s.stats().shots;
        shoot(&mut s, away);
        assert_eq!(s.stats().shots, shots, "no shots while reloading");
        for _ in 0..10 {
            s.update(FrameInput {
                dt: 0.2,
                ..Default::default()
            });
        }
        assert!(!s.is_reloading());
        assert_eq!(s.ammo().unwrap().0, 100.0);
    }

    /// One sphere of `radius` straight ahead at eye height, 10 m away, with recoil on.
    fn recoil_def(radius: f32) -> ScenarioDef {
        let mut def = sphere_def(FireMode::Click, 1000.0);
        def.weapon.recoil = true;
        def.targets.max_alive = 1;
        def.targets.shape = Shape::Sphere { radius };
        def.targets.spawn = SpawnArea::Floor {
            x: Range::fixed(0.0),
            z: Range::fixed(-10.0),
            height: Range::fixed(1.82),
        };
        def
    }

    fn frame(s: &mut Session, view: ViewAngles, held: bool) -> Vec<SessionEvent> {
        s.update(FrameInput {
            dt: 0.01,
            view,
            fire_pressed: held,
            fire_held: held,
            ..Default::default()
        })
    }

    /// Hold fire until `n` more shots are fired, then release for one frame. Returns
    /// whether each shot hit.
    fn spray(s: &mut Session, view: ViewAngles, n: u32) -> Vec<bool> {
        let mut hits = Vec::new();
        while hits.len() < n as usize {
            for e in frame(s, view, true) {
                if let SessionEvent::Shot { hit, .. } = e {
                    hits.push(hit.is_some());
                }
            }
        }
        frame(s, view, false);
        hits
    }

    fn wait(s: &mut Session, secs: f32) {
        for _ in 0..(secs / 0.01).round() as usize {
            frame(s, ViewAngles::default(), false);
        }
    }

    #[test]
    fn without_recoil_holding_fire_does_not_auto_fire_or_count_bursts() {
        let mut s = Session::new(sphere_def(FireMode::Click, 100.0), 1);
        for _ in 0..100 {
            s.update(FrameInput {
                dt: 0.01,
                fire_held: true,
                ..Default::default()
            });
        }
        assert_eq!(s.stats().shots, 0);
        assert_eq!(s.burst_index(), 0);
        assert_eq!(s.recoil_recovery_remaining(), None);
        assert!(s.recoil_profile().is_none());
        assert_eq!((s.stats().bursts, s.stats().shots_by_bullet), (0, [0; 4]));
    }

    #[test]
    fn holding_fire_with_recoil_fires_at_the_weapon_fire_rate() {
        let mut s = Session::new(recoil_def(0.3), 1);
        for _ in 0..200 {
            frame(&mut s, ViewAngles::new(180.0, 0.0), true);
        }
        // Vandal: 9.75 rounds/s. Shots at 0.01 + k / 9.75 s up to 2.0 s give k = 0..=19.
        assert_eq!(s.stats().shots, 20);
        assert_eq!(s.stats().bursts, 1);
        assert_eq!(s.stats().longest_burst, 20);
    }

    #[test]
    fn protected_bullets_climb_into_a_target_aimed_below() {
        // The sphere covers ≈2.87° around its center. Aiming 3.2° below it, the first
        // bullet (0.25° spread) always misses, and from bullet 4 on the climb (0.35° per
        // bullet, estimate) plus spread keeps every protected bullet inside it.
        let mut s = Session::new(recoil_def(0.5), 1);
        let hits = spray(&mut s, ViewAngles::new(0.0, -3.2), 6);
        assert!(!hits[0], "{hits:?}");
        assert!(hits[3..].iter().all(|h| *h), "{hits:?}");
    }

    #[test]
    fn waiting_the_recovery_time_resets_to_the_first_bullet() {
        let mut s = Session::new(recoil_def(0.3), 1);
        let away = ViewAngles::new(180.0, 0.0);
        spray(&mut s, away, 3);
        assert_eq!(s.burst_index(), 3);
        wait(&mut s, 0.2);
        assert_eq!(
            s.burst_index(),
            3,
            "0.2 s is shorter than the 0.375 s recovery"
        );
        assert!(s.recoil_recovery_remaining().unwrap() > 0.0);
        spray(&mut s, away, 1);
        assert_eq!(s.burst_index(), 4);
        wait(&mut s, 0.4);
        assert_eq!(s.burst_index(), 0);
        assert_eq!(s.recoil_recovery_remaining(), Some(0.0));
        spray(&mut s, away, 1);
        assert_eq!(s.burst_index(), 1);
        assert_eq!(s.stats().bursts, 2);
    }

    #[test]
    fn recoil_sprays_are_deterministic_per_seed() {
        let run = |seed| {
            let mut s = Session::new(recoil_def(0.3), seed);
            let hits = spray(&mut s, ViewAngles::new(0.0, -1.0), 25);
            (hits, s.stats().clone())
        };
        assert_eq!(run(4), run(4));
    }

    #[test]
    fn burst_metrics_count_bursts_and_bullet_buckets() {
        let mut s = Session::new(recoil_def(0.3), 1);
        let away = ViewAngles::new(180.0, 0.0);
        spray(&mut s, away, 3);
        wait(&mut s, 0.5);
        spray(&mut s, away, 1);
        wait(&mut s, 0.5);
        spray(&mut s, away, 8);
        let st = s.stats();
        assert_eq!(st.shots, 12);
        assert_eq!(st.bursts, 3);
        assert_eq!(st.longest_burst, 8);
        assert_eq!(st.shots_by_bullet, [3, 4, 3, 2]);
        assert_eq!(st.hits_by_bullet, [0; 4]);
        assert!((st.avg_burst_len() - 4.0).abs() < 1e-6);
        assert!((st.first_bullet_share() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn burst_bonus_only_rewards_hits_within_the_protected_bullets() {
        // A 2 m sphere at 10 m (≈11°) catches every bullet of a 10-round spray.
        let mut def = recoil_def(2.0);
        def.scoring.sphere = 0.0;
        def.scoring.burst_bonus = 10.0;
        let mut s = Session::new(def, 1);
        let hits = spray(&mut s, ViewAngles::new(0.0, 0.0), 10);
        assert!(hits.iter().all(|h| *h), "{hits:?}");
        assert_eq!(s.stats().hits_by_bullet, [1, 2, 3, 4]);
        // Vandal protects 6 bullets: 6 × 10 points.
        assert!((s.stats().score - 60.0).abs() < 1e-3, "{}", s.stats().score);
    }

    #[test]
    fn success_rate_counts_expired_targets_as_failures() {
        let mut def = sphere_def(FireMode::Click, 1.0);
        def.targets.max_alive = 1;
        def.targets.lifetime_secs = Some(0.3);
        let mut s = Session::new(def, 2);
        let t = s.targets().next().unwrap().position;
        s.update(FrameInput {
            dt: 0.05,
            view: aim_at(&s, t),
            fire_pressed: true,
            ..Default::default()
        });
        for _ in 0..8 {
            s.update(FrameInput {
                dt: 0.05,
                ..Default::default()
            });
        }
        assert_eq!(s.stats().hits, 1);
        assert_eq!(s.stats().expired, 1);
        assert!((s.success_rate() - 0.5).abs() < 1e-6);
    }

    fn ring_def(fire: FireMode) -> ScenarioDef {
        let mut def = sphere_def(fire, 100.0);
        def.targets.max_alive = 1;
        def.scoring.rings = Some(ScoreRings {
            radius: vec![0.3, 0.6],
            multiplier: vec![2.0, 1.5],
        });
        def
    }

    /// A view that passes `fraction` of `radius` to the side of `center`.
    fn aim_beside(s: &Session, center: Vec3, radius: f32, fraction: f32) -> ViewAngles {
        let to = center - s.eye_position();
        let side = to.cross(Vec3::new(0.0, 1.0, 0.0)).normalize_or_zero();
        aim_at(s, center + side * (radius * fraction))
    }

    /// Clicks along `view`, then waits long enough for the next click to fire.
    fn click(s: &mut Session, view: ViewAngles) -> Vec<SessionEvent> {
        let ev = s.update(FrameInput {
            dt: 0.2,
            view,
            fire_pressed: true,
            fire_held: true,
            ..Default::default()
        });
        s.update(FrameInput {
            dt: 0.2,
            view,
            ..Default::default()
        });
        ev
    }

    #[test]
    fn score_rings_scale_points_by_distance_from_the_zone_center() {
        let mut s = Session::new(ring_def(FireMode::Click), 4);
        let center = s.targets().next().unwrap().position;
        let mut scores = Vec::new();
        for fraction in [0.0, 0.2, 0.45, 0.8] {
            let before = s.stats().score;
            let view = aim_beside(&s, center, 0.3, fraction);
            let ev = click(&mut s, view);
            assert!(
                ev.iter()
                    .any(|e| matches!(e, SessionEvent::Shot { hit: Some(_), .. })),
                "{fraction}: the shot hits"
            );
            scores.push(s.stats().score - before);
        }
        // Inner ring ×2, second ring ×1.5, outside every ring the flat sphere points.
        for (got, want) in scores.iter().zip([200.0, 200.0, 150.0, 100.0]) {
            assert!((got - want).abs() < 1e-3, "{scores:?}");
        }
        assert_eq!((s.stats().shots, s.stats().hits), (4, 4));
    }

    #[test]
    fn score_rings_use_the_head_center_and_leave_body_hits_flat() {
        let mut def = ring_def(FireMode::Click);
        def.targets.shape = Shape::Humanoid { head_only: false };
        def.targets.spawn = SpawnArea::Angular {
            yaw_deg: Range::fixed(0.0),
            pitch_deg: Range::fixed(0.0),
            distance: Range::fixed(10.0),
        };
        let mut s = Session::new(def, 1);
        let feet = s.targets().next().unwrap().position;
        let head = feet + Vec3::new(0.0, s.dims().head_center_height(), 0.0);
        let view = aim_at(&s, head);
        click(&mut s, view);
        assert!(
            (s.stats().score - 200.0).abs() < 1e-3,
            "{}",
            s.stats().score
        );
        let view = aim_at(&s, feet + Vec3::new(0.0, 1.4, 0.0));
        click(&mut s, view);
        assert!(
            (s.stats().score - 240.0).abs() < 1e-3,
            "{}",
            s.stats().score
        );
        assert_eq!((s.stats().head_hits, s.stats().body_hits), (1, 1));
    }

    #[test]
    fn score_rings_scale_hold_mode_points_per_second() {
        let mut s = Session::new(ring_def(FireMode::Hold), 4);
        let center = s.targets().next().unwrap().position;
        s.update(FrameInput {
            dt: 0.1,
            view: aim_at(&s, center),
            fire_held: true,
            ..Default::default()
        });
        // 100 points per second × 0.1 s × 2 in the inner ring.
        assert!((s.stats().score - 20.0).abs() < 1e-3, "{}", s.stats().score);
    }

    #[test]
    fn without_rings_a_hit_scores_its_flat_points_anywhere_on_the_target() {
        let mut def = ring_def(FireMode::Click);
        def.scoring.rings = None;
        let mut s = Session::new(def, 4);
        let center = s.targets().next().unwrap().position;
        let view = aim_beside(&s, center, 0.3, 0.0);
        click(&mut s, view);
        let view = aim_beside(&s, center, 0.3, 0.8);
        click(&mut s, view);
        assert!(
            (s.stats().score - 200.0).abs() < 1e-3,
            "{}",
            s.stats().score
        );
    }
}
