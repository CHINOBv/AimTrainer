//! A running arena: the player, the bots, shooting with occlusion and the run statistics.

use super::behavior::{Phase, Push};
use super::bot::{Bot, Brain, Ctx, PEEK_DELAY_SECS, head_of, yaw_of};
use super::def::{ArenaDef, ArenaError, ArenaMode, SiteObjective, bot_profile};
use crate::map::{DEFAULT_PLAYER_RADIUS, MapDef, SpawnKind};
use aim_core::activity::{Activity, ActivityEvent};
use aim_core::math::{Vec3, ViewAngles, angle_between_deg};
use aim_core::movement::{self, LocalVelocity, MovementProfile};
use aim_core::rng::Rng;
use aim_core::scenario::hit::{HitZone, Ray, ray_humanoid};
use aim_core::scenario::{FrameInput, RunResult, shot_direction};
use aim_core::sim::AimerProfile;
use aim_core::valorant::{HumanoidDims, Weapon, WeaponStats};

/// Player health. VALORANT players start a deathmatch life with 100 health and 50 armor;
/// the arena gives the player health only, so a bot body shot matters (a simplification).
pub const PLAYER_HEALTH: f32 = 100.0;
/// The weapon every bot carries.
pub const BOT_WEAPON: Weapon = Weapon::Vandal;
/// Score per kill, per death (subtracted) and per headshot hit.
pub const KILL_POINTS: f32 = 100.0;
pub const DEATH_POINTS: f32 = 50.0;
pub const HEADSHOT_BONUS: f32 = 25.0;
/// Seconds after a respawn during which bots ignore the player. An arena rule, not a
/// VALORANT value: it stops bots from killing the player again on the spawn point.
pub const RESPAWN_GRACE_SECS: f32 = 1.5;
/// Score per pre-aimed bot life in arenas that set `pre_aim_tolerance_deg`. A design
/// choice: half a kill, so pre-aiming matters without outweighing the kill itself.
pub const PRE_AIM_POINTS: f32 = 50.0;
/// Score for holding the site of a site objective until the end. A design choice: worth
/// five kills, so losing the site costs more than a few deaths.
pub const SITE_HELD_BONUS: f32 = 500.0;

#[derive(Debug, Clone, PartialEq)]
pub enum ArenaEvent {
    /// The player fired. `hit` is the bot id and zone, or `None` for a miss or a shot that
    /// hit the map first.
    Shot {
        hit: Option<(u64, HitZone)>,
        moving: bool,
    },
    BotKilled {
        id: u64,
        /// Seconds from the bot first becoming visible to the player in this life.
        time_to_kill_secs: f32,
    },
    PlayerHit {
        damage: f32,
    },
    PlayerDied,
    BotSpawned {
        id: u64,
    },
    Finished,
}

/// A bot as the renderer and the HUD see it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BotView {
    pub id: u64,
    pub feet: Vec3,
    pub facing_yaw_deg: f32,
    pub alive: bool,
    /// Whether any part of the bot's head is in the player's line of sight.
    pub visible_to_player: bool,
}

impl BotView {
    /// Head center of the bot.
    pub fn head(&self) -> Vec3 {
        head_of(self.feet)
    }
}

/// Running totals of an arena.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArenaStats {
    pub shots: u32,
    pub hits: u32,
    pub headshots: u32,
    pub moving_shots: u32,
    pub kills: u32,
    pub deaths: u32,
    pub damage_dealt: f32,
    pub damage_taken: f32,
    /// Seconds from first sight to death, per killed bot.
    pub kill_times: Vec<f32>,
    /// Crosshair placement error in degrees at the first frame each bot life became
    /// visible: the angle between the view direction and the bot's head.
    pub crosshair_errors_deg: Vec<f32>,
    /// The vertical part of the same error: how far above or below head level the
    /// crosshair was, in degrees of pitch.
    pub head_level_errors_deg: Vec<f32>,
    /// Bot lives first seen within the arena's `pre_aim_tolerance_deg` (0 when the arena
    /// does not set one).
    pub pre_aimed: u32,
}

fn median(values: &[f32]) -> Option<f32> {
    let mut v = values.to_vec();
    if v.is_empty() {
        return None;
    }
    v.sort_by(f32::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    })
}

