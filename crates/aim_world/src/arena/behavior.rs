//! Bot behavior as small, pure state machines: the angle peek cycle (hidden, peek, hold,
//! hide) and the duel strafe (ADAD). They decide *what* a bot wants; [`super::bot`] turns
//! that into movement with `aim_core::movement::step` and into aim.
//!
//! Both machines only draw from the RNG on the transitions that need a random duration, so
//! a machine that is never entered (strafing off) leaves the arena's random sequence as it
//! was.

use aim_core::math::{Range, Vec3};
use aim_core::rng::Rng;

/// How long a dueling bot holds one strafe key before switching to the other, seconds. A
/// design choice for this trainer (no published VALORANT value): short enough that the bot
/// changes direction a few times per duel, long enough to reach close to run speed.
pub const STRAFE_SECS: Range = Range::new(0.2, 0.45);
/// Longest an attacker of a site objective stops to fight before it pushes on, seconds. A
/// design choice: attackers keep pressure on the site instead of dueling forever.
pub const PUSH_DUEL_SECS: f32 = 2.5;
/// How long an attacker keeps walking after a duel before it stops to fight again, seconds.
/// A design choice.
pub const PUSH_ADVANCE_SECS: f32 = 1.5;

/// The angle peek cycle of an angle-clear bot.
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

/// What the peek cycle needs to know this frame.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PeekSense {
    pub now: f32,
    /// Whether the player's eye sees the bot's head at the peek position.
    pub player_sees_angle: bool,
    /// Whether the bot stands on the peek position.
    pub at_peek: bool,
    /// Whether the bot stands on the hidden spot.
    pub at_hidden: bool,
    pub hold_secs: f32,
    /// Delay from the player seeing the angle to the bot peeking it.
    pub peek_delay: Range,
}

impl Phase {
    /// The phase after this frame.
    pub fn next(self, s: &PeekSense, rng: &mut Rng) -> Self {
        match self {
            Self::Hidden { peek_at: None } if s.player_sees_angle => Self::Hidden {
                peek_at: Some(s.now + rng.in_range(s.peek_delay)),
            },
            Self::Hidden { peek_at: Some(t) } if s.now >= t => Self::Peeking,
            Self::Peeking if s.at_peek => Self::Holding {
                until: s.now + s.hold_secs,
            },
            Self::Holding { until } if s.now >= until => Self::Hiding,
            Self::Hiding if s.at_hidden => Self::Hidden { peek_at: None },
            other => other,
        }
    }

    /// Whether the bot walks toward the peek position (otherwise toward the hidden spot).
    pub fn wants_peek_position(self) -> bool {
        matches!(self, Self::Peeking | Self::Holding { .. })
    }
}

/// The duel strafe: while a bot fights, it holds one strafe key, then the other (ADAD).
/// Pressing the opposite key makes `movement::step` counter-strafe, so the bot passes
/// through the accurate speed window at each switch.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) enum Strafe {
    /// Not fighting: no strafe key pressed.
    #[default]
    Idle,
    /// Holding the key `dir` (-1 = A, 1 = D) until `until`.
    Moving { dir: f32, until: f32 },
}

impl Strafe {
    /// Advance one frame. Returns the strafe input (-1, 0 or 1) the bot presses.
    pub fn step(&mut self, dueling: bool, now: f32, rng: &mut Rng) -> f32 {
        self.step_with(dueling, now, rng, |dir, _| dir)
    }

