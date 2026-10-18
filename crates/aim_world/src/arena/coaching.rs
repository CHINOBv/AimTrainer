//! Live coaching values for an arena run: crosshair placement against head height at the
//! nearest exposed corner, and movement accuracy. Pure reads of the session; the app only
//! draws them.

use aim_core::math::Vec3;

/// Largest head-level error, degrees, that still counts as "at head level". An estimate:
/// about the angular height of a head (0.25 m) at 15 m, the middle of a typical rifle duel.
pub const COACH_HEAD_LEVEL_TOLERANCE_DEG: f32 = 1.0;
/// Largest angle from the crosshair to a corner, degrees, that counts as holding it. The
/// same 5° the pre-aim drill uses as its tolerance (`docs/maps.md`, crosshair placement).
pub const COACH_CORNER_TOLERANCE_DEG: f32 = 5.0;

/// What the live coaching overlay shows, computed from the session this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coaching {
    /// Head center of an enemy at the exposed corner nearest to the crosshair (smallest
    /// angle), or `None` when no corner is exposed.
    pub corner: Option<Vec3>,
    /// Angle from the crosshair to that corner's head point, degrees.
    pub corner_offset_deg: Option<f32>,
    /// Crosshair pitch minus the pitch of that corner's head point, degrees: positive is
    /// above head level, negative below.
    pub head_level_error_deg: Option<f32>,
    /// The player's speed, m/s.
    pub speed: f32,
    /// Whether the player is slow enough for the weapon's first-shot accuracy.
    pub accurate: bool,
}

impl Coaching {
    /// Whether the crosshair is at head level for the nearest corner, if there is one.
    pub fn at_head_level(&self) -> Option<bool> {
        self.head_level_error_deg
            .map(|e| e.abs() <= COACH_HEAD_LEVEL_TOLERANCE_DEG)
    }

    /// Whether the crosshair holds the nearest corner, if there is one.
    pub fn on_corner(&self) -> Option<bool> {
        self.corner_offset_deg
            .map(|a| a <= COACH_CORNER_TOLERANCE_DEG)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::{ArenaDef, ArenaSession};
    use crate::map::MapDef;
    use aim_core::math::ViewAngles;
    use aim_core::movement::MoveInput;
    use aim_core::scenario::FrameInput;

    const DT: f32 = 1.0 / 60.0;

    /// The player at z = 2 with a pillar ahead to the right, a bot spawn far behind it.
    fn session() -> ArenaSession {
        let map = MapDef::load(
            r#"
id = "corner-room"
name = "Corner Room"
floor_x = [-10.0, 10.0]
floor_z = [-10.0, 10.0]

[[boxes]]
kind = "cover"
min = [2.0, 0.0, -4.0]
max = [3.0, 2.5, -3.0]

[[spawns]]
id = "p"
kind = "player"
position = [0.0, 0.0, 2.0]

[[spawns]]
id = "b"
kind = "bot"
position = [8.0, 0.0, -9.0]
"#,
        )
        .unwrap();
        let def = ArenaDef::load(
            r#"
id = "c"
name = "C"
map = "corner-room"
duration_secs = 10.0
weapon = "vandal"

[mode]
kind = "deathmatch"
bots = 1
bot_skill = "recruit"
respawn_secs = 1.0
bots_shoot_back = false
"#,
        )
        .unwrap();
        ArenaSession::new(def, map, 1).unwrap()
    }

    fn look(s: &mut ArenaSession, view: ViewAngles, right: f32) {
        s.update(FrameInput {
            dt: DT,
            view,
            movement: MoveInput {
                right,
                ..Default::default()
            },
            ..Default::default()
        });
    }

    /// Head point of the exposed corner at the pillar's (2, -4) edge, seen from the spawn.
    fn far_corner_head() -> Vec3 {
        Vec3::new(
            2.0 - crate::COVER_SPOT_OFFSET,
            0.0,
            -4.0 - crate::COVER_SPOT_OFFSET,
        ) + Vec3::new(
            0.0,
            aim_core::games::valorant::HumanoidDims::default().head_center_height(),
            0.0,
        )
    }

    #[test]
    fn a_crosshair_on_the_corner_at_head_height_scores_zero() {
        let mut s = session();
        let to = far_corner_head() - s.eye_position();
        look(&mut s, ViewAngles::from_direction(to), 0.0);
        let c = s.coaching();
        assert_eq!(c.corner, Some(far_corner_head()));
        assert!(c.corner_offset_deg.unwrap() < 0.01, "{c:?}");
        assert!(c.head_level_error_deg.unwrap().abs() < 0.01, "{c:?}");
        assert_eq!((c.at_head_level(), c.on_corner()), (Some(true), Some(true)));
        assert!(c.accurate && c.speed == 0.0);
    }

    #[test]
    fn a_low_crosshair_reads_below_head_level() {
        let mut s = session();
        let mut view = ViewAngles::from_direction(far_corner_head() - s.eye_position());
        view.pitch_deg -= 3.0;
        look(&mut s, view, 0.0);
        let c = s.coaching();
        assert_eq!(
            c.corner,
            Some(far_corner_head()),
            "still the nearest corner"
        );
        assert!(
            (c.head_level_error_deg.unwrap() + 3.0).abs() < 0.01,
            "{c:?}"
        );
        assert_eq!(c.at_head_level(), Some(false));
        assert_eq!(
            c.on_corner(),
            Some(true),
            "3 degrees is within the corner tolerance"
        );
    }

    #[test]
    fn the_corner_nearest_the_crosshair_is_chosen_and_moving_is_flagged() {
        let mut s = session();
        // Looking at the other exposed corner, (3, -3), while strafing.
        let other = Vec3::new(3.5, 1.0, -2.5);
        for _ in 0..30 {
            let to = other - s.eye_position();
            look(&mut s, ViewAngles::from_direction(to), 1.0);
        }
        let c = s.coaching();
        let corner = c.corner.unwrap();
        assert!(
            (corner.x - 3.5).abs() < 1e-4 && (corner.z + 2.5).abs() < 1e-4,
            "{c:?}"
        );
        assert!(!c.accurate && c.speed > 1.0, "{c:?}");
    }

    #[test]
    fn coaching_reads_do_not_change_the_run() {
        let run = |read: bool| {
            let mut s = session();
            for i in 0..300 {
                look(&mut s, ViewAngles::new(i as f32 * 0.5, 0.0), 0.0);
                if read {
                    s.coaching();
                }
            }
            (s.result(), s.stats().clone())
        };
        assert_eq!(run(true), run(false));
    }
}
