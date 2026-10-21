//! Target motion: advances one target's position each frame according to the scenario's
//! [`Motion`] (static, VALORANT-style strafing, evasive dodging, free drift, ballistic
//! flight, straight-line bouncing, or an orbit around the player).

use super::MIN_SPHERE_HEIGHT;
use super::def::{BounceArena, DepthDef, Motion, PathRoute, Rhythm, Shape, TargetsDef, WaveForm};
use crate::games::valorant::{
    CROUCH_HEIGHT_DROP_M, CROUCH_TRANSITION_SECS, JUMP_AIR_SECS, JUMP_APEX_M, Weapon,
};
use crate::math::{Range, Vec3, ViewAngles, wrap_degrees};
use crate::movement::{self, LocalVelocity, MoveInput};
use crate::rng::{Rng, ShuffleBag};

/// Area under VALORANT's counter-strafe curve: braking distance ≈ speed × this.
pub(super) const STRAFE_BRAKE_SECS: f32 = 0.075;

/// What drives a target's motion this frame: the motion (the scenario's, or a behaviour
/// rule's override), whether the target is a humanoid, and the speed multiplier.
#[derive(Debug, Clone, Copy)]
pub(super) struct Drive<'a> {
    pub(super) motion: &'a Motion,
    pub(super) humanoid: bool,
    /// In-run speed tuning (`targets.speed_scale`) times any behaviour rule's multiplier.
    pub(super) speed_k: f32,
    /// Height of the target's position when it touches the floor: a sphere's radius, 0
    /// for a humanoid's feet.
    pub(super) floor_y: f32,
}

impl<'a> Drive<'a> {
    /// The scenario's own motion at its own speed.
    pub(super) fn of(targets: &'a TargetsDef) -> Self {
        Self {
            motion: &targets.motion,
            humanoid: matches!(targets.shape, Shape::Humanoid { .. }),
            speed_k: targets.speed_scale,
            floor_y: match targets.shape {
                Shape::Sphere { radius } => radius,
                Shape::Humanoid { .. } => 0.0,
                Shape::Capsule { height, .. } => height / 2.0,
            },
        }
    }
}

/// Per-target motion state, owned by the target and advanced by [`MotionState::step`].
#[derive(Debug, Clone, PartialEq)]
pub(super) struct MotionState {
    /// Spawn point; strafing and drift stay within `max_offset` of it.
    pub(super) anchor: Vec3,
    /// Horizontal unit vector perpendicular to the line towards the player.
    pub(super) lateral: Vec3,
    /// Ground velocity: `right` along `lateral`, `forward` towards the player (depth).
    pub(super) strafe: LocalVelocity,
    /// Strafe direction the target wants: -1, 0 (stop) or 1.
    pub(super) intent: f32,
    pub(super) drift: Vec3,
    pub(super) next_change_at: f32,
    /// How far into a crouch the target is: 0 standing, 1 fully crouched.
    pub(super) crouch: f32,
    /// Whether the target wants to be crouched.
    crouching: bool,
    /// Height above the anchor while jumping, meters.
    lift: f32,
    /// Vertical speed while airborne, m/s (0 on the ground).
    rise: f32,
    /// The player's eye when the target spawned: the center of `orbit` and
    /// `linear_bounce`, and where the `depth` band is measured from.
    viewer: Vec3,
    /// Depth direction the target wants: 1 towards the player, -1 away, 0 none yet.
    fb_intent: f32,
    /// Session time of the next depth decision.
    next_fb_at: f32,
    /// Velocity of the motions that do not move by strafing or drifting (ballistic,
    /// linear bounce, orbit), m/s.
    flight: Vec3,
    /// Angular velocity of `linear_bounce` (yaw, pitch) and `orbit` (yaw), degrees per
    /// second; `None` until the motion first decides.
    glide: Option<(f32, f32)>,
    /// Floor bounces a ballistic target has left.
    bounces_left: u32,
    /// A ballistic target came down for the last time; the session removes it.
    pub(super) landed: bool,
    /// The weave of a `wave` target; `None` until the motion first decides.
    weave: Option<Weave>,
    /// Side a `rhythm.swap_pause_secs` pause moves to when it ends (0: no pause).
    pending: f32,
    /// Session time the swap pause ends.
    resume_at: f32,
    /// A pause is due from a turn at the lane edge; the next step times it.
    edge_pause: bool,
    /// Shuffle bags of a `rhythm.balanced` target; `None` until its first decision.
    bags: Option<Bags>,
    /// Progress along a `path` route; `None` until the motion first decides.
    walk: Option<Walk>,
}

/// Where a [`Motion::Path`] target is on its route.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Walk {
    /// Speed drawn for this target, m/s (before speed tuning).
    speed: f32,
    /// Index of the waypoint it walks to; 0 is the spawn point, `i` is `points[i - 1]`.
    to: usize,
    /// Direction through the waypoints: 1 forwards, -1 back (ping-pong).
    step: isize,
    /// Session time its rest at the last waypoint ends.
    rest_until: f32,
}

/// The shuffle bags a balanced strafe decides with.
#[derive(Debug, Clone, PartialEq)]
struct Bags {
    /// Quarter of `change_secs` the next wait comes from.
    quarter: ShuffleBag,
    /// Side picked from a stop: 0 left, 1 right.
    side: ShuffleBag,
}

impl Bags {
    fn new() -> Self {
        Self {
            quarter: ShuffleBag::new(4),
            side: ShuffleBag::new(2),
        }
    }
}

/// What a [`Motion::Wave`] target decided when it appeared.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Weave {
    /// Signed lateral speed, m/s (the sign is the side it glides to).
    speed: f32,
    amplitude: f32,
    frequency_hz: f32,
    form: WaveForm,
    /// Position in the cycle, in cycles.
    phase: f32,
    /// Height of the line it weaves around, m. The height is set from it each frame, so
    /// a crest or trough cut by the floor clearance never shifts the line.
    centre_y: f32,
}

/// The chances and timings a [`Motion::Dodge`] target decides with.
struct DodgeRules<'a> {
    weapon: Weapon,
    change_secs: Range,
    juke_secs: Range,
    juke_chance: f32,
    stop_chance: f32,
    crouch_chance: f32,
    jump_chance: f32,
    max_offset: f32,
    depth: Option<&'a DepthDef>,
    /// In-run speed tuning applied to the ground movement.
    speed_k: f32,
    rhythm: Option<&'a Rhythm>,
}

impl MotionState {
    /// A target at rest at `anchor` that picks its first motion when it becomes visible,
    /// seen by a player whose eye is at `viewer`.
    pub(super) fn new(anchor: Vec3, lateral: Vec3, visible_at: f32, viewer: Vec3) -> Self {
        Self {
            anchor,
            lateral,
            strafe: LocalVelocity::default(),
            intent: 0.0,
            drift: Vec3::ZERO,
            next_change_at: visible_at,
            crouch: 0.0,
            crouching: false,
            lift: 0.0,
            rise: 0.0,
            viewer,
            fb_intent: 0.0,
            next_fb_at: visible_at,
            flight: Vec3::ZERO,
            glide: None,
            bounces_left: 0,
            landed: false,
            weave: None,
            pending: 0.0,
            resume_at: 0.0,
            edge_pause: false,
            bags: None,
            walk: None,
        }
    }

    pub(super) fn velocity(&self) -> Vec3 {
        let v = self.lateral * self.strafe.right + self.drift + Vec3::UP * self.rise;
        // Only the new motions and the depth axis use these; skipping them otherwise keeps
        // the older motions' velocities bit-identical.
        if self.strafe.forward == 0.0 && self.flight == Vec3::ZERO {
            v
        } else {
            v + self.toward() * self.strafe.forward + self.flight
        }
    }

    /// Horizontal unit vector from the target towards the player at spawn.
    fn toward(&self) -> Vec3 {
        // `lateral` is up × (towards the player), so lateral × up points back at him.
        self.lateral.cross(Vec3::UP)
    }

    /// Points the target in strafe direction `dir` (-1, 0 or 1) and holds it, standing,
    /// until `until`, when the motion decides again: a reaction to the player.
    pub(super) fn react(&mut self, dir: f32, until: f32) {
        self.intent = dir;
        self.pending = 0.0;
        self.edge_pause = false;
        self.crouching = false;
        self.next_change_at = until;
    }

    /// Hands the target over to `motion` (a behaviour rule or a motion profile switched
    /// it) at `now`: the new motion decides at once, and movement it has no part in stops.
    pub(super) fn switch_to(&mut self, motion: &Motion, now: f32) {
        self.next_change_at = now;
        self.next_fb_at = now;
        self.crouching = false;
        self.crouch = 0.0;
        self.glide = None;
        self.flight = Vec3::ZERO;
        self.weave = None;
        self.pending = 0.0;
        self.edge_pause = false;
        self.walk = None;
        match motion {
            Motion::Static
            | Motion::Ballistic { .. }
            | Motion::LinearBounce { .. }
            | Motion::Orbit { .. }
            | Motion::Wave { .. }
            | Motion::Path { .. } => {
                self.intent = 0.0;
                self.fb_intent = 0.0;
                self.strafe = LocalVelocity::default();
                self.drift = Vec3::ZERO;
            }
            Motion::Strafe { .. } | Motion::Dodge { .. } => self.drift = Vec3::ZERO,
            Motion::Drift { .. } => {
                self.intent = 0.0;
                self.strafe = LocalVelocity::default();
            }
        }
    }

