//! Scoring: which hit zones count, how many points each is worth, how shots, hits, burst
//! bullets, kills and expiries update the run's [`Stats`], and the final [`RunResult`].

use super::def::{AccuracyMult, FireMode, ScenarioDef, ScoringDef, Shape};
use super::hit::HitZone;
use super::session::{RunResult, Stats};

/// Whether a hit on `zone` counts: head-only humanoids ignore body and leg hits.
pub(super) fn counts(shape: &Shape, zone: HitZone) -> bool {
    !matches!(shape, Shape::Humanoid { head_only: true }) || zone == HitZone::Head
}

pub(super) fn zone_points(scoring: &ScoringDef, zone: HitZone) -> f32 {
    match zone {
        HitZone::Head => scoring.head,
        HitZone::Body => scoring.body,
        HitZone::Legs => scoring.legs,
        HitZone::Sphere => scoring.sphere,
    }
}

/// Counts a click-mode shot, penalizing it when the player was moving.
pub(super) fn record_shot(stats: &mut Stats, scoring: &ScoringDef, moving: bool) {
    stats.shots += 1;
    if moving {
        stats.moving_shots += 1;
        stats.score -= scoring.moving_shot_penalty;
    }
}

pub(super) fn record_miss(stats: &mut Stats, scoring: &ScoringDef) {
    stats.score -= scoring.miss_penalty;
}

/// Counts a click-mode hit on `zone` and awards its points times `multiplier` (the
/// score-ring multiplier, 1 without rings).
pub(super) fn record_hit(stats: &mut Stats, scoring: &ScoringDef, zone: HitZone, multiplier: f32) {
    stats.hits += 1;
    match zone {
        HitZone::Head => stats.head_hits += 1,
        HitZone::Body => stats.body_hits += 1,
        HitZone::Legs => stats.leg_hits += 1,
        HitZone::Sphere => {}
    }
    stats.score += zone_points(scoring, zone) * multiplier;
}

/// Counts the `bullet`-th bullet (1-based) of a recoil burst: a first bullet starts a
/// new burst.
pub(super) fn record_bullet(stats: &mut Stats, bullet: u32) {
    if bullet == 1 {
        stats.bursts += 1;
    }
    stats.longest_burst = stats.longest_burst.max(bullet);
    stats.shots_by_bullet[Stats::bullet_bucket(bullet)] += 1;
}

/// Counts a hit by the `bullet`-th bullet of a burst and awards the burst bonus when it
/// was one of the weapon's `protected` bullets.
pub(super) fn record_bullet_hit(
    stats: &mut Stats,
    scoring: &ScoringDef,
    bullet: u32,
    protected: u32,
) {
    stats.hits_by_bullet[Stats::bullet_bucket(bullet)] += 1;
    if bullet <= protected {
        stats.score += scoring.burst_bonus;
    }
}

/// Counts `dt` seconds of hold-mode fire on `zone`, awarding its points per second times
/// `multiplier` (the score-ring multiplier, 1 without rings).
pub(super) fn record_on_target(
    stats: &mut Stats,
    scoring: &ScoringDef,
    zone: HitZone,
    dt: f32,
    multiplier: f32,
) {
    stats.on_target_secs += dt;
    if zone == HitZone::Head {
        stats.head_on_target_secs += dt;
    }
    stats.score += zone_points(scoring, zone) * dt * multiplier;
}

pub(super) fn record_kill(stats: &mut Stats, time_to_kill: f32) {
    stats.kills += 1;
    stats.kill_times.push(time_to_kill);
}

pub(super) fn record_expired(stats: &mut Stats, scoring: &ScoringDef) {
    stats.expired += 1;
    stats.score -= scoring.expire_penalty;
}

/// Counts a shot on a go/no-go decoy (a "non-match hit") and applies its penalty.
pub(super) fn record_decoy_hit(stats: &mut Stats, scoring: &ScoringDef) {
    stats.decoy_hits += 1;
    stats.score -= scoring.decoy_penalty;
}

/// Counts a decoy that was left alone until it expired (a correct "non-match wait").
pub(super) fn record_decoy_wait(stats: &mut Stats) {
    stats.decoy_waits += 1;
}