impl ArenaStats {
    /// Median crosshair placement error, degrees.
    pub fn median_crosshair_error_deg(&self) -> Option<f32> {
        median(&self.crosshair_errors_deg)
    }

    /// Median head-level (pitch) error at first sight, degrees.
    pub fn median_head_level_error_deg(&self) -> Option<f32> {
        median(&self.head_level_errors_deg)
    }
}

/// The state of a site objective, for the HUD and the results.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectiveStatus {
    /// Attackers the run sends in total.
    pub attackers: u32,
    /// Attackers not killed yet, alive or still to come.
    pub attackers_left: u32,
    /// How far the current capture has progressed, 0 to 1.
    pub capture_progress: f32,
    /// An attacker captured the site; the run ended.
    pub lost: bool,
}

/// A site objective while it runs.
#[derive(Debug, Clone)]
struct Site {
    center: Vec3,
    def: SiteObjective,
    spawned: u32,
    killed: u32,
    capture_secs: f32,
    lost: bool,
}

#[derive(Debug, Clone)]
struct Player {
    feet: Vec3,
    vel: LocalVelocity,
    view: ViewAngles,
    health: f32,
    last_shot_at: f32,
}

pub struct ArenaSession {
    def: ArenaDef,
    map: MapDef,
    rng: Rng,
    time: f32,
    finished: bool,
    weapon: WeaponStats,
    profile: MovementProfile,
    bot_weapon: WeaponStats,
    bot_movement: MovementProfile,
    aimer: AimerProfile,
    dims: HumanoidDims,
    player: Player,
    bots: Vec<Bot>,
    next_id: u64,
    pending: Vec<ArenaEvent>,
    stats: ArenaStats,
    roam_points: Vec<Vec3>,
    /// Angle clear: index into the mode's angle list of the angle being cleared.
    angle_index: usize,
    /// Bots ignore the player until this time (respawn grace).
    grace_until: f32,
    /// Deathmatch with a site objective.
    site: Option<Site>,
}

/// The aimer preset angle bots use for flick speed and endpoint error; their reaction time
/// comes from the arena's `bot_reaction_secs`.
const ANGLE_BOT_SKILL: &str = "contender";

impl ArenaSession {
    /// Start an arena on its map. Fails if the definition is invalid for the map.
    pub fn new(def: ArenaDef, map: MapDef, seed: u64) -> Result<Self, ArenaError> {
        let errs = def.validate_with_map(&map);
        if !errs.is_empty() {
            return Err(ArenaError::Invalid(errs));
        }
        let skill = match &def.mode {
            ArenaMode::Deathmatch { bot_skill, .. } => bot_skill.as_str(),
            ArenaMode::AngleClear { .. } => ANGLE_BOT_SKILL,
        };
        let aimer = bot_profile(skill).expect("validated above");
        let mut roam_points: Vec<Vec3> = map.waypoints.iter().map(|w| w.position).collect();
        roam_points.extend(map.spawns_of(SpawnKind::Bot).map(|s| s.position));
        let first = map
            .spawns_of(SpawnKind::Player)
            .next()
            .expect("validated map has a player spawn");
        let player = Player {
            feet: first.position,
            vel: LocalVelocity::default(),
            view: ViewAngles::new(first.yaw_deg, 0.0),
            health: PLAYER_HEALTH,
            last_shot_at: f32::NEG_INFINITY,
        };
        let mut s = Self {
            weapon: def.weapon.stats(),
            profile: def.weapon.movement(),
            bot_weapon: BOT_WEAPON.stats(),
            bot_movement: BOT_WEAPON.movement(),
            aimer,
            dims: HumanoidDims::default(),
            rng: Rng::new(seed),
            time: 0.0,
            finished: false,
            player,
            bots: Vec::new(),
            next_id: 1,
            pending: Vec::new(),
            stats: ArenaStats::default(),
            roam_points,
            angle_index: 0,
            grace_until: 0.0,
            site: None,
            def,
            map,
        };
        match s.def.mode.clone() {
            ArenaMode::Deathmatch {
                bots,
                objective: Some(objective),
                ..
            } => {
                let center = s
                    .map
                    .waypoints
                    .iter()
                    .find(|w| w.id == objective.site)
                    .expect("validated above")
                    .position;
                let first = bots.min(objective.attackers);
                s.site = Some(Site {
                    center,
                    def: objective,
                    spawned: 0,
                    killed: 0,
                    capture_secs: 0.0,
                    lost: false,
                });
                for _ in 0..first {
                    s.spawn_roaming_bot(None);
                }
            }
            ArenaMode::Deathmatch { bots, .. } => {
                for _ in 0..bots {
                    s.spawn_roaming_bot(None);
                }
            }
            ArenaMode::AngleClear { .. } => s.start_angle(),
        }
        Ok(s)
    }

