//! Conditional bot behavior (`[[mode.rules]]`) through the public API. Every run is seeded
//! and deterministic.

use super::tests::{DT, builtin, cover_room, duel_room, idle, run};
use super::*;
use crate::map::MapDef;
use aim_core::math::{Vec3, ViewAngles};
use aim_core::movement::MoveInput;
use aim_core::scenario::FrameInput;

/// One veteran bot that never shoots, with `extra` TOML appended to `[mode]`.
pub(super) fn ruled(extra: &str) -> ArenaDef {
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
bots_shoot_back = false
{extra}
"#
    ))
    .unwrap()
}

/// Shoots the bot's chest on each of the `at` frames (1-based), idles otherwise. Records
/// the living bot's feet and whether it is in the player's sight, per frame.
pub(super) fn shoot_at(s: &mut ArenaSession, at: &[usize], secs: f32) -> (Vec<Vec3>, Vec<bool>) {
    let (mut feet, mut seen) = (Vec::new(), Vec::new());
    let mut frame = 0;
    run(s, secs, |s| {
        frame += 1;
        let bot = s.bots().next().unwrap();
        feet.push(bot.feet);
        seen.push(bot.alive && bot.visible_to_player);
        if !at.contains(&frame) {
            return idle(s);
        }
        let chest = bot.feet + Vec3::new(0.0, 1.2, 0.0);
        FrameInput {
            dt: DT,
            view: ViewAngles::from_direction(chest - s.eye_position()),
            fire_pressed: true,
            ..Default::default()
        }
    });
    (feet, seen)
}

pub(super) fn max_abs_x(feet: &[Vec3]) -> f32 {
    feet.iter().fold(0.0f32, |m, p| m.max(p.x.abs()))
}

#[test]
fn a_hold_rule_stops_a_strafing_bot() {
    let strafing = "bot_strafe = true";
    let mut plain = ArenaSession::new(ruled(strafing), duel_room(false), 3).unwrap();
    let (feet, _) = shoot_at(&mut plain, &[], 6.0);
    assert!(max_abs_x(&feet) > 0.5, "control strafes");

    let hold = format!("{strafing}\n[[mode.rules]]\nbehavior = \"hold\"\n");
    let mut s = ArenaSession::new(ruled(&hold), duel_room(false), 3).unwrap();
    let (feet, _) = shoot_at(&mut s, &[], 6.0);
    assert!(feet.iter().all(|p| (*p - feet[0]).length() < 1e-4), "holds");
}

#[test]
fn a_bot_strafes_while_recently_damaged_then_stops() {
    let rule = "[[mode.rules]]\nwhen = { damaged_within_secs = 1.0 }\nbehavior = \"strafe\"\n";
    let mut s = ArenaSession::new(ruled(rule), duel_room(false), 3).unwrap();
    // One body shot at 2 s (after the spawn grace, once the bot fights).
    let hit = 120;
    let (feet, _) = shoot_at(&mut s, &[hit], 6.0);
    assert_eq!((s.stats().hits, s.stats().kills), (1, 0));
    assert!(max_abs_x(&feet[..hit]) < 1e-4, "no strafe before the hit");
    let during = &feet[hit..hit + 60];
    assert!(max_abs_x(during) > 0.3, "strafes after the hit");
    // A second after the rule stops holding, the bot has braked to a stop.
    let late = &feet[hit + 60 + 60..];
    let moved = (late[late.len() - 1] - late[0]).length();
    assert!(moved < 1e-3, "stopped strafing: moved {moved} m");

    // Control: without the rule, a bot shot in the open room holds its ground.
    let mut plain = ArenaSession::new(ruled(""), duel_room(false), 3).unwrap();
    let (feet, _) = shoot_at(&mut plain, &[hit], 6.0);
    assert!(max_abs_x(&feet) < 1e-4);
}

#[test]
fn rules_apply_in_order_and_the_first_match_wins() {
    // The first rule always matches, so the strafe rule is never reached.
    let rules = "[[mode.rules]]\nbehavior = \"hold\"\n[[mode.rules]]\nwhen = { damaged_within_secs = 1.0 }\nbehavior = \"strafe\"\n";
    let mut s = ArenaSession::new(ruled(rules), duel_room(false), 3).unwrap();
    let (feet, _) = shoot_at(&mut s, &[120], 6.0);
    assert!(max_abs_x(&feet) < 1e-4);
}

