//! Death recap: why the player lost a fight. Each bot keeps a [`DuelLog`] of the current
//! player life (who saw whom first, the crosshair error at first sight, the first shot,
//! moving shots, damage both ways); when that bot kills the player the log becomes a
//! [`DeathRecap`] with one main [`DeathCause`]. Pure bookkeeping: no random numbers.

/// A crosshair this many degrees or more from the killer's head at first sight counts as
/// misplaced. A design choice: twice the pre-aim tolerance (5°), so only clearly wrong
/// placement is blamed, not the few degrees a partly hidden head always measures.
pub const RECAP_PLACEMENT_DEG: f32 = 10.0;
/// A first shot this many seconds or more after first sight counts as slow. A design
/// choice: a human visual reaction (about 0.25 s) plus a short flick.
pub const RECAP_SLOW_SHOT_SECS: f32 = 0.5;

/// The main reason a death happened, checked in this order (the first that applies wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DeathCause {
    /// The killer was never in the player's line of sight this life.
    Unseen,
    /// The killing blow was a shoot-back deadline's sure shot: the bot lived too long.
    Deadline,
    /// Most of the player's shots during the fight were fired faster than the accurate
    /// speed, so they carried run spread.
    MovingShots,
    /// The crosshair was at least [`RECAP_PLACEMENT_DEG`] from the killer's head when it
    /// appeared.
    Placement,
    /// The player fired at least [`RECAP_SLOW_SHOT_SECS`] after first sight, or never.
    SlowShot,
    /// None of the above: the player was placed, quick and still, and lost the aim duel.
    Outaimed,
}

/// Why one death happened, recorded when a bot kills the player.
#[derive(Debug, Clone, PartialEq)]
pub struct DeathRecap {
    /// Session time of the death, seconds.
    pub at_secs: f32,
    pub killer_id: u64,
    /// The killer's aimer preset (`sim::AimerProfile::name`).
    pub killer_skill: &'static str,
    /// Horizontal distance from the player to the killer at the death, meters.
    pub distance_m: f32,
    /// The player had the killer in sight before (or in the same frame as) it noticed them.
    pub saw_first: bool,
    /// Angle between the crosshair and the killer's head when it first came into the
    /// player's sight this life, degrees; `None` when it never did.
    pub crosshair_error_deg: Option<f32>,
    /// Seconds from first sight of the killer to the player's first shot; `None` when the
    /// player never saw it or never fired after seeing it.
    pub first_shot_delay_secs: Option<f32>,
    /// Shots the player fired while the killer was in sight.
    pub shots: u32,
    /// Of those, shots fired faster than the accurate speed.
    pub moving_shots: u32,
    /// Damage the player dealt to the killer this life.
    pub damage_dealt: f32,
    /// Damage the killer dealt to the player this life.
    pub damage_taken: f32,
    pub cause: DeathCause,
}

/// What one bot and the player did to each other during the current player life.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct DuelLog {
    /// Session time the bot first came into the player's sight.
    pub player_saw_at: Option<f32>,
    /// Crosshair error at that moment, degrees.
    pub sight_error_deg: Option<f32>,
    /// Session time of the player's first shot after seeing the bot.
    pub first_shot_at: Option<f32>,
    /// Session time the bot first noticed the player.
    pub noticed_at: Option<f32>,
    pub shots: u32,
    pub moving_shots: u32,
    pub damage_dealt: f32,
    pub damage_taken: f32,
}

impl DuelLog {
    /// The player sees the bot this frame, with the crosshair `error_deg` from its head.
    pub fn player_sees(&mut self, now: f32, error_deg: f32) {
        if self.player_saw_at.is_none() {
            self.player_saw_at = Some(now);
            self.sight_error_deg = Some(error_deg);
        }
    }

    /// The bot notices the player this frame.
    pub fn bot_notices(&mut self, now: f32) {
        self.noticed_at.get_or_insert(now);
    }

    /// The player fired; `visible` says whether the bot was in sight.
    pub fn player_fired(&mut self, now: f32, visible: bool, moving: bool) {
        if self.player_saw_at.is_some() {
            self.first_shot_at.get_or_insert(now);
        }
        if visible {
            self.shots += 1;
            self.moving_shots += u32::from(moving);
        }
    }