    /// Advances one visible target by `dt` seconds at time `now`, moving `position`. Every
    /// motion honors `drive.speed_k` (in-run speed tuning, times any behaviour rule's
    /// multiplier) by scaling its speed.
    pub(super) fn step(
        &mut self,
        position: &mut Vec3,
        drive: Drive<'_>,
        rng: &mut Rng,
        now: f32,
        dt: f32,
    ) {
        let Drive {
            motion,
            humanoid,
            speed_k,
            floor_y,
        } = drive;
        match motion {
            Motion::Static => {}
            Motion::Strafe {
                weapon,
                change_secs,
                stop_chance,
                max_offset,
                depth,
                rhythm,
            } => {
                let rhythm = rhythm.as_ref();
                self.resume(rhythm, rng, now);
                if now >= self.next_change_at {
                    self.pending = 0.0;
                    let side = if rng.next_f32() < *stop_chance {
                        0.0
                    } else {
                        self.next_side(rhythm, rng)
                    };
                    self.turn(side, rhythm, rng, now);
                    self.next_change_at =
                        self.moving_from(now) + self.wait(*change_secs, rhythm, rng);
                }
                if let Some(d) = depth {
                    self.decide_depth(d, *position, rng, now);
                }
                let pause_turns = rhythm.is_some_and(|r| r.swap_pause_secs.is_some());
                self.strafe(
                    position,
                    *weapon,
                    *max_offset,
                    depth.as_ref(),
                    speed_k,
                    pause_turns,
                    dt,
                );
            }
            Motion::Dodge {
                weapon,
                change_secs,
                juke_secs,
                juke_chance,
                stop_chance,
                crouch_chance,
                jump_chance,
                max_offset,
                depth,
                rhythm,
            } => {
                self.resume(rhythm.as_ref(), rng, now);
                let rules = DodgeRules {
                    weapon: *weapon,
                    change_secs: *change_secs,
                    juke_secs: *juke_secs,
                    juke_chance: *juke_chance,
                    stop_chance: *stop_chance,
                    crouch_chance: *crouch_chance,
                    jump_chance: *jump_chance,
                    max_offset: *max_offset,
                    depth: depth.as_ref(),
                    speed_k,
                    rhythm: rhythm.as_ref(),
                };
                if now >= self.next_change_at && !self.is_airborne() {
                    self.decide_dodge(&rules, rng, now);
                }
                if let Some(d) = depth
                    && !self.is_airborne()
                {
                    self.decide_depth(d, *position, rng, now);
                }
                self.dodge(position, &rules, humanoid, dt);
            }
            Motion::Drift {
                speed,
                change_secs,
                max_offset,
                depth,
            } => {
                if now >= self.next_change_at {
                    let a = rng.next_f32() * std::f32::consts::TAU;
                    let up = if humanoid { 0.0 } else { a.sin() };
                    self.drift =
                        (self.lateral * a.cos() + Vec3::UP * up) * (rng.in_range(*speed) * speed_k);
                    self.next_change_at = now + rng.in_range(*change_secs);
                }
                let away = *position - self.anchor;
                if away.length() >= *max_offset && away.dot(self.drift) > 0.0 {
                    self.drift = -self.drift;
                }
                *position += self.drift * dt;
                if let Some(d) = depth {
                    self.drift_depth(position, d, *speed, speed_k, rng, now, dt);
                }
                if !humanoid {
                    position.y = position.y.max(MIN_SPHERE_HEIGHT);
                }
            }
            Motion::Ballistic {
                speed,
                angle_deg,
                gravity,
                bounces,
                restitution,
                ..
            } => {
                // Speed tuning plays the same arc faster: speed × k and gravity × k².
                if now >= self.next_change_at {
                    let launch = rng.in_range(*speed) * speed_k;
                    let angle = rng.in_range(*angle_deg).to_radians();
                    let side = rng.sign();
                    self.flight = self.lateral * (launch * angle.cos() * side)
                        + Vec3::UP * (launch * angle.sin());
                    self.bounces_left = *bounces;
                    self.next_change_at = f32::INFINITY;
                }
                let g = gravity * speed_k * speed_k;
                self.fly(position, g, *restitution, floor_y, dt);
            }
            Motion::LinearBounce { speed_deg, arena } => {
                if now >= self.next_change_at {
                    let a = rng.next_f32() * std::f32::consts::TAU;
                    let s = rng.in_range(*speed_deg) * speed_k;
                    self.glide = Some(if humanoid {
                        (s.copysign(a.cos()), 0.0)
                    } else {
                        (s * a.cos(), s * a.sin())
                    });
                    self.next_change_at = f32::INFINITY;
                }
                let before = *position;
                self.bounce(position, arena, humanoid, dt);
                self.measure(before, *position, dt);
            }
            Motion::Orbit {
                speed_deg,
                radius,
                change_secs,
                reverse_chance,
            } => {
                if now >= self.next_change_at {
                    let rate = match self.glide {
                        None => rng.in_range(*speed_deg) * speed_k * rng.sign(),
                        Some((r, _)) if rng.next_f32() < *reverse_chance => -r,
                        Some((r, _)) => r,
                    };
                    self.glide = Some((rate, 0.0));
                    self.next_change_at =
                        change_secs.map_or(f32::INFINITY, |c| now + rng.in_range(c));
                }
                let before = *position;
                self.orbit(position, *radius, dt);
                self.measure(before, *position, dt);
            }
            Motion::Wave {
                speed,
                amplitude,
                frequency_hz,
                form,
                max_offset,
            } => {
                // Speed tuning plays the same path faster: lateral speed and frequency × k.
                if now >= self.next_change_at {
                    let side = rng.sign();
                    let speed = rng.in_range(*speed) * speed_k * side;
                    let amplitude = rng.in_range(*amplitude);
                    let frequency_hz = rng.in_range(*frequency_hz) * speed_k;
                    let phase = rng.next_f32();
                    self.weave = Some(Weave {
                        speed,
                        amplitude,
                        frequency_hz,
                        form: *form,
                        phase,
                        centre_y: position.y - amplitude * form.at(phase),
                    });
                    self.next_change_at = f32::INFINITY;
                }
                let before = *position;
                self.weave(position, *max_offset, humanoid, dt);
                self.measure(before, *position, dt);
            }
            Motion::Path {
                points,
                speed,
                pause_secs,
                route,
            } => {
                if now >= self.next_change_at {
                    self.walk = Some(Walk {
                        speed: rng.in_range(*speed),
                        to: 1,
                        step: 1,
                        rest_until: now,
                    });
                    self.next_change_at = f32::INFINITY;
                }
                let before = *position;
                self.walk_path(position, points, *pause_secs, *route, speed_k, rng, now, dt);
                self.measure(before, *position, dt);
            }
        }
    }

    /// Walks a `path` target towards its next waypoint, resting on arrival and then
    /// heading to the one after it.
    #[allow(clippy::too_many_arguments)]
    fn walk_path(
        &mut self,
        position: &mut Vec3,
        points: &[[f32; 3]],
        pause_secs: Range,
        route: PathRoute,
        speed_k: f32,
        rng: &mut Rng,
        now: f32,
        dt: f32,
    ) {
        let Some(mut walk) = self.walk else {
            return;
        };
        let (anchor, lateral, toward) = (self.anchor, self.lateral, self.toward());
        let waypoint = |i: usize| match i.checked_sub(1) {
            None => anchor,
            Some(p) => {
                let [right, up, closer] = points[p];
                anchor + lateral * right + Vec3::UP * up + toward * closer
            }
        };
        // Time left in this step: walking can reach a waypoint, rest, and go on.
        let mut left = dt;
        // A step covers at most one round of the route, so waypoints at the same spot
        // cannot keep it here.
        let mut legs = 2 * (points.len() + 1);
        while left > 0.0 && now + (dt - left) >= walk.rest_until && legs > 0 {
            legs -= 1;
            let goal = waypoint(walk.to);
            let gap = goal - *position;
            let reach = walk.speed * speed_k * left;
            if gap.length() > reach {
                *position += gap * (reach / gap.length());
                break;
            }
            *position = goal;
            left -= gap.length() / (walk.speed * speed_k);
            walk.rest_until = now + (dt - left) + rng.in_range(pause_secs);
            let last = points.len();
            walk.to = match route {
                PathRoute::Loop => (walk.to + 1) % (last + 1),
                PathRoute::PingPong => {
                    if walk.to as isize + walk.step > last as isize
                        || walk.to as isize + walk.step < 0
                    {
                        walk.step = -walk.step;
                    }
                    (walk.to as isize + walk.step) as usize
                }
            };
        }
        self.walk = Some(walk);
    }

