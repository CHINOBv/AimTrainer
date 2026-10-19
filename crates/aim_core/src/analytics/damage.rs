//! Time-to-kill spread and damage efficiency of a run, recovered by re-simulating its
//! [`Recording`]. Definitions and their limits are in `docs/training-science.md`.

use super::stats;
use crate::scenario::{FireMode, Recording, SessionEvent};
use std::collections::{HashMap, HashSet};

/// Fewest kills for a time-to-kill spread: below this the quartiles say little.
pub const MIN_KILLS_FOR_SPREAD: usize = 4;

/// The distribution of per-kill times to kill (target visible to dead).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TtkSpread {
    /// Kills measured.
    pub kills: usize,
    /// Median time to kill, seconds.
    pub median_secs: f32,
    /// First quartile, seconds.
    pub q1_secs: f32,
    /// Third quartile, seconds.
    pub q3_secs: f32,
}

impl TtkSpread {
    /// The spread of `kill_times` (seconds), or `None` with fewer than
    /// [`MIN_KILLS_FOR_SPREAD`] finite times. Quartiles interpolate linearly between the
    /// sorted times (the common "type 7" definition).
    pub fn from_kill_times(kill_times: &[f32]) -> Option<Self> {
        let mut v: Vec<f32> = kill_times
            .iter()
            .copied()
            .filter(|t| t.is_finite())
            .collect();
        if v.len() < MIN_KILLS_FOR_SPREAD {
            return None;
        }
        v.sort_by(f32::total_cmp);
        Some(Self {
            kills: v.len(),
            median_secs: stats::median(&v)?,
            q1_secs: quantile(&v, 0.25),
            q3_secs: quantile(&v, 0.75),
        })
    }

    /// Interquartile range (Q3 − Q1), seconds: how much the time to kill varies.
    pub fn iqr_secs(&self) -> f32 {
        self.q3_secs - self.q1_secs
    }
}

/// Quantile `p` of `sorted` (ascending, non-empty) by linear interpolation.
fn quantile(sorted: &[f32], p: f32) -> f32 {
    let pos = p * (sorted.len() - 1) as f32;
    let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
    sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f32)
}

/// Damage dealt against the damage the targets offered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamageEfficiency {
    /// Damage dealt: hits (click) or seconds on target (hold).
    pub done: f32,
    /// Damage available: the health of the targets presented (click) or the seconds a
    /// target was up (hold).
    pub possible: f32,
}

impl DamageEfficiency {
    /// `done / possible`, at most 1.
    pub fn ratio(&self) -> f32 {
        (self.done / self.possible).min(1.0)
    }
}

