//! Deathmatch additions through the public API: the death recap, squad roles and the
//! adaptive ladder. Every run is seeded and deterministic.

use super::rules_tests::{hide_room, max_abs_x, ruled, shoot_at, walk_east_and_hide};
use super::tests::{DT, duel_room, run};
use super::*;
use crate::map::MapDef;
use aim_core::math::ViewAngles;
use aim_core::movement::MoveInput;
use aim_core::scenario::FrameInput;

/// One bot in the open duel room, with `extra` TOML appended to `[mode]`.
fn duel(shoot_back: bool, extra: &str) -> ArenaSession {
    let def = ArenaDef::load(&format!(
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
{extra}
"#
    ))
    .unwrap();
    ArenaSession::new(def, duel_room(false), 3).unwrap()
}

/// Run until the first death (or the end) and return its recap.
fn first_death(
    s: &mut ArenaSession,
    mut policy: impl FnMut(&ArenaSession) -> FrameInput,
) -> DeathRecap {
    while s.last_death().is_none() && !s.is_finished() {
        let input = policy(s);
        s.update(input);
    }
    s.last_death().cloned().expect("the player died")
}

/// The view straight at the bot's head.
fn at_bot(s: &ArenaSession) -> ViewAngles {
    let eye = s.eye_position();
    s.bots()
        .find(|b| b.alive)
        .map_or(s.view(), |b| ViewAngles::from_direction(b.head() - eye))
}

#[test]
fn a_player_looking_away_dies_to_crosshair_placement() {
    let mut s = duel(true, "");
    let r = first_death(&mut s, |_| FrameInput {
        dt: DT,
        view: ViewAngles::new(90.0, 0.0),
        ..Default::default()
    });
    assert_eq!(r.killer_id, 1);
    assert_eq!(r.killer_skill, "veteran");
    assert!((r.distance_m - 10.0).abs() < 0.5, "{r:?}");
    // In sight from the first frame; the bot only notices after the spawn grace.
    assert!(r.saw_first, "{r:?}");
    let err = r.crosshair_error_deg.expect("seen");
    assert!(err > 80.0, "{r:?}");
    assert_eq!((r.first_shot_delay_secs, r.shots), (None, 0));
    assert!(
        r.damage_taken >= PLAYER_HEALTH && r.damage_dealt == 0.0,
        "{r:?}"
    );
    assert_eq!(r.cause, DeathCause::Placement);
    assert_eq!(s.stats().death_recaps.len() as u32, s.stats().deaths);
    assert_eq!(
        s.stats().top_death_cause(),
        Some((DeathCause::Placement, 1))
    );
}

#[test]
fn a_placed_player_who_never_fires_dies_to_a_slow_shot() {
    let mut s = duel(true, "");
    let r = first_death(&mut s, |s| FrameInput {
        dt: DT,
        view: at_bot(s),
        ..Default::default()
    });
    assert!(
        r.crosshair_error_deg.expect("seen") < RECAP_PLACEMENT_DEG,
        "{r:?}"
    );
    assert_eq!(r.cause, DeathCause::SlowShot);
}

#[test]
fn running_shots_are_blamed_when_most_shots_ran() {
    let mut s = duel(true, "");
    let mut frame = 0u32;
    let r = first_death(&mut s, |s| {
        frame += 1;
        // ADAD at run speed, firing at the head five times a second.
        let right = if (frame / 40).is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
        FrameInput {
            dt: DT,
            view: at_bot(s),
            fire_pressed: frame.is_multiple_of(12),
            movement: MoveInput {
                right,
                ..Default::default()
            },
            ..Default::default()
        }
    });
    assert!(r.shots > 0 && r.moving_shots * 2 > r.shots, "{r:?}");
    assert!(
        r.first_shot_delay_secs
            .is_some_and(|d| d < RECAP_SLOW_SHOT_SECS),
        "{r:?}"
    );
    assert_eq!(r.cause, DeathCause::MovingShots);
}