    pub(super) fn is_airborne(&self) -> bool {
        self.rise != 0.0 || self.lift > 0.0
    }

    /// The direction a moving decision picks: a reversal, or a random side from a stop.
    pub(super) fn reversed(&self, rng: &mut Rng) -> f32 {
        if self.intent == 0.0 {
            rng.sign()
        } else {
            -self.intent
        }
    }

    /// Ends a swap pause at `resume_at` by moving to the pending side, after timing the
    /// pause a turn at the lane edge asked for.
    fn resume(&mut self, rhythm: Option<&Rhythm>, rng: &mut Rng, now: f32) {
        if self.edge_pause {
            self.edge_pause = false;
            if let Some(p) = rhythm.and_then(|r| r.swap_pause_secs) {
                self.resume_at = now + rng.in_range(p);
                self.next_change_at = self.next_change_at.max(self.resume_at);
            }
        }
        if self.pending != 0.0 && now >= self.resume_at {
            self.intent = self.pending;
            self.pending = 0.0;
        }
    }

    /// The side a moving decision of a strafe or dodge picks: a reversal, or a random side
    /// from a stop (from a shuffled pair with `rhythm.balanced`).
    fn next_side(&mut self, rhythm: Option<&Rhythm>, rng: &mut Rng) -> f32 {
        if !rhythm.is_some_and(|r| r.balanced) || self.intent != 0.0 {
            return self.reversed(rng);
        }
        let bags = self.bags.get_or_insert_with(Bags::new);
        if bags.side.draw(rng) == 0 { -1.0 } else { 1.0 }
    }

    /// Heads to `side`, through a stop of `rhythm.swap_pause_secs` when that reverses a
    /// moving target.
    fn turn(&mut self, side: f32, rhythm: Option<&Rhythm>, rng: &mut Rng, now: f32) {
        match rhythm.and_then(|r| r.swap_pause_secs) {
            Some(p) if side != 0.0 && self.intent == -side => {
                self.intent = 0.0;
                self.pending = side;
                self.resume_at = now + rng.in_range(p);
            }
            _ => self.intent = side,
        }
    }

    /// When the target starts moving after a decision at `now`: after its swap pause.
    fn moving_from(&self, now: f32) -> f32 {
        if self.pending != 0.0 {
            self.resume_at.max(now)
        } else {
            now
        }
    }

    /// Time until the next decision, drawn from `range` (from a shuffled quarter with
    /// `rhythm.balanced`) and scaled by the rhythm's multiplier for the side it moves to.
    fn wait(&mut self, range: Range, rhythm: Option<&Rhythm>, rng: &mut Rng) -> f32 {
        let Some(r) = rhythm else {
            return rng.in_range(range);
        };
        let secs = if r.balanced {
            let quarter = self.bags.get_or_insert_with(Bags::new).quarter.draw(rng);
            range.lerp((f32::from(quarter) + rng.next_f32()) / 4.0)
        } else {
            rng.in_range(range)
        };
        let side = if self.pending != 0.0 {
            self.pending
        } else {
            self.intent
        };
        secs * r.secs_mult(side)
    }

    /// Horizontal distance from the player's eye at spawn, meters.
    fn depth_distance(&self, position: Vec3) -> f32 {
        (position - self.viewer).horizontal().length()
    }

    /// A depth decision when one is due: walk back into the band when outside it,
    /// otherwise reverse (or pick a random direction for the first one).
    fn decide_depth(&mut self, d: &DepthDef, position: Vec3, rng: &mut Rng, now: f32) {
        if now < self.next_fb_at {
            return;
        }
        let dist = self.depth_distance(position);
        self.fb_intent = if dist < d.distance.min {
            -1.0
        } else if dist > d.distance.max {
            1.0
        } else if self.fb_intent == 0.0 {
            rng.sign()
        } else {
            -self.fb_intent
        };
        self.next_fb_at = now + rng.in_range(d.change_secs);
    }

    /// Turns the depth intent around when the target (with `braking` meters to stop) is
    /// about to leave the band.
    fn keep_in_band(&mut self, d: &DepthDef, position: Vec3, braking: f32) {
        let dist = self.depth_distance(position);
        // Moving towards the player (+1) shrinks the distance.
        if self.fb_intent > 0.0 && dist - braking <= d.distance.min {
            self.fb_intent = -1.0;
        } else if self.fb_intent < 0.0 && dist + braking >= d.distance.max {
            self.fb_intent = 1.0;
        }
    }

    /// Ground A/D (and, with `depth`, W/S) movement towards `intent` with the weapon's
    /// VALORANT physics. With `pause_turns`, a turn at the lane edge stops the target and
    /// asks for a swap pause.
    #[allow(clippy::too_many_arguments)]
    fn strafe(
        &mut self,
        position: &mut Vec3,
        weapon: Weapon,
        max_offset: f32,
        depth: Option<&DepthDef>,
        speed_k: f32,
        pause_turns: bool,
        dt: f32,
    ) {
        let offset = (*position - self.anchor).dot(self.lateral);
        let mut profile = weapon.movement();
        // Scaling run speed and acceleration together keeps the strafe's timing.
        profile.run_speed *= speed_k;
        profile.acceleration *= speed_k;
        // Reverse early enough that the counter-strafe braking distance keeps the target
        // inside its lane.
        let braking = self.strafe.right.abs() * STRAFE_BRAKE_SECS;
        if self.intent != 0.0 && offset * self.intent + braking >= max_offset {
            if pause_turns {
                self.pending = -self.intent;
                self.intent = 0.0;
                self.edge_pause = true;
            } else {
                self.intent = -self.intent;
            }
        }
        let Some(d) = depth else {
            let input = MoveInput {
                right: self.intent,
                ..Default::default()
            };
            self.strafe = movement::step(&profile, self.strafe, input, dt);
            *position += self.lateral * (self.strafe.right * dt);
            return;
        };
        self.keep_in_band(d, *position, self.strafe.forward.abs() * STRAFE_BRAKE_SECS);
        // Diagonal input is normalized, as for the player.
        let input = MoveInput {
            right: self.intent,
            forward: self.fb_intent,
            ..Default::default()
        };
        self.strafe = movement::step(&profile, self.strafe, input, dt);
        *position +=
            self.lateral * (self.strafe.right * dt) + self.toward() * (self.strafe.forward * dt);
    }

    /// Drift's depth axis: a forward/back speed drawn from the drift speed at every depth
    /// decision, turning back at the band edges.
    #[allow(clippy::too_many_arguments)]
    fn drift_depth(
        &mut self,
        position: &mut Vec3,
        d: &DepthDef,
        speed: Range,
        speed_k: f32,
        rng: &mut Rng,
        now: f32,
        dt: f32,
    ) {
        if now >= self.next_fb_at {
            self.decide_depth(d, *position, rng, now);
            self.strafe.forward = self.fb_intent * rng.in_range(speed) * speed_k;
        }
        let dist = self.depth_distance(*position);
        let towards = self.strafe.forward > 0.0;
        if (towards && dist <= d.distance.min) || (!towards && dist >= d.distance.max) {
            self.strafe.forward = -self.strafe.forward;
            self.fb_intent = -self.fb_intent;
        }
        *position += self.toward() * (self.strafe.forward * dt);
    }

    /// One grounded dodge decision: jump, crouch in place, stop, or move (reversing).
    fn decide_dodge(&mut self, r: &DodgeRules, rng: &mut Rng, now: f32) {
        let roll = rng.next_f32();
        self.crouching = false;
        self.pending = 0.0;
        if roll < r.jump_chance {
            // Take-off speed that, with the matching gravity, reaches the apex height and
            // lands after the air time.
            self.rise = 4.0 * JUMP_APEX_M / JUMP_AIR_SECS;
        } else if roll < r.jump_chance + r.crouch_chance {
            self.intent = 0.0;
            self.crouching = true;
        } else if roll < r.jump_chance + r.crouch_chance + r.stop_chance {
            self.intent = 0.0;
        } else {
            let side = self.next_side(r.rhythm, rng);
            self.turn(side, r.rhythm, rng, now);
        }
        let wait = if rng.next_f32() < r.juke_chance {
            r.juke_secs
        } else {
            r.change_secs
        };
        self.next_change_at = self.moving_from(now) + self.wait(wait, r.rhythm, rng);
    }

