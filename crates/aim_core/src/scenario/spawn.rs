//! Target spawning: samples where a new target appears, keeps it apart from the live
//! ones, and orients it towards the player.

use super::MIN_SPHERE_HEIGHT;
use super::def::{Shape, SpawnArea, SpawnRules, TargetsDef};
use super::session::Target;
use crate::fov;
use crate::games::valorant::HumanoidDims;
use crate::math::{Range, Vec3, ViewAngles, angle_between_deg, wrap_degrees};
use crate::rng::Rng;

const SPAWN_ATTEMPTS: usize = 24;

/// A spawn point for a new target, resampled up to [`SPAWN_ATTEMPTS`] times until it is at
/// least `min_separation_deg` away (seen from `eye`) from every existing target and obeys
/// the scenario's [`SpawnRules`]. `recent` holds the latest spawn points, newest last.
/// Without spawn rules the checks are the separation alone, so the random draws are the
/// same as before spawn rules existed.
pub(super) fn separated_point(
    def: &TargetsDef,
    existing: &[Target],
    recent: &[Vec3],
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
        let ruled = def.spawn_rules.as_ref().is_none_or(|r| {
            let pattern = SpawnPattern {
                rules: r,
                def,
                recent,
                eye,
                view,
                dims,
            };
            pattern.allows(point)
        });
        if far_enough && ruled {
            break;
        }
        point = sample_point(def, eye, view, rng);
    }
    point
}

/// The [`SpawnRules`] of a scenario together with what they are checked against.
struct SpawnPattern<'a> {
    rules: &'a SpawnRules,
    def: &'a TargetsDef,
    recent: &'a [Vec3],
    eye: Vec3,
    view: ViewAngles,
    dims: &'a HumanoidDims,
}

impl SpawnPattern<'_> {
    fn allows(&self, point: Vec3) -> bool {
        let r = self.rules;
        let shape = &self.def.shape;
        let dir = aim_point(shape, self.dims, point) - self.eye;
        let keep = (r.avoid_recent as usize).min(self.recent.len());
        let away_from_recent = self.recent[self.recent.len() - keep..].iter().all(|p| {
            angle_between_deg(dir, aim_point(shape, self.dims, *p) - self.eye) >= r.avoid_last_deg
        });
        let flick = angle_between_deg(self.view.direction(), dir) >= r.min_flick_deg;
        away_from_recent && flick && !self.in_center(point, r.exclude_center)
    }

    /// Whether `point` lies in the central `fraction` of each axis of the spawn area.
    fn in_center(&self, point: Vec3, fraction: f32) -> bool {
        if fraction <= 0.0 {
            return false;
        }
        // Signed position along an axis: -1 at `min`, 0 at the center, 1 at `max`. A zero-
        // width axis is all center.
        let along = |value: f32, r: Range| {
            let half = (r.max - r.min) / 2.0;
            if half > 0.0 {
                (value - (r.min + half)) / half
            } else {
                0.0
            }
        };
        let central = |u: f32| u.abs() < fraction;
        match self.def.spawn {
            SpawnArea::Angular {
                yaw_deg, pitch_deg, ..
            } => {
                let v = ViewAngles::from_direction(point - self.eye);
                let mid = yaw_deg.min + (yaw_deg.max - yaw_deg.min) / 2.0;
                let yaw = mid + wrap_degrees(v.yaw_deg - mid);
                let humanoid = matches!(self.def.shape, Shape::Humanoid { .. });
                // Humanoids stand on the floor and ignore pitch.
                central(along(yaw, yaw_deg)) && (humanoid || central(along(v.pitch_deg, pitch_deg)))
            }
            SpawnArea::Floor { x, z, .. } => {
                central(along(point.x, x)) && central(along(point.z, z))
            }
            // `validate()` rejects `exclude_center` here: `offset_deg.min` does the job.
            SpawnArea::AroundCrosshair { .. } => false,
        }
    }
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
pub(super) fn aim_point(shape: &Shape, dims: &HumanoidDims, p: Vec3) -> Vec3 {
    match shape {
        Shape::Humanoid { .. } => p + Vec3::new(0.0, dims.head_center_height(), 0.0),
        Shape::Sphere { .. } => p,
    }
}