    /// [`Self::step`], letting `decide` change the key at each strafe-key decision (the
    /// start of a strafe and each switch): it gets the key the plain ADAD would press and
    /// returns the key to press.
    pub fn step_with(
        &mut self,
        dueling: bool,
        now: f32,
        rng: &mut Rng,
        mut decide: impl FnMut(f32, &mut Rng) -> f32,
    ) -> f32 {
        if !dueling {
            *self = Self::Idle;
            return 0.0;
        }
        *self = match *self {
            Self::Idle => {
                let dir = rng.sign();
                let until = now + rng.in_range(STRAFE_SECS);
                Self::Moving {
                    dir: decide(dir, rng),
                    until,
                }
            }
            Self::Moving { dir, until } if now >= until => {
                let until = now + rng.in_range(STRAFE_SECS);
                Self::Moving {
                    dir: decide(-dir, rng),
                    until,
                }
            }
            moving => moving,
        };
        self.key()
    }

    /// A juke: switch to the other strafe key at once and hold it for a fresh
    /// [`STRAFE_SECS`]. A bot that is not strafing does nothing (and draws nothing).
    pub fn juke(&mut self, now: f32, rng: &mut Rng) -> f32 {
        if let Self::Moving { dir, .. } = *self {
            *self = Self::Moving {
                dir: -dir,
                until: now + rng.in_range(STRAFE_SECS),
            };
        }
        self.key()
    }

    /// The strafe key pressed: -1 (A), 1 (D) or 0.
    pub fn key(self) -> f32 {
        match self {
            Self::Moving { dir, .. } => dir,
            Self::Idle => 0.0,
        }
    }
}

/// Slowest lateral speed of the player, m/s, at which a reactive bot reads a strafe
/// direction to copy or oppose. A design choice: well under walking speed, so standing
/// still or drifting does not count as strafing.
pub const PLAYER_STRAFE_MIN_SPEED: f32 = 0.5;

/// The strafe key a reactive bot presses at a strafe-key decision. `dir` is the plain ADAD
/// key and `player_side` the key (-1, 0 or 1) that moves the bot the same world direction
/// as the player strafes (0 when the player is not strafing). One random roll decides
/// between copying the player, opposing them and the plain key.
pub(crate) fn react_strafe(
    dir: f32,
    player_side: f32,
    mirror_chance: f32,
    oppose_chance: f32,
    rng: &mut Rng,
) -> f32 {
    if mirror_chance + oppose_chance <= 0.0 {
        return dir;
    }
    let roll = rng.next_f32();
    if player_side == 0.0 {
        dir
    } else if roll < mirror_chance {
        player_side
    } else if roll < mirror_chance + oppose_chance {
        -player_side
    } else {
        dir
    }
}

/// The push of a site attacker: stop to fight for at most [`PUSH_DUEL_SECS`], then walk on
/// toward the site for [`PUSH_ADVANCE_SECS`] even while it still sees the player.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) enum Push {
    /// Walking toward the site, not fighting.
    #[default]
    Advancing,
    /// Stopped to fight until `until`.
    Dueling { until: f32 },
    /// Walking on after a duel, ignoring the fight until `until`.
    Pressing { until: f32 },
}

impl Push {
    /// Advance one frame given whether the attacker sees and is engaged with the player.
    /// Returns true when it stops to fight this frame.
    pub fn step(&mut self, fighting: bool, now: f32) -> bool {
        *self = match *self {
            Self::Pressing { until } if now < until => Self::Pressing { until },
            _ if !fighting => Self::Advancing,
            Self::Advancing | Self::Pressing { .. } => Self::Dueling {
                until: now + PUSH_DUEL_SECS,
            },
            Self::Dueling { until } if now >= until => Self::Pressing {
                until: now + PUSH_ADVANCE_SECS,
            },
            dueling => dueling,
        };
        matches!(self, Self::Dueling { .. })
    }
}

/// How long a roaming bot stays in cover after it fell back, before it re-peeks, seconds. A
/// design choice: long enough to break the duel, short enough to keep the fight going.
pub const COVER_HOLD_SECS: Range = Range::new(0.6, 1.4);
/// Distance under which a bot counts as having reached its cover spot or its re-peek spot,
/// meters.
const RETREAT_ARRIVE: f32 = 0.3;
/// Longest a bot runs for cover before it gives up and fights again, seconds. A design
/// choice: the longest cover route ([`super::RETREAT_MAX_ROUTE_M`], about 1.5 s at rifle run
/// speed) plus a margin, so a bot whose spot turns out unreachable (collision sliding)
/// never flees forever with its fire held.
pub const RETREAT_GIVE_UP_SECS: f32 = 2.5;

