//! Duel-pressure options through the public API: the shoot-back deadline, reactive bots and
//! conditional behavior rules. Every run is seeded and deterministic.

use super::tests::{DT, builtin, duel_room, idle, perfect, run};
use super::*;
use aim_core::math::{Vec3, ViewAngles};
use aim_core::movement::MoveInput;
use aim_core::scenario::FrameInput;

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

/// Bot strafing in both runs: reactive bots always strafe, so the control sets
/// `bot_strafe` to compare like with like.
fn reactive_duel(reactive: &str) -> ArenaSession {
    let extra = if reactive.is_empty() {
        "bot_strafe = true".to_string()
    } else {
        format!("bot_strafe = true\n[mode.reactive]\n{reactive}")
    };
    ArenaSession::new(duel(false, &extra), duel_room(false), 5).unwrap()
}

/// ADAD, 1.5 s per key, while looking `yaw_off_deg` to the side of the bot's head, firing
/// every 0.5 s when `fire`.
fn adad(yaw_off_deg: f32, fire: bool) -> impl FnMut(&ArenaSession) -> FrameInput {
    let mut t = 0.0f32;
    move |s| {
        t += DT;
        let eye = s.eye_position();
        let mut view = s
            .bots()
            .find(|b| b.alive)
            .map_or(s.view(), |b| ViewAngles::from_direction(b.head() - eye));
        view.yaw_deg += yaw_off_deg;
        let right = if ((t / 1.5) as u32).is_multiple_of(2) {
            1.0
        } else {
            -1.0
        };
        let shot = fire && (t / 0.5).fract() < DT / 0.5;
        FrameInput {
            dt: DT,
            view,
            fire_pressed: shot,
            fire_held: shot,
            movement: MoveInput {
                right,
                ..Default::default()
            },
        }
    }
}

/// Bot feet per frame for a 10 s run.
fn bot_track(
    s: &mut ArenaSession,
    mut policy: impl FnMut(&ArenaSession) -> FrameInput,
) -> Vec<Vec3> {
    let mut track = Vec::new();
    while !s.is_finished() {
        let input = policy(s);
        s.update(input);
        track.extend(s.bots().map(|b| b.feet));
    }
    track
}

/// Share of frames, while both move sideways, in which the bot moves the same world
/// direction along X as the player.
fn same_direction_share(reactive: &str) -> f32 {
    let mut s = reactive_duel(reactive);
    let mut policy = adad(0.0, false);
    let (mut same, mut total) = (0u32, 0u32);
    let (mut player, mut bot) = (s.player_feet(), s.bots().next().unwrap().feet);
    while !s.is_finished() {
        let input = policy(&s);
        s.update(input);
        let (p, b) = (s.player_feet(), s.bots().next().unwrap().feet);
        let (dp, db) = (p.x - player.x, b.x - bot.x);
        if dp.abs() > 1e-3 && db.abs() > 1e-3 {
            total += 1;
            same += u32::from(dp.signum() == db.signum());
        }
        (player, bot) = (p, b);
    }
    assert!(total > 100, "both strafe: {total} frames");
    same as f32 / total as f32
}

#[test]
fn reactive_bots_copy_or_oppose_the_players_strafe() {
    let plain = same_direction_share("");
    let mirror = same_direction_share("mirror_strafe_chance = 1.0");
    let oppose = same_direction_share("oppose_strafe_chance = 1.0");
    assert!(mirror > plain + 0.15, "mirror {mirror} vs plain {plain}");
    assert!(oppose < plain - 0.15, "oppose {oppose} vs plain {plain}");
}

#[test]
fn a_dodge_needs_the_crosshair_near_the_head() {
    let dodge = "dodge_within_deg = 3.0";
    // The crosshair stays 20 degrees off the head: no dodge is ever triggered, so the
    // bot moves exactly as a plain strafing bot and draws the same random numbers.
    let away = bot_track(&mut reactive_duel(dodge), adad(20.0, false));
    assert_eq!(away, bot_track(&mut reactive_duel(""), adad(20.0, false)));
    // On the head, the crosshair enters the dodge cone and the bot jukes.
    let on = bot_track(&mut reactive_duel(dodge), adad(0.0, false));
    assert_ne!(on, bot_track(&mut reactive_duel(""), adad(0.0, false)));
}

#[test]
fn bots_that_react_to_shots_only_change_when_the_player_fires() {
    let react = "react_to_shots = true";
    let quiet = bot_track(&mut reactive_duel(react), adad(20.0, false));
    assert_eq!(quiet, bot_track(&mut reactive_duel(""), adad(20.0, false)));
    // Shots that miss on purpose (20 degrees off): the bot still sees them and jukes.
    let shots = bot_track(&mut reactive_duel(react), adad(20.0, true));
    assert_ne!(shots, bot_track(&mut reactive_duel(""), adad(20.0, true)));
}

#[test]
fn reactive_runs_are_deterministic() {
    let all = "mirror_strafe_chance = 0.3\noppose_strafe_chance = 0.3\ndodge_within_deg = 4.0\ndodge_chance = 0.5\nreact_to_shots = true";
    let go = || {
        let mut s = reactive_duel(all);
        let track = bot_track(&mut s, adad(0.0, true));
        (track, s.result(), s.stats().clone())
    };
    assert_eq!(go(), go());
}

#[test]
fn reactive_options_are_validated() {
    let bad = ArenaDef::from_toml(&format!(
        "{}\n[mode.reactive]\nmirror_strafe_chance = 0.8\noppose_strafe_chance = 0.4\ndodge_within_deg = 0.0\ndodge_chance = 0.0\n",
        toml::to_string(&duel(false, "")).unwrap()
    ))
    .unwrap();
    let fields: Vec<&str> = bad.validate().iter().map(|e| e.field).collect();
    assert_eq!(
        fields,
        [
            "mode.reactive",
            "mode.reactive.dodge_within_deg",
            "mode.reactive.dodge_chance"
        ],
        "{fields:?}"
    );
    let nothing = ArenaDef::from_toml(&format!(
        "{}\n[mode.reactive]\ndodge_chance = 0.5\n",
        toml::to_string(&duel(false, "")).unwrap()
    ))
    .unwrap();
    let errs = nothing.validate();
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert_eq!(errs[0].field, "mode.reactive");
    let typo = ArenaDef::from_toml(&format!(
        "{}\n[mode.reactive]\nreact_to_shots = true\nteleport = true\n",
        toml::to_string(&duel(false, "")).unwrap()
    ));
    assert!(typo.is_err(), "unknown fields are rejected");
}
