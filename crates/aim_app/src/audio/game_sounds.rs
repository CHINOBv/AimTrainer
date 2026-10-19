//! Which game-feel sound to play, and how loud: arena feedback, bot footsteps, streaks,
//! round cues and the low-time warning. Pure decisions with no audio output, so they are
//! unit-tested; `super` plays what they return. The timings are estimates, tuned by ear.

use super::mix::{Category, category_gain};
use super::synth::Effect;
use crate::game::arena::ArenaFeedback;
use crate::persistence::settings::SoundSettings;
use aim_core::math::Vec3;
use std::collections::HashMap;

/// Kills landed within this many seconds of the previous one chain into a multi-kill.
pub const MULTI_KILL_WINDOW_SECS: f32 = 1.5;
/// Streak levels with their own pitch; longer streaks repeat the top one.
pub const STREAK_LEVELS: u8 = 5;
/// Multi-kill stings: a double, a triple, and one for four or more.
pub const MULTI_KILL_LEVELS: u8 = 3;
/// Pitch step between streak levels and multi-kill stings, in semitones.
const LEVEL_SEMITONES: f32 = 2.0;
/// Seconds between two steps of a running bot.
pub const STEP_INTERVAL_SECS: f32 = 0.35;
/// Bots farther than this from the player make no audible steps, meters.
pub const FOOTSTEP_RANGE_M: f32 = 20.0;
/// Slowest ground speed that makes steps, m/s; slower bots are standing or creeping.
pub const MIN_STEP_SPEED: f32 = 1.0;
/// Faster position changes are respawns or teleports, not steps, m/s.
const MAX_STEP_SPEED: f32 = 15.0;
/// The last seconds of a timed run that tick.
pub const LOW_TIME_SECS: f32 = 10.0;
/// Damage at which the damage thud reaches full level (a body shot's worth, estimate).
const FULL_THUD_DAMAGE: f32 = 100.0;
/// Level of the lightest damage thud.
const MIN_THUD_LEVEL: f32 = 0.4;

/// A pre-rendered game-feel clip: an [`Effect`] and, for streaks and multi-kills, its level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sfx {
    BotGunshot,
    Footstep,
    DamageThud,
    Death,
    /// Streak level, 1 to [`STREAK_LEVELS`].
    Streak(u8),
    /// Multi-kill sting, 1 (double) to [`MULTI_KILL_LEVELS`] (four or more).
    MultiKill(u8),
    CountdownBeep,
    RoundStart,
    RoundEnd,
    LowTimeTick,
    UiClick,
}

impl Sfx {
    /// Every clip, so the bank renders them all once at startup.
    pub fn all() -> impl Iterator<Item = Self> {
        let streaks = (1..=STREAK_LEVELS).map(Self::Streak);
        let multi = (1..=MULTI_KILL_LEVELS).map(Self::MultiKill);
        [
            Self::BotGunshot,
            Self::Footstep,
            Self::DamageThud,
            Self::Death,
            Self::CountdownBeep,
            Self::RoundStart,
            Self::RoundEnd,
            Self::LowTimeTick,
            Self::UiClick,
        ]
        .into_iter()
        .chain(streaks)
        .chain(multi)
    }

    /// The effect this clip is rendered from, and its pitch ratio.
    pub fn effect(self) -> (Effect, f32) {
        match self {
            Self::BotGunshot => (Effect::BotGunshot, 1.0),
            Self::Footstep => (Effect::Footstep, 1.0),
            Self::DamageThud => (Effect::DamageThud, 1.0),
            Self::Death => (Effect::Death, 1.0),
            Self::Streak(level) => (Effect::Streak, level_pitch(level)),
            Self::MultiKill(level) => (Effect::MultiKill, level_pitch(level)),
            Self::CountdownBeep => (Effect::CountdownBeep, 1.0),
            Self::RoundStart => (Effect::RoundStart, 1.0),
            Self::RoundEnd => (Effect::RoundEnd, 1.0),
            Self::LowTimeTick => (Effect::LowTimeTick, 1.0),
            Self::UiClick => (Effect::UiClick, 1.0),
        }
    }
}

