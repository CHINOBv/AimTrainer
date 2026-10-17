//! Arena behavior through the public API. Every run is seeded and deterministic.

use super::*;
use crate::map::MapDef;
use aim_core::games::valorant::HumanoidDims;
use aim_core::math::{Vec3, ViewAngles};
use aim_core::movement::MoveInput;
use aim_core::scenario::FrameInput;
use std::collections::HashSet;

const DT: f32 = 1.0 / 60.0;

fn builtin(id: &str) -> (ArenaDef, MapDef) {
    builtin::load(id)
        .expect("known arena")
        .expect("valid arena")
}

/// Run a session to the end (or `max_secs`), feeding it `policy`'s input every frame.
fn run(
    s: &mut ArenaSession,
    max_secs: f32,
    mut policy: impl FnMut(&ArenaSession) -> FrameInput,
) -> Vec<ArenaEvent> {
    let mut events = Vec::new();
    let mut t = 0.0;
    while !s.is_finished() && t < max_secs {
        let input = policy(s);
        events.extend(s.update(input));
        t += DT;
    }
    events
}

/// Looks at the nearest visible bot's head and clicks every frame.
fn perfect(s: &ArenaSession) -> FrameInput {
    let eye = s.eye_position();
    let target = s
        .bots()
        .filter(|b| b.alive && b.visible_to_player)
        .min_by(|a, b| {
            (a.head() - eye)
                .length()
                .total_cmp(&(b.head() - eye).length())
        });
    match target {
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

fn idle(s: &ArenaSession) -> FrameInput {
    FrameInput {
        dt: DT,
        view: s.view(),
        ..Default::default()
    }
}

#[test]
fn every_builtin_arena_parses_validates_and_loads_its_map() {
    let all = builtin::all();
    assert_eq!(all.len(), builtin::SOURCES.len());
    let mut ids = HashSet::new();
    for (def, (file, _)) in all.iter().zip(builtin::SOURCES) {
        assert_eq!(format!("{}.toml", def.id), *file, "file name must match id");
        assert!(ids.insert(def.id.clone()), "duplicate id {}", def.id);
        let (def, map) = builtin(&def.id);
        assert!(def.validate_with_map(&map).is_empty());
        assert!(ArenaSession::new(def, map, 1).is_ok());
    }
}

#[test]
fn builtin_arenas_match_their_spec() {
    let (yard, _) = builtin("yard-deathmatch");
    assert!(matches!(
        yard.mode,
        ArenaMode::Deathmatch { bots: 4, ref bot_skill, bots_shoot_back: true, .. }
            if bot_skill == "contender"
    ));
    let (duels, _) = builtin("corridor-duels");
    assert_eq!(duels.map, "duel-corridor");
    assert!(matches!(
        duels.mode,
        ArenaMode::Deathmatch { bots: 2, ref bot_skill, .. } if bot_skill == "veteran"
    ));
    let (lab, map) = builtin("angles-lab-clear");
    let ArenaMode::AngleClear {
        angles,
        bot_reaction_secs,
        bots_shoot_back,
        ..
    } = lab.mode
    else {
        panic!("angles-lab-clear is an angle clear");
    };
    let all: Vec<String> = map.angles.iter().map(|a| a.id.clone()).collect();
    assert_eq!(angles, all);
    assert_eq!((bot_reaction_secs.min, bot_reaction_secs.max), (0.35, 0.6));
    assert!(bots_shoot_back);
}

/// A 20 x 20 m room, the player at z = 5 and one bot spawn at z = -5, optionally split by
/// a full wall at z = 0.
fn duel_room(wall: bool) -> MapDef {
    let wall = if wall {
        "[[boxes]]\nkind = \"wall\"\nmin = [-10.0, 0.0, -0.2]\nmax = [10.0, 3.0, 0.2]\n"
    } else {
        ""
    };
    MapDef::load(&format!(
        r#"
id = "duel-room"
name = "Duel Room"
floor_x = [-10.0, 10.0]
floor_z = [-10.0, 10.0]
{wall}
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
"#
    ))
    .unwrap()
}

fn duel_def(shoot_back: bool) -> ArenaDef {
    ArenaDef::load(&format!(
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
bots_shoot_back = {shoot_back}
"#
    ))
    .unwrap()
}

/// Aims at the bot's head through whatever is in between and clicks every frame.
fn wallhack(s: &ArenaSession) -> FrameInput {
    let eye = s.eye_position();
    let bot = s.bots().find(|b| b.alive);
    FrameInput {
        dt: DT,
        view: bot.map_or(s.view(), |b| ViewAngles::from_direction(b.head() - eye)),
        fire_pressed: true,
        ..Default::default()
    }
}

#[test]
fn a_bot_behind_a_wall_cannot_be_hit_and_cannot_hit_the_player() {
    let mut s = ArenaSession::new(duel_def(true), duel_room(true), 3).unwrap();
    let events = run(&mut s, 10.0, wallhack);
    assert!(s.stats().shots > 50);
    assert_eq!(s.stats().hits, 0);
    assert_eq!(s.stats().damage_taken, 0.0);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ArenaEvent::PlayerHit { .. }))
    );
    assert!(s.bots().all(|b| !b.visible_to_player));

    // Control: the same duel without the wall lands hits.
    let mut open = ArenaSession::new(duel_def(true), duel_room(false), 3).unwrap();
    run(&mut open, 10.0, wallhack);
    assert!(open.stats().hits > 0 && open.stats().kills > 0);
}

