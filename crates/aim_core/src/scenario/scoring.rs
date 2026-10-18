//! Scoring: which hit zones count, how many points each is worth, how shots, hits, burst
//! bullets, kills and expiries update the run's [`Stats`], and the final [`RunResult`].

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
