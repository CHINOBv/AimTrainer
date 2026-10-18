//! Cover spots: standable points that hug head-high boxes, derived from the map geometry,
//! and the search for the nearest one that hides a bot from a threat.

use crate::map::{BoxKind, DEFAULT_PLAYER_RADIUS, MapDef};
use aim_core::math::Vec3;

/// Distance from a box face to the feet of a cover spot, meters: the body radius plus a
/// small margin, so a bot standing there hugs the box without touching it.
pub const COVER_SPOT_OFFSET: f32 = DEFAULT_PLAYER_RADIUS + 0.15;
/// Longest gap between two cover spots along one face of a box, meters.
pub const COVER_SPOT_SPACING: f32 = 2.0;

impl MapDef {
    /// Cover spots of the map: points along every face of every `wall` and `cover` box, one
    /// at each end of the face (the corner a player peeks from) and more in between, at most
    /// [`COVER_SPOT_SPACING`] apart, all [`COVER_SPOT_OFFSET`] out from the face. `low_cover`
    /// is skipped because it leaves a standing head visible. Spots a player could not stand
    /// on (outside the floor, inside another box) are dropped. The order is deterministic.
    pub fn cover_spots(&self) -> Vec<Vec3> {
        let mut spots = Vec::new();
        for b in self.boxes.iter().filter(|b| b.kind != BoxKind::LowCover) {
            let d = COVER_SPOT_OFFSET;
            let xs = spaced(b.min.x, b.max.x);
            let zs = spaced(b.min.z, b.max.z);
            for &x in &xs {
                spots.push(Vec3::new(x, 0.0, b.min.z - d));
                spots.push(Vec3::new(x, 0.0, b.max.z + d));
            }
            for &z in &zs {
                spots.push(Vec3::new(b.min.x - d, 0.0, z));
                spots.push(Vec3::new(b.max.x + d, 0.0, z));
            }
        }
        spots.retain(|&p| self.can_stand(p));
        spots
    }

    /// The cover spot nearest to `from` (straight-line distance; ties go to the lower index)
    /// where a standing head is out of sight of `threat_eye` and that a bot at `from` can
    /// reach along [`MapDef::path`] within `max_route` meters. `None` when no spot qualifies.
    pub fn cover_from(
        &self,
        spots: &[Vec3],
        threat_eye: Vec3,
        from: Vec3,
        max_route: f32,
    ) -> Option<Vec3> {
        let mut near: Vec<(usize, f32)> = spots
            .iter()
            .enumerate()
            .map(|(i, &s)| (i, (s - from).horizontal().length()))
            .filter(|&(_, d)| d <= max_route)
            .collect();
        near.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        near.into_iter().map(|(i, _)| spots[i]).find(|&spot| {
            !self.visible_head(threat_eye, spot)
                && self
                    .path(from, spot)
                    .is_some_and(|route| route_length(&route) <= max_route)
        })
    }
}

/// Points from `lo` to `hi`, both ends included, at most [`COVER_SPOT_SPACING`] apart.
fn spaced(lo: f32, hi: f32) -> Vec<f32> {
    let n = ((hi - lo) / COVER_SPOT_SPACING).ceil().max(1.0) as usize;
    (0..=n)
        .map(|i| lo + (hi - lo) * i as f32 / n as f32)
        .collect()
}