#[test]
fn a_deadline_sure_shot_is_its_own_cause() {
    let mut s = duel(false, "[mode.deadline]\nsecs = 0.5\ndamage = 200.0");
    let r = first_death(&mut s, |s| FrameInput {
        dt: DT,
        view: at_bot(s),
        ..Default::default()
    });
    assert_eq!(r.cause, DeathCause::Deadline);
    assert_eq!(r.damage_taken, 200.0);
}

#[test]
fn each_life_starts_a_new_duel() {
    let mut s = duel(true, "");
    run(&mut s, 10.0, |_| FrameInput {
        dt: DT,
        view: ViewAngles::new(90.0, 0.0),
        ..Default::default()
    });
    let recaps = &s.stats().death_recaps;
    assert!(recaps.len() >= 2, "{recaps:?}");
    // Each recap counts only its own life, so together they never exceed the run's total.
    let per_life: f32 = recaps.iter().map(|r| r.damage_taken).sum();
    assert!(per_life <= s.stats().damage_taken + 1e-3, "{recaps:?}");
    assert!(recaps.iter().all(|r| r.damage_taken >= PLAYER_HEALTH));
}

// --- Squad roles -------------------------------------------------------------------------

fn squad(role: &str) -> String {
    format!("[[mode.squad]]\nrole = \"{role}\"\n")
}

#[test]
fn a_pusher_hunts_the_player_where_it_last_saw_them() {
    let (bot, player) = walk_east_and_hide(&squad("pusher"));
    let hide = player[player.len() - 1];
    let lost_at = 216;
    let walked = (bot[bot.len() - 1] - bot[lost_at]).horizontal().length();
    assert!(
        walked > 3.0,
        "the pusher walks after the player: {walked} m"
    );
    let end = MapDef::eye(bot[bot.len() - 1]);
    assert!(hide_room().visible_head(end, hide), "and finds them again");
}

#[test]
fn an_angle_holder_never_leaves_its_spawn() {
    let (bot, _) = walk_east_and_hide(&squad("angle_holder"));
    assert!(bot.iter().all(|p| (*p - bot[0]).length() < 1e-4));
}

#[test]
fn a_jiggle_peeker_strafes_in_a_fight_without_bot_strafe() {
    let mut roamer = ArenaSession::new(ruled(&squad("roamer")), duel_room(false), 3).unwrap();
    let (feet, _) = shoot_at(&mut roamer, &[], 6.0);
    assert!(max_abs_x(&feet) < 0.1, "a roamer stops to fight");
    let mut jiggle =
        ArenaSession::new(ruled(&squad("jiggle_peeker")), duel_room(false), 3).unwrap();
    let (feet, _) = shoot_at(&mut jiggle, &[], 6.0);
    assert!(max_abs_x(&feet) > 0.5, "a jiggle peeker strafes");
}

#[test]
fn a_squad_entry_gives_its_bots_their_own_skill() {
    let extra = "[[mode.squad]]\nrole = \"roamer\"\nskill = \"elite\"\n";
    let mut s = duel(true, extra);
    let r = first_death(&mut s, |_| FrameInput {
        dt: DT,
        view: ViewAngles::new(90.0, 0.0),
        ..Default::default()
    });
    assert_eq!(r.killer_skill, "elite");
    assert_eq!(r.killer_role, Some(BotRole::Roamer));
    // Without a squad the arena's bot_skill aims, and the recap has no role.
    let mut plain = duel(true, "");
    let r = first_death(&mut plain, |_| FrameInput {
        dt: DT,
        view: ViewAngles::new(90.0, 0.0),
        ..Default::default()
    });
    assert_eq!((r.killer_skill, r.killer_role), ("veteran", None));
}