/// The fall-back of a roaming bot that was shot: run to a cover spot out of the shooter's
/// sight, hide there for [`COVER_HOLD_SECS`], then walk back to where it was hit and fight
/// again. It never learns where the player is beyond the spot the shot came from.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) enum Retreat {
    /// Not falling back: the bot roams and fights as usual.
    #[default]
    Off,
    /// Running to `spot` until `give_up_at` at the latest; it will re-peek `back_to`.
    ToCover {
        spot: Vec3,
        back_to: Vec3,
        give_up_at: f32,
    },
    /// Hiding on `spot` until `until`.
    Hiding {
        spot: Vec3,
        back_to: Vec3,
        until: f32,
    },
    /// Walking back to `back_to` to re-peek the fight.
    Repeek { back_to: Vec3 },
}

/// What the fall-back needs to know this frame.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RetreatSense {
    pub now: f32,
    pub feet: Vec3,
    /// Whether the bot sees and is engaged with the player.
    pub fighting: bool,
}

impl Retreat {
    /// Start falling back at `now` to `spot` from `feet`, the place the bot will re-peek.
    pub fn start(spot: Vec3, feet: Vec3, now: f32) -> Self {
        Self::ToCover {
            spot,
            back_to: feet,
            give_up_at: now + RETREAT_GIVE_UP_SECS,
        }
    }

    /// The state after this frame. A bot running to cover keeps running even if it still
    /// sees the player, until [`RETREAT_GIVE_UP_SECS`]; a hiding or re-peeking bot that sees
    /// the player stops to fight.
    pub fn next(self, s: &RetreatSense, rng: &mut Rng) -> Self {
        let at = |p: Vec3| (p - s.feet).horizontal().length() < RETREAT_ARRIVE;
        match self {
            Self::ToCover { spot, back_to, .. } if at(spot) => Self::Hiding {
                spot,
                back_to,
                until: s.now + rng.in_range(COVER_HOLD_SECS),
            },
            // The spot is out of reach (blocked or sliding along a wall): fight again.
            Self::ToCover { give_up_at, .. } if s.now >= give_up_at => Self::Off,
            Self::Hiding { .. } | Self::Repeek { .. } if s.fighting => Self::Off,
            Self::Hiding { back_to, until, .. } if s.now >= until => Self::Repeek { back_to },
            Self::Repeek { back_to } if at(back_to) => Self::Off,
            other => other,
        }
    }

    /// Where the bot walks: its cover spot, then back to the re-peek spot. `None` when off.
    pub fn goal(self) -> Option<Vec3> {
        match self {
            Self::Off => None,
            Self::ToCover { spot, .. } | Self::Hiding { spot, .. } => Some(spot),
            Self::Repeek { back_to } => Some(back_to),
        }
    }

