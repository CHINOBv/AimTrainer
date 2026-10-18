//! Sensitivity randomiser: an opt-in robustness drill. Every interval the look
//! sensitivity changes to a random multiplier of the player's own, within `[min, max]`,
//! optionally easing from the previous value. It only scales the look; the saved
//! sensitivity never changes.
//!
//! No controlled study supports or rejects sensitivity randomisation (see
//! `docs/training-science.md`), so the drill is off by default and its bounds are
//! estimates.

use crate::rng::Rng;
use serde::{Deserialize, Serialize};

/// Smallest multiplier the drill accepts.
pub const MIN_MULTIPLIER: f64 = 0.5;
/// Largest multiplier the drill accepts.
pub const MAX_MULTIPLIER: f64 = 2.0;
/// Shortest interval between changes, seconds.
pub const MIN_INTERVAL_SECS: f32 = 2.0;

/// The drill's settings. Missing keys take the defaults (off, ×0.8–×1.2 every 10 s).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RandomizerConfig {
    /// The drill runs only when enabled.
    pub enabled: bool,
    /// Lowest multiplier of the base sensitivity.
    pub min: f64,
    /// Highest multiplier of the base sensitivity.
    pub max: f64,
    /// Seconds between changes.
    pub interval_secs: f32,
    /// Seconds to ease from the previous multiplier to the new one; 0 jumps.
    pub smoothing_secs: f32,
}

impl Default for RandomizerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min: 0.8,
            max: 1.2,
            interval_secs: 10.0,
            smoothing_secs: 0.0,
        }
    }
}

impl RandomizerConfig {
    /// The config with bounds inside the accepted limits and `min ≤ max`; non-finite
    /// values fall back to the defaults.
    pub fn sanitized(&self) -> Self {
        let d = Self::default();
        let finite = |v: f64, fallback: f64| if v.is_finite() { v } else { fallback };
        let a = finite(self.min, d.min).clamp(MIN_MULTIPLIER, MAX_MULTIPLIER);
        let b = finite(self.max, d.max).clamp(MIN_MULTIPLIER, MAX_MULTIPLIER);
        let interval = if self.interval_secs.is_finite() {
            self.interval_secs.max(MIN_INTERVAL_SECS)
        } else {
            d.interval_secs
        };
        let smoothing = if self.smoothing_secs.is_finite() {
            self.smoothing_secs.clamp(0.0, interval)
        } else {
            0.0
        };
        Self {
            enabled: self.enabled,
            min: a.min(b),
            max: a.max(b),
            interval_secs: interval,
            smoothing_secs: smoothing,
        }
    }
}

/// A running drill: deterministic for a given config and seed.
#[derive(Debug, Clone)]
pub struct SensRandomizer {
    config: RandomizerConfig,
    rng: Rng,
    /// Seconds since the last change.
    since_change: f32,
    from: f64,
    to: f64,
}

impl SensRandomizer {
    /// Starts the drill at a first random multiplier, so it is in bounds from the start.
    pub fn new(config: RandomizerConfig, seed: u64) -> Self {
        let config = config.sanitized();
        let mut rng = Rng::new(seed);
        let first = draw(&config, &mut rng);
        Self {
            config,
            rng,
            since_change: 0.0,
            from: first,
            to: first,
        }
    }

    /// The config in use (sanitized).
    pub fn config(&self) -> &RandomizerConfig {
        &self.config
    }

    /// Advance by `dt` seconds and return the current multiplier.
    pub fn advance(&mut self, dt: f32) -> f64 {
        if dt.is_finite() && dt > 0.0 {
            self.since_change += dt;
            while self.since_change >= self.config.interval_secs {
                self.since_change -= self.config.interval_secs;
                self.from = self.multiplier_at(self.config.interval_secs);
                self.to = draw(&self.config, &mut self.rng);
            }
        }
        self.multiplier()
    }