#[cfg(test)]
mod tests {
    use crate::fov;
    use crate::math::{Vec3, ViewAngles, angle_between_deg};
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

    /// One spawn of a kill chain: the view that killed the previous target, the eye, and
    /// the aim point of the target that replaced it.
    struct Respawn {
        kill_view: ViewAngles,
        eye: Vec3,
        aim: Vec3,
    }

    fn area_def(shape: &str, spawn: &str, rules: &str) -> ScenarioDef {
        ScenarioDef::from_toml(&format!(
            r#"
            id = "x"
            name = "x"
            category = "flicking"
            duration_secs = 1000.0
            [targets]
            shape = {shape}
            spawn = {spawn}
            {rules}
            "#
        ))
        .unwrap()
    }

    const ANGULAR: &str =
        r#"{ kind = "angular", yaw_deg = [-40, 40], pitch_deg = [-10, 20], distance = [12, 12] }"#;
    const FLOOR: &str = r#"{ kind = "floor", x = [-6, 6], z = [-20, -10], height = [1.0, 2.0] }"#;

    fn aim_of(s: &Session, p: Vec3) -> Vec3 {
        if matches!(
            s.def().targets.shape,
            crate::scenario::Shape::Humanoid { .. }
        ) {
            p + Vec3::new(0.0, s.dims().head_center_height(), 0.0)
        } else {
            p
        }
    }

    /// Kill `kills` targets in a row with perfect clicks, starting from the first spawn.
    fn kill_chain(def: ScenarioDef, seed: u64, kills: usize) -> (Vec3, Vec<Respawn>) {
        let mut s = Session::new(def, seed);
        let first = aim_of(&s, s.targets().next().expect("a target").position);
        let mut out = Vec::new();
        for _ in 0..kills {
            let t = s.targets().next().expect("a target is alive").clone();
            let eye = s.eye_position();
            let kill_view = ViewAngles::from_direction(aim_of(&s, t.position) - eye);
            s.update(FrameInput {
                dt: 0.2,
                view: kill_view,
                fire_pressed: true,
                ..Default::default()
            });
            let next = s.targets().next().expect("respawned").clone();
            assert_ne!(next.id, t.id, "a perfect click kills");
            out.push(Respawn {
                kill_view,
                eye,
                aim: aim_of(&s, next.position),
            });
        }
        (first, out)
    }

    /// Angles between each spawn and the `back`-th spawn before it.
    fn gaps(first: Vec3, chain: &[Respawn], back: usize) -> Vec<f32> {
        let aims: Vec<Vec3> = std::iter::once(first)
            .chain(chain.iter().map(|r| r.aim))
            .collect();
        let eye = chain[0].eye;
        aims.windows(back + 1)
            .map(|w| angle_between_deg(w[0] - eye, w[back] - eye))
            .collect()
    }

    #[test]
    fn avoid_last_deg_blocks_the_recent_spawn_points() {
        for shape in [SPHERE, HUMANOID] {
            let free = area_def(shape, ANGULAR, "");
            let (first, chain) = kill_chain(free, 3, 200);
            assert!(gaps(first, &chain, 1).iter().any(|g| *g < 10.0), "{shape}");

            let ruled = area_def(
                shape,
                ANGULAR,
                "spawn_rules = { avoid_last_deg = 10, avoid_recent = 2 }",
            );
            let (first, chain) = kill_chain(ruled, 3, 200);
            for back in [1, 2] {
                let g = gaps(first, &chain, back);
                assert!(g.iter().all(|g| *g >= 10.0), "{shape} {back}: {g:?}");
            }
            assert!(
                gaps(first, &chain, 3).iter().any(|g| *g < 10.0),
                "{shape}: only the two most recent spawns are blocked"
            );
        }
    }

