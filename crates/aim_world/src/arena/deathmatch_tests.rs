//! Deathmatch additions through the public API: the death recap, squad roles and the
//! adaptive ladder. Every run is seeded and deterministic.

use super::tests::{DT, duel_room, run};
use super::*;
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
        let right = if (frame / 40).is_multiple_of(2) { 1.0 } else { -1.0 };
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