    /// The current multiplier of the base sensitivity.
    pub fn multiplier(&self) -> f64 {
        self.multiplier_at(self.since_change)
    }

    fn multiplier_at(&self, t: f32) -> f64 {
        let s = self.config.smoothing_secs;
        if s <= 0.0 || t >= s {
            return self.to;
        }
        let k = f64::from(t / s);
        self.from + (self.to - self.from) * k
    }
}

fn draw(config: &RandomizerConfig, rng: &mut Rng) -> f64 {
    config.min + (config.max - config.min) * f64::from(rng.next_f32())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(min: f64, max: f64, interval: f32, smoothing: f32) -> RandomizerConfig {
        RandomizerConfig {
            enabled: true,
            min,
            max,
            interval_secs: interval,
            smoothing_secs: smoothing,
        }
    }

    #[test]
    fn off_by_default() {
        let d = RandomizerConfig::default();
        assert!(!d.enabled);
        assert!(d.min < 1.0 && d.max > 1.0);
        let parsed: RandomizerConfig = toml::from_str("").unwrap();
        assert_eq!(parsed, d);
    }

    #[test]
    fn multipliers_stay_within_bounds() {
        let mut r = SensRandomizer::new(cfg(0.7, 1.3, 2.0, 0.5), 9);
        for _ in 0..5000 {
            let m = r.advance(0.05);
            assert!((0.7..=1.3).contains(&m), "{m}");
        }
    }

    #[test]
    fn changes_only_at_the_interval() {
        let mut r = SensRandomizer::new(cfg(0.5, 2.0, 5.0, 0.0), 3);
        let first = r.multiplier();
        assert_eq!(r.advance(4.9), first, "no change before the interval");
        let second = r.advance(0.2);
        assert_ne!(second, first, "a new value at the interval");
        assert_eq!(r.advance(4.0), second);
    }

    #[test]
    fn smoothing_eases_between_values() {
        let mut jump = SensRandomizer::new(cfg(0.5, 2.0, 4.0, 0.0), 11);
        let mut smooth = SensRandomizer::new(cfg(0.5, 2.0, 4.0, 2.0), 11);
        let start = smooth.multiplier();
        assert_eq!(start, jump.multiplier(), "same seed, same first value");
        let target = jump.advance(4.0);
        assert_ne!(target, start, "the seed draws a new value");
        assert_eq!(
            smooth.advance(4.0),
            start,
            "the ease starts at the old value"
        );
        let mid = smooth.advance(1.0);
        let expected = start + (target - start) * 0.5;
        assert!((mid - expected).abs() < 1e-9, "{mid} vs {expected}");
        assert_eq!(
            smooth.advance(1.0),
            target,
            "the ease ends at the new value"
        );
    }

    #[test]
    fn deterministic_per_seed() {
        let run = |seed| {
            let mut r = SensRandomizer::new(cfg(0.8, 1.2, 2.0, 0.5), seed);
            (0..200).map(|_| r.advance(0.1)).collect::<Vec<_>>()
        };
        assert_eq!(run(5), run(5));
        assert!(
            run(5).windows(2).any(|w| w[0] != w[1]),
            "values change over time"
        );
        assert_ne!(run(5), run(6));
    }

    #[test]
    fn bad_configs_are_sanitized() {
        let c = cfg(1.6, 0.1, 0.0, 99.0).sanitized();
        assert_eq!((c.min, c.max), (MIN_MULTIPLIER, 1.6));
        assert_eq!(c.interval_secs, MIN_INTERVAL_SECS);
        assert_eq!(c.smoothing_secs, MIN_INTERVAL_SECS);
        let nan = cfg(f64::NAN, f64::INFINITY, f32::NAN, f32::NAN).sanitized();
        let d = RandomizerConfig::default();
        assert_eq!((nan.min, nan.max), (d.min, d.max));
        assert_eq!(
            (nan.interval_secs, nan.smoothing_secs),
            (d.interval_secs, 0.0)
        );
    }
}