#[test]
fn bots_that_shoot_back_damage_an_idle_player_in_the_open() {
    let mut s = ArenaSession::new(duel_def(true), duel_room(false), 3).unwrap();
    let events = run(&mut s, 10.0, idle);
    assert!(s.stats().damage_taken > 0.0);
    assert!(s.stats().deaths > 0);
    assert!(events.contains(&ArenaEvent::PlayerDied));
}

#[test]
fn perfect_aim_kills_bots_and_dies_less_than_a_player_who_never_shoots() {
    let (def, map) = builtin("yard-deathmatch");
    let mut sharp = ArenaSession::new(def.clone(), map.clone(), 11).unwrap();
    let events = run(&mut sharp, 120.0, perfect);
    let mut passive = ArenaSession::new(def, map, 11).unwrap();
    run(&mut passive, 120.0, idle);

    let (a, b) = (sharp.result(), passive.result());
    assert!(a.kills > 0, "{a:?}");
    assert!(passive.stats().deaths > 0, "{b:?}");
    assert!(
        sharp.stats().deaths < passive.stats().deaths,
        "{a:?} vs {b:?}"
    );
    assert_eq!(a.expired, sharp.stats().deaths);
    assert!(a.accuracy > 0.0 && a.accuracy <= 1.0);
    assert!(a.avg_time_to_kill.is_some());
    assert!(a.score > b.score);
    let kills = events
        .iter()
        .filter(|e| matches!(e, ArenaEvent::BotKilled { .. }))
        .count();
    assert_eq!(kills as u32, a.kills);
    // Respawns reuse the dead bots' slots: the deathmatch keeps its bot count.
    assert_eq!(sharp.bots().count(), 4);
}

#[test]
fn bots_that_do_not_shoot_back_never_damage_the_player() {
    let (mut def, map) = builtin("yard-deathmatch");
    if let ArenaMode::Deathmatch {
        bots_shoot_back, ..
    } = &mut def.mode
    {
        *bots_shoot_back = false;
    }
    let mut s = ArenaSession::new(def, map, 5).unwrap();
    let events = run(&mut s, 120.0, idle);
    assert!(s.is_finished());
    assert_eq!(s.stats().damage_taken, 0.0);
    assert_eq!(s.stats().deaths, 0);
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, ArenaEvent::PlayerHit { .. }))
    );
    assert!(
        !s.stats().crosshair_errors_deg.is_empty(),
        "bots came into view"
    );
}

