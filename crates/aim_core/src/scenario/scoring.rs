//! Scoring: which hit zones count, how many points each is worth, how shots, hits, kills
//! and expiries update the run's [`Stats`], and the final [`RunResult`].

use super::def::{FireMode, ScenarioDef, ScoringDef, Shape};
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

/// Counts a click-mode hit on `zone` and awards its points.
pub(super) fn record_hit(stats: &mut Stats, scoring: &ScoringDef, zone: HitZone) {
    stats.hits += 1;
    match zone {
        HitZone::Head => stats.head_hits += 1,
        HitZone::Body => stats.body_hits += 1,
        HitZone::Legs => stats.leg_hits += 1,
        HitZone::Sphere => {}
    }
    stats.score += zone_points(scoring, zone);
}

/// Counts `dt` seconds of hold-mode fire on `zone`, awarding its points per second.
pub(super) fn record_on_target(stats: &mut Stats, scoring: &ScoringDef, zone: HitZone, dt: f32) {
    stats.on_target_secs += dt;
    if zone == HitZone::Head {
        stats.head_on_target_secs += dt;
    }
    stats.score += zone_points(scoring, zone) * dt;
}

pub(super) fn record_kill(stats: &mut Stats, time_to_kill: f32) {
    stats.kills += 1;
    stats.kill_times.push(time_to_kill);
}

pub(super) fn record_expired(stats: &mut Stats, scoring: &ScoringDef) {
    stats.expired += 1;
    stats.score -= scoring.expire_penalty;
}

/// The final numbers of a run of `def` that lasted `time` seconds.
pub(super) fn run_result(def: &ScenarioDef, stats: &Stats, time: f32) -> RunResult {
    let s = stats;
    let hold = def.weapon.fire == FireMode::Hold;
    let ratio = |a: f32, b: f32| if b > 0.0 { a / b } else { 0.0 };
    let elapsed = time.max(f32::EPSILON);
    RunResult {
        scenario_id: def.id.clone(),
        score: s.score.max(0.0),
        accuracy: if hold {
            ratio(s.on_target_secs, s.hold_secs)
        } else {
            ratio(s.hits as f32, s.shots as f32)
        },
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
