//! Arena behavior through the public API. Every run is seeded and deterministic.

use super::*;
use crate::map::MapDef;
use aim_core::math::{Vec3, ViewAngles};
use aim_core::movement::MoveInput;
use aim_core::scenario::FrameInput;
use aim_core::valorant::HumanoidDims;
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