#[test]
fn angle_clear_bots_appear_at_their_markers_in_order() {
    let (def, map) = builtin("angles-lab-clear");
    let mut s = ArenaSession::new(def, map.clone(), 2).unwrap();
    let mut spawned_at = Vec::new();
    let mut t = 0.0;
    while !s.is_finished() && t < 90.0 {
        let input = perfect(&s);
        for e in s.update(input) {
            if let ArenaEvent::BotSpawned { id } = e {
                let bot = s
                    .bots()
                    .find(|b| b.id == id)
                    .expect("spawned bot is listed");
                spawned_at.push(bot.feet);
            }
        }
        t += DT;
    }
    let hidden: Vec<Vec3> = map.angles.iter().map(|a| a.hidden_position()).collect();
    assert_eq!(spawned_at.len(), hidden.len(), "{:?}", s.stats());
    for (got, want) in spawned_at.iter().zip(&hidden) {
        assert!((*got - *want).length() < 1e-4, "{got:?} vs {want:?}");
    }
    assert_eq!(s.stats().kills, 5);
    assert!(
        s.is_finished() && s.time() < 90.0,
        "every angle cleared early"
    );
}

fn first_angle_head(map: &MapDef) -> Vec3 {
    map.angles[0].position + Vec3::new(0.0, HumanoidDims::default().head_center_height(), 0.0)
}

fn placement_error(yaw_offset: f32) -> f32 {
    let (mut def, map) = builtin("angles-lab-clear");
    if let ArenaMode::AngleClear {
        bots_shoot_back, ..
    } = &mut def.mode
    {
        *bots_shoot_back = false;
    }
    let head = first_angle_head(&map);
    let mut s = ArenaSession::new(def, map, 9).unwrap();
    run(&mut s, 10.0, |s| {
        let v = ViewAngles::from_direction(head - s.eye_position());
        FrameInput {
            dt: DT,
            view: v.rotated(yaw_offset, 0.0),
            ..Default::default()
        }
    });
    assert_eq!(s.stats().crosshair_errors_deg.len(), 1);
    s.stats().median_crosshair_error_deg().unwrap()
}

#[test]
fn crosshair_placement_error_is_small_when_pre_aimed_and_large_when_looking_away() {
    let pre_aimed = placement_error(0.0);
    let away = placement_error(90.0);
    assert!(pre_aimed < 5.0, "pre-aimed error {pre_aimed}");
    assert!(away > 45.0, "looking-away error {away}");
}

#[test]
fn shots_while_running_are_flagged_as_moving() {
    let mut s = ArenaSession::new(duel_def(false), duel_room(false), 4).unwrap();
    let mut saw_inaccurate = false;
    run(&mut s, 2.0, |s| {
        saw_inaccurate |= !s.is_accurate();
        FrameInput {
            movement: MoveInput {
                right: 1.0,
                ..Default::default()
            },
            ..wallhack(s)
        }
    });
    assert!(saw_inaccurate);
    assert!(s.player_speed() > 5.0);
    assert!(s.stats().moving_shots > 0);
}

#[test]
fn same_seed_and_inputs_give_the_same_run() {
    let go = || {
        let (def, map) = builtin("corridor-duels");
        let mut s = ArenaSession::new(def, map, 42).unwrap();
        let events = run(&mut s, 90.0, perfect);
        (events, s.result(), s.stats().clone())
    };
    assert_eq!(go(), go());
}

#[test]
fn an_arena_for_another_map_is_rejected() {
    let (def, _) = builtin("yard-deathmatch");
    let other = crate::builtin::get("angles-lab").unwrap();
    assert!(matches!(
        ArenaSession::new(def, other, 1),
        Err(ArenaError::Invalid(_))
    ));
}

