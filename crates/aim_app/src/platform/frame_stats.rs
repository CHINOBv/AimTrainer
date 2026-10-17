//! Frame-time statistics for the system check: average FPS, 1% low and frame time.

use bevy::prelude::*;
use std::collections::VecDeque;

/// Frames kept for the statistics (about 2–4 s at typical frame rates).
const WINDOW: usize = 512;

#[derive(Resource, Debug, Default)]
pub struct FrameStats {
    deltas: VecDeque<f32>,
}

impl FrameStats {
    pub fn push(&mut self, dt: f32) {
        if dt > 0.0 && dt.is_finite() {
            if self.deltas.len() == WINDOW {
                self.deltas.pop_front();
            }
            self.deltas.push_back(dt);
        }
    }

    /// `(average fps, 1% low fps, average frame time ms)` once enough frames exist.
    pub fn summary(&self) -> Option<(f32, f32, f32)> {
        if self.deltas.len() < 30 {
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
    fn systems_have_no_conflicting_access() {
        bevy::ecs::system::assert_is_system(track_frames);
    }
}