/// The final numbers of a run of `def` that lasted `time` seconds.
pub(super) fn run_result(def: &ScenarioDef, stats: &Stats, time: f32) -> RunResult {
    let s = stats;
    let hold = def.weapon.fire == FireMode::Hold;
    let ratio = |a: f32, b: f32| if b > 0.0 { a / b } else { 0.0 };
    let elapsed = time.max(f32::EPSILON);
    let accuracy = if hold {
        ratio(s.on_target_secs, s.hold_secs)
    } else {
        ratio(s.hits as f32, s.shots as f32)
    };
    let score = s.score.max(0.0);
    RunResult {
        scenario_id: def.id.clone(),
        // Without a multiplier the score is exactly as earned.
        score: match def.scoring.accuracy_mult {
            AccuracyMult::None => score,
            mult => score * mult.factor(accuracy),
        },
        accuracy,
        kills: s.kills,
        kills_per_min: s.kills as f32 * 60.0 / elapsed,
        avg_time_to_kill: (!s.kill_times.is_empty())
            .then(|| s.kill_times.iter().sum::<f32>() / s.kill_times.len() as f32),
        headshot_rate: if hold {
            ratio(s.head_on_target_secs, s.on_target_secs)
        } else {
            ratio(s.head_hits as f32, s.hits as f32)
        },
        moving_shots: s.moving_shots,
        expired: s.expired,
        duration_secs: time,
    }
}

#[cfg(test)]
mod tests {
    use crate::math::{Vec3, ViewAngles};
    use crate::scenario::{FrameInput, ScenarioDef, Session, Shape};

    /// One static sphere of `radius` straight ahead at 10 m, with `targets` and `scoring`
    /// lines added.
    fn def(radius: f32, targets: &str, scoring: &str) -> ScenarioDef {
        let d = ScenarioDef::from_toml(&format!(
            r#"
            id = "s"
            name = "s"
            category = "custom"
            duration_secs = 100.0
            [targets]
            shape = {{ kind = "sphere", radius = {radius} }}
            spawn = {{ kind = "angular", yaw_deg = [0, 0], pitch_deg = [0, 0], distance = [10, 10] }}
            {targets}
            [scoring]
            {scoring}
            "#
        ))
        .unwrap();
        assert!(d.validate().is_empty(), "{:?}", d.validate());
        d
    }

    /// Clicks once along `view`, after waiting out the fire rate.
    fn click(s: &mut Session, view: ViewAngles) {
        s.update(FrameInput {
            dt: 0.2,
            ..Default::default()
        });
        s.update(FrameInput {
            dt: 0.001,
            view,
            fire_pressed: true,
            ..Default::default()
        });
    }

    /// The view from the eye to `p` turned `off` meters sideways at its distance.
    fn view_beside(s: &Session, p: Vec3, off: f32) -> ViewAngles {
        ViewAngles::from_direction(p + Vec3::new(off, 0.0, 0.0) - s.eye_position())
    }

    fn sphere_radius(shape: &Shape) -> f32 {
        match shape {
            Shape::Sphere { radius } => *radius,
            other => panic!("sphere expected, got {other:?}"),
        }
    }

    #[test]
    fn the_accuracy_multiplier_scales_the_final_score() {
        for (mult, expected) in [
            ("none", 100.0),
            ("linear", 50.0),
            ("sqrt", 100.0 * 0.5f32.sqrt()),
        ] {
            let mut s = Session::new(def(0.3, "", &format!("accuracy_mult = \"{mult}\"")), 1);
            let p = s.targets().next().unwrap().position;
            let v = view_beside(&s, p, 0.0);
            click(&mut s, v);
            click(&mut s, ViewAngles::new(180.0, 0.0));
            let r = s.result();
            assert!((r.accuracy - 0.5).abs() < 1e-6, "{mult}");
            assert!((r.score - expected).abs() < 1e-3, "{mult}: {}", r.score);
            assert!(
                (s.stats().score - 100.0).abs() < 1e-3,
                "the live score is unchanged"
            );
        }
        let plain = def(0.3, "", "");
        assert_eq!(
            plain.scoring,
            def(0.3, "", "accuracy_mult = \"none\"").scoring
        );
    }