#[test]
fn an_arena_session_is_an_activity_with_the_same_result() {
    use aim_core::activity::{Activity, ActivityEvent};
    let (def, map) = builtin("yard-deathmatch");
    let mut direct = ArenaSession::new(def.clone(), map.clone(), 5).unwrap();
    let direct_events = run(&mut direct, 300.0, perfect);

    let mut session = ArenaSession::new(def, map, 5).unwrap();
    let mut events = Vec::new();
    let mut t = 0.0;
    while !Activity::is_finished(&session) && t < 300.0 {
        let input = perfect(&session);
        let activity: &mut dyn Activity = &mut session;
        events.extend(activity.update(input));
        t += DT;
    }
    assert_eq!(
        Activity::result(&session),
        direct.result(),
        "same seed and inputs, same result"
    );
    assert_eq!(Activity::view(&session), Some(session.view()));
    let activity_kills = events
        .iter()
        .filter(|e| matches!(e, ActivityEvent::Kill { .. }))
        .count();
    let arena_kills = direct_events
        .iter()
        .filter(|e| matches!(e, ArenaEvent::BotKilled { .. }))
        .count();
    assert!(activity_kills > 0);
    assert_eq!(activity_kills, arena_kills);
    assert_eq!(events.last(), Some(&ActivityEvent::Finished));
}

#[test]
fn a_shortened_arena_ends_early_under_its_own_key() {
    let (def, map) = builtin("yard-deathmatch");
    let mut s = ArenaSession::new(def, map, 3).unwrap().with_duration(10.0);
    assert_eq!(s.def().id, "yard-deathmatch~10s");
    run(&mut s, 300.0, idle);
    assert!(s.is_finished());
    assert!((s.time() - 10.0).abs() < 1e-3, "{}", s.time());
    assert_eq!(s.result().scenario_id, "yard-deathmatch~10s");
    let (def, map) = builtin("yard-deathmatch");
    let unchanged = ArenaSession::new(def.clone(), map, 3)
        .unwrap()
        .with_duration(0.0);
    assert_eq!(unchanged.def(), &def);
}

/// Results recorded from the arenas before bot strafing, jiggle peeks, pre-aim checks and
/// site objectives existed. Arenas that do not opt in to those features must keep exactly
/// these results for the same seed and inputs.
#[test]
fn existing_arenas_keep_their_results_for_a_fixed_seed_and_input() {
    struct Golden {
        id: &'static str,
        seed: u64,
        perfect: (f32, u32, f32),
        idle: (f32, u32, f32, f32),
    }
    let goldens = [
        Golden {
            id: "yard-deathmatch",
            seed: 11,
            perfect: (20000.0, 160, 0.96428573),
            idle: (-1900.0, 38, 6320.0, 318.109),
        },
        Golden {
            id: "corridor-duels",
            seed: 42,
            perfect: (8875.0, 71, 0.54615384),
            idle: (-1150.0, 23, 3680.0, 58.630672),
        },
        Golden {
            id: "angles-lab-clear",
            seed: 2,
            perfect: (625.0, 5, 0.5),
            idle: (-1850.0, 37, 6040.0, 6.2998023),
        },
    ];
    for g in goldens {
        let (def, map) = builtin(g.id);
        let mut s = ArenaSession::new(def.clone(), map.clone(), g.seed).unwrap();
        run(&mut s, 300.0, perfect);
        let r = s.result();
        assert_eq!((r.score, r.kills, r.accuracy), g.perfect, "{}: {r:?}", g.id);

        let mut s = ArenaSession::new(def, map, g.seed).unwrap();
        run(&mut s, 300.0, idle);
        let r = s.result();
        let st = s.stats();
        let err_sum: f32 = st.crosshair_errors_deg.iter().sum();
        assert_eq!(
            (r.score, r.expired, st.damage_taken, err_sum),
            g.idle,
            "{}: {r:?}",
            g.id
        );
    }
}

fn site_hold(shoot_back: bool) -> (ArenaDef, MapDef) {
    let (mut def, map) = builtin("outpost-site-hold");
    if let ArenaMode::Deathmatch {
        bots_shoot_back, ..
    } = &mut def.mode
    {
        *bots_shoot_back = shoot_back;
    }
    (def, map)
}

