//! Runtime simulation of one scenario run. The engine feeds view angles, buttons and
//! movement keys every frame and renders whatever `targets()` returns.
//!
//! [`Session`] owns the run state and orchestrates each frame; spawning, target motion,
//! weapons and scoring live in the sibling `spawn`, `motion`, `weapon` and `scoring`
//! modules.

use super::def::{FireMode, ScenarioDef, Shape};
use super::hit::{HitZone, Ray};
use super::motion::MotionState;
use super::{scoring, spawn, weapon};
use crate::math::{Vec3, ViewAngles};
use crate::movement::{self, LocalVelocity, MoveInput, MovementProfile};
use crate::rng::Rng;
use crate::valorant::{HumanoidDims, WeaponStats};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FrameInput {
    pub dt: f32,
    pub view: ViewAngles,
    /// Fire button went down this frame.
    pub fire_pressed: bool,
    /// Fire button is held.
    pub fire_held: bool,
    pub movement: MoveInput,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    Spawned {
        id: u64,
    },
    Shot {
        hit: Option<(u64, HitZone)>,
        moving: bool,
    },
    Killed {
        id: u64,
        time_to_kill: f32,
    },
    Expired {
        id: u64,
    },
    Finished,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub id: u64,
    /// Sphere center, or humanoid feet.
    pub position: Vec3,
    /// Yaw the humanoid faces (towards the player at spawn).
    pub facing_yaw_deg: f32,
    pub health: f32,
    pub max_health: f32,
    pub visible_at: f32,
    expires_at: Option<f32>,
    motion: MotionState,
}

impl Target {
    pub fn is_visible(&self, now: f32) -> bool {
        now >= self.visible_at
    }

    pub fn velocity(&self) -> Vec3 {
        self.motion.velocity()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub score: f32,
    pub shots: u32,
    pub hits: u32,
    pub head_hits: u32,
    pub body_hits: u32,
    pub leg_hits: u32,
    pub kills: u32,
    pub expired: u32,
    pub moving_shots: u32,
    pub hold_secs: f32,
    pub on_target_secs: f32,
    pub head_on_target_secs: f32,
    /// Seconds from a target becoming visible to its death.
    pub kill_times: Vec<f32>,
}

/// Final numbers of a run, ready to display and persist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunResult {
    pub scenario_id: String,
    pub score: f32,
    /// Hits / shots in click mode, time on target / time firing in hold mode.
    pub accuracy: f32,
    pub kills: u32,
    pub kills_per_min: f32,
    pub avg_time_to_kill: Option<f32>,
    pub headshot_rate: f32,
    pub moving_shots: u32,
    pub expired: u32,
    pub duration_secs: f32,
}

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
}

impl Session {
    pub fn new(def: ScenarioDef, seed: u64) -> Self {
        let weapon = def.weapon.kind.stats();
        let profile = def.weapon.kind.movement();
        let mut s = Self {
            def,
            rng: Rng::new(seed),
            time: 0.0,
            finished: false,
            targets: Vec::new(),
            next_id: 1,
            stats: Stats::default(),
            player_pos: Vec3::ZERO,
            player_vel: LocalVelocity::default(),
            view: ViewAngles::default(),
            profile,
            weapon,
            dims: HumanoidDims::default(),
            last_shot_at: f32::NEG_INFINITY,
        };
        s.refill(&mut Vec::new());
        s
    }

    pub fn def(&self) -> &ScenarioDef {
        &self.def
    }
    pub fn time(&self) -> f32 {
        self.time
    }
    pub fn remaining(&self) -> f32 {
        (self.def.duration_secs - self.time).max(0.0)
    }
    pub fn is_finished(&self) -> bool {
        self.finished
    }
    pub fn stats(&self) -> &Stats {
        &self.stats
    }
    pub fn dims(&self) -> &HumanoidDims {
        &self.dims
    }
    pub fn player_position(&self) -> Vec3 {
        self.player_pos
    }
    pub fn eye_position(&self) -> Vec3 {
        self.player_pos + Vec3::new(0.0, self.dims.eye_height, 0.0)
    }
    pub fn player_speed(&self) -> f32 {
        self.player_vel.speed()
    }
    pub fn movement_profile(&self) -> &MovementProfile {
        &self.profile
    }
    pub fn is_accurate(&self) -> bool {
        self.player_speed() <= self.profile.accuracy_speed_limit()
    }

    /// Targets currently visible to the player.
    pub fn targets(&self) -> impl Iterator<Item = &Target> {
        let now = self.time;
        self.targets.iter().filter(move |t| t.is_visible(now))
    }

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
            0.0,
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
        let motion = &self.def.targets.motion;
        let humanoid = matches!(self.def.targets.shape, Shape::Humanoid { .. });
        for t in &mut self.targets {
            if t.is_visible(now) {
                t.motion
                    .step(&mut t.position, motion, humanoid, &mut self.rng, now, dt);
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

    fn shoot(&mut self, events: &mut Vec<SessionEvent>) {
        if !weapon::can_fire(&self.weapon, self.time, self.last_shot_at) {
            return;
        }
        self.last_shot_at = self.time;
        let (dir, moving) = self.shot_direction();
        scoring::record_shot(&mut self.stats, &self.def.scoring, moving);

        let Some((idx, zone)) = self.hit_along(dir) else {
            scoring::record_miss(&mut self.stats, &self.def.scoring);
            events.push(SessionEvent::Shot { hit: None, moving });
            return;
        };
        scoring::record_hit(&mut self.stats, &self.def.scoring, zone);
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

    fn hold_fire(&mut self, dt: f32, events: &mut Vec<SessionEvent>) {
        self.stats.hold_secs += dt;
        let (dir, _) = self.shot_direction();
        let Some((idx, zone)) = self.hit_along(dir) else {
            return;
        };
        scoring::record_on_target(&mut self.stats, &self.def.scoring, zone, dt);
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
            },
            scoring: ScoringDef::default(),
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
            weapon: crate::valorant::Weapon::Vandal,
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

    fn movement_def() -> ScenarioDef {
        let mut def = sphere_def(FireMode::Click, 1.0);
        def.player = PlayerDef {
            movement: true,
            bounds_x: Range::new(-6.0, 6.0),
            bounds_z: Range::new(-1.0, 1.0),
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
}
