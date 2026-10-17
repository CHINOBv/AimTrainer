//! Bot movement and behavior: a small state machine per mode, VALORANT movement through
//! `aim_core::movement::step`, and the Fitts-law aim of [`super::aim`].

use super::aim::{BotAim, Sight, angular_radius_deg};
use crate::map::{AngleMarker, DEFAULT_PLAYER_RADIUS, MapDef};
use aim_core::math::{Range, Vec3, ViewAngles, angle_between_deg};
use aim_core::movement::{self, LocalVelocity, MoveInput, MovementProfile};
use aim_core::rng::Rng;
use aim_core::sim::AimerProfile;
use aim_core::valorant::HumanoidDims;

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

/// Yaw (core convention) of a horizontal direction.
pub(crate) fn yaw_of(dir: Vec3) -> f32 {
    dir.x.atan2(-dir.z).to_degrees()
}

/// Head center of a standing humanoid whose feet are at `feet`.
pub(crate) fn head_of(feet: Vec3) -> Vec3 {
    feet + Vec3::new(0.0, HumanoidDims::default().head_center_height(), 0.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Phase {
    /// Waiting at the hidden spot; `peek_at` is set once the player can see the angle.
    Hidden {
        peek_at: Option<f32>,
    },
    Peeking,
    Holding {
        until: f32,
    },
    Hiding,
}

#[derive(Debug, Clone)]
pub(crate) enum Brain {
    Roam {
        route: Vec<Vec3>,
        next: usize,
        pause_until: f32,
    },
    Angle {
        marker: AngleMarker,
        phase: Phase,
    },
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
    /// Points a roaming bot picks its next destination from.
    pub roam_points: &'a [Vec3],
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
        self.drive(ctx, goal);

        let sight = self.sight_of_player(ctx.player_feet);
        let roaming = matches!(self.brain, Brain::Roam { .. });
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
        landed
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
        match &mut self.brain {
            Brain::Roam {
                route,
                next,
                pause_until,
            } => {
                // Stop to fight: release the keys and let the stop curve brake.
                if sees && engaged {
                    route.clear();
                    *pause_until = ctx.now;
                    return None;
                }
                while *next < route.len() && (route[*next] - feet).horizontal().length() < 0.3 {
                    *next += 1;
                }
                if *next < route.len() {
                    return Some(route[*next]);
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
                match *phase {
                    Phase::Hidden { peek_at } => match peek_at {
                        None if player_sees_angle => {
                            *phase = Phase::Hidden {
                                peek_at: Some(ctx.now + rng.in_range(PEEK_DELAY_SECS)),
                            };
                        }
                        Some(t) if ctx.now >= t => *phase = Phase::Peeking,
                        _ => {}
                    },
                    Phase::Peeking => {
                        if (marker.position - feet).horizontal().length() < ARRIVE_EPS {
                            *phase = Phase::Holding {
                                until: ctx.now + ctx.hold_secs,
                            };
                        }
                    }
                    Phase::Holding { until } => {
                        if ctx.now >= until {
                            *phase = Phase::Hiding;
                        }
                    }
                    Phase::Hiding => {
                        if (marker.hidden_position() - feet).horizontal().length() < ARRIVE_EPS {
                            *phase = Phase::Hidden { peek_at: None };
                        }
                    }
                }
                match *phase {
                    Phase::Peeking | Phase::Holding { .. } => Some(marker.position),
                    Phase::Hidden { .. } | Phase::Hiding => Some(marker.hidden_position()),
                }
            }
        }
    }

    /// Walk toward `goal` (or brake without one) with the VALORANT movement model, then
    /// resolve collisions with the map. Bots stop exactly on the final point of an angle
    /// peek (a simplification: a perfect counter-strafe).
    fn drive(&mut self, ctx: &Ctx, goal: Option<Vec3>) {
        if ctx.dt <= 0.0 {
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
