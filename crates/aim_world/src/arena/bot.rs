//! Bot movement and behavior: the state machines of [`super::behavior`], VALORANT movement
//! through `aim_core::movement::step`, and the Fitts-law aim of [`super::aim`].

use super::aim::{BotAim, Sight, angular_radius_deg};
use super::behavior::{PeekSense, Phase, Push, Retreat, RetreatSense, Strafe};
use crate::map::{AngleMarker, DEFAULT_PLAYER_RADIUS, MapDef};
use aim_core::games::valorant::HumanoidDims;
use aim_core::math::{Range, Vec3, ViewAngles, angle_between_deg};
use aim_core::movement::{self, LocalVelocity, MoveInput, MovementProfile};
use aim_core::rng::Rng;
use aim_core::sim::AimerProfile;

/// Bot health: 100 health plus 50 armor, counted as one pool (a simplification: armor
/// damage reduction is not modeled).
pub const BOT_HEALTH: f32 = 150.0;
/// Half-angle in degrees inside which a roaming bot notices the player. An estimate based
/// on VALORANT's 103 degree horizontal field of view, rounded up; once a bot is engaged it
/// keeps tracking whatever the angle.
pub const BOT_NOTICE_HALF_ANGLE_DEG: f32 = 60.0;
/// Pause at the end of a roaming route before the next one, seconds.
pub const ROAM_PAUSE_SECS: Range = Range::new(0.3, 1.2);
/// Delay between the player gaining sight of an angle and the bot peeking it, seconds.
pub const PEEK_DELAY_SECS: Range = Range::new(0.4, 1.2);
/// Distance under which a bot counts as having reached a point, meters.
const ARRIVE_EPS: f32 = 0.05;
/// Distance under which a moving bot counts as having passed a route point, meters.
const ROUTE_EPS: f32 = 0.3;
/// Longest route a shot roaming bot runs to reach cover, meters. A design choice: about
/// 1.5 s at rifle run speed, so a bot only falls back to cover that is close by.
pub const RETREAT_MAX_ROUTE_M: f32 = 8.0;

/// Yaw (core convention) of a horizontal direction.
pub(crate) fn yaw_of(dir: Vec3) -> f32 {
    dir.x.atan2(-dir.z).to_degrees()
}

/// The next point of `route` to walk to from `feet`, skipping the points already reached.
fn follow(route: &[Vec3], next: &mut usize, feet: Vec3) -> Option<Vec3> {
    while *next < route.len() && (route[*next] - feet).horizontal().length() < ROUTE_EPS {
        *next += 1;
    }
    route.get(*next).copied()
}

/// Head center of a standing humanoid whose feet are at `feet`.
pub(crate) fn head_of(feet: Vec3) -> Vec3 {
    feet + Vec3::new(0.0, HumanoidDims::default().head_center_height(), 0.0)
}

#[derive(Debug, Clone)]
pub(crate) enum Brain {
    Roam {
        route: Vec<Vec3>,
        next: usize,
        pause_until: f32,
        /// Falling back to cover after being shot, and re-peeking.
        retreat: Retreat,
    },
    Angle {
        marker: AngleMarker,
        phase: Phase,
    },
    /// Walk toward the site of a site objective, stopping to fight on the way.
    Push {
        route: Vec<Vec3>,
        next: usize,
        push: Push,
    },
}

impl Brain {
    /// Roaming and pushing bots move through the map and fight duels; angle bots hold an
    /// angle.
    fn duels(&self) -> bool {
        matches!(self, Self::Roam { .. } | Self::Push { .. })
    }

    /// Whether the bot is running for cover, holding its fire.
    fn fleeing(&self) -> bool {
        matches!(self, Self::Roam { retreat, .. } if retreat.fleeing())
    }
}

/// What a bot needs to know about the world this frame.
pub(crate) struct Ctx<'a> {
    pub map: &'a MapDef,
    pub now: f32,
    pub dt: f32,
    pub player_feet: Vec3,
    /// False during the player's respawn grace: bots act as if they cannot see them.
    pub player_targetable: bool,
    pub movement: &'a MovementProfile,
    pub aimer: &'a AimerProfile,
    /// Reaction time range for angle bots; roaming bots use the aimer's reaction time.
    pub reaction: Option<Range>,
    pub hold_secs: f32,
    /// Delay before an angle bot peeks once the player sees its angle.
    pub peek_delay: Range,
    /// Points a roaming bot picks its next destination from.
    pub roam_points: &'a [Vec3],
    /// Whether dueling bots strafe while they fight.
    pub bot_strafe: bool,
    /// Where pushing bots walk to.
    pub site: Option<Vec3>,
    /// The map's cover spots ([`MapDef::cover_spots`]), where shot roaming bots fall back.
    pub cover_spots: &'a [Vec3],
}