/// Horizontal length of a route, meters.
pub(crate) fn route_length(route: &[Vec3]) -> f32 {
    route
        .windows(2)
        .map(|w| (w[1] - w[0]).horizontal().length())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::SolidBox;
    use aim_core::math::Range;

    fn solid(kind: BoxKind, min: [f32; 3], max: [f32; 3]) -> SolidBox {
        SolidBox {
            kind,
            min: Vec3::new(min[0], min[1], min[2]),
            max: Vec3::new(max[0], max[1], max[2]),
        }
    }

    fn map(boxes: Vec<SolidBox>) -> MapDef {
        MapDef {
            id: "t".into(),
            name: "T".into(),
            description: String::new(),
            floor_x: Range::new(-10.0, 10.0),
            floor_z: Range::new(-10.0, 10.0),
            boxes,
            spawns: vec![],
            angles: vec![],
            waypoints: vec![],
        }
    }

    /// Horizontal distance from `p` to the footprint of `b`.
    fn gap(p: Vec3, b: &SolidBox) -> f32 {
        let dx = (b.min.x - p.x).max(p.x - b.max.x).max(0.0);
        let dz = (b.min.z - p.z).max(p.z - b.max.z).max(0.0);
        (dx * dx + dz * dz).sqrt()
    }

    #[test]
    fn spots_hug_head_high_boxes_and_skip_low_cover() {
        let pillar = solid(BoxKind::Cover, [2.0, 0.0, 2.0], [3.2, 2.5, 3.2]);
        let crate_ = solid(BoxKind::LowCover, [-4.0, 0.0, -4.0], [-3.0, 1.0, -3.0]);
        let m = map(vec![pillar, crate_]);
        let spots = m.cover_spots();
        // A 1.2 m face gets a spot at each end: two per face, four faces.
        assert_eq!(spots.len(), 8, "{spots:?}");
        for s in &spots {
            assert!(
                m.body_overlaps(*s, DEFAULT_PLAYER_RADIUS).is_none(),
                "{s:?}"
            );
            assert!((gap(*s, &pillar) - COVER_SPOT_OFFSET).abs() < 1e-4, "{s:?}");
            assert!(gap(*s, &crate_) > 3.0, "{s:?} is a low-cover spot");
        }
    }

    #[test]
    fn long_faces_get_evenly_spaced_spots_and_unstandable_ones_are_dropped() {
        // A 10 m wall flush against the west edge of the floor: its west face has no room.
        let wall = solid(BoxKind::Wall, [-10.0, 0.0, -5.0], [-9.6, 3.0, 5.0]);
        let m = map(vec![wall]);
        let spots = m.cover_spots();
        let east: Vec<f32> = spots
            .iter()
            .filter(|s| (s.x - (-9.6 + COVER_SPOT_OFFSET)).abs() < 1e-4)
            .map(|s| s.z)
            .collect();
        assert_eq!(east, [-5.0, -3.0, -1.0, 1.0, 3.0, 5.0]);
        assert!(spots.iter().all(|s| s.x > -10.0), "{spots:?}");
        assert!(spots.iter().all(|&s| m.can_stand(s)));
    }

    /// A 10 m wall across X at z in [-0.2, 0.2]; the threat stands south of it at z = 5.
    fn walled() -> MapDef {
        map(vec![solid(
            BoxKind::Wall,
            [-5.0, 0.0, -0.2],
            [5.0, 3.0, 0.2],
        )])
    }

    #[test]
    fn the_nearest_spot_out_of_the_threats_sight_is_chosen() {
        let m = walled();
        let spots = m.cover_spots();
        let threat = MapDef::eye(Vec3::new(0.0, 0.0, 5.0));
        let from = Vec3::new(7.0, 0.0, -0.7);
        // The east-face spots are closer but in plain sight; the north-east corner hides.
        let spot = m.cover_from(&spots, threat, from, 8.0).unwrap();
        assert_eq!(spot, Vec3::new(5.0, 0.0, -0.2 - COVER_SPOT_OFFSET));
        assert!(!m.visible_head(threat, spot));
    }

    #[test]
    fn no_spot_when_every_hidden_one_is_too_far_or_nothing_hides() {
        let m = walled();
        let spots = m.cover_spots();
        let threat = MapDef::eye(Vec3::new(0.0, 0.0, 5.0));
        assert_eq!(
            m.cover_from(&spots, threat, Vec3::new(7.0, 0.0, -0.7), 1.0),
            None
        );
        let open = map(vec![]);
        assert!(open.cover_spots().is_empty());
        assert_eq!(
            open.cover_from(&spots, threat, Vec3::new(7.0, 0.0, -0.7), 8.0),
            None,
            "an open floor hides nothing"
        );
    }
}
