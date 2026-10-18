//! Pace and stamina: how the scoring rate changes over one run.
//!
//! The run is split into [`WINDOWS`] equal time windows. Each window's rate is kills per
//! second for click scenarios and score per second for hold (tracking) scenarios. The
//! opening window is compared with the rest of the run (slow or strong start), and the
//! last [`EDGE_WINDOWS`] windows with the first ones (stamina). Every ratio and threshold
//! here is provisional; see "Pace and stamina" in `docs/training-science.md`.

use super::stats::mean;
use super::trace::Sample;

/// Number of equal time windows a run is split into.
pub const WINDOWS: usize = 6;
/// Windows at each end of the run compared by the stamina ratio.
pub const EDGE_WINDOWS: usize = 2;
/// Fewer kills than this in a click run give no pace analysis (two per window on
/// average; fewer make one window's rate mostly noise).
pub const MIN_KILLS: u32 = 12;
/// Runs shorter than this give no pace analysis (2 s per window).
pub const MIN_DURATION_SECS: f32 = 12.0;
/// An opening-window rate below this share of the rest of the run is a slow start.
pub const SLOW_START_RATIO: f32 = 0.70;
/// An opening-window rate above this share of the rest of the run is a strong start.
pub const STRONG_START_RATIO: f32 = 1.30;
/// A late rate below this share of the early rate is a stamina drop.
pub const STAMINA_DROP_RATIO: f32 = 0.80;

/// What a pace window counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaceUnit {
    /// Kills (click scenarios).
    Kills,
    /// Score points (hold scenarios, where score accrues while on target).
    Score,
}

impl PaceUnit {
    /// Plural noun of the unit, for labels.
    pub fn noun(self) -> &'static str {
        match self {
            PaceUnit::Kills => "kills",
            PaceUnit::Score => "points",
        }
    }
}

/// One time window of a run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaceWindow {
    /// Window start, seconds from the start of the run.
    pub start_secs: f32,
    /// Window end, seconds from the start of the run.
    pub end_secs: f32,
    /// Kills or points gained in the window.
    pub amount: f32,
    /// `amount` per second of window.
    pub rate_per_sec: f32,
}

/// Rate per time window of one run.
#[derive(Debug, Clone, PartialEq)]
pub struct PaceAnalysis {
    /// What the windows count.
    pub unit: PaceUnit,
    /// [`WINDOWS`] consecutive windows covering the run, in time order.
    pub windows: Vec<PaceWindow>,
}

fn mean_rate(windows: &[PaceWindow]) -> Option<f32> {
    let rates: Vec<f32> = windows.iter().map(|w| w.rate_per_sec).collect();
    mean(&rates)
}

fn ratio(numerator: Option<f32>, denominator: Option<f32>) -> Option<f32> {
    let d = denominator?;
    (d > 0.0).then(|| numerator.unwrap_or(0.0) / d)
}

impl PaceAnalysis {
    /// The window with the highest rate (the earliest one on a tie).
    pub fn best(&self) -> &PaceWindow {
        self.windows
            .iter()
            .reduce(|a, b| {
                if b.rate_per_sec > a.rate_per_sec {
                    b
                } else {
                    a
                }
            })
            .expect("a pace analysis has windows")
    }

    /// The window with the lowest rate (the earliest one on a tie).
    pub fn worst(&self) -> &PaceWindow {
        self.windows
            .iter()
            .reduce(|a, b| {
                if b.rate_per_sec < a.rate_per_sec {
                    b
                } else {
                    a
                }
            })
            .expect("a pace analysis has windows")
    }

    /// Opening-window rate ÷ mean rate of the other windows. `None` when the rest of the
    /// run scored nothing.
    pub fn start_ratio(&self) -> Option<f32> {
        ratio(
            self.windows.first().map(|w| w.rate_per_sec),
            mean_rate(&self.windows[1..]),
        )
    }

    /// Mean rate of the last [`EDGE_WINDOWS`] windows ÷ mean rate of the first ones.
    /// `None` when the early windows scored nothing.
    pub fn stamina_ratio(&self) -> Option<f32> {
        let n = self.windows.len();
        ratio(
            mean_rate(&self.windows[n - EDGE_WINDOWS..]),
            mean_rate(&self.windows[..EDGE_WINDOWS]),
        )
    }

    /// The opening window was below [`SLOW_START_RATIO`] of the rest of the run.
    pub fn slow_start(&self) -> bool {
        self.start_ratio().is_some_and(|r| r < SLOW_START_RATIO)
    }

    /// The opening window was above [`STRONG_START_RATIO`] of the rest of the run.
    pub fn strong_start(&self) -> bool {
        self.start_ratio().is_some_and(|r| r > STRONG_START_RATIO)
    }

