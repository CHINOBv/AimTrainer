//! Duel-pressure options through the public API: the shoot-back deadline, reactive bots and
//! conditional behavior rules. Every run is seeded and deterministic.

use super::tests::{builtin, duel_room, idle, perfect, run};
use super::*;

/// The open duel room arena with `extra` TOML appended to its `[mode]` table.
fn duel(shoot_back: bool, extra: &str) -> ArenaDef {
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
{extra}
"#
    ))
    .unwrap()
}

const DEADLINE: &str = "\n[mode.deadline]\nsecs = 1.0\ndamage = 30.0\npenalty_points = 20.0\n";

fn player_hits(events: &[ArenaEvent]) -> Vec<f32> {
    events
        .iter()
        .filter_map(|e| match e {
            ArenaEvent::PlayerHit { damage } => Some(*damage),
            _ => None,
        })
        .collect()
}

#[test]
fn a_bot_not_killed_before_the_deadline_lands_one_shot_per_life() {
    let mut s = ArenaSession::new(duel(false, DEADLINE), duel_room(false), 3).unwrap();
    // The bot is in sight from the first frame; the spawn grace ends at 1.5 s, so the
    // deadline shot lands as soon as the player is targetable.
    let events = run(&mut s, 3.0, idle);
    assert_eq!(player_hits(&events), [30.0], "{events:?}");
    let st = s.stats();
    assert_eq!((st.deadline_hits, st.deadline_kills), (1, 0), "{st:?}");
    assert_eq!(st.damage_taken, 30.0);
    assert_eq!(s.player_health(), PLAYER_HEALTH - 30.0);
    assert_eq!(s.result().score, -20.0, "the penalty is subtracted");
}

#[test]
fn kills_before_the_deadline_are_counted_and_never_punished() {
    let mut s = ArenaSession::new(duel(false, DEADLINE), duel_room(false), 3).unwrap();
    let events = run(&mut s, 10.0, perfect);
    let st = s.stats();
    assert!(st.kills >= 3, "{st:?}");
    assert_eq!(st.deadline_kills, st.kills, "{st:?}");
    assert_eq!(st.deadline_hits, 0);
    assert!(player_hits(&events).is_empty());
}

#[test]
fn a_deadline_shot_needs_line_of_sight() {
    let mut s = ArenaSession::new(duel(false, DEADLINE), duel_room(true), 3).unwrap();
    let events = run(&mut s, 10.0, idle);
    assert!(player_hits(&events).is_empty());
    assert_eq!(s.stats().deadline_hits, 0);
}

#[test]
fn practice_has_no_deadline_shots() {
    let s = ArenaSession::new(duel(false, DEADLINE), duel_room(false), 3).unwrap();
    let mut s = s.practice();
    let events = run(&mut s, 5.0, idle);
    assert!(player_hits(&events).is_empty());
    assert_eq!(s.stats().deadline_hits, 0);
}

#[test]
fn deadline_values_are_validated() {
    let def = duel(false, "");
    assert_eq!(def.mode.deadline(), None, "off by default");
    let bad = ArenaDef::from_toml(&format!(
        "{}\n[mode.deadline]\nsecs = 0.0\ndamage = -1.0\n",
        toml::to_string(&duel(false, "")).unwrap()
    ))
    .unwrap();
    let fields: Vec<&str> = bad.validate().iter().map(|e| e.field).collect();
    assert_eq!(
        fields,
        ["mode.deadline.secs", "mode.deadline.damage"],
        "{fields:?}"
    );
    let empty = ArenaDef::from_toml(&format!(
        "{}\n[mode.deadline]\nsecs = 1.0\n",
        toml::to_string(&duel(false, "")).unwrap()
    ))
    .unwrap();
    let errs = empty.validate();
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(errs[0].message.contains("damage or penalty_points"));
    let typo = ArenaDef::from_toml(&format!(
        "{}\n[mode.deadline]\nsecs = 1.0\ndamage = 10.0\nlives = 2\n",
        toml::to_string(&duel(false, "")).unwrap()
    ));
    assert!(typo.is_err(), "unknown fields are rejected");
}

#[test]
fn deadline_runs_are_deterministic() {
    let go = || {
        let mut s = ArenaSession::new(duel(true, DEADLINE), duel_room(false), 9).unwrap();
        let events = run(&mut s, 10.0, idle);
        (events, s.result(), s.stats().clone())
    };
    assert_eq!(go(), go());
}

#[test]
fn the_built_in_arenas_do_not_use_the_new_options() {
    for def in builtin::all() {
        if def.id == "corridor-reactive" {
            continue;
        }
        let (def, _) = builtin(&def.id);
        assert_eq!(def.mode.deadline(), None, "{}", def.id);
    }
}
