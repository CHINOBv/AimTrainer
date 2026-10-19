//! Frame-time statistics: average FPS, 1% low and frame time, for the system check (the last
//! few seconds) and for each run's settings snapshot (the whole run).

use bevy::prelude::*;
use std::collections::VecDeque;

/// Frames kept for the system check (about 2–4 s at typical frame rates).
const WINDOW: usize = 512;
/// Frames kept for one run: 10 minutes at 240 FPS.
pub const RUN_WINDOW: usize = 144_000;
/// Fewest frames for a meaningful summary.
const MIN_FRAMES: usize = 30;

#[derive(Resource, Debug)]
pub struct FrameStats {
    deltas: VecDeque<f32>,
    window: usize,
}

impl Default for FrameStats {
    fn default() -> Self {
        Self::with_window(WINDOW)
    }
}

impl FrameStats {
    /// Statistics over the last `window` frames.
    pub fn with_window(window: usize) -> Self {
        Self {
            deltas: VecDeque::new(),
            window: window.max(1),
        }
    }

    pub fn push(&mut self, dt: f32) {
        if dt > 0.0 && dt.is_finite() {
            if self.deltas.len() == self.window {
                self.deltas.pop_front();
            }
            self.deltas.push_back(dt);
        }
    }

    /// `(average fps, 1% low fps, average frame time ms)` once enough frames exist.
    pub fn summary(&self) -> Option<(f32, f32, f32)> {
        if self.deltas.len() < MIN_FRAMES {
            return None;
        }
        let avg = self.deltas.iter().sum::<f32>() / self.deltas.len() as f32;
        let mut sorted: Vec<f32> = self.deltas.iter().copied().collect();
        sorted.sort_by(|a, b| b.total_cmp(a));
        let worst = (sorted.len() / 100).max(1);
        let low = sorted[..worst].iter().sum::<f32>() / worst as f32;
        Some((1.0 / avg, 1.0 / low, avg * 1000.0))
    }
}

pub fn track_frames(time: Res<Time<Real>>, mut stats: ResMut<FrameStats>) {
    stats.push(time.delta_secs());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn averages_and_one_percent_low() {
        let mut s = FrameStats::default();
        assert_eq!(s.summary(), None);
        for i in 0..200 {
            s.push(if i % 100 == 0 { 0.02 } else { 0.004 });
        }
        let (avg, low, ms) = s.summary().unwrap();
        assert!((low - 50.0).abs() < 0.1, "{low}");
        assert!(avg > 200.0 && avg < 250.0, "{avg}");
        assert!((ms - 1000.0 / avg).abs() < 1e-3);
    }

    #[test]
    fn a_window_keeps_only_the_latest_frames() {
        let mut s = FrameStats::with_window(40);
        for _ in 0..40 {
            s.push(0.02);
        }
        for _ in 0..40 {
            s.push(0.005);
        }
        let (avg, low, _) = s.summary().unwrap();
        assert!((avg - 200.0).abs() < 0.1, "{avg}");
        assert!(
            (low - 200.0).abs() < 0.1,
            "the slow frames left the window: {low}"
        );
        assert_eq!(FrameStats::default().window, WINDOW);
    }

    #[test]
    fn systems_have_no_conflicting_access() {
        bevy::ecs::system::assert_is_system(track_frames);
    }
}