#[test]
fn the_site_hold_arena_matches_its_spec() {
    let (def, map) = builtin("outpost-site-hold");
    let ArenaMode::Deathmatch {
        bots,
        bot_strafe,
        objective: Some(o),
        ..
    } = &def.mode
    else {
        panic!("outpost-site-hold is a deathmatch with a site objective");
    };
    assert_eq!((*bots, *bot_strafe, o.attackers), (2, true, 6));
    assert!(map.waypoints.iter().any(|w| w.id == o.site));
    let s = ArenaSession::new(def, map, 1).unwrap();
    assert_eq!(
        s.objective(),
        Some(ObjectiveStatus {
            attackers: 6,
            attackers_left: 6,
            capture_progress: 0.0,
            lost: false,
        })
    );
    assert_eq!(s.bots().count(), 2, "two attackers alive at once");
}

#[test]
fn attackers_capture_the_site_of_a_player_who_never_shoots() {
    let (def, map) = site_hold(false);
    let mut s = ArenaSession::new(def, map, 7).unwrap();
    let events = run(&mut s, 90.0, idle);
    let o = s.objective().unwrap();
    assert!(o.lost, "{o:?}");
    assert_eq!(o.capture_progress, 1.0);
    assert!(
        s.is_finished() && s.time() < 90.0,
        "lost before time ran out"
    );
    assert_eq!(events.last(), Some(&ArenaEvent::Finished));
    assert_eq!(s.result().score, 0.0, "no kills and no site bonus");
}

#[test]
fn a_perfect_defender_holds_the_site_and_earns_the_bonus() {
    let (def, map) = site_hold(true);
    let mut s = ArenaSession::new(def, map, 7).unwrap();
    run(&mut s, 90.0, perfect);
    let o = s.objective().unwrap();
    assert!(!o.lost, "{o:?} {:?}", s.stats());
    assert!(s.is_finished());
    let st = s.stats();
    assert_eq!((st.kills, o.attackers_left), (6, 0));
    assert!(s.time() < 90.0, "killing every attacker ends the run early");
    assert!(
        s.bots().all(|b| !b.alive),
        "no attacker respawns after the sixth"
    );
    let r = s.result();
    let base = st.kills as f32 * KILL_POINTS - st.deaths as f32 * DEATH_POINTS
        + st.headshots as f32 * HEADSHOT_BONUS;
    assert_eq!(r.score, base + SITE_HELD_BONUS);
    assert_eq!(r.scenario_id, "outpost-site-hold");
}

#[test]
fn site_hold_runs_are_deterministic() {
    let go = || {
        let (def, map) = site_hold(true);
        let mut s = ArenaSession::new(def, map, 99).unwrap();
        let events = run(&mut s, 90.0, idle);
        (events, s.result(), s.stats().clone(), s.objective())
    };
    assert_eq!(go(), go());
}

fn strafe_duel_def(strafe: bool) -> ArenaDef {
    let mut def = duel_def(true);
    if let ArenaMode::Deathmatch { bot_strafe, .. } = &mut def.mode {
        *bot_strafe = strafe;
    }
    def
}

/// Runs an idle player against one bot in the open duel room and returns the bot's x
/// offsets and, for every frame the player was hit, the bot's speed in that frame.
fn strafe_duel(strafe: bool) -> (Vec<f32>, Vec<f32>) {
    let mut s = ArenaSession::new(strafe_duel_def(strafe), duel_room(false), 3).unwrap();
    let (mut xs, mut hit_speeds) = (Vec::new(), Vec::new());
    let mut t = 0.0;
    while !s.is_finished() && t < 10.0 {
        let before: Vec<BotView> = s.bots().collect();
        let events = s.update(idle(&s));
        let after: Vec<BotView> = s.bots().collect();
        if let (Some(a), Some(b)) = (before.first(), after.first())
            && a.id == b.id
            && b.alive
        {
            xs.push(b.feet.x);
            if events
                .iter()
                .any(|e| matches!(e, ArenaEvent::PlayerHit { .. }))
            {
                hit_speeds.push((b.feet - a.feet).horizontal().length() / DT);
            }
        }
        t += DT;
    }
    (xs, hit_speeds)
}

