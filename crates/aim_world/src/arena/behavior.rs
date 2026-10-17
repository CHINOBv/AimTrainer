//! Bot behavior as small, pure state machines: the angle peek cycle (hidden, peek, hold,
//! hide) and the duel strafe (ADAD). They decide *what* a bot wants; [`super::bot`] turns
//! that into movement with `aim_core::movement::step` and into aim.
//!
//! Both machines only draw from the RNG on the transitions that need a random duration, so
//! a machine that is never entered (strafing off) leaves the arena's random sequence as it
//! was.

use aim_core::math::Range;
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
        if !dueling {
            *self = Self::Idle;
            return 0.0;
        }
        *self = match *self {
            Self::Idle => Self::Moving {
                dir: rng.sign(),
                until: now + rng.in_range(STRAFE_SECS),
            },
            Self::Moving { dir, until } if now >= until => Self::Moving {
                dir: -dir,
                until: now + rng.in_range(STRAFE_SECS),
            },
            moving => moving,
        };
        match *self {
            Self::Moving { dir, .. } => dir,
            Self::Idle => 0.0,
        }
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
}