#[test]
fn a_bot_under_half_health_retreats_and_stays_hidden() {
    let rule = "[[mode.rules]]\nwhen = { health_below = 0.5 }\nbehavior = \"retreat\"\n";
    // Two body shots: 150 - 2 x 40 = 70 health, under half of 150.
    let shots = [120, 135];
    let mut s = ArenaSession::new(ruled(rule), cover_room(true), 3).unwrap();
    let (_, seen) = shoot_at(&mut s, &shots, 8.0);
    assert_eq!((s.stats().hits, s.stats().kills), (2, 0));
    let hidden_at = seen[136..]
        .iter()
        .position(|v| !v)
        .expect("the bot breaks line of sight")
        + 136;
    assert!(
        seen[hidden_at..].iter().all(|v| !v),
        "the bot stays hidden instead of re-peeking"
    );

    // Control: without the rule, the bot falls back and re-peeks.
    let mut plain = ArenaSession::new(ruled(""), cover_room(true), 3).unwrap();
    let (_, seen) = shoot_at(&mut plain, &shots, 8.0);
    let hidden_at = seen[136..].iter().position(|v| !v).unwrap() + 136;
    assert!(seen[hidden_at..].iter().any(|v| *v), "re-peeks");
}

#[test]
fn a_speed_multiplier_slows_the_bot() {
    let fastest = |extra: &str| {
        let mut s = ArenaSession::new(ruled(extra), duel_room(false), 3).unwrap();
        let (feet, _) = shoot_at(&mut s, &[], 8.0);
        feet.windows(2)
            .map(|w| (w[1] - w[0]).horizontal().length() / DT)
            .fold(0.0f32, f32::max)
    };
    let run_speed = BOT_WEAPON.movement().run_speed;
    let plain = fastest("bot_strafe = true");
    let slow = fastest("bot_strafe = true\n[[mode.rules]]\nspeed_multiplier = 0.5\n");
    assert!(plain > 0.6 * run_speed, "control {plain}");
    assert!(slow > 0.1 && slow <= 0.5 * run_speed + 1e-3, "slow {slow}");
}

/// The open room with a head-high box east of the player and two waypoints on the west
/// side: a player who walks east to x = 8 is hidden from the bot spawn by the box.
pub(super) fn hide_room() -> MapDef {
    MapDef::load(
        r#"
id = "duel-room"
name = "Hide Room"
floor_x = [-10.0, 10.0]
floor_z = [-10.0, 10.0]

[[boxes]]
kind = "cover"
min = [6.0, 0.0, 3.0]
max = [7.0, 2.5, 4.0]

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

[[waypoints]]
id = "south"
position = [-6.0, 0.0, -5.0]
links = ["north"]

[[waypoints]]
id = "north"
position = [-6.0, 0.0, 5.0]
links = ["south"]
"#,
    )
    .unwrap()
}

/// Waits 2 s, walks east for 1.6 s and stands still. Returns the bot's feet and the
/// player's feet per frame.
pub(super) fn walk_east_and_hide(rules: &str) -> (Vec<Vec3>, Vec<Vec3>) {
    let mut s = ArenaSession::new(ruled(rules), hide_room(), 3).unwrap();
    let (mut bot, mut player) = (Vec::new(), Vec::new());
    let mut frame = 0;
    run(&mut s, 8.0, |s| {
        frame += 1;
        bot.push(s.bots().next().unwrap().feet);
        player.push(s.player_feet());
        let right = if (120..216).contains(&frame) {
            1.0
        } else {
            0.0
        };
        FrameInput {
            dt: DT,
            view: ViewAngles::new(0.0, 0.0),
            movement: MoveInput {
                right,
                ..Default::default()
            },
            ..Default::default()
        }
    });
    (bot, player)
}

#[test]
fn a_peeking_bot_walks_to_where_it_last_saw_the_player() {
    let when = "when = { sees_player = false, alive_over_secs = 2.0 }";
    let map = hide_room();
    let sees_at_end =
        |bot: &[Vec3], hide: Vec3| map.visible_head(MapDef::eye(bot[bot.len() - 1]), hide);
    // Control: a bot that holds while it cannot see the player stays where it lost them.
    let (bot, player) =
        walk_east_and_hide(&format!("[[mode.rules]]\n{when}\nbehavior = \"hold\"\n"));
    let hide = player[player.len() - 1];
    assert!(hide.x > 7.0, "the player hid behind the box: {hide:?}");
    let lost_at = 216;
    assert!((bot[bot.len() - 1] - bot[lost_at]).length() < 0.1);
    assert!(
        !sees_at_end(&bot, hide),
        "the box hides the player from the holder"
    );

    let (bot, _) = walk_east_and_hide(&format!("[[mode.rules]]\n{when}\nbehavior = \"peek\"\n"));
    let walked = (bot[bot.len() - 1] - bot[lost_at]).horizontal().length();
    assert!(
        walked > 3.0,
        "the peeker walks after the player: {walked} m"
    );
    assert!(sees_at_end(&bot, hide), "and finds them again");
}