#[test]
fn strafing_bots_move_side_to_side_and_only_hit_when_slow() {
    let limit = BOT_WEAPON.movement().accuracy_speed_limit();
    let (xs, hits) = strafe_duel(true);
    let spread = xs.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    assert!(spread > 0.5, "the bot strafes: max |x| = {spread}");
    assert!(!hits.is_empty(), "a strafing bot still lands hits");
    for v in &hits {
        assert!(*v <= limit + 1e-3, "hit while moving at {v} m/s");
    }

    // Control: the same duel without strafing stays on its spawn line.
    let (xs, hits) = strafe_duel(false);
    assert!(xs.iter().all(|x| x.abs() < 1e-4));
    assert!(!hits.is_empty());
}

#[test]
fn jiggle_peeks_show_the_enemy_only_briefly_and_repeat() {
    let (mut def, map) = builtin("corridor-jiggle");
    def.duration_secs = 10.0;
    let mut s = ArenaSession::new(def, map, 4).unwrap();
    let (mut streak, mut streaks) = (0u32, Vec::new());
    run(&mut s, 10.0, |s| {
        let seen = s.bots().any(|b| b.visible_to_player);
        if seen {
            streak += 1;
        } else if streak > 0 {
            streaks.push(streak);
            streak = 0;
        }
        idle(s)
    });
    assert!(streaks.len() >= 3, "{streaks:?}");
    let longest = *streaks.iter().max().unwrap() as f32 * DT;
    assert!(longest < 0.7, "a jiggle peek shows for {longest} s");

    let (def, map) = builtin("corridor-jiggle");
    let mut sharp = ArenaSession::new(def, map, 4).unwrap();
    run(&mut sharp, 60.0, perfect);
    assert_eq!(sharp.stats().kills, 8, "{:?}", sharp.stats());
}

/// Plays the pre-aim drill's first angle looking `offset` (yaw, pitch) degrees away from
/// where the head appears.
fn prefire(offset: (f32, f32)) -> ArenaSession {
    let (mut def, map) = builtin("angles-lab-prefire");
    if let ArenaMode::AngleClear {
        bots_shoot_back, ..
    } = &mut def.mode
    {
        *bots_shoot_back = false;
    }
    let head = first_angle_head(&map);
    let mut s = ArenaSession::new(def, map, 9).unwrap();
    run(&mut s, 10.0, |s| {
        let v = ViewAngles::from_direction(head - s.eye_position());
        FrameInput {
            dt: DT,
            view: v.rotated(offset.0, offset.1),
            ..Default::default()
        }
    });
    s
}

#[test]
fn the_pre_aim_drill_counts_and_scores_pre_aimed_sightings() {
    let pre_aimed = prefire((0.0, 0.0));
    assert_eq!(pre_aimed.stats().pre_aimed, 1);
    assert_eq!(pre_aimed.result().score, PRE_AIM_POINTS);

    let low = prefire((0.0, -10.0));
    assert_eq!(low.stats().pre_aimed, 0);
    assert_eq!(low.result().score, 0.0);
    let h = low.stats().median_head_level_error_deg().unwrap();
    assert!(
        (h - 10.0).abs() < 1.5,
        "aiming 10 degrees low: head-level error {h}"
    );

    // Arenas without a tolerance never count pre-aims, even when pre-aimed.
    assert!(placement_error(0.0) < 5.0);
    let (def, map) = builtin("angles-lab-clear");
    let mut s = ArenaSession::new(def, map, 9).unwrap();
    run(&mut s, 5.0, idle);
    assert_eq!(s.stats().pre_aimed, 0);
}

#[test]
fn running_out_the_clock_also_holds_the_site() {
    let (def, map) = site_hold(false);
    let mut s = ArenaSession::new(def, map, 7).unwrap().with_duration(3.0);
    run(&mut s, 10.0, idle);
    assert!(s.is_finished());
    assert!(!s.objective().unwrap().lost);
    assert_eq!(s.result().score, SITE_HELD_BONUS);
}
