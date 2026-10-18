//! Miss compass: where missed shots went relative to the target's aim point.
//!
//! Each missed shot at a (non-decoy) target is classified by the crosshair's screen-space
//! offset from the target's aim point (sphere center, humanoid head center) into one of
//! eight compass directions. Mostly-below misses on heads point at a crosshair kept too low.
//! See "Miss compass" in `docs/training-science.md`.

use super::trace::{Sample, tangent_offset_deg};

/// Fewer misses than this give no above/below share for the rules (provisional).
pub const MIN_MISSES_FOR_BIAS: u32 = 10;

/// Width of each compass sector, degrees.
const SECTOR_DEG: f32 = 45.0;

/// One of the eight directions a miss can go, relative to the target's aim point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compass {
    /// Above the aim point.
    Up,
    /// Above and to the right.
    UpRight,
    /// To the right.
    Right,
    /// Below and to the right.
    DownRight,
    /// Below the aim point.
    Down,
    /// Below and to the left.
    DownLeft,
    /// To the left.
    Left,
    /// Above and to the left.
    UpLeft,
}

impl Compass {
    /// Every direction, clockwise from up. [`MissCompass::counts`] uses this order.
    pub const ALL: [Compass; 8] = [
        Compass::Up,
        Compass::UpRight,
        Compass::Right,
        Compass::DownRight,
        Compass::Down,
        Compass::DownLeft,
        Compass::Left,
        Compass::UpLeft,
    ];

    /// Lowercase name, such as "up-left".
    pub fn label(self) -> &'static str {
        match self {
            Compass::Up => "up",
            Compass::UpRight => "up-right",
            Compass::Right => "right",
            Compass::DownRight => "down-right",
            Compass::Down => "down",
            Compass::DownLeft => "down-left",
            Compass::Left => "left",
            Compass::UpLeft => "up-left",
        }
    }

    /// The direction of a screen-space offset, degrees (`.0` positive right, `.1` positive
    /// up), or `None` for a zero offset.
    ///
    /// Each sector is 45° wide, centered on its direction, and includes its counter-clockwise
    /// start edge: right is [−22.5°, 22.5°), up-right [22.5°, 67.5°), up [67.5°, 112.5°) and so
    /// on, measured as `atan2(dy, dx)`.
    pub fn of_offset(offset_deg: (f32, f32)) -> Option<Self> {
        let (dx, dy) = offset_deg;
        if dx == 0.0 && dy == 0.0 {
            return None;
        }
        // Sector 0 is right, then counter-clockwise; shifting by half a sector puts each
        // sector's start edge on a multiple of 45°.
        let angle = dy.atan2(dx).to_degrees();
        let sector = ((angle + SECTOR_DEG / 2.0) / SECTOR_DEG).floor() as i32;
        const COUNTER_CLOCKWISE_FROM_RIGHT: [Compass; 8] = [
            Compass::Right,
            Compass::UpRight,
            Compass::Up,
            Compass::UpLeft,
            Compass::Left,
            Compass::DownLeft,
            Compass::Down,
            Compass::DownRight,
        ];
        Some(COUNTER_CLOCKWISE_FROM_RIGHT[sector.rem_euclid(8) as usize])
    }

    fn index(self) -> usize {
        Compass::ALL
            .iter()
            .position(|&c| c == self)
            .expect("ALL holds every direction")
    }

    fn is_above(self) -> bool {
        matches!(self, Compass::UpLeft | Compass::Up | Compass::UpRight)
    }

    fn is_below(self) -> bool {
        matches!(self, Compass::DownLeft | Compass::Down | Compass::DownRight)
    }
}

/// Missed shots counted per compass direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MissCompass {
    /// Misses per direction, in [`Compass::ALL`] order.
    pub counts: [u32; 8],
}

impl MissCompass {
    /// Count misses from their screen-space offsets to the target; zero offsets are skipped.
    pub fn from_offsets(offsets: impl IntoIterator<Item = (f32, f32)>) -> Self {
        let mut compass = Self::default();
        for d in offsets.into_iter().filter_map(Compass::of_offset) {
            compass.counts[d.index()] += 1;
        }
        compass
    }