/// Pitch ratio of `level` (1 = the effect's own pitch), [`LEVEL_SEMITONES`] per level.
fn level_pitch(level: u8) -> f32 {
    2f32.powf(LEVEL_SEMITONES * f32::from(level.saturating_sub(1)) / 12.0)
}

/// A sound to play now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Play {
    pub sfx: Sfx,
    /// Linear gain: master × category, times the event's own level.
    pub gain: f32,
    /// Where the sound comes from, for positional sounds; `None` plays it centered.
    pub from: Option<Vec3>,
}

/// Remembers recent kills to tell a multi-kill from a streak.
#[derive(Debug, Default)]
pub struct StreakTimer {
    last_kill_at: Option<f32>,
    chain: u32,
}

impl StreakTimer {
    /// The sting for a kill at run time `now` that makes a streak of `streak`: a multi-kill
    /// when it lands within [`MULTI_KILL_WINDOW_SECS`] of the previous kill, otherwise the
    /// streak's level from the second kill on. A first, lone kill adds nothing to the kill
    /// sound the player already hears.
    pub fn kill(&mut self, now: f32, streak: u32) -> Option<Sfx> {
        let chained = self
            .last_kill_at
            .is_some_and(|t| now >= t && now - t <= MULTI_KILL_WINDOW_SECS);
        self.chain = if chained { self.chain + 1 } else { 1 };
        self.last_kill_at = Some(now);
        let capped = |n: u32, max: u8| n.min(u32::from(max)) as u8;
        if self.chain >= 2 {
            Some(Sfx::MultiKill(capped(self.chain - 1, MULTI_KILL_LEVELS)))
        } else if streak >= 2 {
            Some(Sfx::Streak(capped(streak - 1, STREAK_LEVELS)))
        } else {
            None
        }
    }