    /// Change the run's length before it starts (routines shorten arenas). The result's id
    /// becomes `id~Ns`, the key shortened scenarios use, so the history never mixes
    /// lengths. A non-positive length, or a run that has already started, is left as is.
    pub fn with_duration(mut self, secs: f32) -> Self {
        if secs.is_finite() && secs > 0.0 && self.time == 0.0 {
            self.def.id = format!("{}~{}s", self.def.id, secs.round() as u32);
            self.def.duration_secs = secs;
        }
        self
    }

    pub fn def(&self) -> &ArenaDef {
        &self.def
    }
    pub fn map(&self) -> &MapDef {
        &self.map
    }
    pub fn stats(&self) -> &ArenaStats {
        &self.stats
    }
    pub fn time(&self) -> f32 {
        self.time
    }
    pub fn is_finished(&self) -> bool {
        self.finished
    }
    pub fn remaining(&self) -> f32 {
        (self.def.duration_secs - self.time).max(0.0)
    }
    /// The player's view angles as of the last update.
    pub fn view(&self) -> ViewAngles {
        self.player.view
    }
    pub fn player_feet(&self) -> Vec3 {
        self.player.feet
    }
    pub fn eye_position(&self) -> Vec3 {
        MapDef::eye(self.player.feet)
    }
    pub fn player_health(&self) -> f32 {
        self.player.health
    }
    pub fn player_speed(&self) -> f32 {
        self.player.vel.speed()
    }
    /// Whether the player is slow enough for the weapon's first-shot accuracy.
    pub fn is_accurate(&self) -> bool {
        self.player_speed() <= self.profile.accuracy_speed_limit()
    }

    /// The site objective, if the arena has one.
    pub fn objective(&self) -> Option<ObjectiveStatus> {
        self.site.as_ref().map(|s| ObjectiveStatus {
            attackers: s.def.attackers,
            attackers_left: s.def.attackers - s.killed,
            capture_progress: (s.capture_secs / s.def.capture_secs).clamp(0.0, 1.0),
            lost: s.lost,
        })
    }

