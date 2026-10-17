//! Target spawning: samples where a new target appears, keeps it apart from the live
//! ones, and orients it towards the player.

use super::MIN_SPHERE_HEIGHT;
use super::def::{Shape, SpawnArea, TargetsDef};
use super::session::Target;
use crate::fov;
use crate::games::valorant::HumanoidDims;
use crate::math::{Range, Vec3, ViewAngles, angle_between_deg};
use crate::rng::Rng;

const SPAWN_ATTEMPTS: usize = 24;

/// A spawn point for a new target, resampled up to [`SPAWN_ATTEMPTS`] times until it is at
/// least `min_separation_deg` away (seen from `eye`) from every existing target.
pub(super) fn separated_point(
    def: &TargetsDef,
    existing: &[Target],
    eye: Vec3,
    view: ViewAngles,
    dims: &HumanoidDims,
    rng: &mut Rng,
) -> Vec3 {
    let sep = def.min_separation_deg;
    let mut point = sample_point(def, eye, view, rng);
    for _ in 1..SPAWN_ATTEMPTS {
        let dir = aim_point(&def.shape, dims, point) - eye;
        let far_enough = existing
            .iter()
            .all(|t| angle_between_deg(dir, aim_point(&def.shape, dims, t.position) - eye) >= sep);
        if far_enough {
            break;
        }
        point = sample_point(def, eye, view, rng);
    }
    point
}

/// The yaw a target at `point` faces to look at the player, and its horizontal lateral axis.
pub(super) fn face_player(point: Vec3, player_pos: Vec3) -> (f32, Vec3) {
    let to_player = (player_pos - point).horizontal();
    let facing_yaw_deg = to_player.x.atan2(to_player.z).to_degrees();
    let lateral = Vec3::UP.cross(to_player).normalize_or_zero();
    let lateral = if lateral == Vec3::ZERO {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        lateral
    };
    (facing_yaw_deg, lateral)
}

fn sample_point(def: &TargetsDef, eye: Vec3, view: ViewAngles, rng: &mut Rng) -> Vec3 {
    let humanoid = matches!(def.shape, Shape::Humanoid { .. });
    let mut p = match def.spawn {
        SpawnArea::Angular {
            yaw_deg,
            pitch_deg,
            distance,
        } => {
            let yaw = rng.in_range(yaw_deg);
            let pitch = if humanoid {
                0.0
            } else {
                rng.in_range(pitch_deg)
            };
            eye + ViewAngles::new(yaw, pitch).direction() * rng.in_range(distance)
        }
        SpawnArea::AroundCrosshair {
            offset_deg,
            distance,
            pitch_limit_deg,
            on_screen,
        } => {
            let offset = rng.in_range(offset_deg);
            let dist = rng.in_range(distance);
            if humanoid {
                // Humanoids only use a horizontal offset; `validate()` keeps it within the
                // on-screen limit when `on_screen` is set.
                let yaw = view.yaw_deg + offset * rng.sign();
                eye + ViewAngles::new(yaw, 0.0).direction() * dist
            } else {
                let band = SphereBand {
                    pitch: pitch_limit_deg,
                    on_screen,
                };
                crosshair_sphere_point(eye, view, offset, dist, band, rng)
            }
        }
        SpawnArea::Floor { x, z, height } => {
            Vec3::new(rng.in_range(x), rng.in_range(height), rng.in_range(z))
        }
    };
    if humanoid {
        p.y = 0.0;
    } else {
        p.y = p.y.max(MIN_SPHERE_HEIGHT);
    }
    p
}

/// Where a sphere spawned around the crosshair may land.
struct SphereBand {
    /// Absolute pitch band, degrees from the horizon.
    pitch: Range,
    /// Also stay inside VALORANT's view from the current aim (minus the margin).
    on_screen: bool,
}