#[derive(Debug, Clone)]
pub(crate) struct Bot {
    pub id: u64,
    pub feet: Vec3,
    /// World-space velocity, m/s.
    pub vel: Vec3,
    pub facing_yaw_deg: f32,
    pub health: f32,
    pub alive: bool,
    pub died_at: f32,
    pub aim: BotAim,
    pub last_shot_at: f32,
    pub sees_player: bool,
    pub visible_to_player: bool,
    pub first_seen_at: Option<f32>,
    pub brain: Brain,
    pub strafe: Strafe,
    /// Eye of the player who shot the bot since its last think, if anyone did: the only
    /// thing a shot bot learns about the player's position.
    pub shot_from: Option<Vec3>,
}

impl Bot {
    pub fn new(id: u64, feet: Vec3, facing_yaw_deg: f32, brain: Brain) -> Self {
        Self {
            id,
            feet,
            vel: Vec3::ZERO,
            facing_yaw_deg,
            health: BOT_HEALTH,
            alive: true,
            died_at: 0.0,
            aim: BotAim::new(ViewAngles::new(facing_yaw_deg, 0.0)),
            last_shot_at: f32::NEG_INFINITY,
            sees_player: false,
            visible_to_player: false,
            first_seen_at: None,
            brain,
            strafe: Strafe::Idle,
            shot_from: None,
        }
    }

    pub fn eye(&self) -> Vec3 {
        MapDef::eye(self.feet)
    }

    fn sight_of_player(&self, player_feet: Vec3) -> Sight {
        let eye = self.eye();
        let point = head_of(player_feet);
        let half = HumanoidDims::default().head_size / 2.0;
        Sight {
            eye,
            point,
            radius_deg: angular_radius_deg(half, (point - eye).length()),
        }
    }

    /// Advance the bot one frame. Returns true when its crosshair has landed on the
    /// player and it wants to fire.
    pub fn think(&mut self, ctx: &Ctx, rng: &mut Rng) -> bool {
        if !self.alive {
            return false;
        }
        let sees = ctx.player_targetable && ctx.map.visible_head(self.eye(), ctx.player_feet);
        let goal = self.move_goal(ctx, sees, rng);
        let strafe = if ctx.bot_strafe {
            let dueling = sees && self.aim.is_engaged() && self.brain.duels() && goal.is_none();
            self.strafe.step(dueling, ctx.now, rng)
        } else {
            0.0
        };
        self.drive(ctx, goal, strafe);

        let sight = self.sight_of_player(ctx.player_feet);
        let roaming = self.brain.duels();
        let noticed = sees
            && (self.aim.is_engaged()
                || !roaming
                || angle_between_deg(
                    ViewAngles::new(self.facing_yaw_deg, 0.0).direction(),
                    (sight.point - sight.eye).horizontal(),
                ) <= BOT_NOTICE_HALF_ANGLE_DEG);
        self.sees_player = noticed;

        if !noticed {
            self.aim.disengage();
            if roaming {
                if self.vel.horizontal().length() > 0.1 {
                    self.facing_yaw_deg = yaw_of(self.vel);
                }
                self.aim.view = ViewAngles::new(self.facing_yaw_deg, 0.0);
            } else {
                // Angle bots keep their crosshair on where the player is coming from.
                self.aim.view = ViewAngles::from_direction(sight.point - sight.eye);
                self.facing_yaw_deg = self.aim.view.yaw_deg;
            }
            return false;
        }
        if !self.aim.is_engaged() {
            let reaction = ctx
                .reaction
                .map_or(ctx.aimer.reaction_secs, |r| rng.in_range(r));
            self.aim.plan(ctx.now, reaction, sight, ctx.aimer, rng);
        }
        let landed = self.aim.track(ctx.now, sight);
        self.facing_yaw_deg = self.aim.view.yaw_deg;
        landed && !self.brain.fleeing()
    }

    /// After a shot, plan the corrective flick: half the reaction time, as in `SimAimer`.
    pub fn after_shot(&mut self, ctx: &Ctx, rng: &mut Rng) {
        let reaction = ctx
            .reaction
            .map_or(ctx.aimer.reaction_secs, |r| rng.in_range(r))
            * 0.5;
        let sight = self.sight_of_player(ctx.player_feet);
        self.aim.plan(ctx.now, reaction, sight, ctx.aimer, rng);
    }