    pub fn bots(&self) -> impl Iterator<Item = BotView> + '_ {
        self.bots.iter().map(|b| BotView {
            id: b.id,
            feet: b.feet,
            facing_yaw_deg: b.facing_yaw_deg,
            alive: b.alive,
            visible_to_player: b.alive && b.visible_to_player,
        })
    }

    pub fn update(&mut self, input: FrameInput) -> Vec<ArenaEvent> {
        let mut events = std::mem::take(&mut self.pending);
        if self.finished {
            return events;
        }
        let dt = input.dt.clamp(0.0, 0.25).min(self.remaining());
        self.time += dt;
        self.player.view = input.view;

        self.move_player(input, dt);
        self.update_bots(dt, &mut events);
        self.update_player_sight();
        if input.fire_pressed {
            self.player_shoot(&mut events);
        }
        self.respawn_bots();
        self.update_site(dt);
        events.append(&mut self.pending);

        if self.time >= self.def.duration_secs && !self.finished {
            self.finished = true;
        }
        if self.finished {
            events.push(ArenaEvent::Finished);
        }
        events
    }

    pub fn result(&self) -> RunResult {
        let s = &self.stats;
        let minutes = self.time / 60.0;
        let ratio = |a: u32, b: u32| if b == 0 { 0.0 } else { a as f32 / b as f32 };
        RunResult {
            scenario_id: self.def.id.clone(),
            score: s.kills as f32 * KILL_POINTS - s.deaths as f32 * DEATH_POINTS
                + s.headshots as f32 * HEADSHOT_BONUS
                + s.pre_aimed as f32 * PRE_AIM_POINTS
                + if self.site_held() {
                    SITE_HELD_BONUS
                } else {
                    0.0
                },
            accuracy: ratio(s.hits, s.shots),
            kills: s.kills,
            kills_per_min: if minutes > 0.0 {
                s.kills as f32 / minutes
            } else {
                0.0
            },
            avg_time_to_kill: (!s.kill_times.is_empty())
                .then(|| s.kill_times.iter().sum::<f32>() / s.kill_times.len() as f32),
            headshot_rate: ratio(s.headshots, s.hits),
            moving_shots: s.moving_shots,
            expired: s.deaths,
            duration_secs: self.time,
        }
    }

    fn move_player(&mut self, input: FrameInput, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        let p = &mut self.player;
        p.vel = movement::step(&self.profile, p.vel, input.movement, dt);
        let yaw = p.view.yaw_deg;
        let before = p.feet;
        p.feet =
            self.map
                .move_with_collision(before, p.vel.to_world(yaw) * dt, DEFAULT_PLAYER_RADIUS);
        // Velocity pushing into a wall is lost; the rest keeps sliding along it.
        p.vel = LocalVelocity::from_world((p.feet - before) * (1.0 / dt), yaw);
    }

    /// Whether the run ended with the site of its objective still held.
    fn site_held(&self) -> bool {
        self.finished && self.site.as_ref().is_some_and(|s| !s.lost)
    }

    /// Site objective: advance the capture by living attackers on the site, and end the
    /// run when it is captured or every attacker is dead.
    fn update_site(&mut self, dt: f32) {
        let Some(site) = &mut self.site else {
            return;
        };
        let on_site = self.bots.iter().any(|b| {
            b.alive && (b.feet - site.center).horizontal().length() <= site.def.site_radius
        });
        // Killing or pushing the attacker off the site resets the capture.
        site.capture_secs = if on_site { site.capture_secs + dt } else { 0.0 };
        if site.capture_secs >= site.def.capture_secs {
            site.lost = true;
            self.finished = true;
        } else if site.killed >= site.def.attackers {
            self.finished = true;
        }
    }

    fn update_player_sight(&mut self) {
        let eye = self.eye_position();
        let view = self.player.view;
        let dir = view.direction();
        let tolerance = match &self.def.mode {
            ArenaMode::AngleClear {
                pre_aim_tolerance_deg,
                ..
            } => *pre_aim_tolerance_deg,
            ArenaMode::Deathmatch { .. } => None,
        };
        for b in self.bots.iter_mut().filter(|b| b.alive) {
            b.visible_to_player = self.map.visible_head(eye, b.feet);
            if b.visible_to_player && b.first_seen_at.is_none() {
                b.first_seen_at = Some(self.time);
                let to_head = head_of(b.feet) - eye;
                let err = angle_between_deg(dir, to_head);
                self.stats.crosshair_errors_deg.push(err);
                let pitch = ViewAngles::from_direction(to_head).pitch_deg;
                self.stats
                    .head_level_errors_deg
                    .push((pitch - view.pitch_deg).abs());
                if tolerance.is_some_and(|t| err <= t) {
                    self.stats.pre_aimed += 1;
                }
            }
        }
    }

    /// The shot direction and whether the shooter was too fast to be accurate. Same rule as
    /// the scenario session: first-shot spread always, plus run spread scaled by how far the
    /// speed is above the accuracy limit (27.5% of run speed for rifles).
    fn damage(weapon: &WeaponStats, zone: HitZone) -> f32 {
        match zone {
            HitZone::Head => weapon.damage_head,
            HitZone::Body | HitZone::Sphere => weapon.damage_body,
            HitZone::Legs => weapon.damage_legs,
        }
    }

    fn player_shoot(&mut self, events: &mut Vec<ArenaEvent>) {
        let p = &self.player;
        if self.time - p.last_shot_at < 1.0 / self.weapon.fire_rate {
            return;
        }
        let speed = p.vel.speed();
        let (dir, moving) = shot_direction(
            p.view,
            speed,
            &self.profile,
            &self.weapon,
            true,
            &mut self.rng,
        );
        self.player.last_shot_at = self.time;
        self.stats.shots += 1;
        if moving {
            self.stats.moving_shots += 1;
        }
        let ray = Ray {
            origin: self.eye_position(),
            dir,
        };
        let hit = self
            .bots
            .iter()
            .enumerate()
            .filter(|(_, b)| b.alive)
            .filter_map(|(i, b)| {
                ray_humanoid(ray, b.feet, b.facing_yaw_deg, &self.dims).map(|(t, z)| (t, i, z))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            // Occluded when the map is hit first.
            .filter(|&(t, _, _)| self.map.raycast(ray, t).is_none());
        let Some((_, idx, zone)) = hit else {
            events.push(ArenaEvent::Shot { hit: None, moving });
            return;
        };
        let dmg = Self::damage(&self.weapon, zone);
        self.stats.hits += 1;
        if zone == HitZone::Head {
            self.stats.headshots += 1;
        }
        self.stats.damage_dealt += dmg;
        let now = self.time;
        let bot = &mut self.bots[idx];
        events.push(ArenaEvent::Shot {
            hit: Some((bot.id, zone)),
            moving,
        });
        bot.health -= dmg;
        if bot.health <= 0.0 {
            bot.alive = false;
            bot.died_at = now;
            bot.visible_to_player = false;
            let ttk = now - bot.first_seen_at.unwrap_or(now);
            self.stats.kills += 1;
            self.stats.kill_times.push(ttk);
            events.push(ArenaEvent::BotKilled {
                id: bot.id,
                time_to_kill_secs: ttk,
            });
            if let Some(site) = &mut self.site {
                site.killed += 1;
            }
            if matches!(self.def.mode, ArenaMode::AngleClear { .. }) {
                self.angle_index += 1;
                self.start_angle();
            }
        }
    }

    fn update_bots(&mut self, dt: f32, events: &mut Vec<ArenaEvent>) {
        let (reaction, hold_secs, peek_delay, bot_strafe) = match &self.def.mode {
            ArenaMode::Deathmatch { bot_strafe, .. } => (None, 0.0, PEEK_DELAY_SECS, *bot_strafe),
            ArenaMode::AngleClear {
                bot_reaction_secs,
                hold_secs,
                peek_delay_secs,
                ..
            } => (
                Some(*bot_reaction_secs),
                *hold_secs,
                peek_delay_secs.unwrap_or(PEEK_DELAY_SECS),
                false,
            ),
        };
        let site = self.site.as_ref().map(|s| s.center);
        let shoot_back = self.def.mode.bots_shoot_back();
        for i in 0..self.bots.len() {
            let ctx = Ctx {
                map: &self.map,
                now: self.time,
                dt,
                player_feet: self.player.feet,
                player_targetable: self.time >= self.grace_until,
                movement: &self.bot_movement,
                aimer: &self.aimer,
                reaction,
                hold_secs,
                peek_delay,
                roam_points: &self.roam_points,
                bot_strafe,
                site,
            };
            let bot = &mut self.bots[i];
            let landed = bot.think(&ctx, &mut self.rng);
            let can_fire = self.time - bot.last_shot_at >= 1.0 / self.bot_weapon.fire_rate;
            // A strafing bot waits for the slow moment of its counter-strafe to fire.
            let steady = !bot_strafe
                || bot.vel.horizontal().length() <= self.bot_movement.accuracy_speed_limit();
            if !(landed && shoot_back && can_fire && steady) {
                continue;
            }
            bot.last_shot_at = self.time;
            let speed = bot.vel.horizontal().length();
            let (dir, _) = shot_direction(
                bot.aim.view,
                speed,
                &self.bot_movement,
                &self.bot_weapon,
                true,
                &mut self.rng,
            );
            let ray = Ray {
                origin: bot.eye(),
                dir,
            };
            bot.after_shot(&ctx, &mut self.rng);
            let hit = ray_humanoid(ray, self.player.feet, self.player.view.yaw_deg, &self.dims)
                .filter(|&(t, _)| self.map.raycast(ray, t).is_none());
            if let Some((_, zone)) = hit {
                let dmg = Self::damage(&self.bot_weapon, zone);
                self.player.health -= dmg;
                self.stats.damage_taken += dmg;
                events.push(ArenaEvent::PlayerHit { damage: dmg });
                if self.player.health <= 0.0 {
                    events.push(ArenaEvent::PlayerDied);
                    self.player_died();
                }
            }
        }
    }

    fn player_died(&mut self) {
        self.stats.deaths += 1;
        self.player.health = PLAYER_HEALTH;
        let spawn = match &self.def.mode {
            ArenaMode::Deathmatch { .. } if self.site.is_some() => {
                self.safest_spawn(&self.player_spawn_indices())
            }
            ArenaMode::Deathmatch { .. } => {
                self.safest_spawn(&(0..self.map.spawns.len()).collect::<Vec<_>>())
            }
            ArenaMode::AngleClear { .. } => self.player_spawn_for_angle(),
        };
        self.place_player(spawn);
        self.grace_until = self.time + RESPAWN_GRACE_SECS;
        for b in &mut self.bots {
            b.aim.disengage();
            b.sees_player = false;
            if let Brain::Angle { marker, phase } = &mut b.brain {
                b.feet = marker.hidden_position();
                b.vel = Vec3::ZERO;
                *phase = Phase::Hidden { peek_at: None };
            }
        }
    }

    fn place_player(&mut self, spawn: usize) {
        let s = &self.map.spawns[spawn];
        self.player.feet = s.position;
        self.player.view = ViewAngles::new(s.yaw_deg, 0.0);
        self.player.vel = LocalVelocity::default();
    }

    fn player_spawn_indices(&self) -> Vec<usize> {
        (0..self.map.spawns.len())
            .filter(|&i| self.map.spawns[i].kind == SpawnKind::Player)
            .collect()
    }

    /// Of the `candidates` spawn indices, the spawn seen by the fewest alive bots, then the
    /// one farthest from them. Deathmatch respawns use every spawn (player or bot) so an
    /// open map does not spawn-kill; a site hold keeps the defender on player spawns.
    fn safest_spawn(&self, candidates: &[usize]) -> usize {
        let alive: Vec<&Bot> = self.bots.iter().filter(|b| b.alive).collect();
        let score = |i: usize| {
            let p = self.map.spawns[i].position;
            let seen = alive
                .iter()
                .filter(|b| self.map.visible_head(b.eye(), p))
                .count();
            let nearest = alive
                .iter()
                .map(|b| (b.feet - p).horizontal().length())
                .fold(f32::INFINITY, f32::min);
            (seen, -nearest)
        };
        candidates
            .iter()
            .copied()
            .min_by(|&a, &b| {
                let (sa, da) = score(a);
                let (sb, db) = score(b);
                sa.cmp(&sb).then(da.total_cmp(&db)).then(a.cmp(&b))
            })
            .expect("validated map has a player spawn")
    }

    /// The player spawn that sees the current angle's peek but not its hidden spot,
    /// closest to the angle.
    fn player_spawn_for_angle(&self) -> usize {
        let Some(marker) = self.current_marker() else {
            return self.player_spawn_indices()[0];
        };
        let candidates = self.player_spawn_indices();
        let sees = |i: &usize| {
            let eye = MapDef::eye(self.map.spawns[*i].position);
            self.map.visible_head(eye, marker.position)
                && !self.map.visible_head(eye, marker.hidden_position())
        };
        let dist = |i: usize| (self.map.spawns[i].position - marker.position).length();
        candidates
            .iter()
            .copied()
            .filter(sees)
            .min_by(|&a, &b| dist(a).total_cmp(&dist(b)).then(a.cmp(&b)))
            .unwrap_or(candidates[0])
    }

    fn current_marker(&self) -> Option<crate::map::AngleMarker> {
        let ArenaMode::AngleClear { angles, .. } = &self.def.mode else {
            return None;
        };
        let id = angles.get(self.angle_index)?;
        self.map.angles.iter().find(|a| &a.id == id).cloned()
    }

    fn new_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Angle clear: put the player at the spawn that sees the next angle and the bot at its
    /// hidden spot, or finish when every angle is cleared.
    fn start_angle(&mut self) {
        let Some(marker) = self.current_marker() else {
            self.finished = true;
            return;
        };
        let spawn = self.player_spawn_for_angle();
        self.place_player(spawn);
        // The player keeps the health they had; only a death restores it.
        let id = self.new_id();
        let hidden = marker.hidden_position();
        let facing = yaw_of(self.player.feet - hidden);
        let bot = Bot::new(
            id,
            hidden,
            facing,
            Brain::Angle {
                marker,
                phase: Phase::Hidden { peek_at: None },
            },
        );
        self.bots.clear();
        self.bots.push(bot);
        self.pending.push(ArenaEvent::BotSpawned { id });
    }

    /// Deathmatch: a bot spawn the player cannot see and no living bot stands on, picked at
    /// random; any bot spawn if none qualifies.
    fn pick_bot_spawn(&mut self) -> (Vec3, f32) {
        let eye = self.eye_position();
        let all: Vec<(Vec3, f32)> = self
            .map
            .spawns_of(SpawnKind::Bot)
            .map(|s| (s.position, s.yaw_deg))
            .collect();
        let free: Vec<(Vec3, f32)> = all
            .iter()
            .copied()
            .filter(|(p, _)| {
                !self.map.visible_head(eye, *p)
                    && self
                        .bots
                        .iter()
                        .all(|b| !b.alive || (b.feet - *p).horizontal().length() > 1.0)
            })
            .collect();
        let pool = if free.is_empty() { &all } else { &free };
        pool[(self.rng.next_u64() % pool.len() as u64) as usize]
    }

    /// Deathmatch: spawn a fresh roaming bot into `slot`, or append one. With a site
    /// objective the bot is an attacker that pushes the site.
    fn spawn_roaming_bot(&mut self, slot: Option<usize>) {
        let (feet, yaw) = self.pick_bot_spawn();
        let id = self.new_id();
        let brain = match &mut self.site {
            Some(site) => {
                site.spawned += 1;
                Brain::Push {
                    route: Vec::new(),
                    next: 0,
                    push: Push::default(),
                }
            }
            None => Brain::Roam {
                route: Vec::new(),
                next: 0,
                pause_until: 0.0,
            },
        };
        let bot = Bot::new(id, feet, yaw, brain);
        match slot {
            Some(i) => self.bots[i] = bot,
            None => self.bots.push(bot),
        }
        self.pending.push(ArenaEvent::BotSpawned { id });
    }

    fn respawn_bots(&mut self) {
        let ArenaMode::Deathmatch { respawn_secs, .. } = self.def.mode else {
            return;
        };
        for i in 0..self.bots.len() {
            let b = &self.bots[i];
            let more = self
                .site
                .as_ref()
                .is_none_or(|s| s.spawned < s.def.attackers);
            if more && !b.alive && self.time - b.died_at >= respawn_secs {
                self.spawn_roaming_bot(Some(i));
            }
        }
    }
}

impl Activity for ArenaSession {
    fn update(&mut self, input: FrameInput) -> Vec<ActivityEvent> {
        ArenaSession::update(self, input)
            .into_iter()
            .filter_map(|e| match e {
                ArenaEvent::Shot { hit, .. } => Some(ActivityEvent::Shot { hit: hit.is_some() }),
                ArenaEvent::BotKilled {
                    time_to_kill_secs, ..
                } => Some(ActivityEvent::Kill { time_to_kill_secs }),
                ArenaEvent::PlayerHit { .. } => Some(ActivityEvent::Damaged),
                ArenaEvent::Finished => Some(ActivityEvent::Finished),
                ArenaEvent::PlayerDied | ArenaEvent::BotSpawned { .. } => None,
            })
            .collect()
    }

    fn is_finished(&self) -> bool {
        ArenaSession::is_finished(self)
    }

    fn result(&self) -> RunResult {
        ArenaSession::result(self)
    }

    fn time(&self) -> f32 {
        ArenaSession::time(self)
    }

    fn view(&self) -> Option<ViewAngles> {
        Some(ArenaSession::view(self))
    }
}