/// A point `offset` degrees away from the current aim, in a random direction, that stays
/// inside the pitch band and above the floor. Each spawn is relative to the previous aim,
/// so without the band chained flicks drift into the sky or the floor. With `on_screen`,
/// directions that would leave the screen are rejected too; that only adds a check, so
/// the random draws are the same as without it.
fn crosshair_sphere_point(
    eye: Vec3,
    view: ViewAngles,
    offset: f32,
    dist: f32,
    band: SphereBand,
    rng: &mut Rng,
) -> Vec3 {
    let limit = band.pitch;
    let (half_h, half_v) = (
        fov::valorant_on_screen_half_h_deg() as f32,
        fov::valorant_on_screen_half_v_deg() as f32,
    );
    let fits = |v: ViewAngles| {
        let p = eye + v.direction() * dist;
        let in_band = (limit.min..=limit.max).contains(&v.pitch_deg);
        let visible =
            !band.on_screen || fov::offscreen_bearing(view, eye, p, half_h, half_v).is_none();
        (in_band && visible && p.y >= MIN_SPHERE_HEIGHT).then_some(p)
    };
    for _ in 0..SPAWN_ATTEMPTS {
        let phi = rng.next_f32() * std::f32::consts::TAU;
        let (d_yaw, d_pitch) = (offset * phi.cos(), offset * phi.sin());
        // Try the sampled direction, then its vertical mirror.
        for v in [view.rotated(d_yaw, d_pitch), view.rotated(d_yaw, -d_pitch)] {
            if let Some(p) = fits(v) {
                return p;
            }
        }
    }
    // The aim itself is outside the band (e.g. looking at the sky): flick horizontally
    // and bring the pitch back into the band.
    let yaw = view.yaw_deg + offset * rng.sign();
    let pitch = view.pitch_deg.clamp(limit.min, limit.max);
    let mut p = eye + ViewAngles::new(yaw, pitch).direction() * dist;
    p.y = p.y.max(MIN_SPHERE_HEIGHT);
    p
}

/// Where the player aims to separate targets: a humanoid's head, a sphere's center.
fn aim_point(shape: &Shape, dims: &HumanoidDims, p: Vec3) -> Vec3 {
    match shape {
        Shape::Humanoid { .. } => p + Vec3::new(0.0, dims.head_center_height(), 0.0),
        Shape::Sphere { .. } => p,
    }
}

#[cfg(test)]
mod tests {
    use crate::fov;
    use crate::math::{ViewAngles, angle_between_deg};
    use crate::scenario::{FrameInput, ScenarioDef, Session};

    fn flick_def(shape: &str, offset: &str, on_screen: bool) -> ScenarioDef {
        ScenarioDef::from_toml(&format!(
            r#"
            id = "x"
            name = "x"
            category = "flicking"
            duration_secs = 1000.0
            [targets]
            shape = {shape}
            spawn = {{ kind = "around_crosshair", offset_deg = {offset}, distance = [10, 30], on_screen = {on_screen} }}
            "#
        ))
        .unwrap()
    }

    /// Chain perfect flicks and return, for every respawn, whether its aim point was inside
    /// the on-screen limits seen from the kill view, and its angle from that view.
    fn chained_spawns(def: ScenarioDef, kills: usize) -> Vec<(bool, f32)> {
        let (h, v) = (
            fov::valorant_on_screen_half_h_deg() as f32,
            fov::valorant_on_screen_half_v_deg() as f32,
        );
        let head = matches!(def.targets.shape, crate::scenario::Shape::Humanoid { .. });
        let mut s = Session::new(def, 7);
        let aim = |s: &Session, p: crate::math::Vec3| {
            if head {
                p + crate::math::Vec3::new(0.0, s.dims().head_center_height(), 0.0)
            } else {
                p
            }
        };
        let mut out = Vec::new();
        for _ in 0..kills {
            let t = s.targets().next().expect("a target is alive").clone();
            let eye = s.eye_position();
            let view = ViewAngles::from_direction(aim(&s, t.position) - eye);
            s.update(FrameInput {
                dt: 0.2,
                view,
                fire_pressed: true,
                ..Default::default()
            });
            let next = s.targets().next().expect("respawned").clone();
            if next.id == t.id {
                continue;
            }
            let p = aim(&s, next.position);
            let inside = fov::offscreen_bearing(view, eye, p, h, v).is_none();
            out.push((inside, angle_between_deg(view.direction(), p - eye)));
        }
        out
    }

    const SPHERE: &str = r#"{ kind = "sphere", radius = 0.22 }"#;
    const HUMANOID: &str = r#"{ kind = "humanoid", head_only = true }"#;

    #[test]
    fn on_screen_wide_flicks_always_spawn_inside_the_view() {
        for shape in [SPHERE, HUMANOID] {
            let spawns = chained_spawns(flick_def(shape, "[20, 47]", true), 300);
            assert!(spawns.len() > 250, "{shape}: {}", spawns.len());
            assert!(spawns.iter().all(|(inside, _)| *inside), "{shape}");
            // Still a wide flick: some spawns land near the edge.
            assert!(spawns.iter().any(|(_, a)| *a > 40.0), "{shape}");
        }
    }

    #[test]
    fn without_on_screen_wide_flicks_can_leave_the_view() {
        let spawns = chained_spawns(flick_def(SPHERE, "[20, 70]", false), 300);
        assert!(spawns.iter().any(|(inside, _)| !*inside));
    }
}