    /// Where the bot wants to walk this frame, updating its state machine.
    fn move_goal(&mut self, ctx: &Ctx, sees: bool, rng: &mut Rng) -> Option<Vec3> {
        let engaged = self.aim.is_engaged();
        let feet = self.feet;
        let shot_from = self.shot_from.take();
        match &mut self.brain {
            Brain::Roam {
                route,
                next,
                pause_until,
                retreat,
            } => {
                let before = retreat.goal();
                if let Some(threat) = shot_from
                    && !retreat.fleeing()
                    && let Some(spot) =
                        ctx.map
                            .cover_from(ctx.cover_spots, threat, feet, RETREAT_MAX_ROUTE_M)
                {
                    *retreat = Retreat::start(spot, feet, ctx.now);
                }
                let sense = RetreatSense {
                    now: ctx.now,
                    feet,
                    fighting: sees && engaged,
                };
                *retreat = retreat.next(&sense, rng);
                let goal = retreat.goal();
                if goal != before {
                    // A new leg of the fall-back, or back to roaming.
                    *route = goal.and_then(|g| ctx.map.path(feet, g)).unwrap_or_default();
                    *next = 1;
                    *pause_until = ctx.now;
                    if goal.is_some() && route.is_empty() {
                        *retreat = Retreat::Off;
                    }
                }
                if *retreat != Retreat::Off {
                    return follow(route, next, feet);
                }
                // Stop to fight: release the keys and let the stop curve brake.
                if sees && engaged {
                    route.clear();
                    *pause_until = ctx.now;
                    return None;
                }
                if let Some(point) = follow(route, next, feet) {
                    return Some(point);
                }
                if !route.is_empty() {
                    route.clear();
                    *pause_until = ctx.now + rng.in_range(ROAM_PAUSE_SECS);
                }
                if ctx.now >= *pause_until && !ctx.roam_points.is_empty() {
                    let pick = (rng.next_u64() % ctx.roam_points.len() as u64) as usize;
                    let dest = ctx.roam_points[pick];
                    if (dest - feet).horizontal().length() > 1.0
                        && let Some(path) = ctx.map.path(feet, dest)
                    {
                        *route = path;
                        *next = 1;
                    }
                }
                None
            }
            Brain::Angle { marker, phase } => {
                let player_sees_angle = ctx
                    .map
                    .visible_head(MapDef::eye(ctx.player_feet), marker.position);
                let sense = PeekSense {
                    now: ctx.now,
                    player_sees_angle,
                    at_peek: (marker.position - feet).horizontal().length() < ARRIVE_EPS,
                    at_hidden: (marker.hidden_position() - feet).horizontal().length() < ARRIVE_EPS,
                    hold_secs: ctx.hold_secs,
                    peek_delay: ctx.peek_delay,
                };
                *phase = phase.next(&sense, rng);
                Some(if phase.wants_peek_position() {
                    marker.position
                } else {
                    marker.hidden_position()
                })
            }
            Brain::Push { route, next, push } => {
                if push.step(sees && engaged, ctx.now) {
                    route.clear();
                    return None;
                }
                let site = ctx.site?;
                if route.is_empty() && (site - feet).horizontal().length() > ROUTE_EPS {
                    *route = ctx.map.path(feet, site).unwrap_or_else(|| vec![feet, site]);
                    *next = 1;
                }
                follow(route, next, feet)
            }
        }
    }

    /// Walk toward `goal` (or brake without one) with the VALORANT movement model, then
    /// resolve collisions with the map. Bots stop exactly on the final point of an angle
    /// peek (a simplification: a perfect counter-strafe). A non-zero `strafe` presses that
    /// strafe key relative to where the bot faces instead.
    fn drive(&mut self, ctx: &Ctx, goal: Option<Vec3>, strafe: f32) {
        if ctx.dt <= 0.0 {
            return;
        }
        if strafe != 0.0 {
            let yaw = self.facing_yaw_deg;
            let input = MoveInput {
                right: strafe,
                ..Default::default()
            };
            let local = movement::step(
                ctx.movement,
                LocalVelocity::from_world(self.vel, yaw),
                input,
                ctx.dt,
            );
            let before = self.feet;
            self.feet = ctx.map.move_with_collision(
                before,
                local.to_world(yaw) * ctx.dt,
                DEFAULT_PLAYER_RADIUS,
            );
            self.vel = (self.feet - before) * (1.0 / ctx.dt);
            return;
        }
        let to = goal.map_or(Vec3::ZERO, |g| (g - self.feet).horizontal());
        let dist = to.length();
        let (yaw, input) = if dist > ARRIVE_EPS {
            (
                yaw_of(to),
                MoveInput {
                    forward: 1.0,
                    ..Default::default()
                },
            )
        } else if self.vel.horizontal().length() > 0.0 {
            (yaw_of(self.vel), MoveInput::default())
        } else {
            return;
        };
        let local = LocalVelocity::from_world(self.vel, yaw);
        let local = movement::step(ctx.movement, local, input, ctx.dt);
        let mut delta = local.to_world(yaw) * ctx.dt;
        let snap = matches!(self.brain, Brain::Angle { .. }) && dist > ARRIVE_EPS;
        if snap && delta.length() >= dist {
            delta = to;
        }
        let before = self.feet;
        self.feet = ctx
            .map
            .move_with_collision(before, delta, DEFAULT_PLAYER_RADIUS);
        self.vel = (self.feet - before) * (1.0 / ctx.dt);
        if snap
            && (goal.unwrap_or(self.feet) - self.feet)
                .horizontal()
                .length()
                < ARRIVE_EPS
        {
            self.vel = Vec3::ZERO;
        }
    }
}
