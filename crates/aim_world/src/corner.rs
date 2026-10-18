//! Peek corners: the vertical edges of head-high boxes where an enemy can step out, found
//! from the map geometry, and which of them are exposed to a player's eye.

use crate::cover::COVER_SPOT_OFFSET;
use crate::map::{BoxKind, MapDef};
use aim_core::games::valorant::HumanoidDims;
use aim_core::math::Vec3;

/// Closest a corner may be to the player's eye, horizontally, and still count as exposed,
/// meters. A design choice: the corner the player stands at is not one an enemy appears
/// from.
pub const MIN_CORNER_DISTANCE_M: f32 = 1.0;

/// A vertical edge of a `wall` or `cover` box where an enemy can step out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeekCorner {
    /// The box edge on the floor (`y = 0`).
    pub edge: Vec3,
    /// Outward direction of the edge's X face (-1 or 1).
    pub out_x: f32,
    /// Outward direction of the edge's Z face (-1 or 1).
    pub out_z: f32,
    /// Head center of a standing enemy just around the corner.
    pub head: Vec3,
}

impl PeekCorner {
    /// Whether the edge is on the silhouette of its box as seen from `eye`: the eye sees
    /// exactly one of the edge's two faces, so an enemy hidden behind the other face can
    /// step out here.
    pub fn is_silhouette_from(&self, eye: Vec3) -> bool {
        let sees_x_face = self.out_x * (eye.x - self.edge.x) > 0.0;
        let sees_z_face = self.out_z * (eye.z - self.edge.z) > 0.0;
        sees_x_face != sees_z_face
    }
}

impl MapDef {
    /// Every peek corner of the map: the four vertical edges of each `wall` and `cover`
    /// box, with the head of an enemy standing [`COVER_SPOT_OFFSET`] out from both faces.
    /// `low_cover` is skipped (a standing head shows over it), and so are edges where a
    /// player could not stand (against another box or the floor edge). Deterministic order.
    pub fn peek_corners(&self) -> Vec<PeekCorner> {
        let head_height = HumanoidDims::default().head_center_height();
        let d = COVER_SPOT_OFFSET;
        let mut corners = Vec::new();
        for b in self.boxes.iter().filter(|b| b.kind != BoxKind::LowCover) {
            for (x, out_x) in [(b.min.x, -1.0), (b.max.x, 1.0)] {
                for (z, out_z) in [(b.min.z, -1.0), (b.max.z, 1.0)] {
                    let feet = Vec3::new(x + out_x * d, 0.0, z + out_z * d);
                    if self.can_stand(feet) {
                        corners.push(PeekCorner {
                            edge: Vec3::new(x, 0.0, z),
                            out_x,
                            out_z,
                            head: feet + Vec3::new(0.0, head_height, 0.0),
                        });
                    }
                }
            }
        }
        corners
    }

    /// The corners exposed to `eye`: on the silhouette of their box, at least
    /// [`MIN_CORNER_DISTANCE_M`] away and with the corner's head point in line of sight.
    pub fn exposed_corners<'c>(
        &'c self,
        corners: &'c [PeekCorner],
        eye: Vec3,
    ) -> impl Iterator<Item = &'c PeekCorner> + 'c {
        corners.iter().filter(move |c| {
            (c.head - eye).horizontal().length() >= MIN_CORNER_DISTANCE_M
                && c.is_silhouette_from(eye)
                && self.line_of_sight(eye, c.head)
        })
    }
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

    fn pillar() -> SolidBox {
        solid(BoxKind::Cover, [2.0, 0.0, -4.0], [3.0, 2.5, -3.0])
    }

    #[test]
    fn every_head_high_box_has_four_corners_at_standing_head_height() {
        let crate_ = solid(BoxKind::LowCover, [-4.0, 0.0, -4.0], [-3.0, 1.0, -3.0]);
        let m = map(vec![pillar(), crate_]);
        let corners = m.peek_corners();
        assert_eq!(corners.len(), 4, "the low crate has none: {corners:?}");
        let head = HumanoidDims::default().head_center_height();
        for c in &corners {
            assert!((c.head.y - head).abs() < 1e-5, "{c:?}");
            // Diagonally out from the edge, clear of the box.
            assert!((c.head.x - (c.edge.x + c.out_x * COVER_SPOT_OFFSET)).abs() < 1e-5);
            assert!((c.head.z - (c.edge.z + c.out_z * COVER_SPOT_OFFSET)).abs() < 1e-5);
            assert!(m.can_stand(Vec3::new(c.head.x, 0.0, c.head.z)));
        }
        let edges: Vec<(f32, f32)> = corners.iter().map(|c| (c.edge.x, c.edge.z)).collect();
        assert_eq!(edges, [(2.0, -4.0), (2.0, -3.0), (3.0, -4.0), (3.0, -3.0)]);
    }

    #[test]
    fn corners_against_another_box_or_the_floor_edge_are_dropped() {
        // A wall flush with the west floor edge: its two west corners have no room.
        let wall = solid(BoxKind::Wall, [-10.0, 0.0, -1.0], [-6.0, 3.0, 1.0]);
        let m = map(vec![wall]);
        let corners = m.peek_corners();
        assert_eq!(corners.len(), 2, "{corners:?}");
        assert!(corners.iter().all(|c| c.edge.x == -6.0 && c.out_x == 1.0));
    }

    #[test]
    fn only_silhouette_corners_in_sight_are_exposed() {
        let m = map(vec![pillar()]);
        let corners = m.peek_corners();
        // The eye sees the pillar's west face (x = 2) and its +Z face (z = -3): the corner
        // between them shows both faces, the far one neither; the other two are exposed.
        let eye = MapDef::eye(Vec3::new(0.0, 0.0, 2.0));
        let exposed: Vec<(f32, f32)> = m
            .exposed_corners(&corners, eye)
            .map(|c| (c.edge.x, c.edge.z))
            .collect();
        assert_eq!(exposed, [(2.0, -4.0), (3.0, -3.0)]);
    }

    #[test]
    fn a_wall_in_between_hides_a_corner_and_a_close_one_does_not_count() {
        let wall = solid(BoxKind::Wall, [-5.0, 0.0, -1.2], [5.0, 3.0, -1.0]);
        let m = map(vec![pillar(), wall]);
        let corners = m.peek_corners();
        let eye = MapDef::eye(Vec3::new(0.0, 0.0, 2.0));
        let behind_wall: Vec<_> = m
            .exposed_corners(&corners, eye)
            .filter(|c| c.edge.z < -2.0)
            .collect();
        assert!(behind_wall.is_empty(), "{behind_wall:?}");

        let close = map(vec![pillar()]);
        let corners = close.peek_corners();
        // Standing right at the pillar's south-east corner.
        let eye = MapDef::eye(Vec3::new(3.6, 0.0, -2.4));
        assert!(
            close
                .exposed_corners(&corners, eye)
                .all(|c| (c.head - eye).horizontal().length() >= MIN_CORNER_DISTANCE_M)
        );
        assert!(
            !close
                .exposed_corners(&corners, eye)
                .any(|c| c.edge == Vec3::new(3.0, 0.0, -3.0))
        );
    }

    #[test]
    fn built_in_maps_have_corners() {
        for m in crate::builtin::all() {
            assert!(m.peek_corners().len() >= 4, "{}", m.id);
        }
    }
}