    /// Misses that went in direction `d`.
    pub fn count(&self, d: Compass) -> u32 {
        self.counts[d.index()]
    }

    /// Every classified miss.
    pub fn misses(&self) -> u32 {
        self.counts.iter().sum()
    }

    /// Misses above the aim point (up-left, up, up-right).
    pub fn above(&self) -> u32 {
        self.sum_where(Compass::is_above)
    }

    /// Misses below the aim point (down-left, down, down-right).
    pub fn below(&self) -> u32 {
        self.sum_where(Compass::is_below)
    }

    /// Share of misses that went above, once at least [`MIN_MISSES_FOR_BIAS`] were counted.
    pub fn above_share(&self) -> Option<f32> {
        self.share(self.above())
    }

    /// Share of misses that went below, once at least [`MIN_MISSES_FOR_BIAS`] were counted.
    pub fn below_share(&self) -> Option<f32> {
        self.share(self.below())
    }

    /// Add another run's counts (the skill profile pools a category's runs).
    pub fn merge(&mut self, other: &MissCompass) {
        for (a, b) in self.counts.iter_mut().zip(other.counts) {
            *a += b;
        }
    }

    fn sum_where(&self, pick: fn(Compass) -> bool) -> u32 {
        Compass::ALL
            .iter()
            .filter(|&&d| pick(d))
            .map(|&d| self.count(d))
            .sum()
    }

    fn share(&self, part: u32) -> Option<f32> {
        let misses = self.misses();
        (misses >= MIN_MISSES_FOR_BIAS).then(|| part as f32 / misses as f32)
    }
}