    /// The late windows fell below [`STAMINA_DROP_RATIO`] of the early ones.
    pub fn stamina_drop(&self) -> bool {
        self.stamina_ratio().is_some_and(|r| r < STAMINA_DROP_RATIO)
    }
}

/// Split a traced run into [`WINDOWS`] equal windows and measure the rate in each.
///
/// Returns `None` for runs shorter than [`MIN_DURATION_SECS`], click runs with fewer
/// than [`MIN_KILLS`] kills, and hold runs that scored nothing.
pub fn analyze(samples: &[Sample], unit: PaceUnit) -> Option<PaceAnalysis> {
    let duration_secs = samples.last()?.time_secs;
    if duration_secs < MIN_DURATION_SECS {
        return None;
    }
    let total = |s: &Sample| match unit {
        PaceUnit::Kills => s.kills as f32,
        PaceUnit::Score => s.score,
    };
    let window_secs = duration_secs / WINDOWS as f32;
    let mut amounts = [0.0_f32; WINDOWS];
    let mut previous = 0.0;
    for s in samples {
        let now = total(s);
        // A frame's gain belongs to the window in which the frame started.
        let start = (s.time_secs - s.dt_secs).max(0.0);
        let i = ((start / window_secs) as usize).min(WINDOWS - 1);
        amounts[i] += now - previous;
        previous = now;
    }
    let enough = match unit {
        PaceUnit::Kills => previous >= MIN_KILLS as f32,
        PaceUnit::Score => previous > 0.0,
    };
    enough.then(|| PaceAnalysis {
        unit,
        windows: amounts
            .iter()
            .enumerate()
            .map(|(i, &amount)| PaceWindow {
                start_secs: i as f32 * window_secs,
                end_secs: (i + 1) as f32 * window_secs,
                amount,
                rate_per_sec: amount / window_secs,
            })
            .collect(),
    })
}

/// Pace aggregated over several runs of one category: the mean of each run's ratios.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaceSummary {
    /// Runs with a pace analysis.
    pub runs: usize,
    /// Mean [`PaceAnalysis::start_ratio`] over the runs that have one.
    pub start_ratio: Option<f32>,
    /// Mean [`PaceAnalysis::stamina_ratio`] over the runs that have one.
    pub stamina_ratio: Option<f32>,
}