#[test]
fn squad_bots_take_their_slots_in_order_and_respawn_in_them() {
    let (def, map) = super::tests::builtin("yard-deathmatch");
    let mut src = toml::to_string(&def).unwrap();
    src.push_str(
        "\n[[mode.squad]]\nrole = \"angle_holder\"\ncount = 3\n\n[[mode.squad]]\nrole = \"roamer\"\n",
    );
    let def = ArenaDef::load(&src).unwrap();
    let mut s = ArenaSession::new(def, map, 5).unwrap();
    let start: Vec<_> = s.bots().map(|b| b.feet).collect();
    run(&mut s, 10.0, super::tests::idle);
    let end: Vec<_> = s.bots().map(|b| b.feet).collect();
    for i in 0..3 {
        assert!((end[i] - start[i]).length() < 1e-4, "holder {i} moved");
    }
    assert!((end[3] - start[3]).length() > 1.0, "the roamer roams");
}

#[test]
fn squads_are_validated() {
    let bad = |extra: &str| {
        let src = format!(
            "id = \"d\"\nname = \"D\"\nmap = \"duel-room\"\nduration_secs = 10.0\nweapon = \"vandal\"\n\n[mode]\nkind = \"deathmatch\"\nbots = 2\nbot_skill = \"contender\"\nrespawn_secs = 1.0\nbots_shoot_back = true\n{extra}"
        );
        let errs = ArenaDef::from_toml(&src).unwrap().validate();
        errs.iter().map(|e| e.to_string()).collect::<Vec<_>>()
    };
    let errs = bad("[[mode.squad]]\nrole = \"pusher\"\ncount = 0\nskill = \"godlike\"\n");
    assert!(
        errs.iter()
            .any(|e| e.contains("entry 1: count must be in 1..=16")),
        "{errs:?}"
    );
    assert!(
        errs.iter().any(|e| e.contains("entry 1: skill 'godlike'")),
        "{errs:?}"
    );
    assert!(
        errs.iter().any(|e| e.contains("add up to 0, but bots = 2")),
        "{errs:?}"
    );
    let errs = bad(
        "[[mode.squad]]\nrole = \"pusher\"\n[mode.objective]\nsite = \"s\"\nsite_radius = 2.0\ncapture_secs = 3.0\nattackers = 4\n",
    );
    assert!(
        errs.iter()
            .any(|e| e.contains("squad roles do not apply to site attackers")),
        "{errs:?}"
    );
    let ok = bad(
        "[[mode.squad]]\nrole = \"pusher\"\n[[mode.squad]]\nrole = \"angle_holder\"\nskill = \"elite\"\n",
    );
    assert!(ok.is_empty(), "{ok:?}");
    let typo = "id = \"d\"\nname = \"D\"\nmap = \"duel-room\"\nduration_secs = 10.0\nweapon = \"vandal\"\n[mode]\nkind = \"deathmatch\"\nbots = 1\nbot_skill = \"contender\"\nrespawn_secs = 1.0\nbots_shoot_back = true\n[[mode.squad]]\nrole = \"sniper\"\n";
    assert!(matches!(ArenaDef::load(typo), Err(ArenaError::Parse(_))));
}

#[test]
fn squad_runs_are_deterministic() {
    let go = || {
        let (def, map) = super::tests::builtin("yard-deathmatch");
        let mut src = toml::to_string(&def).unwrap();
        src.push_str("\n[[mode.squad]]\nrole = \"pusher\"\ncount = 2\n\n[[mode.squad]]\nrole = \"jiggle_peeker\"\nskill = \"veteran\"\ncount = 2\n");
        let mut s = ArenaSession::new(ArenaDef::load(&src).unwrap(), map, 9).unwrap();
        run(&mut s, 30.0, super::tests::perfect);
        (s.result(), s.stats().clone())
    };
    assert_eq!(go(), go());
}

#[test]
fn the_squad_arena_mixes_every_role() {
    let (def, _) = super::tests::builtin("yard-squad");
    let ArenaMode::Deathmatch { bots, squad, .. } = def.mode else {
        panic!("yard-squad is a deathmatch");
    };
    assert_eq!(bots, 5);
    let roles: Vec<BotRole> = squad.iter().map(|e| e.role).collect();
    for role in [
        BotRole::Roamer,
        BotRole::Pusher,
        BotRole::AngleHolder,
        BotRole::JigglePeeker,
    ] {
        assert!(roles.contains(&role), "{role:?}");
    }
}