    #[test]
    fn min_flick_deg_keeps_new_targets_away_from_the_crosshair() {
        let flick = |r: &Respawn| angle_between_deg(r.kill_view.direction(), r.aim - r.eye);
        let (_, free) = kill_chain(area_def(SPHERE, ANGULAR, ""), 5, 200);
        assert!(free.iter().any(|r| flick(r) < 25.0));

        let rules = "spawn_rules = { min_flick_deg = 25 }";
        let (_, chain) = kill_chain(area_def(SPHERE, ANGULAR, rules), 5, 200);
        let flicks: Vec<f32> = chain.iter().map(flick).collect();
        assert!(flicks.iter().all(|f| *f >= 25.0), "{flicks:?}");
    }

    /// Whether `aim` lies in the central `fraction` of the `angular` test area: yaw
    /// [-40, 40] (center 0, half-width 40) and pitch [-10, 20] (center 5, half-width 15).
    fn in_angular_center(eye: Vec3, aim: Vec3, fraction: f32, humanoid: bool) -> bool {
        let v = ViewAngles::from_direction(aim - eye);
        let yaw_in = (v.yaw_deg / 40.0).abs() < fraction;
        yaw_in && (humanoid || ((v.pitch_deg - 5.0) / 15.0).abs() < fraction)
    }

    #[test]
    fn exclude_center_keeps_angular_spawns_out_of_the_middle() {
        for (shape, humanoid) in [(SPHERE, false), (HUMANOID, true)] {
            let rules = "spawn_rules = { exclude_center = 0.5 }";
            let (first, chain) = kill_chain(area_def(shape, ANGULAR, rules), 2, 300);
            let eye = chain[0].eye;
            // The humanoid head sits above the eye-level spawn point, so its pitch is not
            // the sampled one: only the yaw is checked for humanoids.
            let inside = std::iter::once(first)
                .chain(chain.iter().map(|r| r.aim))
                .filter(|a| in_angular_center(eye, *a, 0.5, humanoid))
                .count();
            assert_eq!(inside, 0, "{shape}");

            let (first, chain) = kill_chain(area_def(shape, ANGULAR, ""), 2, 300);
            let free_inside = std::iter::once(first)
                .chain(chain.iter().map(|r| r.aim))
                .filter(|a| in_angular_center(eye, *a, 0.5, humanoid))
                .count();
            assert!(free_inside > 20, "{shape}: {free_inside}");
        }
    }

    #[test]
    fn exclude_center_keeps_floor_spawns_out_of_the_middle() {
        // x [-6, 6]: center 0, half-width 6. z [-20, -10]: center -15, half-width 5.
        let central = |p: Vec3| (p.x / 6.0).abs() < 0.4 && ((p.z + 15.0) / 5.0).abs() < 0.4;
        let rules = "spawn_rules = { exclude_center = 0.4 }";
        let (first, chain) = kill_chain(area_def(SPHERE, FLOOR, rules), 4, 300);
        let points: Vec<Vec3> = std::iter::once(first)
            .chain(chain.iter().map(|r| r.aim))
            .collect();
        assert!(!points.iter().any(|p| central(*p)));
        let (first, chain) = kill_chain(area_def(SPHERE, FLOOR, ""), 4, 300);
        let free = std::iter::once(first).chain(chain.iter().map(|r| r.aim));
        assert!(free.filter(|p| central(*p)).count() > 20);
    }

    #[test]
    fn spawn_rules_are_deterministic_per_seed() {
        let rules =
            "spawn_rules = { avoid_last_deg = 8, min_flick_deg = 10, exclude_center = 0.2 }";
        let aims = |seed| {
            let (first, chain) = kill_chain(area_def(SPHERE, ANGULAR, rules), seed, 50);
            std::iter::once(first)
                .chain(chain.iter().map(|r| r.aim))
                .collect::<Vec<_>>()
        };
        assert_eq!(aims(6), aims(6));
        assert_ne!(aims(6), aims(7));
    }
}