#[test]
fn rule_runs_are_deterministic() {
    let rules = "bot_strafe = true\n[[mode.rules]]\nwhen = { health_below = 0.5 }\nbehavior = \"retreat\"\n[[mode.rules]]\nwhen = { damaged_within_secs = 1.0 }\nbehavior = \"strafe\"\nspeed_multiplier = 1.2\n[[mode.rules]]\nwhen = { sees_player = true }\nbehavior = \"hold\"\n";
    let go = || {
        let mut s = ArenaSession::new(ruled(rules), cover_room(true), 7).unwrap();
        let track = shoot_at(&mut s, &[120, 140, 200], 8.0);
        (track, s.result(), s.stats().clone())
    };
    assert_eq!(go(), go());
}

#[test]
fn rules_are_validated() {
    let src = r#"
[[mode.rules]]
when = { health_below = 1.5, damaged_within_secs = 0.0, distance_below_m = -1.0 }

[[mode.rules]]
behavior = "hold"
speed_multiplier = 3.0
"#;
    let bad =
        ArenaDef::from_toml(&format!("{}{src}", toml::to_string(&ruled("")).unwrap())).unwrap();
    let messages: Vec<String> = bad.validate().iter().map(|e| e.message.clone()).collect();
    assert_eq!(
        messages,
        [
            "rule 1: needs a behavior or a speed_multiplier",
            "rule 1: when.health_below must be in (0, 1]",
            "rule 1: when.damaged_within_secs must be > 0",
            "rule 1: when.distance_below_m must be >= 0",
            "rule 2: speed_multiplier must be in (0, 2]",
        ],
        "{messages:?}"
    );
    let typo = ArenaDef::from_toml(&format!(
        "{}\n[[mode.rules]]\nwhen = {{ helth_below = 0.5 }}\nbehavior = \"hold\"\n",
        toml::to_string(&ruled("")).unwrap()
    ));
    assert!(typo.is_err(), "unknown condition names are rejected");
    let unknown = ArenaDef::from_toml(&format!(
        "{}\n[[mode.rules]]\nbehavior = \"teleport\"\n",
        toml::to_string(&ruled("")).unwrap()
    ));
    assert!(unknown.is_err(), "unknown behaviors are rejected");

    let (mut hold, _) = builtin("outpost-site-hold");
    if let ArenaMode::Deathmatch { rules, .. } = &mut hold.mode {
        rules.push(BehaviorRule {
            when: RuleConditions::default(),
            behavior: Some(BotBehavior::Hold),
            speed_multiplier: None,
        });
    }
    let errs = hold.validate();
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].message.contains("site attackers"));
}

#[test]
fn existing_arenas_have_no_rules() {
    for def in builtin::all() {
        if let ArenaMode::Deathmatch {
            rules, reactive, ..
        } = &def.mode
            && def.id != "corridor-reactive"
        {
            assert!(rules.is_empty() && reactive.is_none(), "{}", def.id);
        }
    }
}

#[test]
fn the_reactive_duel_arena_uses_reactions_rules_and_a_deadline() {
    let (def, map) = builtin("corridor-reactive");
    assert_eq!(map.id, "duel-corridor");
    let ArenaMode::Deathmatch {
        reactive,
        rules,
        deadline,
        objective,
        ..
    } = &def.mode
    else {
        panic!("corridor-reactive is a deathmatch");
    };
    assert!(reactive.is_some() && deadline.is_some() && objective.is_none());
    let behaviors: Vec<_> = rules.iter().map(|r| r.behavior).collect();
    assert_eq!(
        behaviors,
        [
            Some(BotBehavior::Retreat),
            Some(BotBehavior::Strafe),
            Some(BotBehavior::Hold)
        ]
    );

    // Playable: a perfect player kills bots, mostly before the deadline; an idle player
    // takes deadline hits.
    let mut s = ArenaSession::new(def.clone(), map.clone(), 4).unwrap();
    run(&mut s, 90.0, super::tests::perfect);
    let st = s.stats();
    assert!(st.kills >= 10, "{st:?}");
    assert!(st.deadline_kills * 2 > st.kills, "{st:?}");
    let mut s = ArenaSession::new(def, map, 4).unwrap();
    run(&mut s, 90.0, idle);
    assert!(s.stats().deadline_hits > 0, "{:?}", s.stats());
}
