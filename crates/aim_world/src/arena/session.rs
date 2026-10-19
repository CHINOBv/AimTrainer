//! A running arena: the player, the bots, shooting with occlusion and the run statistics.

use super::behavior::{Phase, Push, Retreat};
use super::bot::{Bot, Brain, Ctx, PEEK_DELAY_SECS, head_of, yaw_of};
use super::coaching::Coaching;
use super::def::{ArenaDef, ArenaError, ArenaMode, PRACTICE_SUFFIX, SiteObjective, bot_profile};
use super::recap::{DeathRecap, Killer, top_cause};
use super::squad::{self, BotRole};
use crate::corner::PeekCorner;
use crate::map::{DEFAULT_PLAYER_RADIUS, MapDef, SpawnKind};
use aim_core::activity::{Activity, ActivityEvent};
use aim_core::games::valorant::{HumanoidDims, Weapon, WeaponStats};
use aim_core::math::{Vec3, ViewAngles, angle_between_deg};
use aim_core::movement::{self, LocalVelocity, MovementProfile};
use aim_core::rng::Rng;
use aim_core::scenario::hit::{HitZone, Ray, ray_humanoid};
use aim_core::scenario::{FrameInput, RunResult, shot_direction};
use aim_core::sim::AimerProfile;

/// Player health. VALORANT players start a deathmatch life with 100 health and 50 armor;
/// the arena gives the player health only, so a bot body shot matters (a simplification).
pub const PLAYER_HEALTH: f32 = 100.0;
/// The weapon every bot carries.
pub const BOT_WEAPON: Weapon = Weapon::Vandal;
/// Score per kill, per death (subtracted) and per headshot hit.
pub const KILL_POINTS: f32 = 100.0;
pub const DEATH_POINTS: f32 = 50.0;
pub const HEADSHOT_BONUS: f32 = 25.0;
/// Seconds after a spawn (entering the arena or a respawn) during which bots ignore the
/// player. An arena rule, not a VALORANT value: it stops bots from killing the player on the
/// spawn point before they can move.
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
    /// The player took damage. `from` is the eye of the bot that dealt it, in world space;
    /// `zone` is the body zone a bot bullet hit, or `None` for a shoot-back deadline's sure
    /// shot, which fires no bullet.
    PlayerHit {
        damage: f32,
        from: Vec3,
        zone: Option<HitZone>,
    },
    PlayerDied,
    /// A bot fired a bullet at the player, hit or miss, from `origin` (its eye, world space).
    /// It comes before the `PlayerHit` the bullet causes. Deadline sure shots fire no bullet
    /// and are not reported here.
    BotFired {
        bot_id: u64,
        origin: Vec3,
    },
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
    /// Peek efficiency: seconds from first sight of a bot life to the player's first shot
    /// (at any target) during that first exposure. An exposure without a shot adds nothing.
    pub first_shot_delays_secs: Vec<f32>,
    /// Peek efficiency: seconds the first exposure of each bot life lasted, from first sight
    /// to the kill, to line of sight breaking, or to the player's death. An exposure still
    /// open when the run ends adds nothing.
    pub exposure_times_secs: Vec<f32>,
    /// Shoot-back deadline: bot lives killed before their deadline (0 without a deadline).
    pub deadline_kills: u32,
    /// Shoot-back deadline: sure shots bots landed because their deadline passed.
    pub deadline_hits: u32,
    /// Why each death happened, in order (deathmatch and angle clear alike).
    pub death_recaps: Vec<DeathRecap>,
}

/// The first exposure of a bot life while it lasts, for peek efficiency.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Engagement {
    bot_id: u64,
    /// Session time of first sight, seconds.
    seen_at: f32,
    /// The player has fired since first sight.
    shot: bool,
}