    #[test]
    fn shrink_per_kill_shrinks_targets_down_to_their_floor() {
        let d = def(0.4, "shrink_per_kill = { factor = 0.5, floor = 0.3 }", "");
        let mut s = Session::new(d, 1);
        let mut radii = Vec::new();
        for _ in 0..3 {
            let p = s.targets().next().unwrap().position;
            let v = view_beside(&s, p, 0.0);
            click(&mut s, v);
            radii.push(sphere_radius(&s.active_targets().shape));
        }
        assert_eq!(s.stats().kills, 3);
        let floor = 0.4 * 0.3;
        assert!((radii[0] - 0.2).abs() < 1e-6, "{radii:?}");
        assert!((radii[1] - floor).abs() < 1e-6, "{radii:?}");
        assert!((radii[2] - floor).abs() < 1e-6, "{radii:?}");
        assert_eq!(
            sphere_radius(&s.def().targets.shape),
            0.4,
            "the definition is kept"
        );

        // The hit test uses the shrunken radius: 0.2 m beside the center now misses.
        let p = s.targets().next().unwrap().position;
        let v = view_beside(&s, p, 0.2);
        click(&mut s, v);
        assert_eq!(s.stats().kills, 3);
        assert_eq!(s.stats().shots, 4);
    }

    /// A static capsule straight ahead at 10 m, its center at eye height.
    fn capsule(head: &str) -> ScenarioDef {
        let d = ScenarioDef::from_toml(&format!(
            r#"
            id = "c"
            name = "c"
            category = "custom"
            duration_secs = 100.0
            [targets]
            shape = {{ kind = "capsule", radius = 0.3, height = 1.6{head} }}
            spawn = {{ kind = "angular", yaw_deg = [0, 0], pitch_deg = [0, 0], distance = [10, 10] }}
            health = 100
            "#
        ))
        .unwrap();
        assert!(d.validate().is_empty(), "{:?}", d.validate());
        d
    }

    /// The zone a click at `p` + (`dx`, `dy`) hits, if any.
    fn zone_at(s: &mut Session, dx: f32, dy: f32) -> Option<crate::scenario::HitZone> {
        let p = s.targets().next().unwrap().position;
        let v = ViewAngles::from_direction(p + Vec3::new(dx, dy, 0.0) - s.eye_position());
        s.update(FrameInput {
            dt: 0.2,
            ..Default::default()
        });
        let events = s.update(FrameInput {
            dt: 0.001,
            view: v,
            fire_pressed: true,
            ..Default::default()
        });
        events.iter().find_map(|e| match e {
            crate::scenario::SessionEvent::Shot { hit, .. } => Some(hit.map(|h| h.1)),
            _ => None,
        })?
    }

    #[test]
    fn capsules_have_a_body_and_an_optional_head() {
        use crate::scenario::HitZone::{Body, Head};
        let mut s = Session::new(capsule(", head = { radius = 0.15, offset = 0.95 }"), 1);
        assert_eq!(zone_at(&mut s, 0.0, 0.0), Some(Body), "center");
        assert_eq!(
            zone_at(&mut s, 0.25, 0.0),
            Some(Body),
            "side of the cylinder"
        );
        assert_eq!(zone_at(&mut s, 0.0, 0.75), Some(Body), "inside the top cap");
        assert_eq!(zone_at(&mut s, 0.0, 0.95), Some(Head), "head center");
        assert_eq!(zone_at(&mut s, 0.12, 0.95), Some(Head), "head edge");
        assert_eq!(zone_at(&mut s, 0.35, 0.0), None, "beside");
        // Past the rounded corner of the cap: inside the bounding box but outside the capsule.
        assert_eq!(zone_at(&mut s, 0.27, 0.77), None, "corner");
        assert_eq!(zone_at(&mut s, 0.0, 1.2), None, "above the head");

        let mut plain = Session::new(capsule(""), 1);
        assert_eq!(zone_at(&mut plain, 0.0, 0.95), None, "no head without one");
        assert_eq!(zone_at(&mut plain, 0.0, -0.7), Some(Body), "bottom cap");
        // Head hits score `head`, body hits score `body`.
        let mut s = Session::new(capsule(", head = { radius = 0.15, offset = 0.95 }"), 1);
        zone_at(&mut s, 0.0, 0.0);
        zone_at(&mut s, 0.0, 0.95);
        assert!(
            (s.stats().score - 140.0).abs() < 1e-3,
            "{}",
            s.stats().score
        );
        assert_eq!((s.stats().body_hits, s.stats().head_hits), (1, 1));
    }
}