/// The run's time-to-kill spread and damage efficiency.
///
/// Damage efficiency applies to hold runs and to click runs with multi-hit targets
/// (starting health above 1); it is `None` for one-hit click runs, where it would only
/// repeat the kill count.
/// - Hold: seconds on target ÷ seconds during which at least one target was visible.
/// - Click: hits ÷ the health of every target presented. A killed or expired target offers
///   its whole health (rounded up to whole hits); a target still up at the end offers only
///   the damage it took, so the run's end costs nothing. Misses do not count: accuracy
///   covers them.
pub fn analyze(recording: &Recording) -> (Option<TtkSpread>, Option<DamageEfficiency>) {
    let hold = recording.def.weapon.fire == FireMode::Hold;
    let mut s = recording.start();
    let mut visible_secs = 0.0;
    // Click: starting health of every target seen, and which of them left play.
    let mut presented: HashMap<u64, f32> = HashMap::new();
    let mut ended: HashSet<u64> = HashSet::new();
    for input in &recording.inputs {
        let now = s.time();
        let mut any_visible = false;
        for t in s.targets().filter(|t| !t.decoy && t.is_visible(now)) {
            any_visible = true;
            presented.entry(t.id).or_insert(t.max_health);
        }
        for event in s.update(*input) {
            if let SessionEvent::Killed { id, .. } | SessionEvent::Expired { id } = event {
                ended.insert(id);
            }
        }
        if any_visible {
            visible_secs += s.time() - now;
        }
    }
    let ttk = TtkSpread::from_kill_times(&s.stats().kill_times);
    let efficiency = if hold {
        Some(DamageEfficiency {
            done: s.stats().on_target_secs,
            possible: visible_secs,
        })
    } else if presented.values().any(|&h| h > 1.0) {
        let alive: HashMap<u64, f32> = s.targets().map(|t| (t.id, t.health)).collect();
        let possible = presented
            .iter()
            .map(|(id, &max)| match alive.get(id) {
                Some(&health) if !ended.contains(id) => (max - health).max(0.0),
                _ => max.ceil(),
            })
            .sum();
        Some(DamageEfficiency {
            done: s.stats().hits as f32,
            possible,
        })
    } else {
        None
    };
    (ttk, efficiency.filter(|e| e.possible > 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::ViewAngles;
    use crate::scenario::{FrameInput, ScenarioDef, Session};

    #[test]
    fn ttk_spread_is_the_median_and_quartiles_of_the_kill_times() {
        let s = TtkSpread::from_kill_times(&[1.0, 0.2, 0.8, 0.4, 0.6]).unwrap();
        assert_eq!(s.kills, 5);
        assert!((s.median_secs - 0.6).abs() < 1e-6);
        assert!((s.q1_secs - 0.4).abs() < 1e-6);
        assert!((s.q3_secs - 0.8).abs() < 1e-6);
        assert!((s.iqr_secs() - 0.4).abs() < 1e-6);
        // Four times: Q1 and Q3 fall between samples.
        let s = TtkSpread::from_kill_times(&[0.1, 0.2, 0.3, 0.5]).unwrap();
        assert!((s.median_secs - 0.25).abs() < 1e-6);
        assert!((s.q1_secs - 0.175).abs() < 1e-6);
        assert!((s.q3_secs - 0.35).abs() < 1e-6);
        assert_eq!(TtkSpread::from_kill_times(&[0.3, 0.4, 0.5]), None);
        assert_eq!(
            TtkSpread::from_kill_times(&[0.3, 0.4, 0.5, f32::NAN]),
            None,
            "non-finite times are ignored"
        );
    }

    const DT: f32 = 1.0 / 100.0;

    /// One static target straight ahead at 10 m.
    fn ahead(fire: &str, health: &str, lifetime: &str) -> ScenarioDef {
        let def = ScenarioDef::from_toml(&format!(
            r#"
            id = "t"
            name = "t"
            category = "custom"
            duration_secs = 10
            [weapon]
            fire = "{fire}"
            [targets]
            shape = {{ kind = "sphere", radius = 0.3 }}
            spawn = {{ kind = "angular", yaw_deg = [0, 0], pitch_deg = [0, 0], distance = [10, 10] }}
            health = {health}
            {lifetime}
            "#
        ))
        .unwrap();
        assert!(def.validate().is_empty(), "{:?}", def.validate());
        def
    }

    /// Record `frames` frames; `act(frame, session)` returns (aim at the target, fire).
    fn record(
        def: ScenarioDef,
        frames: usize,
        act: impl Fn(usize, &Session) -> (bool, bool),
    ) -> Recording {
        let mut rec = Recording::new(def, 1);
        let mut live = rec.start();
        for f in 0..frames {
            let (aim, fire) = act(f, &live);
            let view = match live.targets().next().filter(|_| aim) {
                Some(t) => ViewAngles::from_direction(t.position - live.eye_position()),
                None => ViewAngles::new(90.0, 0.0),
            };
            let input = FrameInput {
                dt: DT,
                view,
                fire_pressed: fire,
                fire_held: fire,
                movement: Default::default(),
            };
            rec.inputs.push(input);
            live.update(input);
        }
        rec
    }

    #[test]
    fn hold_efficiency_is_time_on_target_over_time_a_target_was_up() {
        // On target for the first 200 frames, looking away for the next 200.
        let rec = record(ahead("hold", "inf", ""), 400, |f, _| (f < 200, true));
        let (ttk, e) = analyze(&rec);
        assert_eq!(ttk, None, "an endless target is never killed");
        let e = e.expect("hold runs always get an efficiency");
        assert!((e.ratio() - 0.5).abs() < 0.02, "{e:?}");
    }

    #[test]
    fn click_efficiency_counts_health_left_on_expired_targets_but_not_misses() {
        // Three-hit targets that live 1 s: one hit and one miss each, then they expire. The
        // third is still up when the recording stops, so it offers only the hit it took.
        let def = ahead("click", "3", "lifetime_secs = 1.0");
        let rec = record(def.clone(), 280, |f, _| {
            let phase = f % 100;
            ((5..15).contains(&phase), phase == 10 || phase == 50)
        });
        let (_, e) = analyze(&rec);
        let e = e.expect("multi-hit click runs get an efficiency");
        assert_eq!((e.done, e.possible), (3.0, 7.0), "{e:?}");
        assert_eq!(e.ratio(), 3.0 / 7.0);

        // Killing every target scores 1, whatever the misses.
        let rec = record(def, 300, |f, _| (f % 30 < 15, f % 15 == 0));
        let (_, e) = analyze(&rec);
        let e = e.unwrap();
        assert!(e.done > 0.0);
        assert_eq!(e.ratio(), 1.0, "{e:?}");
    }

    #[test]
    fn one_hit_click_runs_get_no_efficiency_and_kills_get_a_spread() {
        let rec = record(ahead("click", "1", ""), 300, |f, _| (true, f % 30 == 0));
        let (ttk, e) = analyze(&rec);
        assert_eq!(e, None);
        let ttk = ttk.expect("ten kills");
        assert_eq!(ttk.kills, 10);
        assert!(ttk.median_secs >= 0.0 && ttk.iqr_secs() >= 0.0);
    }
}