    fn dodge(&mut self, position: &mut Vec3, r: &DodgeRules, humanoid: bool, dt: f32) {
        let goal = if self.crouching { 1.0 } else { 0.0 };
        let rate = dt / CROUCH_TRANSITION_SECS;
        self.crouch += (goal - self.crouch).clamp(-rate, rate);

        if self.is_airborne() {
            // No air control: the target keeps its take-off speed, stopping at the lane edge.
            let gravity = 8.0 * JUMP_APEX_M / (JUMP_AIR_SECS * JUMP_AIR_SECS);
            self.lift += self.rise * dt - 0.5 * gravity * dt * dt;
            self.rise -= gravity * dt;
            if self.lift <= 0.0 {
                self.lift = 0.0;
                self.rise = 0.0;
            }
            let next = (*position - self.anchor).dot(self.lateral) + self.strafe.right * dt;
            if next.abs() >= r.max_offset {
                self.strafe.right = 0.0;
            }
            *position += self.lateral * (self.strafe.right * dt);
            if let Some(d) = r.depth {
                let next = *position + self.toward() * (self.strafe.forward * dt);
                if !(d.distance.min..=d.distance.max).contains(&self.depth_distance(next)) {
                    self.strafe.forward = 0.0;
                }
                *position += self.toward() * (self.strafe.forward * dt);
            }
        } else {
            let pause_turns = r.rhythm.is_some_and(|r| r.swap_pause_secs.is_some());
            self.strafe(
                position,
                r.weapon,
                r.max_offset,
                r.depth,
                r.speed_k,
                pause_turns,
                dt,
            );
        }

        position.y = self.anchor.y + self.lift;
        if !humanoid {
            // Spheres have no hitbox to compress: they drop by the crouch height instead.
            position.y -= CROUCH_HEIGHT_DROP_M * self.crouch;
            position.y = position.y.max(MIN_SPHERE_HEIGHT);
        }
    }

    /// Ballistic flight under gravity `g`: exact constant-acceleration steps, bouncing off
    /// the floor (`floor_y`) while bounces remain, then landing.
    fn fly(&mut self, position: &mut Vec3, g: f32, restitution: f32, floor_y: f32, dt: f32) {
        if self.landed {
            return;
        }
        let (y0, vy0) = (position.y, self.flight.y);
        *position += self.flight * dt - Vec3::UP * (0.5 * g * dt * dt);
        self.flight.y -= g * dt;
        if position.y > floor_y || self.flight.y >= 0.0 {
            return;
        }
        if self.bounces_left == 0 {
            position.y = floor_y;
            self.flight = Vec3::ZERO;
            self.landed = true;
            return;
        }
        self.bounces_left -= 1;
        // When in the step it touched the floor: y0 + vy0·τ − g·τ²/2 = floor_y.
        let disc = (vy0 * vy0 + 2.0 * g * (y0 - floor_y)).max(0.0);
        let tau = ((vy0 + disc.sqrt()) / g).clamp(0.0, dt);
        // Bounce with the impact speed, then fly the rest of the step.
        let up = -(vy0 - g * tau) * restitution;
        let rest = dt - tau;
        position.y = floor_y + up * rest - 0.5 * g * rest * rest;
        self.flight.y = up - g * rest;
    }

    /// Straight-line angular motion inside `arena`, reflecting off its walls.
    fn bounce(&mut self, position: &mut Vec3, arena: &BounceArena, humanoid: bool, dt: f32) {
        let Some((mut v_yaw, mut v_pitch)) = self.glide else {
            return;
        };
        let rel = *position - self.viewer;
        let mid_yaw = (arena.yaw_deg.min + arena.yaw_deg.max) / 2.0;
        if humanoid {
            // Humanoids stand on the floor: move around the player horizontally.
            let flat = rel.horizontal();
            let angles = ViewAngles::from_direction(flat);
            let mut yaw = mid_yaw + wrap_degrees(angles.yaw_deg - mid_yaw) + v_yaw * dt;
            reflect_axis(&mut yaw, &mut v_yaw, arena.yaw_deg);
            let p = self.viewer + ViewAngles::new(yaw, 0.0).direction() * flat.length();
            *position = Vec3::new(p.x, position.y, p.z);
        } else {
            let angles = ViewAngles::from_direction(rel);
            let mut yaw = mid_yaw + wrap_degrees(angles.yaw_deg - mid_yaw) + v_yaw * dt;
            let mut pitch = angles.pitch_deg + v_pitch * dt;
            if arena.sides == 0 {
                reflect_axis(&mut yaw, &mut v_yaw, arena.yaw_deg);
                reflect_axis(&mut pitch, &mut v_pitch, arena.pitch_deg);
            } else {
                reflect_polygon(arena, &mut yaw, &mut pitch, &mut v_yaw, &mut v_pitch);
            }
            *position = self.viewer + ViewAngles::new(yaw, pitch).direction() * rel.length();
            position.y = position.y.max(MIN_SPHERE_HEIGHT);
        }
        self.glide = Some((v_yaw, v_pitch));
    }

    /// Circular motion around the player at `radius` (or the current distance).
    fn orbit(&mut self, position: &mut Vec3, radius: Option<f32>, dt: f32) {
        let Some((rate, _)) = self.glide else {
            return;
        };
        let flat = (*position - self.viewer).horizontal();
        let r = radius.unwrap_or_else(|| flat.length());
        let yaw = ViewAngles::from_direction(flat).yaw_deg + rate * dt;
        let p = self.viewer + ViewAngles::new(yaw, 0.0).direction() * r;
        *position = Vec3::new(p.x, position.y, p.z);
    }

    /// Glides along `lateral`, turning back at `max_offset`, while weaving vertically.
    fn weave(&mut self, position: &mut Vec3, max_offset: f32, humanoid: bool, dt: f32) {
        let Some(w) = &mut self.weave else {
            return;
        };
        let side = (*position - self.anchor).dot(self.lateral);
        if side.abs() >= max_offset && side * w.speed > 0.0 {
            w.speed = -w.speed;
        }
        w.phase = (w.phase + w.frequency_hz * dt).rem_euclid(1.0);
        *position += self.lateral * (w.speed * dt);
        position.y = w.centre_y + w.amplitude * w.form.at(w.phase);
        if !humanoid {
            position.y = position.y.max(MIN_SPHERE_HEIGHT);
        }
    }

    /// Records the velocity of a motion that moved from `before` to `after`.
    fn measure(&mut self, before: Vec3, after: Vec3, dt: f32) {
        if dt > 0.0 {
            self.flight = (after - before) * (1.0 / dt);
        }
    }
}

/// Reflects a coordinate that left `[r.min, r.max]` back inside, reversing its speed when
/// it was moving outwards.
fn reflect_axis(x: &mut f32, v: &mut f32, r: Range) {
    if *x < r.min {
        *x = (2.0 * r.min - *x).min(r.max);
        *v = v.abs();
    } else if *x > r.max {
        *x = (2.0 * r.max - *x).max(r.min);
        *v = -v.abs();
    }
}