    /// Forget the chain (the player died or a new round started).
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// The sound of one arena feedback message, if its toggle is on. `now` is the run time
/// and `streaks` the kill timing, kept even when the sound is off.
pub fn feedback_sound(
    feedback: &ArenaFeedback,
    sound: &SoundSettings,
    now: f32,
    streaks: &mut StreakTimer,
) -> Option<Play> {
    // Kill timing is kept whatever the settings, so toggling streaks mid-run stays right.
    let streak = match *feedback {
        ArenaFeedback::BotKilled { streak, .. } => streaks.kill(now, streak),
        ArenaFeedback::PlayerDied { .. } | ArenaFeedback::RoundStarted => {
            streaks.reset();
            None
        }
        _ => None,
    };
    let centered = |sfx, toggle: bool, level: f32| {
        audible(sound, Category::Cues, toggle).map(|gain| Play {
            sfx,
            gain: gain * level,
            from: None,
        })
    };
    match *feedback {
        ArenaFeedback::BotFired { origin, .. } => {
            world_gain(sound, sound.bot_gunfire).map(|gain| Play {
                sfx: Sfx::BotGunshot,
                gain,
                from: Some(Vec3::new(origin.x, origin.y, origin.z)),
            })
        }
        ArenaFeedback::PlayerHit { damage, .. } => {
            centered(Sfx::DamageThud, sound.damage_taken, thud_level(damage))
        }
        ArenaFeedback::PlayerDied { .. } => centered(Sfx::Death, sound.death, 1.0),
        ArenaFeedback::BotKilled { .. } => centered(streak?, sound.streak, 1.0),
        ArenaFeedback::CountdownTick { .. } => centered(Sfx::CountdownBeep, sound.round_cues, 1.0),
        ArenaFeedback::RoundStarted => centered(Sfx::RoundStart, sound.round_cues, 1.0),
        ArenaFeedback::RoundEnded => centered(Sfx::RoundEnd, sound.round_cues, 1.0),
    }
}

/// Level of the damage thud for `damage`: heavier hits thud louder, up to full level.
fn thud_level(damage: f32) -> f32 {
    let share = (damage / FULL_THUD_DAMAGE).clamp(0.0, 1.0);
    MIN_THUD_LEVEL + (1.0 - MIN_THUD_LEVEL) * share
}

/// The gain of a sound in `category`, or `None` when its toggle, the sound effects or
/// the resulting volume are off.
pub fn audible(sound: &SoundSettings, category: Category, toggle: bool) -> Option<f32> {
    let gain = category_gain(sound, category);
    (toggle && sound.enabled && gain > 0.0).then_some(gain)
}

/// The gain of a world sound, or `None` when its toggle or the sound is off.
pub fn world_gain(sound: &SoundSettings, toggle: bool) -> Option<f32> {
    audible(sound, Category::World, toggle)
}

/// Paces the steps of moving bots from their positions frame to frame.
#[derive(Debug, Default)]
pub struct FootstepTracker {
    last_now: Option<f32>,
    bots: HashMap<u64, Walker>,
}

#[derive(Debug, Clone, Copy)]
struct Walker {
    feet: Vec3,
    next_step_at: f32,
    /// Run time of the last update that listed the bot alive.
    seen_at: f32,
}

impl FootstepTracker {
    /// Feet positions of the bots that take a step at run time `now`: alive, moving at a
    /// running pace, within [`FOOTSTEP_RANGE_M`] of `listener`, and at most one step per
    /// [`STEP_INTERVAL_SECS`] each. `bots` lists `(id, feet, alive)`. A paused run (time
    /// not moving) steps nobody; a run restarted (time going back) starts over.
    pub fn update(
        &mut self,
        now: f32,
        listener: Vec3,
        bots: impl IntoIterator<Item = (u64, Vec3, bool)>,
    ) -> Vec<Vec3> {
        if self.last_now.is_some_and(|last| now < last) {
            self.bots.clear();
            self.last_now = None;
        }
        let dt = self.last_now.map_or(0.0, |last| now - last);
        if self.last_now.is_some() && dt <= 0.0 {
            return Vec::new();
        }
        self.last_now = Some(now);
        let mut steps = Vec::new();
        for (id, feet, alive) in bots {
            if !alive {
                continue;
            }
            let walker = self.bots.entry(id).or_insert(Walker {
                feet,
                next_step_at: now,
                seen_at: now,
            });
            let (dx, dz) = (feet.x - walker.feet.x, feet.z - walker.feet.z);
            let speed = if dt > 0.0 {
                (dx * dx + dz * dz).sqrt() / dt
            } else {
                0.0
            };
            walker.feet = feet;
            walker.seen_at = now;
            let running = (MIN_STEP_SPEED..=MAX_STEP_SPEED).contains(&speed);
            let in_range = (feet - listener).length() <= FOOTSTEP_RANGE_M;
            if running && in_range && now >= walker.next_step_at {
                walker.next_step_at = now + STEP_INTERVAL_SECS;
                steps.push(feet);
            }
        }
        // Dead and despawned bots are forgotten; a respawn starts a new walk.
        self.bots.retain(|_, w| w.seen_at == now);
        steps
    }
}

/// Whether the low-time tick plays on a frame where the run's time left went from
/// `before` (`None` on the run's first frame) to `after` seconds: once per whole second
/// crossed in the last [`LOW_TIME_SECS`], from 10 down to 1.
pub fn low_time_tick(before: Option<f32>, after: f32) -> bool {
    let Some(before) = before else {
        return false;
    };
    // The last whole second at or below `before`'s: crossed when `after` reaches it.
    let second = before.ceil() - 1.0;
    (1.0..=LOW_TIME_SECS).contains(&second) && after <= second && after < before
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::gain;
    use aim_core::scenario::HitZone;

    fn v(x: f32, z: f32) -> Vec3 {
        Vec3::new(x, 0.0, z)
    }

    fn play(f: ArenaFeedback, s: &SoundSettings) -> Option<Play> {
        feedback_sound(&f, s, 0.0, &mut StreakTimer::default())
    }

    #[test]
    fn every_clip_has_its_own_effect_and_pitch() {
        let all: Vec<_> = Sfx::all().collect();
        assert_eq!(all.len(), 9 + 5 + 3);
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a.effect(), b.effect(), "{a:?} and {b:?}");
            }
        }
        assert_eq!(Sfx::BotGunshot.effect(), (Effect::BotGunshot, 1.0));
        assert_eq!(Sfx::Streak(1).effect(), (Effect::Streak, 1.0));
    }

    #[test]
    fn streak_levels_and_multi_kills_rise_in_pitch() {
        let pitch = |s: Sfx| s.effect().1;
        for level in 1..STREAK_LEVELS {
            assert!(pitch(Sfx::Streak(level + 1)) > pitch(Sfx::Streak(level)));
        }
        for level in 1..MULTI_KILL_LEVELS {
            assert!(pitch(Sfx::MultiKill(level + 1)) > pitch(Sfx::MultiKill(level)));
        }
        // Two semitones per level.
        assert!((pitch(Sfx::Streak(2)) - 2f32.powf(2.0 / 12.0)).abs() < 1e-5);
    }

    #[test]
    fn bot_gunfire_plays_from_the_bot_in_the_world_category() {
        let s = SoundSettings {
            world_percent: 50,
            ..SoundSettings::default()
        };
        let origin = bevy::math::Vec3::new(3.0, 1.6, -4.0);
        let p = play(ArenaFeedback::BotFired { bot_id: 1, origin }, &s).unwrap();
        assert_eq!(p.sfx, Sfx::BotGunshot);
        assert_eq!(p.from, Some(Vec3::new(3.0, 1.6, -4.0)));
        assert_eq!(p.gain, category_gain(&s, Category::World));
        assert!(p.gain < gain(s.volume_percent));
    }

    #[test]
    fn damage_thuds_scale_with_damage_and_death_plays_centered() {
        let s = SoundSettings::default();
        let hit = |damage| {
            play(
                ArenaFeedback::PlayerHit {
                    from: bevy::math::Vec3::ZERO,
                    damage,
                    zone: Some(HitZone::Body),
                },
                &s,
            )
            .unwrap()
        };
        let cues = category_gain(&s, Category::Cues);
        let (light, heavy, lethal) = (hit(10.0), hit(60.0), hit(160.0));
        assert_eq!(light.sfx, Sfx::DamageThud);
        assert_eq!(light.from, None);
        assert!(light.gain < heavy.gain && heavy.gain < lethal.gain + 1e-6);
        assert!((light.gain - cues * (MIN_THUD_LEVEL + 0.06)).abs() < 1e-5);
        assert_eq!(lethal.gain, cues, "full level is capped");
        let death = play(ArenaFeedback::PlayerDied { from: None }, &s).unwrap();
        assert_eq!(
            (death.sfx, death.gain, death.from),
            (Sfx::Death, cues, None)
        );
    }

    #[test]
    fn round_cues_follow_the_countdown_start_and_end() {
        let s = SoundSettings::default();
        let sfx = |f| play(f, &s).map(|p| p.sfx);
        assert_eq!(
            sfx(ArenaFeedback::CountdownTick { seconds_left: 2 }),
            Some(Sfx::CountdownBeep)
        );
        assert_eq!(sfx(ArenaFeedback::RoundStarted), Some(Sfx::RoundStart));
        assert_eq!(sfx(ArenaFeedback::RoundEnded), Some(Sfx::RoundEnd));
    }

    #[test]
    fn each_sound_respects_its_toggle_and_the_master_switch() {
        let origin = bevy::math::Vec3::ZERO;
        let cases = [
            (
                ArenaFeedback::BotFired { bot_id: 1, origin },
                SoundSettings {
                    bot_gunfire: false,
                    ..SoundSettings::default()
                },
            ),
            (
                ArenaFeedback::PlayerHit {
                    from: origin,
                    damage: 30.0,
                    zone: None,
                },
                SoundSettings {
                    damage_taken: false,
                    ..SoundSettings::default()
                },
            ),
            (
                ArenaFeedback::PlayerDied { from: None },
                SoundSettings {
                    death: false,
                    ..SoundSettings::default()
                },
            ),
            (
                ArenaFeedback::RoundStarted,
                SoundSettings {
                    round_cues: false,
                    ..SoundSettings::default()
                },
            ),
        ];
        for (f, off) in cases {
            assert!(play(f, &SoundSettings::default()).is_some(), "{f:?}");
            assert_eq!(play(f, &off), None, "{f:?} toggled off");
            let disabled = SoundSettings {
                enabled: false,
                ..SoundSettings::default()
            };
            assert_eq!(play(f, &disabled), None, "{f:?} with effects disabled");
            let muted = SoundSettings {
                volume_percent: 0,
                ..SoundSettings::default()
            };
            assert_eq!(play(f, &muted), None, "{f:?} muted");
        }
    }

    fn kill(streak: u32) -> ArenaFeedback {
        ArenaFeedback::BotKilled {
            bot_id: u64::from(streak),
            headshot: false,
            streak,
        }
    }

    #[test]
    fn spaced_kills_raise_the_streak_level_from_the_second_kill() {
        let mut t = StreakTimer::default();
        assert_eq!(
            t.kill(0.0, 1),
            None,
            "a lone kill keeps just the kill sound"
        );
        assert_eq!(t.kill(5.0, 2), Some(Sfx::Streak(1)));
        assert_eq!(t.kill(10.0, 3), Some(Sfx::Streak(2)));
        assert_eq!(
            t.kill(100.0, 40),
            Some(Sfx::Streak(STREAK_LEVELS)),
            "capped"
        );
    }

    #[test]
    fn quick_kills_chain_into_multi_kills() {
        let mut t = StreakTimer::default();
        assert_eq!(t.kill(0.0, 1), None);
        assert_eq!(t.kill(1.0, 2), Some(Sfx::MultiKill(1)), "double");
        assert_eq!(t.kill(2.4, 3), Some(Sfx::MultiKill(2)), "triple");
        assert_eq!(t.kill(3.0, 4), Some(Sfx::MultiKill(3)), "four");
        assert_eq!(t.kill(3.5, 5), Some(Sfx::MultiKill(3)), "capped");
        // Out of the window: back to the streak level.
        assert_eq!(
            t.kill(3.5 + MULTI_KILL_WINDOW_SECS + 0.1, 6),
            Some(Sfx::Streak(5))
        );
    }

    #[test]
    fn a_death_or_a_new_round_breaks_the_chain() {
        let s = SoundSettings::default();
        let mut t = StreakTimer::default();
        let mut at = |f, now| feedback_sound(&f, &s, now, &mut t).map(|p| p.sfx);
        at(kill(1), 0.0);
        at(ArenaFeedback::PlayerDied { from: None }, 0.5);
        assert_eq!(at(kill(1), 1.0), None, "no multi-kill across a death");
        at(ArenaFeedback::RoundStarted, 1.2);
        assert_eq!(at(kill(1), 1.4), None, "nor across rounds");
        // A restarted run's clock goes back: never a chain.
        let mut fresh = StreakTimer::default();
        fresh.kill(30.0, 1);
        assert_eq!(fresh.kill(0.5, 1), None);
    }

    #[test]
    fn streak_sounds_follow_their_toggle_but_keep_counting() {
        let off = SoundSettings {
            streak: false,
            ..SoundSettings::default()
        };
        let mut t = StreakTimer::default();
        assert_eq!(feedback_sound(&kill(1), &off, 0.0, &mut t), None);
        assert_eq!(feedback_sound(&kill(2), &off, 0.5, &mut t), None);
        // Turned on mid-chain, the next quick kill is already a triple.
        let on = SoundSettings::default();
        let p = feedback_sound(&kill(3), &on, 1.0, &mut t).unwrap();
        assert_eq!(p.sfx, Sfx::MultiKill(2));
        assert_eq!(p.gain, category_gain(&on, Category::Cues));
        assert_eq!(p.from, None);
    }

    #[test]
    fn world_gain_needs_the_toggle_and_audible_sound() {
        let s = SoundSettings::default();
        assert_eq!(
            world_gain(&s, true),
            Some(category_gain(&s, Category::World))
        );
        assert_eq!(world_gain(&s, false), None);
        let silent = SoundSettings {
            world_percent: 0,
            ..s.clone()
        };
        assert_eq!(world_gain(&silent, true), None);
        let disabled = SoundSettings {
            enabled: false,
            ..s
        };
        assert_eq!(world_gain(&disabled, true), None);
    }

    /// Steps of one bot walking along +X at `speed` m/s for `secs`, at 100 frames a second.
    fn walk(t: &mut FootstepTracker, start: f32, speed: f32, secs: f32, listener: Vec3) -> usize {
        let frames = (secs * 100.0) as usize;
        (0..=frames)
            .map(|i| {
                let now = start + i as f32 / 100.0;
                t.update(now, listener, [(7, v(speed * (now - start), 0.0), true)])
                    .len()
            })
            .sum()
    }

    #[test]
    fn a_running_bot_steps_at_a_steady_pace() {
        let mut t = FootstepTracker::default();
        let steps = walk(&mut t, 0.0, 5.0, 3.0, v(0.0, 2.0));
        // One step per interval over 3 s, the first on the first moving frame.
        assert_eq!(steps, 9);
    }

    #[test]
    fn standing_far_dead_or_teleporting_bots_make_no_steps() {
        let near = v(0.0, 2.0);
        assert_eq!(
            walk(&mut FootstepTracker::default(), 0.0, 0.0, 2.0, near),
            0
        );
        assert_eq!(
            walk(&mut FootstepTracker::default(), 0.0, 0.5, 2.0, near),
            0
        );
        let far = v(0.0, FOOTSTEP_RANGE_M + 5.0);
        assert_eq!(walk(&mut FootstepTracker::default(), 0.0, 5.0, 2.0, far), 0);
        let mut t = FootstepTracker::default();
        t.update(0.0, near, [(1, v(0.0, 0.0), false)]);
        assert!(t.update(0.1, near, [(1, v(1.0, 0.0), false)]).is_empty());
        // A respawn jumps across the map in one frame: not a step.
        let mut t = FootstepTracker::default();
        t.update(0.0, near, [(2, v(0.0, 0.0), true)]);
        assert!(t.update(0.01, near, [(2, v(3.0, 0.0), true)]).is_empty());
    }

    #[test]
    fn steps_come_from_the_bot_and_stop_while_paused() {
        let near = v(0.0, 2.0);
        let mut t = FootstepTracker::default();
        t.update(0.0, near, [(3, v(0.0, 0.0), true)]);
        assert_eq!(t.update(0.1, near, [(3, v(0.5, 0.0), true)]), [v(0.5, 0.0)]);
        // Paused: the clock stands still and nothing steps, even after the interval.
        for _ in 0..50 {
            assert!(t.update(0.1, near, [(3, v(0.5, 0.0), true)]).is_empty());
        }
    }

    #[test]
    fn a_restarted_run_forgets_the_bots() {
        let near = v(0.0, 2.0);
        let mut t = FootstepTracker::default();
        t.update(10.0, near, [(3, v(0.0, 0.0), true)]);
        // The clock went back: positions from the old run must not count as movement.
        assert!(t.update(0.0, near, [(3, v(1.0, 0.0), true)]).is_empty());
        assert_eq!(t.update(0.1, near, [(3, v(1.5, 0.0), true)]), [v(1.5, 0.0)]);
    }

    #[test]
    fn the_last_ten_seconds_tick_once_per_second() {
        let mut before = None;
        let mut ticks = Vec::new();
        let mut left = 15.0f32;
        while left > 0.0 {
            let after = (left - 1.0 / 60.0).max(0.0);
            if low_time_tick(before, after) {
                ticks.push(after.ceil() as u32);
            }
            before = Some(after);
            left = after;
        }
        assert_eq!(ticks, [10, 9, 8, 7, 6, 5, 4, 3, 2, 1]);
    }

    #[test]
    fn low_time_ticks_need_time_to_pass_inside_the_window() {
        assert!(!low_time_tick(None, 9.5), "a run's first frame");
        assert!(!low_time_tick(Some(9.5), 9.5), "paused");
        assert!(!low_time_tick(Some(3.0), 60.0), "restarted");
        assert!(!low_time_tick(Some(0.2), 0.0), "the end sting covers zero");
        assert!(!low_time_tick(Some(30.5), 30.4), "outside the window");
        assert!(low_time_tick(Some(10.01), 9.99));
        // A long frame that skips several seconds ticks once.
        assert!(low_time_tick(Some(8.5), 5.5));
    }
}