    /// The recap of a death by this bot's hand. `deadline` says the killing blow was a
    /// shoot-back deadline's sure shot.
    pub fn recap(&self, killer: Killer, now: f32, deadline: bool) -> DeathRecap {
        let saw_first = self
            .player_saw_at
            .is_some_and(|p| self.noticed_at.is_none_or(|b| p <= b));
        let first_shot_delay_secs = self
            .player_saw_at
            .zip(self.first_shot_at)
            .map(|(seen, shot)| shot - seen);
        let cause = if self.player_saw_at.is_none() {
            DeathCause::Unseen
        } else if deadline {
            DeathCause::Deadline
        } else if self.shots > 0 && self.moving_shots * 2 > self.shots {
            DeathCause::MovingShots
        } else if self
            .sight_error_deg
            .is_some_and(|e| e >= RECAP_PLACEMENT_DEG)
        {
            DeathCause::Placement
        } else if first_shot_delay_secs.is_none_or(|d| d >= RECAP_SLOW_SHOT_SECS) {
            DeathCause::SlowShot
        } else {
            DeathCause::Outaimed
        };
        DeathRecap {
            at_secs: now,
            killer_id: killer.id,
            killer_skill: killer.skill,
            distance_m: killer.distance_m,
            saw_first,
            crosshair_error_deg: self.sight_error_deg,
            first_shot_delay_secs,
            shots: self.shots,
            moving_shots: self.moving_shots,
            damage_dealt: self.damage_dealt,
            damage_taken: self.damage_taken,
            cause,
        }
    }
}

/// Who killed the player, for [`DuelLog::recap`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct Killer {
    pub id: u64,
    pub skill: &'static str,
    pub distance_m: f32,
}

/// The most common cause among `recaps` and how many deaths it explains; ties go to the
/// cause checked first.
pub(crate) fn top_cause(recaps: &[DeathRecap]) -> Option<(DeathCause, u32)> {
    let mut counts: Vec<(DeathCause, u32)> = Vec::new();
    for r in recaps {
        match counts.iter_mut().find(|(c, _)| *c == r.cause) {
            Some((_, n)) => *n += 1,
            None => counts.push((r.cause, 1)),
        }
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KILLER: Killer = Killer {
        id: 1,
        skill: "veteran",
        distance_m: 10.0,
    };

    fn seen(error: f32) -> DuelLog {
        let mut d = DuelLog::default();
        d.player_sees(1.0, error);
        d
    }

    #[test]
    fn causes_are_checked_in_order() {
        let cause = |d: &DuelLog, deadline| d.recap(KILLER, 3.0, deadline).cause;
        assert_eq!(cause(&DuelLog::default(), false), DeathCause::Unseen);
        let mut d = seen(30.0);
        assert_eq!(cause(&d, true), DeathCause::Deadline);
        d.player_fired(1.2, true, true);
        assert_eq!(cause(&d, false), DeathCause::MovingShots);
        d.player_fired(1.3, true, false);
        // One of two shots moving is not most of them.
        assert_eq!(cause(&d, false), DeathCause::Placement);
        let mut d = seen(2.0);
        assert_eq!(cause(&d, false), DeathCause::SlowShot, "never fired");
        d.player_fired(1.6, true, false);
        assert_eq!(cause(&d, false), DeathCause::SlowShot, "0.6 s is slow");
        let mut d = seen(2.0);
        d.player_fired(1.2, true, false);
        assert_eq!(cause(&d, false), DeathCause::Outaimed);
    }

    #[test]
    fn the_top_cause_counts_deaths_and_breaks_ties_by_order() {
        let r = |cause| DeathRecap {
            cause,
            ..seen(1.0).recap(KILLER, 2.0, false)
        };
        assert_eq!(top_cause(&[]), None);
        let recaps = [
            r(DeathCause::SlowShot),
            r(DeathCause::Placement),
            r(DeathCause::SlowShot),
        ];
        assert_eq!(top_cause(&recaps), Some((DeathCause::SlowShot, 2)));
        let tie = [r(DeathCause::SlowShot), r(DeathCause::Placement)];
        assert_eq!(top_cause(&tie), Some((DeathCause::Placement, 1)));
    }
}
