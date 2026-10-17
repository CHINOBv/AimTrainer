//! Full-auto recoil for click scenarios with `weapon.recoil = true`.
//!
//! Each consecutive bullet of a spray is pushed away from the crosshair: it climbs
//! vertically over the weapon's protected bullets, then sways horizontally to a random side
//! that can switch on any later bullet. Spread grows from first-shot spread to the
//! maximum, and everything resets once the player stops firing for the recovery time.
//! Recoil moves the bullet, not the camera. The model and which of its numbers are
//! estimates are documented in `docs/valorant-mechanics.md` ("Recoil").

use crate::games::valorant::RecoilProfile;
use crate::rng::Rng;

/// Where one bullet of a spray goes relative to the crosshair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Kick {
    /// 1-based index of the bullet in its burst.
    pub bullet: u32,
    /// Upward offset from the crosshair, degrees.
    pub pitch_deg: f32,
    /// Rightward offset from the crosshair, degrees.
    pub yaw_deg: f32,
    /// Radius of the spread cone for this bullet, degrees.
    pub spread_deg: f32,
}

/// Recoil state of one weapon across a run.
#[derive(Debug, Clone)]
pub(super) struct RecoilState {
    profile: RecoilProfile,
    first_shot_spread_deg: f32,
    /// Index of the last bullet fired in the current burst; 0 before the first shot.
    bullet: u32,
    last_shot_at: f32,
    pitch_deg: f32,
    yaw_deg: f32,
    /// Current sway direction in [-1, 1]; eases towards `side_target`.
    side: f32,
    /// Side the sway is swinging to: -1 (left) or 1 (right).
    side_target: f32,
}

impl RecoilState {
    pub fn new(profile: RecoilProfile, first_shot_spread_deg: f32) -> Self {
        Self {
            profile,
            first_shot_spread_deg,
            bullet: 0,
            last_shot_at: f32::NEG_INFINITY,
            pitch_deg: 0.0,
            yaw_deg: 0.0,
            side: 0.0,
            side_target: 0.0,
        }
    }

    pub fn profile(&self) -> &RecoilProfile {
        &self.profile
    }

    /// Whether the next shot at `now` is a fresh first bullet.
    pub fn is_recovered(&self, now: f32) -> bool {
        self.bullet == 0 || now - self.last_shot_at >= self.profile.recovery_secs
    }

    /// Index of the last bullet of the burst in progress at `now`; 0 once recovered.
    pub fn bullet_index(&self, now: f32) -> u32 {
        if self.is_recovered(now) {
            0
        } else {
            self.bullet
        }
    }

    /// Seconds until a full reset at `now`; 0 once recovered.
    pub fn recovery_remaining(&self, now: f32) -> f32 {
        if self.is_recovered(now) {
            0.0
        } else {
            self.profile.recovery_secs - (now - self.last_shot_at)
        }
    }

    /// Fire one bullet at `now` and return where it goes.
    pub fn fire(&mut self, now: f32, rng: &mut Rng) -> Kick {
        let since = now - self.last_shot_at;
        if self.is_recovered(now) {
            self.bullet = 0;
            self.pitch_deg = 0.0;
            self.yaw_deg = 0.0;
            self.side = 0.0;
            self.side_target = 0.0;
        }
        self.last_shot_at = now;
        self.bullet += 1;
        let p = self.profile;
        if self.bullet > 1 && self.bullet <= p.protected_bullets {
            self.pitch_deg += p.climb_deg_per_bullet;
        } else if self.bullet > p.protected_bullets {
            self.sway(since, rng);
        }
        let ramp = p.spread_ramp_bullets.max(1) as f32;
        let t = ((self.bullet - 1) as f32 / ramp).min(1.0);
        Kick {
            bullet: self.bullet,
            pitch_deg: self.pitch_deg,
            yaw_deg: self.yaw_deg,
            spread_deg: self.first_shot_spread_deg
                + (p.max_spread_deg - self.first_shot_spread_deg) * t,
        }
    }

    /// Advance the horizontal sway by one bullet fired `since` seconds after the previous.
    fn sway(&mut self, since: f32, rng: &mut Rng) {
        let p = self.profile;
        if self.bullet == p.protected_bullets + 1 {
            self.side_target = rng.sign();
            self.side = self.side_target;
        } else {
            // The side swings from -1 to 1 (a distance of 2) in `yaw_switch_secs`.
            let step = 2.0 * since / p.yaw_switch_secs.max(f32::EPSILON);
            let delta = (self.side_target - self.side).clamp(-step, step);
            self.side += delta;
            if rng.next_f32() < p.yaw_switch_chance {
                self.side_target = -self.side_target;
            }
        }
        self.yaw_deg += p.sway_deg_per_bullet * self.side;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::valorant::Weapon;

    fn vandal() -> RecoilState {
        RecoilState::new(Weapon::Vandal.recoil().unwrap(), 0.25)
    }

    #[test]
    fn protected_bullets_only_climb_and_later_bullets_sway() {
        let mut r = vandal();
        let mut rng = Rng::new(3);
        let interval = 1.0 / 9.75;
        let kicks: Vec<Kick> = (0..10)
            .map(|i| r.fire(i as f32 * interval, &mut rng))
            .collect();
        assert_eq!(kicks[0].pitch_deg, 0.0);
        for w in kicks[..6].windows(2) {
            assert!(w[1].pitch_deg > w[0].pitch_deg);
            assert_eq!(w[1].yaw_deg, 0.0);
        }
        assert!(kicks[6].yaw_deg != 0.0);
        assert_eq!(kicks[9].pitch_deg, kicks[5].pitch_deg);
        assert!(kicks[9].spread_deg > kicks[0].spread_deg);
        assert_eq!(kicks[0].spread_deg, 0.25);
    }

    #[test]
    fn spread_reaches_the_maximum_after_the_ramp() {
        let mut r = vandal();
        let mut rng = Rng::new(1);
        let last = (0..15).map(|i| r.fire(i as f32 * 0.1, &mut rng)).last();
        assert_eq!(last.unwrap().spread_deg, 1.0);
    }
}