impl PaceSummary {
    /// Summary of `runs`, or `None` when there are none.
    pub fn from_runs(runs: &[&PaceAnalysis]) -> Option<Self> {
        if runs.is_empty() {
            return None;
        }
        let collect = |f: fn(&PaceAnalysis) -> Option<f32>| -> Vec<f32> {
            runs.iter().filter_map(|p| f(p)).collect()
        };
        Some(Self {
            runs: runs.len(),
            start_ratio: mean(&collect(PaceAnalysis::start_ratio)),
            stamina_ratio: mean(&collect(PaceAnalysis::stamina_ratio)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::test_support::record;
    use crate::analytics::trace::trace;
    use crate::sim::AimerProfile;

    const FRAME_SECS: f32 = 0.1;

    /// A 60 s run at 10 FPS with `per_window[i]` kills in the middle of window `i`.
    fn run_with_kills(per_window: [u32; WINDOWS]) -> Vec<Sample> {
        let mut kills = 0;
        (0..600)
            .map(|i| {
                let time_secs = (i + 1) as f32 * FRAME_SECS;
                let window = i / 100;
                // One kill per frame, starting at frame 50 of the window (its middle).
                if (50..50 + per_window[window] as usize).contains(&(i % 100)) {
                    kills += 1;
                }
                Sample {
                    time_secs,
                    dt_secs: FRAME_SECS,
                    kills,
                    score: kills as f32 * 100.0,
                    ..Default::default()
                }
            })
            .collect()
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn a_run_is_split_into_equal_windows_with_a_rate_each() {
        let pace = analyze(&run_with_kills([1, 4, 4, 4, 4, 4]), PaceUnit::Kills).unwrap();
        assert_eq!(pace.unit, PaceUnit::Kills);
        assert_eq!(pace.windows.len(), WINDOWS);
        for (i, w) in pace.windows.iter().enumerate() {
            assert!(close(w.start_secs, i as f32 * 10.0), "{w:?}");
            assert!(close(w.end_secs, (i + 1) as f32 * 10.0), "{w:?}");
        }
        let amounts: Vec<f32> = pace.windows.iter().map(|w| w.amount).collect();
        assert_eq!(amounts, vec![1.0, 4.0, 4.0, 4.0, 4.0, 4.0]);
        assert!(close(pace.windows[0].rate_per_sec, 0.1));
        assert!(close(pace.windows[1].rate_per_sec, 0.4));
    }

    #[test]
    fn a_slow_opening_window_is_a_slow_start_and_the_worst_window() {
        let pace = analyze(&run_with_kills([1, 4, 4, 4, 4, 4]), PaceUnit::Kills).unwrap();
        assert!(close(pace.start_ratio().unwrap(), 0.25));
        assert!(close(pace.stamina_ratio().unwrap(), 1.6));
        assert!(pace.slow_start());
        assert!(!pace.strong_start());
        assert!(!pace.stamina_drop());
        assert!(close(pace.worst().start_secs, 0.0));
        // Ties go to the earliest window.
        assert!(close(pace.best().start_secs, 10.0));
        assert!(close(pace.best().end_secs, 20.0));
    }

    #[test]
    fn a_fading_run_is_a_strong_start_and_a_stamina_drop() {
        let pace = analyze(&run_with_kills([4, 4, 4, 4, 2, 1]), PaceUnit::Kills).unwrap();
        // 0.4 kills/s against a mean of 0.3 for the rest.
        assert!(close(pace.start_ratio().unwrap(), 4.0 / 3.0));
        // Last two windows 1.5 kills each against 4 for the first two.
        assert!(close(pace.stamina_ratio().unwrap(), 0.375));
        assert!(pace.strong_start());
        assert!(!pace.slow_start());
        assert!(pace.stamina_drop());
        assert!(close(pace.worst().start_secs, 50.0));
        assert!(close(pace.best().start_secs, 0.0));
    }

    #[test]
    fn an_even_run_raises_no_flag() {
        let pace = analyze(&run_with_kills([3; WINDOWS]), PaceUnit::Kills).unwrap();
        assert!(close(pace.start_ratio().unwrap(), 1.0));
        assert!(close(pace.stamina_ratio().unwrap(), 1.0));
        assert!(!pace.slow_start() && !pace.strong_start() && !pace.stamina_drop());
    }

    #[test]
    fn score_pace_counts_points() {
        let pace = analyze(&run_with_kills([1, 4, 4, 4, 4, 4]), PaceUnit::Score).unwrap();
        assert_eq!(pace.unit, PaceUnit::Score);
        assert!(close(pace.windows[0].amount, 100.0));
        assert!(close(pace.windows[1].rate_per_sec, 40.0));
    }

    #[test]
    fn short_or_sparse_runs_have_no_pace() {
        // MIN_KILLS - 1 kills is too few; MIN_KILLS is enough.
        assert_eq!(
            analyze(&run_with_kills([2, 2, 2, 2, 2, 1]), PaceUnit::Kills),
            None
        );
        assert!(analyze(&run_with_kills([2; WINDOWS]), PaceUnit::Kills).is_some());
        let short: Vec<Sample> = run_with_kills([3; WINDOWS]).into_iter().take(110).collect();
        assert_eq!(analyze(&short, PaceUnit::Kills), None);
        assert_eq!(
            analyze(&run_with_kills([0; WINDOWS]), PaceUnit::Score),
            None
        );
        assert_eq!(analyze(&[], PaceUnit::Kills), None);
    }

    #[test]
    fn simulated_runs_account_for_every_kill_and_point() {
        let rec = record("flick-wide", AimerProfile::PRESETS[2], 3);
        let samples = trace(&rec);
        let stats = rec.replay(|_, _| {}).stats().clone();
        let pace = analyze(&samples, PaceUnit::Kills).expect("enough kills");
        let total: f32 = pace.windows.iter().map(|w| w.amount).sum();
        assert_eq!(total, stats.kills as f32);

        let rec = record("track-precise", AimerProfile::PRESETS[2], 3);
        let samples = trace(&rec);
        let stats = rec.replay(|_, _| {}).stats().clone();
        let pace = analyze(&samples, PaceUnit::Score).expect("scored");
        let total: f32 = pace.windows.iter().map(|w| w.amount).sum();
        assert!((total - stats.score).abs() < 1e-2 * stats.score.max(1.0));
    }

    #[test]
    fn summaries_average_the_runs_ratios() {
        let a = analyze(&run_with_kills([1, 4, 4, 4, 4, 4]), PaceUnit::Kills).unwrap();
        let b = analyze(&run_with_kills([3; WINDOWS]), PaceUnit::Kills).unwrap();
        let s = PaceSummary::from_runs(&[&a, &b]).unwrap();
        assert_eq!(s.runs, 2);
        assert!(close(s.start_ratio.unwrap(), (0.25 + 1.0) / 2.0));
        assert!(close(s.stamina_ratio.unwrap(), (1.6 + 1.0) / 2.0));
        assert_eq!(PaceSummary::from_runs(&[]), None);
    }
}