/// Reflects a (yaw, pitch) point off the walls of the regular polygon of `arena` that it
/// crossed, mirroring the point and its velocity across each crossed wall.
fn reflect_polygon(
    arena: &BounceArena,
    yaw: &mut f32,
    pitch: &mut f32,
    v_yaw: &mut f32,
    v_pitch: &mut f32,
) {
    let (cy, cp) = (
        (arena.yaw_deg.min + arena.yaw_deg.max) / 2.0,
        (arena.pitch_deg.min + arena.pitch_deg.max) / 2.0,
    );
    let (hy, hp) = (
        (arena.yaw_deg.max - arena.yaw_deg.min) / 2.0,
        (arena.pitch_deg.max - arena.pitch_deg.min) / 2.0,
    );
    let n = arena.sides as usize;
    let vertex = |k: usize| {
        let a = std::f32::consts::TAU * k as f32 / n as f32;
        (cy + hy * a.sin(), cp + hp * a.cos())
    };
    for k in 0..n {
        let (ax, ay) = vertex(k);
        let (bx, by) = vertex(k + 1);
        // Outward normal: perpendicular to the wall, pointing away from the center.
        let (mut nx, mut ny) = (by - ay, ax - bx);
        if nx * (ax - cy) + ny * (ay - cp) < 0.0 {
            (nx, ny) = (-nx, -ny);
        }
        let len = (nx * nx + ny * ny).sqrt();
        if len <= 0.0 {
            continue;
        }
        (nx, ny) = (nx / len, ny / len);
        let outside = (*yaw - ax) * nx + (*pitch - ay) * ny;
        let outward = *v_yaw * nx + *v_pitch * ny;
        if outside > 0.0 {
            *yaw -= 2.0 * outside * nx;
            *pitch -= 2.0 * outside * ny;
            if outward > 0.0 {
                *v_yaw -= 2.0 * outward * nx;
                *v_pitch -= 2.0 * outward * ny;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::math::{Vec3, ViewAngles};
    use crate::scenario::{FailReason, FrameInput, RunOutcome, ScenarioDef, Session, SessionEvent};

    /// A one-target scenario of `shape`, spawning at `spawn` and moving with `motion`.
    fn def(shape: &str, spawn: &str, motion: &str) -> ScenarioDef {
        let d = ScenarioDef::from_toml(&format!(
            r#"
            id = "m"
            name = "m"
            category = "custom"
            duration_secs = 100.0
            [targets]
            shape = {shape}
            spawn = {spawn}
            motion = {motion}
            "#
        ))
        .unwrap();
        assert!(d.validate().is_empty(), "{:?}", d.validate());
        d
    }

    const SPHERE: &str = r#"{ kind = "sphere", radius = 0.2 }"#;
    /// One point 10 m ahead, 1 m above the floor.
    const POINT: &str = r#"{ kind = "floor", x = [0, 0], z = [-10, -10], height = [1, 1] }"#;
    /// Straight ahead at 10 m, 5° up.
    const AHEAD: &str =
        r#"{ kind = "angular", yaw_deg = [0, 0], pitch_deg = [5, 5], distance = [10, 10] }"#;

    /// Steps an idle player `frames` times by `dt`, returning the first target's position
    /// after each step (`None` while there is none) and every event.
    fn run(s: &mut Session, frames: usize, dt: f32) -> (Vec<Option<Vec3>>, Vec<SessionEvent>) {
        let (mut path, mut events) = (Vec::new(), Vec::new());
        for _ in 0..frames {
            events.extend(s.update(FrameInput {
                dt,
                ..Default::default()
            }));
            path.push(s.targets().next().map(|t| t.position));
        }
        (path, events)
    }

    /// The session time of the first `Expired` event, stepping by `dt`.
    fn first_expiry(s: &mut Session, dt: f32, max_frames: usize) -> Option<f32> {
        for _ in 0..max_frames {
            let ev = s.update(FrameInput {
                dt,
                ..Default::default()
            });
            if ev.iter().any(|e| matches!(e, SessionEvent::Expired { .. })) {
                return Some(s.time());
            }
        }
        None
    }

    const UP: &str =
        r#"{ kind = "ballistic", speed = [5, 5], angle_deg = [90, 90], gravity = 10 }"#;

    #[test]
    fn ballistic_targets_follow_the_launch_arc_and_expire_on_the_floor() {
        let mut s = Session::new(def(SPHERE, POINT, UP), 1);
        let (path, _) = run(&mut s, 50, 0.01);
        // y = 1 + 5t − 5t² at t = 0.5 s; the steps are exact for constant acceleration.
        let y = path[49].unwrap().y;
        assert!((y - 2.25).abs() < 1e-3, "{y}");
        assert!(path[49].unwrap().x.abs() < 1e-3, "straight up");

        // The center reaches the floor (its radius, 0.2 m) when 5t² − 5t − 0.8 = 0.
        let mut s = Session::new(def(SPHERE, POINT, UP), 1);
        let landed = first_expiry(&mut s, 0.01, 300).expect("lands");
        let t = (5.0 + 41f32.sqrt()) / 10.0;
        assert!((landed - t).abs() < 0.011, "{landed} vs {t}");
        assert_eq!(s.stats().expired, 1);
        assert_eq!(s.targets().count(), 1, "a new target replaces it");
        assert_eq!(s.outcome(), None, "landing only expires the target");
    }

    #[test]
    fn ballistic_bounces_lose_speed_before_the_target_lands() {
        let motion = r#"{ kind = "ballistic", speed = [5, 5], angle_deg = [90, 90], gravity = 10, bounces = 1, restitution = 0.5 }"#;
        let mut s = Session::new(def(SPHERE, POINT, motion), 1);
        let first = (5.0 + 41f32.sqrt()) / 10.0;
        let (path, _) = run(&mut s, (first * 100.0) as usize + 3, 0.01);
        assert!(
            path.iter().all(Option::is_some),
            "the bounce keeps it alive"
        );
        // Impact speed 10·t1 − 5; half of it is kept, so the second hop peaks lower.
        let rebound = (10.0 * first - 5.0) * 0.5;
        let peak = 0.2 + rebound * rebound / 20.0;
        let mut top: f32 = 0.0;
        let mut landed = None;
        for _ in 0..200 {
            let ev = s.update(FrameInput {
                dt: 0.01,
                ..Default::default()
            });
            if ev.iter().any(|e| matches!(e, SessionEvent::Expired { .. })) {
                landed = Some(s.time());
                break;
            }
            top = top.max(s.targets().next().unwrap().position.y);
        }
        assert!((top - peak).abs() < 0.03, "{top} vs {peak}");
        let second = first + 2.0 * rebound / 10.0;
        let landed = landed.expect("lands after its last bounce");
        assert!((landed - second).abs() < 0.03, "{landed} vs {second}");
    }

    #[test]
    fn a_ballistic_landing_can_fail_the_run() {
        let motion = r#"{ kind = "ballistic", speed = [5, 5], angle_deg = [90, 90], gravity = 10, on_floor = "fail" }"#;
        let mut s = Session::new(def(SPHERE, POINT, motion), 1);
        let (_, events) = run(&mut s, 200, 0.01);
        assert_eq!(s.outcome(), Some(RunOutcome::Failed(FailReason::Dropped)));
        assert!(events.contains(&SessionEvent::RunFailed {
            reason: FailReason::Dropped
        }));
        assert_eq!(s.stats().expired, 1, "the landing still counts as expired");
        assert!(s.time() < 1.2, "{}", s.time());
    }

    #[test]
    fn ballistic_flights_are_seeded_and_launch_sideways_at_random() {
        let motion = r#"{ kind = "ballistic", speed = [4, 8], angle_deg = [30, 70] }"#;
        let flight = |seed| {
            let mut s = Session::new(def(SPHERE, POINT, motion), seed);
            run(&mut s, 60, 0.01).0
        };
        assert_eq!(flight(3), flight(3));
        assert_ne!(flight(3), flight(4));
        let sides: Vec<f32> = (1..=8)
            .map(|seed| flight(seed)[30].unwrap().x.signum())
            .collect();
        assert!(sides.contains(&1.0) && sides.contains(&-1.0), "{sides:?}");
    }

    /// Horizontal distance of `p` from the eye.
    fn flat_distance(s: &Session, p: Vec3) -> f32 {
        (p - s.eye_position()).horizontal().length()
    }

    #[test]
    fn depth_keeps_strafing_and_drifting_targets_inside_their_distance_band() {
        let spawn =
            r#"{ kind = "angular", yaw_deg = [0, 0], pitch_deg = [0, 0], distance = [10, 10] }"#;
        for motion in [
            r#"{ kind = "strafe", change_secs = [0.4, 0.8], depth = { change_secs = [0.3, 0.7], distance = [8, 12] } }"#,
            r#"{ kind = "dodge", change_secs = [0.4, 0.8], jump_chance = 0.2, depth = { change_secs = [0.3, 0.7], distance = [8, 12] } }"#,
            r#"{ kind = "drift", speed = [1, 3], change_secs = [0.5, 1], depth = { change_secs = [0.3, 0.7], distance = [8, 12] } }"#,
        ] {
            let mut s = Session::new(def(SPHERE, spawn, motion), 5);
            let (path, _) = run(&mut s, 3000, 0.01);
            let d: Vec<f32> = path.iter().map(|p| flat_distance(&s, p.unwrap())).collect();
            let (lo, hi) = d
                .iter()
                .fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(*x), b.max(*x)));
            assert!(lo >= 8.0 - 0.3 && hi <= 12.0 + 0.3, "{motion}: {lo}..{hi}");
            assert!(hi - lo > 2.0, "{motion}: moves in depth ({lo}..{hi})");
        }
    }

    #[test]
    fn depth_starts_by_walking_into_the_band() {
        let spawn =
            r#"{ kind = "angular", yaw_deg = [0, 0], pitch_deg = [0, 0], distance = [20, 20] }"#;
        let motion = r#"{ kind = "strafe", change_secs = [0.4, 0.8], depth = { change_secs = [5, 5], distance = [8, 12] } }"#;
        let mut s = Session::new(def(SPHERE, spawn, motion), 2);
        let (path, _) = run(&mut s, 800, 0.01);
        let d: Vec<f32> = path.iter().map(|p| flat_distance(&s, p.unwrap())).collect();
        let entered = d
            .iter()
            .position(|x| *x <= 12.0)
            .expect("walks into the band");
        assert!(entered < 400, "{entered}");
        assert!(
            d[entered..].iter().all(|x| (7.7..=12.3).contains(x)),
            "and stays in it"
        );
    }

    /// Yaw and pitch of `p` seen from the eye.
    fn angles(s: &Session, p: Vec3) -> ViewAngles {
        ViewAngles::from_direction(p - s.eye_position())
    }

    #[test]
    fn linear_bounce_reflects_off_the_box_walls_at_a_constant_angular_speed() {
        let motion = r#"{ kind = "linear_bounce", speed_deg = [30, 30], arena = { yaw_deg = [-10, 10], pitch_deg = [0, 10] } }"#;
        let mut s = Session::new(def(SPHERE, AHEAD, motion), 4);
        let (path, _) = run(&mut s, 2000, 0.01);
        let a: Vec<ViewAngles> = path.iter().map(|p| angles(&s, p.unwrap())).collect();
        for v in &a {
            assert!((-10.01..=10.01).contains(&v.yaw_deg), "{v:?}");
            assert!((-0.01..=10.01).contains(&v.pitch_deg), "{v:?}");
        }
        // It reaches both side walls and both the floor and the ceiling of the box.
        let near = |f: fn(&ViewAngles) -> f32, x: f32| a.iter().any(|v| (f(v) - x).abs() < 0.5);
        assert!(near(|v| v.yaw_deg, -10.0) && near(|v| v.yaw_deg, 10.0));
        assert!(near(|v| v.pitch_deg, 0.0) && near(|v| v.pitch_deg, 10.0));
        // Away from the walls each frame turns 0.3° (30 °/s): straight lines at one speed.
        let steady = a
            .windows(2)
            .filter(|w| {
                let (dy, dp) = (w[1].yaw_deg - w[0].yaw_deg, w[1].pitch_deg - w[0].pitch_deg);
                ((dy * dy + dp * dp).sqrt() - 0.3).abs() < 0.01
            })
            .count();
        assert!(steady > 1900, "{steady}");
        let d = (path[1999].unwrap() - s.eye_position()).length();
        assert!((d - 10.0).abs() < 1e-2, "the distance never changes: {d}");
    }

    #[test]
    fn linear_bounce_stays_inside_a_polygon_arena() {
        let spawn =
            r#"{ kind = "angular", yaw_deg = [0, 0], pitch_deg = [10, 10], distance = [12, 12] }"#;
        let motion = r#"{ kind = "linear_bounce", speed_deg = [40, 40], arena = { yaw_deg = [-15, 15], pitch_deg = [0, 20], sides = 5 } }"#;
        let mut s = Session::new(def(SPHERE, spawn, motion), 8);
        let (path, _) = run(&mut s, 3000, 0.01);
        // A pentagon with a vertex at the top: its bottom wall is at pitch 10 − 10·cos 36°.
        let bottom = 10.0 - 10.0 * 36f32.to_radians().cos();
        let mut lowest = f32::MAX;
        for p in &path {
            let v = angles(&s, p.unwrap());
            assert!((-15.05..=15.05).contains(&v.yaw_deg), "{v:?}");
            assert!(
                v.pitch_deg >= bottom - 0.05 && v.pitch_deg <= 20.05,
                "{v:?}"
            );
            lowest = lowest.min(v.pitch_deg);
        }
        assert!(
            (lowest - bottom).abs() < 0.5,
            "reaches the bottom wall: {lowest}"
        );
    }

    #[test]
    fn orbit_circles_the_player_at_its_radius_and_can_reverse() {
        let motion = r#"{ kind = "orbit", speed_deg = [45, 45], radius = 6 }"#;
        let mut s = Session::new(def(SPHERE, AHEAD, motion), 3);
        let (path, _) = run(&mut s, 400, 0.01);
        let y0 = path[0].unwrap().y;
        for p in &path {
            let p = p.unwrap();
            assert!((flat_distance(&s, p) - 6.0).abs() < 1e-3, "{p:?}");
            assert!((p.y - y0).abs() < 1e-5, "keeps its height");
        }
        let yaw = |s: &Session, p: Option<Vec3>| angles(s, p.unwrap()).yaw_deg;
        let turned = (yaw(&s, path[399]) - yaw(&s, path[0])).abs();
        assert!((turned - 45.0 * 3.99).abs() < 0.5, "{turned}");

        let reversing = r#"{ kind = "orbit", speed_deg = [45, 45], change_secs = [0.5, 0.5], reverse_chance = 1.0 }"#;
        let mut s = Session::new(def(SPHERE, AHEAD, reversing), 3);
        let (path, _) = run(&mut s, 200, 0.01);
        let first = yaw(&s, path[40]) - yaw(&s, path[10]);
        let second = yaw(&s, path[90]) - yaw(&s, path[60]);
        assert!(first * second < 0.0, "{first} then {second}");
        let r0 = flat_distance(&s, path[0].unwrap());
        assert!((flat_distance(&s, path[150].unwrap()) - r0).abs() < 1e-2);
    }

    #[test]
    fn humanoids_bounce_and_orbit_on_the_floor() {
        let humanoid = r#"{ kind = "humanoid" }"#;
        let spawn =
            r#"{ kind = "angular", yaw_deg = [0, 0], pitch_deg = [0, 0], distance = [10, 10] }"#;
        for motion in [
            r#"{ kind = "linear_bounce", speed_deg = [30, 30], arena = { yaw_deg = [-20, 20], pitch_deg = [0, 0] } }"#,
            r#"{ kind = "orbit", speed_deg = [30, 30] }"#,
        ] {
            let mut s = Session::new(def(humanoid, spawn, motion), 6);
            let (path, _) = run(&mut s, 500, 0.01);
            assert!(path.iter().all(|p| p.unwrap().y == 0.0), "{motion}");
            let moved = (path[499].unwrap() - path[0].unwrap()).length();
            assert!(moved > 1.0, "{motion}: {moved}");
        }
    }

    #[test]
    fn new_motions_report_their_velocity() {
        let motion = r#"{ kind = "orbit", speed_deg = [45, 45] }"#;
        let mut s = Session::new(def(SPHERE, AHEAD, motion), 3);
        run(&mut s, 10, 0.01);
        let t = s.targets().next().unwrap();
        let expected = flat_distance(&s, t.position) * 45f32.to_radians();
        let speed = t.velocity().length();
        assert!((speed - expected).abs() < 0.05, "{speed} vs {expected}");
    }

    /// A sphere at `AHEAD` with these `[[targets.motion_profiles]]` tables.
    fn profiled(profiles: &str) -> ScenarioDef {
        let d = ScenarioDef::from_toml(&format!(
            r#"
            id = "p"
            name = "p"
            category = "custom"
            duration_secs = 100.0
            [targets]
            shape = {SPHERE}
            spawn = {AHEAD}
            {profiles}
            "#
        ))
        .unwrap();
        assert!(d.validate().is_empty(), "{:?}", d.validate());
        d
    }

    const STILL_AND_ORBIT: &str = r#"
            [[targets.motion_profiles]]
            weight = 1
            switch_secs = [0.5, 0.5]
            motion = { kind = "static" }
            [[targets.motion_profiles]]
            weight = 1
            switch_secs = [0.5, 0.5]
            motion = { kind = "orbit", speed_deg = [40, 40] }
    "#;

    /// Whether the target moved during each 0.5 s window of a 10 s run.
    fn moving_windows(s: &mut Session) -> Vec<bool> {
        let (path, _) = run(s, 1000, 0.01);
        path.chunks(50)
            .map(|w| (w[48].unwrap() - w[1].unwrap()).length() > 0.05)
            .collect()
    }

    #[test]
    fn motion_profiles_switch_between_weighted_motions_on_their_timer() {
        let mut s = Session::new(profiled(STILL_AND_ORBIT), 4);
        let windows = moving_windows(&mut s);
        assert!(
            windows.contains(&true) && windows.contains(&false),
            "{windows:?}"
        );

        let never = STILL_AND_ORBIT.replacen("weight = 1", "weight = 0", 1);
        let mut s = Session::new(profiled(&never), 4);
        assert!(
            moving_windows(&mut s).iter().all(|m| *m),
            "a zero weight is never picked"
        );
    }

    #[test]
    fn motion_profiles_are_deterministic_per_seed() {
        let path = |seed| {
            let mut s = Session::new(profiled(STILL_AND_ORBIT), seed);
            run(&mut s, 600, 0.01).0
        };
        assert_eq!(path(5), path(5));
        assert_ne!(path(5), path(6));
    }

    #[test]
    fn a_profile_without_a_timer_lasts_the_target_s_life() {
        let once = r#"
            [[targets.motion_profiles]]
            weight = 1
            motion = { kind = "static" }
            [[targets.motion_profiles]]
            weight = 1
            motion = { kind = "orbit", speed_deg = [40, 40] }
        "#;
        for seed in 1..=6 {
            let mut s = Session::new(profiled(once), seed);
            let windows = moving_windows(&mut s);
            assert!(
                windows.iter().all(|m| *m == windows[0]),
                "{seed}: {windows:?}"
            );
        }
    }

    /// A `wave` motion with these extra fields, at 2 m/s, 0.5 m amplitude and 1 Hz.
    fn wave(extra: &str) -> String {
        format!(
            r#"{{ kind = "wave", speed = [2, 2], amplitude = [0.5, 0.5], frequency_hz = [1, 1]{extra} }}"#
        )
    }

    #[test]
    fn wave_glides_sideways_while_weaving_up_and_down() {
        let mut s = Session::new(def(SPHERE, AHEAD, &wave(", max_offset = 50")), 4);
        let (path, _) = run(&mut s, 101, 0.01);
        let start = path[0].unwrap();
        let end = path[100].unwrap();
        // One second at 2 m/s along the lateral axis (world x here), in either direction.
        assert!(((end.x - start.x).abs() - 2.0).abs() < 0.05, "{end:?}");
        assert!((end.z - start.z).abs() < 1e-4, "keeps its distance");
        // A whole period later it is back at its height, having swept 2 × amplitude.
        assert!((end.y - start.y).abs() < 0.02, "{} vs {}", end.y, start.y);
        let ys: Vec<f32> = path.iter().map(|p| p.unwrap().y).collect();
        let span = ys.iter().cloned().fold(f32::MIN, f32::max)
            - ys.iter().cloned().fold(f32::MAX, f32::min);
        assert!((span - 1.0).abs() < 0.02, "sweeps 2 × amplitude: {span}");
        let speed = s.targets().next().unwrap().velocity().length();
        assert!(speed > 1.9, "reports its velocity: {speed}");
    }

    #[test]
    fn a_zigzag_wave_climbs_and_falls_at_a_constant_speed() {
        let vertical_steps = |motion: &str| {
            let mut s = Session::new(def(SPHERE, AHEAD, motion), 4);
            let (path, _) = run(&mut s, 200, 0.01);
            path.windows(2)
                .map(|w| (w[1].unwrap().y - w[0].unwrap().y).abs())
                .collect::<Vec<f32>>()
        };
        // 4 × amplitude × frequency = 2 m/s, so 0.02 m per frame except at the turns.
        let steady = |steps: &[f32]| steps.iter().filter(|d| (**d - 0.02).abs() < 1e-3).count();
        let zigzag = vertical_steps(&wave(r#", form = "zigzag""#));
        assert!(steady(&zigzag) >= 190, "{zigzag:?}");
        let sine = vertical_steps(&wave(""));
        assert!(steady(&sine) < 100, "a sine eases into its turns");
    }

    #[test]
    fn a_wave_clipped_by_the_floor_keeps_its_centre_line() {
        // 1 m up with a 1.5 m amplitude: every trough is cut at the floor clearance.
        let motion = r#"{ kind = "wave", speed = [0, 0], amplitude = [1.5, 1.5], frequency_hz = [1, 1], max_offset = 50 }"#;
        let mut s = Session::new(def(SPHERE, POINT, motion), 4);
        let (path, _) = run(&mut s, 1000, 0.01);
        let ys: Vec<f32> = path.iter().map(|p| p.unwrap().y).collect();
        let peak = |cycle: &[f32]| cycle.iter().cloned().fold(f32::MIN, f32::max);
        let (first, last) = (peak(&ys[..100]), peak(&ys[900..]));
        assert!((last - first).abs() < 0.02, "peaks drift: {first} → {last}");
        assert!(ys.iter().all(|y| *y >= 0.3 - 1e-4), "stays above the floor");
    }

    #[test]
    fn wave_turns_back_at_its_max_offset() {
        let mut s = Session::new(def(SPHERE, AHEAD, &wave(", max_offset = 1")), 2);
        let (path, _) = run(&mut s, 400, 0.01);
        let xs: Vec<f32> = path.iter().map(|p| p.unwrap().x).collect();
        assert!(xs.iter().all(|x| x.abs() <= 1.03), "{xs:?}");
        let turns = xs
            .windows(3)
            .filter(|w| (w[1] - w[0]) * (w[2] - w[1]) < 0.0)
            .count();
        assert!(turns >= 3, "goes back and forth: {turns}");
    }

    #[test]
    fn wave_is_deterministic_per_seed_and_speed_tuning_plays_it_faster() {
        let motion = wave("");
        let path = |seed, k: f32| {
            let mut d = def(SPHERE, AHEAD, &motion);
            d.targets.speed_scale = k;
            let mut s = Session::new(d, seed);
            run(&mut s, 50, 0.01).0
        };
        assert_eq!(path(7, 1.0), path(7, 1.0));
        let (slow, fast) = (path(7, 1.0), path(7, 2.0));
        let moved = |p: &[Option<Vec3>]| (p[49].unwrap().x - p[0].unwrap().x).abs();
        assert!((moved(&fast) / moved(&slow) - 2.0).abs() < 0.05);
    }

    #[test]
    fn wave_parameters_are_validated() {
        let problems = |shape: &str, motion: &str| {
            let d = ScenarioDef::from_toml(&format!(
                r#"
                id = "m"
                name = "m"
                category = "custom"
                [targets]
                shape = {shape}
                spawn = {AHEAD}
                motion = {motion}
                "#
            ))
            .unwrap();
            d.validate()
                .into_iter()
                .map(|e| e.field)
                .collect::<Vec<_>>()
        };
        let bad = r#"{ kind = "wave", speed = [-1, 2], amplitude = [0, 0.5], frequency_hz = [0, 1], max_offset = 0 }"#;
        assert_eq!(
            problems(SPHERE, bad),
            [
                "targets.motion.speed",
                "targets.motion.amplitude",
                "targets.motion.frequency_hz",
                "targets.motion.max_offset",
            ]
        );
        assert_eq!(
            problems(r#"{ kind = "humanoid" }"#, &wave("")),
            ["targets.motion"]
        );
        assert!(problems(SPHERE, &wave("")).is_empty());
        assert!(
            ScenarioDef::from_toml(&format!(
                "id = \"m\"\nname = \"m\"\ncategory = \"custom\"\n[targets]\nshape = {SPHERE}\nspawn = {AHEAD}\nmotion = {}\n",
                wave(", wobble = 1")
            ))
            .is_err(),
            "unknown fields are rejected"
        );
    }

    /// Lateral speed (m/s, + is the player's right) of the first target at every step.
    fn lateral_speeds(s: &mut Session, frames: usize, dt: f32) -> Vec<f32> {
        let (path, _) = run(s, frames, dt);
        path.windows(2)
            .map(|w| match (w[0], w[1]) {
                (Some(a), Some(b)) => (b.x - a.x) / dt,
                _ => 0.0,
            })
            .collect()
    }

    /// Lengths in seconds of the runs of steps where `keep` holds, ignoring the first run
    /// when it starts at step 0 (the target is still at rest from its spawn).
    fn runs_where(v: &[f32], dt: f32, keep: impl Fn(f32) -> bool) -> Vec<f32> {
        let mut runs = Vec::new();
        let mut len = 0usize;
        let mut from_start = true;
        for &x in v {
            if keep(x) {
                len += 1;
            } else {
                if len > 0 && !from_start {
                    runs.push(len as f32 * dt);
                }
                len = 0;
                from_start = false;
            }
        }
        runs
    }

    fn strafe(extra: &str) -> String {
        format!(r#"{{ kind = "strafe", change_secs = [0.6, 0.6], max_offset = 100.0{extra} }}"#)
    }

    #[test]
    fn a_swap_pause_stops_the_target_at_every_reversal() {
        let stopped = |motion: &str| {
            let mut s = Session::new(def(SPHERE, POINT, motion), 3);
            let v = lateral_speeds(&mut s, 600, 0.01);
            runs_where(&v, 0.01, |x| x.abs() < 0.05)
        };
        // Without a pause a reversal only passes through zero speed.
        assert!(stopped(&strafe("")).iter().all(|&t| t <= 0.02));
        let paused = stopped(&strafe(", rhythm = { swap_pause_secs = [0.4, 0.4] }"));
        assert!(paused.len() >= 4, "{paused:?}");
        // It brakes first, then stands still until the pause ends.
        assert!(
            paused.iter().all(|&t| (0.2..=0.41).contains(&t)),
            "{paused:?}"
        );
    }

    #[test]
    fn dodge_targets_take_the_same_rhythm() {
        let dodge = |rhythm: &str| {
            format!(r#"{{ kind = "dodge", change_secs = [0.6, 0.6], max_offset = 100.0{rhythm} }}"#)
        };
        let mut s = Session::new(def(SPHERE, POINT, &dodge("")), 3);
        let v = lateral_speeds(&mut s, 600, 0.01);
        assert!(
            runs_where(&v, 0.01, |x| x.abs() < 0.05)
                .iter()
                .all(|&t| t <= 0.02)
        );

        let paused = dodge(", rhythm = { swap_pause_secs = [0.4, 0.4], right_secs_mult = 2.0 }");
        let mut s = Session::new(def(SPHERE, POINT, &paused), 3);
        let v = lateral_speeds(&mut s, 1200, 0.01);
        let stops = runs_where(&v, 0.01, |x| x.abs() < 0.05);
        assert!(
            stops.len() >= 4 && stops.iter().all(|&t| (0.2..=0.41).contains(&t)),
            "{stops:?}"
        );
        let right = v.iter().filter(|&&x| x > 0.05).count() as f32;
        let left = v.iter().filter(|&&x| x < -0.05).count() as f32;
        assert!((1.4..=2.4).contains(&(right / left)), "{right} / {left}");
    }

    #[test]
    fn a_swap_pause_also_stops_the_target_when_it_turns_at_the_lane_edge() {
        let motion = r#"{ kind = "strafe", change_secs = [30, 30], max_offset = 1.5, rhythm = { swap_pause_secs = [0.4, 0.4] } }"#;
        let mut s = Session::new(def(SPHERE, POINT, motion), 3);
        let v = lateral_speeds(&mut s, 600, 0.01);
        let paused = runs_where(&v, 0.01, |x| x.abs() < 0.05);
        assert!(paused.len() >= 2, "{paused:?}");
        assert!(
            paused.iter().all(|&t| (0.2..=0.41).contains(&t)),
            "{paused:?}"
        );
    }

    #[test]
    fn side_multipliers_make_strafes_to_one_side_last_longer() {
        let motion = strafe(", rhythm = { left_secs_mult = 1.0, right_secs_mult = 3.0 }");
        let mut s = Session::new(def(SPHERE, POINT, &motion), 3);
        let v = lateral_speeds(&mut s, 1200, 0.01);
        let right = v.iter().filter(|&&x| x > 0.05).count() as f32;
        let left = v.iter().filter(|&&x| x < -0.05).count() as f32;
        assert!((2.4..=3.6).contains(&(right / left)), "{right} / {left}");

        let mut even = Session::new(def(SPHERE, POINT, &strafe("")), 3);
        let v = lateral_speeds(&mut even, 1200, 0.01);
        let right = v.iter().filter(|&&x| x > 0.05).count() as f32;
        let left = v.iter().filter(|&&x| x < -0.05).count() as f32;
        assert!((0.8..=1.25).contains(&(right / left)), "{right} / {left}");
    }

    /// Times between the target's direction changes (the velocity's sign flips).
    fn strafe_lengths(motion: &str, seed: u64) -> Vec<f32> {
        let mut s = Session::new(def(SPHERE, POINT, motion), seed);
        let v = lateral_speeds(&mut s, 4000, 0.01);
        // A reversal can pass through a step of zero speed, so compare with the last side
        // the target was clearly moving to.
        let (mut flips, mut side) = (Vec::new(), 0.0f32);
        for (i, &x) in v.iter().enumerate() {
            if x.abs() > 0.05 {
                if side != 0.0 && x.signum() != side {
                    flips.push(i as f32 * 0.01);
                }
                side = x.signum();
            }
        }
        flips.windows(2).map(|w| w[1] - w[0]).collect()
    }

    #[test]
    fn balanced_strafes_spread_their_lengths_evenly_over_the_range() {
        let motion = r#"{ kind = "strafe", change_secs = [0.4, 1.2], max_offset = 100.0, rhythm = { balanced = true } }"#;
        let lengths = strafe_lengths(motion, 5);
        assert!(lengths.len() >= 40, "{}", lengths.len());
        assert!(round_means_stay_central(&lengths), "{lengths:?}");
        assert_eq!(lengths, strafe_lengths(motion, 5), "seeded");
        assert_ne!(lengths, strafe_lengths(motion, 6));
        // Independent draws wander further from the middle.
        let iid = r#"{ kind = "strafe", change_secs = [0.4, 1.2], max_offset = 100.0 }"#;
        assert!(!round_means_stay_central(&strafe_lengths(iid, 5)));
    }

    /// Lengths come in shuffled rounds of four, one from each quarter of [0.4, 1.2], so
    /// every round's mean stays within 0.1 s of the middle (0.8 s). The first decision is
    /// made from rest, so length `i` is the wait of decision `i + 1` and rounds start at 3.
    fn round_means_stay_central(lengths: &[f32]) -> bool {
        lengths[3..]
            .as_chunks::<4>()
            .0
            .iter()
            .all(|round| (round.iter().sum::<f32>() / 4.0 - 0.8).abs() <= 0.12)
    }

    /// Position of the first target after `secs` of 10 ms steps.
    fn at(motion: &str, secs: f32) -> Vec3 {
        let mut s = Session::new(def(SPHERE, POINT, motion), 1);
        let (path, _) = run(&mut s, (secs / 0.01).round() as usize, 0.01);
        path.last().copied().flatten().expect("a target")
    }

    fn near(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 0.03
    }

    #[test]
    fn a_path_walks_its_waypoints_in_order_and_loops_back_to_the_spawn() {
        // POINT spawns at (0, 1, -10): +x is the player's right and +z is towards him.
        let motion = r#"{ kind = "path", points = [[2, 0, 0], [2, 1, 1]], speed = [1, 1] }"#;
        assert!(
            near(at(motion, 1.0), Vec3::new(1.0, 1.0, -10.0)),
            "{:?}",
            at(motion, 1.0)
        );
        assert!(near(at(motion, 2.0), Vec3::new(2.0, 1.0, -10.0)));
        // The second leg is √2 m long, diagonally up and closer.
        let half = 2.0 + 2f32.sqrt() / 2.0;
        assert!(
            near(at(motion, half), Vec3::new(2.0, 1.5, -9.5)),
            "{:?}",
            at(motion, half)
        );
        // Then straight back to the spawn point (√6 m) and round again.
        let home = 2.0 + 2f32.sqrt() + 6f32.sqrt();
        assert!(
            near(at(motion, home), Vec3::new(0.0, 1.0, -10.0)),
            "{:?}",
            at(motion, home)
        );
        assert!(near(at(motion, home + 1.0), Vec3::new(1.0, 1.0, -10.0)));
    }

    #[test]
    fn a_ping_pong_path_retraces_its_route() {
        let route = |order: &str| {
            format!(
                r#"{{ kind = "path", points = [[2, 0, 0], [2, 2, 0]], speed = [1, 1]{order} }}"#
            )
        };
        // After the last waypoint (4 s), a loop heads straight home, a ping-pong goes back.
        let d = 1.0 / 2f32.sqrt();
        let looped = at(&route(""), 5.0);
        assert!(
            near(looped, Vec3::new(2.0 - d, 3.0 - d, -10.0)),
            "{looped:?}"
        );
        let ping_pong = at(&route(r#", route = "ping_pong""#), 5.0);
        assert!(near(ping_pong, Vec3::new(2.0, 2.0, -10.0)), "{ping_pong:?}");
        assert!(near(
            at(&route(r#", route = "ping_pong""#), 7.0),
            Vec3::new(1.0, 1.0, -10.0)
        ));
    }

    #[test]
    fn a_path_rests_at_each_waypoint_and_plays_faster_with_speed_tuning() {
        let motion =
            r#"{ kind = "path", points = [[2, 0, 0]], speed = [1, 1], pause_secs = [0.5, 0.5] }"#;
        assert!(near(at(motion, 2.4), Vec3::new(2.0, 1.0, -10.0)), "resting");
        assert!(
            near(at(motion, 3.5), Vec3::new(1.0, 1.0, -10.0)),
            "{:?}",
            at(motion, 3.5)
        );

        let mut d = def(
            SPHERE,
            POINT,
            r#"{ kind = "path", points = [[4, 0, 0]], speed = [1, 1] }"#,
        );
        d.targets.speed_scale = 2.0;
        let mut s = Session::new(d, 1);
        let (path, _) = run(&mut s, 100, 0.01);
        assert!(
            near(path[99].unwrap(), Vec3::new(2.0, 1.0, -10.0)),
            "{:?}",
            path[99]
        );
    }

    #[test]
    fn path_parameters_are_validated() {
        let problems = |shape: &str, motion: &str| {
            let d = ScenarioDef::from_toml(&format!(
                "id = \"m\"\nname = \"m\"\ncategory = \"custom\"\n[targets]\nshape = {shape}\nspawn = {POINT}\nmotion = {motion}\n"
            ))
            .unwrap();
            d.validate()
                .into_iter()
                .map(|e| e.field)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            problems(
                SPHERE,
                r#"{ kind = "path", points = [], speed = [0, 1], pause_secs = [-1, 0] }"#
            ),
            [
                "targets.motion.points",
                "targets.motion.speed",
                "targets.motion.pause_secs"
            ]
        );
        assert_eq!(
            problems(
                SPHERE,
                r#"{ kind = "path", points = [[60, 0, 0]], speed = [1, 1] }"#
            ),
            ["targets.motion.points"]
        );
        assert_eq!(
            problems(
                SPHERE,
                r#"{ kind = "path", points = [[0, 0, 0]], speed = [1, 1] }"#
            ),
            ["targets.motion.points"],
            "a route that never leaves the spawn point"
        );
        assert_eq!(
            problems(
                r#"{ kind = "humanoid" }"#,
                r#"{ kind = "path", points = [[2, 1, 0]], speed = [1, 1] }"#
            ),
            ["targets.motion.points"],
            "humanoids walk on the floor"
        );
        assert!(
            problems(
                r#"{ kind = "humanoid" }"#,
                r#"{ kind = "path", points = [[2, 0, 3]], speed = [1, 1] }"#
            )
            .is_empty()
        );
    }

    #[test]
    fn rhythm_parameters_are_validated() {
        let problems = |rhythm: &str| {
            let d = ScenarioDef::from_toml(&format!(
                "id = \"m\"\nname = \"m\"\ncategory = \"custom\"\n[targets]\nshape = {SPHERE}\nspawn = {POINT}\nmotion = {}\n",
                strafe(&format!(", rhythm = {rhythm}"))
            ))
            .unwrap();
            d.validate()
                .into_iter()
                .map(|e| e.field)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            problems("{ swap_pause_secs = [-0.1, 0.2] }"),
            ["targets.motion.rhythm.swap_pause_secs"]
        );
        assert_eq!(
            problems("{ swap_pause_secs = [0.1, 3.0] }"),
            ["targets.motion.rhythm.swap_pause_secs"]
        );
        assert_eq!(
            problems("{ left_secs_mult = 0.1, right_secs_mult = 5.0 }"),
            [
                "targets.motion.rhythm.left_secs_mult",
                "targets.motion.rhythm.right_secs_mult"
            ]
        );
        assert!(problems("{ swap_pause_secs = [0.1, 0.3], balanced = true }").is_empty());
    }
}