/// The miss compass of a run, or `None` when no shot missed a target.
pub fn analyze(samples: &[Sample]) -> Option<MissCompass> {
    let compass = MissCompass::from_offsets(samples.iter().filter_map(|s| {
        let target = s.aimed_target().filter(|t| s.shot && !s.hit && !t.decoy)?;
        // Positive = the crosshair was right of / above the aim point.
        Some(tangent_offset_deg(target.aim_point, s.view))
    }));
    (compass.misses() > 0).then_some(compass)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::test_support::record;
    use crate::analytics::trace::{TargetSample, trace};
    use crate::math::ViewAngles;
    use crate::sim::AimerProfile;

    /// Unit offset at `deg` degrees counter-clockwise from right.
    fn at(deg: f32) -> (f32, f32) {
        let r = deg.to_radians();
        (r.cos(), r.sin())
    }

    #[test]
    fn the_eight_directions_are_centered_on_their_axis() {
        let got: Vec<Option<Compass>> = [90.0, 45.0, 0.0, -45.0, -90.0, -135.0, 180.0, 135.0]
            .iter()
            .map(|&d| Compass::of_offset(at(d)))
            .collect();
        let want: Vec<Option<Compass>> = Compass::ALL.iter().copied().map(Some).collect();
        assert_eq!(got, want);
        assert_eq!(Compass::of_offset((0.0, 0.0)), None);
    }

    #[test]
    fn each_sector_boundary_splits_at_22_5_degrees_past_its_axis() {
        // (angle just before the edge, angle just after it) going counter-clockwise.
        let edges = [
            (22.4, Compass::Right, Compass::UpRight),
            (67.4, Compass::UpRight, Compass::Up),
            (112.4, Compass::Up, Compass::UpLeft),
            (157.4, Compass::UpLeft, Compass::Left),
            (-157.6, Compass::Left, Compass::DownLeft),
            (-112.6, Compass::DownLeft, Compass::Down),
            (-67.6, Compass::Down, Compass::DownRight),
            (-22.6, Compass::DownRight, Compass::Right),
        ];
        for (before, below_edge, above_edge) in edges {
            assert_eq!(
                Compass::of_offset(at(before)),
                Some(below_edge),
                "{before}°"
            );
            assert_eq!(
                Compass::of_offset(at(before + 0.2)),
                Some(above_edge),
                "{}°",
                before + 0.2
            );
        }
        // Straight left is left whichever side of ±180° it comes from.
        assert_eq!(Compass::of_offset((-1.0, -0.0)), Some(Compass::Left));
        assert_eq!(Compass::of_offset((-1.0, 0.0)), Some(Compass::Left));
    }

    #[test]
    fn counts_split_into_above_and_below_with_a_minimum_for_the_shares() {
        let mut offsets = vec![at(90.0); 6];
        offsets.extend([at(-90.0), at(-45.0), at(-135.0)]);
        let c = MissCompass::from_offsets(offsets.iter().copied());
        assert_eq!(c.count(Compass::Up), 6);
        assert_eq!((c.misses(), c.above(), c.below()), (9, 6, 3));
        assert_eq!(c.below_share(), None, "9 misses is under the minimum");
        offsets.push(at(0.0));
        let c = MissCompass::from_offsets(offsets);
        assert_eq!(c.misses(), MIN_MISSES_FOR_BIAS);
        assert_eq!(c.above_share(), Some(0.6));
        assert_eq!(c.below_share(), Some(0.3));
        let mut sum = c;
        sum.merge(&c);
        assert_eq!(sum.count(Compass::Up), 12);
        assert_eq!(sum.misses(), 20);
    }

    fn shot_at(target: TargetSample, view: ViewAngles, hit: bool) -> Sample {
        Sample {
            view,
            targets: vec![target],
            aimed: Some(0),
            shot: true,
            hit,
            ..Default::default()
        }
    }

    fn target(aim_point: ViewAngles, decoy: bool) -> TargetSample {
        TargetSample {
            id: 1,
            aim_point,
            radius_deg: 1.0,
            visible_at_secs: 0.0,
            decoy,
        }
    }

    #[test]
    fn only_missed_shots_at_real_targets_are_counted_from_the_aim_point() {
        let t = target(ViewAngles::new(10.0, 5.0), false);
        let samples = vec![
            // Below and left of the aim point: a down-left miss.
            shot_at(t, ViewAngles::new(8.0, 3.0), false),
            // Straight above.
            shot_at(t, ViewAngles::new(10.0, 7.0), false),
            // A hit is not a miss.
            shot_at(t, ViewAngles::new(10.0, 2.0), true),
            // A shot at a go/no-go decoy belongs to the decision analysis.
            shot_at(
                target(ViewAngles::new(0.0, 0.0), true),
                ViewAngles::new(0.0, -3.0),
                false,
            ),
            // No shot this frame.
            Sample {
                shot: false,
                ..shot_at(t, ViewAngles::new(10.0, 0.0), false)
            },
        ];
        let c = analyze(&samples).expect("two misses");
        assert_eq!(c.misses(), 2);
        assert_eq!(c.count(Compass::DownLeft), 1);
        assert_eq!(c.count(Compass::Up), 1);
        assert_eq!(analyze(&samples[2..3]), None, "no miss, no compass");
    }

    #[test]
    fn humanoid_misses_are_measured_from_the_head_and_sphere_misses_from_the_center() {
        for id in ["flick-headshot", "flick-wide"] {
            let samples = trace(&record(id, AimerProfile::PRESETS[0], 5));
            let c = analyze(&samples).unwrap_or_else(|| panic!("{id}: recruit misses"));
            let misses = samples
                .iter()
                .filter(|s| s.shot && !s.hit && s.aimed_target().is_some_and(|t| !t.decoy))
                .count() as u32;
            assert_eq!(c.misses(), misses, "{id}");
            // Re-derive one miss by hand from the aim point the trace exposes.
            let s = samples.iter().find(|s| s.shot && !s.hit).expect("a miss");
            let t = s.aimed_target().expect("aimed");
            let one = MissCompass::from_offsets([tangent_offset_deg(t.aim_point, s.view)]);
            assert_eq!(one.misses(), 1, "{id}");
        }
    }
}