    /// Whether the bot is running for cover: it holds its fire while it runs.
    pub fn fleeing(self) -> bool {
        matches!(self, Self::ToCover { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::movement::{self, LocalVelocity, MoveInput, MovementProfile};

    const DELAY: Range = Range::new(0.4, 1.2);

    fn sense(now: f32) -> PeekSense {
        PeekSense {
            now,
            player_sees_angle: false,
            at_peek: false,
            at_hidden: true,
            hold_secs: 1.5,
            peek_delay: DELAY,
        }
    }

    #[test]
    fn a_hidden_bot_waits_until_the_player_sees_the_angle() {
        let mut rng = Rng::new(1);
        let hidden = Phase::Hidden { peek_at: None };
        assert_eq!(hidden.next(&sense(3.0), &mut rng), hidden);
        let seen = PeekSense {
            player_sees_angle: true,
            ..sense(3.0)
        };
        let Phase::Hidden { peek_at: Some(t) } = hidden.next(&seen, &mut rng) else {
            panic!("the peek is scheduled");
        };
        assert!((3.0 + DELAY.min..=3.0 + DELAY.max).contains(&t), "{t}");
    }

    #[test]
    fn peek_hold_hide_cycle() {
        let mut rng = Rng::new(2);
        let scheduled = Phase::Hidden { peek_at: Some(1.0) };
        assert_eq!(scheduled.next(&sense(0.9), &mut rng), scheduled);
        let peeking = scheduled.next(&sense(1.0), &mut rng);
        assert_eq!(peeking, Phase::Peeking);
        assert!(peeking.wants_peek_position());

        let on_peek = PeekSense {
            at_peek: true,
            at_hidden: false,
            ..sense(1.3)
        };
        let holding = peeking.next(&on_peek, &mut rng);
        assert_eq!(holding, Phase::Holding { until: 2.8 });
        assert!(holding.wants_peek_position());
        assert_eq!(holding.next(&sense(2.7), &mut rng), holding);

        let hiding = holding.next(&sense(2.8), &mut rng);
        assert_eq!(hiding, Phase::Hiding);
        assert!(!hiding.wants_peek_position());
        let walking_back = PeekSense {
            at_hidden: false,
            ..sense(3.0)
        };
        assert_eq!(hiding.next(&walking_back, &mut rng), Phase::Hiding);
        assert_eq!(
            hiding.next(&sense(3.2), &mut rng),
            Phase::Hidden { peek_at: None }
        );
    }

    #[test]
    fn a_bot_that_is_not_dueling_does_not_strafe_or_use_the_rng() {
        let mut rng = Rng::new(3);
        let mut probe = rng.clone();
        let mut s = Strafe::Moving {
            dir: 1.0,
            until: 5.0,
        };
        assert_eq!(s.step(false, 1.0, &mut rng), 0.0);
        assert_eq!(s, Strafe::Idle);
        assert_eq!(rng.next_u64(), probe.next_u64(), "no random draw");
    }

    #[test]
    fn a_dueling_bot_alternates_strafe_keys() {
        let mut rng = Rng::new(4);
        let mut s = Strafe::Idle;
        let first = s.step(true, 0.0, &mut rng);
        assert!(first == 1.0 || first == -1.0);
        let Strafe::Moving { until, .. } = s else {
            panic!("strafing");
        };
        assert!((STRAFE_SECS.min..=STRAFE_SECS.max).contains(&until));
        assert_eq!(s.step(true, until - 0.01, &mut rng), first, "keeps the key");
        assert_eq!(s.step(true, until, &mut rng), -first, "switches at the end");
    }

    #[test]
    fn plain_and_identity_strafes_draw_the_same_numbers() {
        let (mut a, mut b) = (Rng::new(10), Rng::new(10));
        let (mut sa, mut sb) = (Strafe::Idle, Strafe::Idle);
        for i in 0..200 {
            let now = i as f32 * 0.05;
            assert_eq!(
                sa.step(true, now, &mut a),
                sb.step_with(true, now, &mut b, |d, _| d)
            );
        }
        assert_eq!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn a_juke_switches_keys_at_once_and_an_idle_bot_does_not_juke() {
        let mut rng = Rng::new(11);
        let mut probe = rng.clone();
        let mut idle = Strafe::Idle;
        assert_eq!(idle.juke(1.0, &mut rng), 0.0);
        assert_eq!(rng.next_u64(), probe.next_u64(), "no random draw");

        let mut s = Strafe::Moving {
            dir: 1.0,
            until: 9.0,
        };
        assert_eq!(s.juke(2.0, &mut rng), -1.0);
        let Strafe::Moving { until, .. } = s else {
            panic!("still strafing");
        };
        assert!((2.0 + STRAFE_SECS.min..=2.0 + STRAFE_SECS.max).contains(&until));
    }

    #[test]
    fn a_reactive_strafe_copies_opposes_or_keeps_the_plain_key() {
        let mut rng = Rng::new(12);
        // Certain mirror and certain oppose.
        assert_eq!(react_strafe(-1.0, 1.0, 1.0, 0.0, &mut rng), 1.0);
        assert_eq!(react_strafe(1.0, 1.0, 0.0, 1.0, &mut rng), -1.0);
        // A player who is not strafing leaves the plain key, but the roll still happens
        // so the random sequence does not depend on the player's movement.
        let mut probe = rng.clone();
        assert_eq!(react_strafe(-1.0, 0.0, 1.0, 0.0, &mut rng), -1.0);
        probe.next_f32();
        assert_eq!(rng.next_u64(), probe.next_u64());
        // No strafe chances: the plain key and no roll.
        let mut probe = rng.clone();
        assert_eq!(react_strafe(1.0, -1.0, 0.0, 0.0, &mut rng), 1.0);
        assert_eq!(rng.next_u64(), probe.next_u64());
        // Half and half: both reactions show up.
        let picks: Vec<f32> = (0..200)
            .map(|_| react_strafe(1.0, -1.0, 0.5, 0.5, &mut rng))
            .collect();
        assert!(picks.contains(&1.0) && picks.contains(&-1.0));
    }

    #[test]
    fn an_attacker_duels_for_a_while_then_pushes_on() {
        let mut p = Push::default();
        assert!(!p.step(false, 0.0), "nothing to fight: advance");
        assert!(p.step(true, 1.0), "stops to fight");
        assert!(p.step(true, 1.0 + PUSH_DUEL_SECS - 0.01));
        assert!(
            !p.step(true, 1.0 + PUSH_DUEL_SECS),
            "pushes on after the duel"
        );
        let pressing_until = 1.0 + PUSH_DUEL_SECS + PUSH_ADVANCE_SECS;
        assert!(!p.step(true, pressing_until - 0.01), "keeps walking");
        assert!(p.step(true, pressing_until), "fights again");
        assert!(!p.step(false, pressing_until + 0.1), "lost sight: advance");
        assert_eq!(p, Push::Advancing);
    }

    /// ADAD through the VALORANT movement model: the bot is fast between switches and slow
    /// enough to be accurate around each switch.
    #[test]
    fn strafing_crosses_the_accurate_speed_window_at_each_switch() {
        let profile = MovementProfile::valorant_rifle();
        let limit = profile.accuracy_speed_limit();
        let mut rng = Rng::new(5);
        let mut s = Strafe::Idle;
        let mut v = LocalVelocity::default();
        let dt = 1.0 / 128.0;
        let (mut fast, mut accurate_after_fast, mut was_fast) = (0, 0, false);
        for i in 0..(128 * 4) {
            let right = s.step(true, i as f32 * dt, &mut rng);
            v = movement::step(
                &profile,
                v,
                MoveInput {
                    right,
                    ..Default::default()
                },
                dt,
            );
            if v.speed() > limit * 2.0 {
                was_fast = true;
                fast += 1;
            } else if was_fast && v.speed() <= limit {
                was_fast = false;
                accurate_after_fast += 1;
            }
        }
        assert!(fast > 0);
        assert!(accurate_after_fast >= 4, "{accurate_after_fast}");
    }

    fn at(now: f32, x: f32, fighting: bool) -> RetreatSense {
        RetreatSense {
            now,
            feet: Vec3::new(x, 0.0, 0.0),
            fighting,
        }
    }

    #[test]
    fn a_shot_bot_runs_to_cover_hides_then_repeeks_where_it_was_hit() {
        let mut rng = Rng::new(6);
        let (spot, hit_at) = (Vec3::new(4.0, 0.0, 0.0), Vec3::ZERO);
        let running = Retreat::start(spot, hit_at, 0.0);
        assert!(running.fleeing());
        assert_eq!(running.goal(), Some(spot));
        assert_eq!(
            running.next(&at(0.5, 2.0, true), &mut rng),
            running,
            "keeps running while it still sees the player"
        );

        let hiding = running.next(&at(1.0, 4.0, false), &mut rng);
        let Retreat::Hiding { until, .. } = hiding else {
            panic!("hides on the spot: {hiding:?}");
        };
        assert!((1.0 + COVER_HOLD_SECS.min..=1.0 + COVER_HOLD_SECS.max).contains(&until));
        assert!(!hiding.fleeing());
        assert_eq!(hiding.goal(), Some(spot));
        assert_eq!(hiding.next(&at(until - 0.01, 4.0, false), &mut rng), hiding);

        let repeek = hiding.next(&at(until, 4.0, false), &mut rng);
        assert_eq!(repeek, Retreat::Repeek { back_to: hit_at });
        assert_eq!(repeek.goal(), Some(hit_at));
        assert_eq!(repeek.next(&at(until + 0.5, 2.0, false), &mut rng), repeek);
        assert_eq!(
            repeek.next(&at(until + 1.0, 0.1, false), &mut rng),
            Retreat::Off,
            "back on the re-peek spot: roam and fight as usual"
        );
        assert_eq!(Retreat::Off.goal(), None);
    }

    #[test]
    fn a_hiding_or_repeeking_bot_that_sees_the_player_fights() {
        let mut rng = Rng::new(7);
        let hiding = Retreat::Hiding {
            spot: Vec3::ZERO,
            back_to: Vec3::new(3.0, 0.0, 0.0),
            until: 9.0,
        };
        assert_eq!(hiding.next(&at(1.0, 0.0, true), &mut rng), Retreat::Off);
        let repeek = Retreat::Repeek {
            back_to: Vec3::new(3.0, 0.0, 0.0),
        };
        assert_eq!(repeek.next(&at(1.0, 1.0, true), &mut rng), Retreat::Off);
    }

    #[test]
    fn only_reaching_cover_draws_a_random_number() {
        let mut rng = Rng::new(8);
        let mut probe = rng.clone();
        let repeek = Retreat::Repeek {
            back_to: Vec3::new(3.0, 0.0, 0.0),
        };
        repeek.next(&at(1.0, 0.0, false), &mut rng);
        Retreat::Off.next(&at(1.0, 0.0, true), &mut rng);
        let running = Retreat::start(Vec3::new(5.0, 0.0, 0.0), Vec3::ZERO, 0.0);
        running.next(&at(1.0, 0.0, false), &mut rng);
        running.next(&at(RETREAT_GIVE_UP_SECS, 0.0, false), &mut rng);
        assert_eq!(rng.next_u64(), probe.next_u64(), "no random draw");
    }

    #[test]
    fn a_bot_that_cannot_reach_its_cover_gives_up_and_fights_again() {
        let mut rng = Rng::new(9);
        // Stuck short of the spot (e.g. sliding along a wall): the feet never arrive.
        let running = Retreat::start(Vec3::new(4.0, 0.0, 0.0), Vec3::ZERO, 1.0);
        let stuck = 1.5;
        let cap = 1.0 + RETREAT_GIVE_UP_SECS;
        let before = running.next(&at(cap - 0.01, stuck, false), &mut rng);
        assert_eq!(before, running, "still running before the cap");
        assert!(before.fleeing());
        let after = running.next(&at(cap, stuck, false), &mut rng);
        assert_eq!(after, Retreat::Off, "gives up the unreachable spot");
        assert!(!after.fleeing(), "fires again");
        assert_eq!(after.goal(), None, "back to roaming");
    }
}