/// Record the exposure of `bot_id`'s open engagement as ended at `now`, if there is one.
fn end_exposure(engagements: &mut Vec<Engagement>, stats: &mut ArenaStats, bot_id: u64, now: f32) {
    if let Some(i) = engagements.iter().position(|e| e.bot_id == bot_id) {
        let e = engagements.swap_remove(i);
        stats.exposure_times_secs.push(now - e.seen_at);
    }
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

    /// Median seconds from first sight to the first shot (peek efficiency).
    pub fn median_first_shot_delay_secs(&self) -> Option<f32> {
        median(&self.first_shot_delays_secs)
    }

    /// Median seconds a first exposure lasted (peek efficiency).
    pub fn median_exposure_secs(&self) -> Option<f32> {
        median(&self.exposure_times_secs)
    }

    /// The most common cause of death and how many deaths it explains (ties go to the
    /// cause checked first, see [`super::recap::DeathCause`]).
    pub fn top_death_cause(&self) -> Option<(super::recap::DeathCause, u32)> {
        top_cause(&self.death_recaps)
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
    /// The map's cover spots, computed once.
    cover_spots: Vec<Vec3>,
    /// The map's peek corners, computed once, for live coaching.
    corners: Vec<PeekCorner>,
    /// Angle clear: index into the mode's angle list of the angle being cleared.
    angle_index: usize,
    /// Bots ignore the player until this time (spawn grace).
    grace_until: f32,
    /// Practice run: bots never shoot (see [`ArenaSession::practice`]).
    practice: bool,
    /// Deathmatch with a site objective.
    site: Option<Site>,
    /// Bot lives in their first exposure, for peek efficiency.
    engagements: Vec<Engagement>,
    /// The player fired this frame; reactive bots see it in the next frame.
    shot_fired: bool,
    /// Deathmatch squad: the role and fixed skill of each bot slot (empty without a squad).
    squad_slots: Vec<(BotRole, Option<AimerProfile>)>,
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
            cover_spots: map.cover_spots(),
            corners: map.peek_corners(),
            angle_index: 0,
            // Entering the arena is a spawn too.
            grace_until: RESPAWN_GRACE_SECS,
            practice: false,
            site: None,
            engagements: Vec::new(),
            shot_fired: false,
            squad_slots: match &def.mode {
                ArenaMode::Deathmatch { squad, .. } => squad::slots(squad),
                ArenaMode::AngleClear { .. } => Vec::new(),
            },
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

    /// Turn this run into practice before it starts: bots move, peek and push as usual
    /// but never shoot, so the player can learn the map and the angles. The result's id
    /// gains [`PRACTICE_SUFFIX`]. A run that has already started is left as is.
    pub fn practice(mut self) -> Self {
        if self.time == 0.0 && !self.practice {
            self.practice = true;
            self.def.mode.set_bots_shoot_back(false);
            self.def.id = format!("{}{PRACTICE_SUFFIX}", self.def.id);
        }
        self
    }

    /// Whether this is a practice run (bots never shoot).
    pub fn is_practice(&self) -> bool {
        self.practice
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
    /// Seconds left of the spawn grace during which bots ignore the player (0 when over).
    pub fn spawn_protection_secs(&self) -> f32 {
        (self.grace_until - self.time).max(0.0)
    }
    /// Whether the player is slow enough for the weapon's first-shot accuracy.
    pub fn is_accurate(&self) -> bool {
        self.player_speed() <= self.profile.accuracy_speed_limit()
    }

    /// Live coaching values for this frame: crosshair placement against head height at
    /// the exposed corner nearest to the crosshair (`MapDef::exposed_corners`), and
    /// movement accuracy. A pure read: it changes nothing and draws no random number.
    pub fn coaching(&self) -> Coaching {
        let eye = self.eye_position();
        let view = self.player.view;
        let dir = view.direction();
        let nearest = self
            .map
            .exposed_corners(&self.corners, eye)
            .map(|c| (angle_between_deg(dir, c.head - eye), c.head))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        Coaching {
            corner: nearest.map(|(_, head)| head),
            corner_offset_deg: nearest.map(|(offset, _)| offset),
            head_level_error_deg: nearest
                .map(|(_, head)| view.pitch_deg - ViewAngles::from_direction(head - eye).pitch_deg),
            speed: self.player_speed(),
            accurate: self.is_accurate(),
        }
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
        self.shot_fired = false;
        self.update_player_sight();
        if input.fire_pressed {
            self.player_shoot(&mut events);
        }
        self.apply_deadlines(&mut events);
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
                - self
                    .def
                    .mode
                    .deadline()
                    .map_or(0.0, |d| s.deadline_hits as f32 * d.penalty_points)
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
            if !b.visible_to_player {
                end_exposure(&mut self.engagements, &mut self.stats, b.id, self.time);
            } else if b.duel.player_saw_at.is_none() {
                let err = angle_between_deg(dir, head_of(b.feet) - eye);
                b.duel.player_sees(self.time, err);
            }
            if b.visible_to_player && b.first_seen_at.is_none() {
                b.first_seen_at = Some(self.time);
                self.engagements.push(Engagement {
                    bot_id: b.id,
                    seen_at: self.time,
                    shot: false,
                });
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
        self.shot_fired = true;
        self.stats.shots += 1;
        for e in self.engagements.iter_mut().filter(|e| !e.shot) {
            e.shot = true;
            self.stats
                .first_shot_delays_secs
                .push(self.time - e.seen_at);
        }
        if moving {
            self.stats.moving_shots += 1;
        }
        for b in self.bots.iter_mut().filter(|b| b.alive) {
            b.duel.player_fired(self.time, b.visible_to_player, moving);
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
        bot.duel.damage_dealt += dmg;
        // A bot that survives knows where the shot came from (roaming bots fall back).
        bot.shot_from = Some(ray.origin);
        bot.last_hit_at = now;
        if bot.health <= 0.0 {
            bot.alive = false;
            bot.died_at = now;
            bot.visible_to_player = false;
            let ttk = now - bot.first_seen_at.unwrap_or(now);
            self.stats.kills += 1;
            self.stats.kill_times.push(ttk);
            if self
                .def
                .mode
                .deadline()
                .is_some_and(|d| !bot.deadline_hit && ttk < d.secs)
            {
                self.stats.deadline_kills += 1;
            }
            end_exposure(&mut self.engagements, &mut self.stats, bot.id, now);
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
        // Cloned so the loop can damage the player; empty for arenas without rules.
        let (reactive, rules) = match &self.def.mode {
            ArenaMode::Deathmatch {
                reactive, rules, ..
            } => (*reactive, rules.clone()),
            ArenaMode::AngleClear { .. } => (None, Vec::new()),
        };
        let player_vel = self.player.vel.to_world(self.player.view.yaw_deg);
        for i in 0..self.bots.len() {
            let aimer = self.bots[i].aimer.unwrap_or(self.aimer);
            let ctx = Ctx {
                map: &self.map,
                now: self.time,
                dt,
                player_feet: self.player.feet,
                player_targetable: self.time >= self.grace_until,
                movement: &self.bot_movement,
                aimer: &aimer,
                reaction,
                hold_secs,
                peek_delay,
                roam_points: &self.roam_points,
                cover_spots: &self.cover_spots,
                bot_strafe,
                site,
                player_view: self.player.view,
                player_vel,
                player_fired: self.shot_fired,
                reactive,
                rules: &rules,
                player_speed: self.player.vel.speed(),
            };
            let bot = &mut self.bots[i];
            let landed = bot.think(&ctx, &mut self.rng);
            if bot.sees_player {
                bot.duel.bot_notices(self.time);
            }
            let can_fire = self.time - bot.last_shot_at >= 1.0 / self.bot_weapon.fire_rate;
            // A strafing bot waits for the slow moment of its counter-strafe to fire.
            let steady = !bot.careful_fire
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
            events.push(ArenaEvent::BotFired {
                bot_id: bot.id,
                origin: ray.origin,
            });
            bot.after_shot(&ctx, &mut self.rng);
            let hit = ray_humanoid(ray, self.player.feet, self.player.view.yaw_deg, &self.dims)
                .filter(|&(t, _)| self.map.raycast(ray, t).is_none());
            if let Some((_, zone)) = hit {
                let dmg = Self::damage(&self.bot_weapon, zone);
                self.damage_player(dmg, i, Some(zone), events);
            }
        }
    }

    /// Damage the player by `dmg` from the bot at index `shooter`; `zone` is the body zone
    /// a bullet hit, or `None` for a deadline's sure shot. A killing blow records the
    /// death recap before the respawn.
    fn damage_player(
        &mut self,
        dmg: f32,
        shooter: usize,
        zone: Option<HitZone>,
        events: &mut Vec<ArenaEvent>,
    ) {
        let bot = &mut self.bots[shooter];
        let from = bot.eye();
        bot.duel.damage_taken += dmg;
        self.player.health -= dmg;
        self.stats.damage_taken += dmg;
        events.push(ArenaEvent::PlayerHit {
            damage: dmg,
            from,
            zone,
        });
        if self.player.health <= 0.0 {
            let bot = &self.bots[shooter];
            let killer = Killer {
                id: bot.id,
                skill: bot.aimer.unwrap_or(self.aimer).name,
                role: bot.role,
                distance_m: (bot.feet - self.player.feet).horizontal().length(),
            };
            let recap = bot.duel.recap(killer, self.time, zone.is_none());
            self.stats.death_recaps.push(recap);
            events.push(ArenaEvent::PlayerDied);
            self.player_died();
        }
    }

    /// The recap of the player's latest death, if they have died.
    pub fn last_death(&self) -> Option<&DeathRecap> {
        self.stats.death_recaps.last()
    }

    /// Shoot-back deadline: every living bot the player first saw at least `secs` ago and
    /// has not killed lands one sure shot per life, as soon as it has line of sight to the
    /// targetable player. Never in practice; no random draw.
    fn apply_deadlines(&mut self, events: &mut Vec<ArenaEvent>) {
        let Some(deadline) = self.def.mode.deadline() else {
            return;
        };
        if self.practice || self.time < self.grace_until {
            return;
        }
        for i in 0..self.bots.len() {
            let b = &self.bots[i];
            let due = b.alive
                && !b.deadline_hit
                && b.first_seen_at
                    .is_some_and(|seen| self.time - seen >= deadline.secs)
                && self.map.visible_head(b.eye(), self.player.feet);
            if !due {
                continue;
            }
            self.bots[i].deadline_hit = true;
            self.stats.deadline_hits += 1;
            if deadline.damage > 0.0 {
                self.damage_player(deadline.damage, i, None, events);
            }
        }
    }

    fn player_died(&mut self) {
        self.stats.deaths += 1;
        for e in self.engagements.drain(..) {
            self.stats.exposure_times_secs.push(self.time - e.seen_at);
        }
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
            // A new life starts a new duel with every bot.
            b.duel = Default::default();
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
                retreat: Retreat::default(),
            },
        };
        let mut bot = Bot::new(id, feet, yaw, brain);
        bot.spawned_at = self.time;
        if let Some(&(role, aimer)) = self.squad_slots.get(slot.unwrap_or(self.bots.len())) {
            bot.role = Some(role);
            bot.aimer = aimer;
        }
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
                ArenaEvent::PlayerDied
                | ArenaEvent::BotSpawned { .. }
                | ArenaEvent::BotFired { .. } => None,
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

#[cfg(test)]
mod peek_efficiency_tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    /// An open room: the bot is in the player's sight from the first frame.
    fn open_duel() -> ArenaSession {
        let map = MapDef::load(
            r#"
id = "duel-room"
name = "Duel Room"
floor_x = [-10.0, 10.0]
floor_z = [-10.0, 10.0]

[[spawns]]
id = "p"
kind = "player"
position = [0.0, 0.0, 5.0]
yaw_deg = 0.0

[[spawns]]
id = "b"
kind = "bot"
position = [0.0, 0.0, -5.0]
yaw_deg = 180.0
"#,
        )
        .unwrap();
        let def = ArenaDef::load(
            r#"
id = "duel"
name = "Duel"
map = "duel-room"
duration_secs = 10.0
weapon = "vandal"

[mode]
kind = "deathmatch"
bots = 1
bot_skill = "veteran"
respawn_secs = 1.0
bots_shoot_back = false
"#,
        )
        .unwrap();
        ArenaSession::new(def, map, 3).unwrap()
    }

    fn idle(s: &ArenaSession) -> FrameInput {
        FrameInput {
            dt: DT,
            view: s.view(),
            ..Default::default()
        }
    }

    /// Looks at the first visible bot's head and clicks.
    fn shoot_visible(s: &ArenaSession) -> FrameInput {
        let eye = s.eye_position();
        match s.bots().find(|b| b.visible_to_player) {
            Some(b) => FrameInput {
                dt: DT,
                view: ViewAngles::from_direction(b.head() - eye),
                fire_pressed: true,
                fire_held: true,
                ..Default::default()
            },
            None => idle(s),
        }
    }

    fn run(
        s: &mut ArenaSession,
        frames: usize,
        mut policy: impl FnMut(usize, &ArenaSession) -> FrameInput,
    ) {
        for frame in 1..=frames {
            let input = policy(frame, s);
            s.update(input);
        }
    }

    #[test]
    fn the_first_shot_and_the_exposure_until_the_kill_are_timed() {
        let mut s = open_duel();
        run(&mut s, 60, |frame, s| {
            if frame <= 31 {
                idle(s)
            } else {
                shoot_visible(s)
            }
        });
        let st = s.stats();
        assert_eq!(st.kills, 1, "{st:?}");
        // Seen on frame 1, first shot on frame 32: 31 frames later.
        assert_eq!(st.first_shot_delays_secs.len(), 1, "{st:?}");
        assert!(
            (st.first_shot_delays_secs[0] - 31.0 * DT).abs() < 1e-4,
            "{st:?}"
        );
        // The exposure ended with the kill, so it lasted exactly the time to kill.
        assert_eq!(st.exposure_times_secs, st.kill_times);
        assert_eq!(
            st.median_first_shot_delay_secs(),
            Some(st.first_shot_delays_secs[0])
        );
        assert_eq!(st.median_exposure_secs(), Some(st.kill_times[0]));
    }

    #[test]
    fn an_exposure_ends_when_line_of_sight_breaks_and_needs_a_shot_to_time_one() {
        let (mut def, map) = super::super::builtin::load("corridor-jiggle")
            .expect("known arena")
            .expect("valid arena");
        def.duration_secs = 10.0;
        let mut s = ArenaSession::new(def, map, 4).unwrap();
        let (mut first_streak, mut ended) = (0u32, false);
        run(&mut s, 600, |_, s| {
            let seen = s.bots().any(|b| b.visible_to_player);
            if seen && !ended {
                first_streak += 1;
            } else if first_streak > 0 {
                ended = true;
            }
            idle(s)
        });
        let st = s.stats();
        assert!(first_streak > 0 && ended);
        assert!(st.first_shot_delays_secs.is_empty(), "{st:?}");
        // One bot life, never killed: only its first exposure is an engagement, and it
        // runs from the first frame in sight to the first frame out of sight.
        assert_eq!(st.exposure_times_secs.len(), 1, "{st:?}");
        assert!(
            (st.exposure_times_secs[0] - first_streak as f32 * DT).abs() < 1e-4,
            "{st:?} vs {first_streak} frames"
        );
        assert_eq!(st.median_first_shot_delay_secs(), None);
    }

    #[test]
    fn a_player_death_ends_the_open_exposures() {
        let mut s = open_duel();
        run(&mut s, 5, |_, s| idle(s));
        s.player_died();
        let st = s.stats();
        assert_eq!(st.exposure_times_secs.len(), 1);
        assert!(
            (st.exposure_times_secs[0] - 4.0 * DT).abs() < 1e-4,
            "{st:?}"
        );
    }
}
