//! Per-frame samples recovered by re-simulating a [`Recording`].
//!
//! The input log only holds what the player did; replaying it through the deterministic
//! `Session` recovers where every target was, which one the crosshair was nearest to, and
//! what each shot did. Flick and tracking analysis read these samples, never the session.

use super::decision::DecisionCounts;
use crate::math::{Vec3, ViewAngles, angle_between_deg, wrap_degrees};
use crate::scenario::Stats;
use crate::scenario::{Recording, Session, Shape, Target};

/// One visible target as seen from the eye in a given frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetSample {
    /// Id of the target.
    pub id: u64,
    /// Angles that look at the target's aim point (sphere center, or humanoid head center).
    pub aim_point: ViewAngles,
    /// Angular radius of the aim zone (sphere, or half the head size), degrees.
    pub radius_deg: f32,
    /// Session time when the target became visible, seconds.
    pub visible_at_secs: f32,
    /// Go/no-go "non-match" decoy, to be left alone.
    pub decoy: bool,
}

/// What happened in one `Session::update` call.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sample {
    /// Session time after this frame.
    pub time_secs: f32,
    /// Duration of this frame, seconds.
    pub dt_secs: f32,
    /// Crosshair orientation after this frame.
    pub view: ViewAngles,
    /// Crosshair angular velocity over this frame, in the tangent plane (degrees/s):
    /// `.0` is horizontal (positive right), `.1` vertical (positive up).
    pub view_velocity_deg_per_sec: (f32, f32),
    /// Magnitude of the crosshair rotation over this frame, degrees/s.
    pub angular_speed_deg_per_sec: f32,
    /// Visible targets, including any killed by this frame's shot.
    pub targets: Vec<TargetSample>,
    /// Index into `targets` of the one nearest to the crosshair, by angle.
    pub aimed: Option<usize>,
    /// Fire was held during this frame.
    pub fire_held: bool,
    /// A shot was fired this frame (click mode).
    pub shot: bool,
    /// This frame's shot hit a counted zone.
    pub hit: bool,
    /// The crosshair was on a counted zone while firing this frame (hold mode).
    pub on_target: bool,
    /// Go/no-go outcomes resolved this frame (all zero without decoys).
    pub decision: DecisionCounts,
    /// Player movement speed after this frame (the speed any shot this frame was fired
    /// at), m/s. Always 0 in scenarios without movement.
    pub player_speed: f32,
    /// The player was at or below the weapon's accuracy speed limit after this frame.
    pub accurate: bool,
    /// Points scored from the start of the run to the end of this frame.
    pub score: f32,
    /// Targets killed from the start of the run to the end of this frame.
    pub kills: u32,
}

impl Sample {
    /// The target nearest to the crosshair this frame, if any.
    pub fn aimed_target(&self) -> Option<&TargetSample> {
        self.aimed.and_then(|i| self.targets.get(i))
    }

    /// The visible target with `id`, if present this frame.
    pub fn target(&self, id: u64) -> Option<&TargetSample> {
        self.targets.iter().find(|t| t.id == id)
    }
}

/// Offset from `from` to `to` in the tangent plane at `from`, degrees:
/// `.0` horizontal (positive right), `.1` vertical (positive up).
///
/// Yaw differences shrink with `cos(pitch)`, so the result approximates the on-screen
/// angular offset; it is exact at the horizon and good to a few percent within ±30° pitch.
pub fn tangent_offset_deg(from: ViewAngles, to: ViewAngles) -> (f32, f32) {
    let cos_pitch = from.pitch_deg.to_radians().cos();
    (
        wrap_degrees(to.yaw_deg - from.yaw_deg) * cos_pitch,
        to.pitch_deg - from.pitch_deg,
    )
}

/// Aim point (world) and angular radius (degrees) of a target, the same point the
/// simulated aimer goes for.
fn aim_zone(session: &Session, t: &Target) -> (Vec3, f32) {
    let eye = session.eye_position();
    match session.def().targets.shape {
        Shape::Sphere { radius } => {
            let d = (t.position - eye).length().max(1e-3);
            (t.position, (radius / d).atan().to_degrees())
        }
        Shape::Humanoid { .. } => {
            let dims = session.target_dims(t);
            let head = t.position + Vec3::new(0.0, dims.head_center_height(), 0.0);
            let d = (head - eye).length().max(1e-3);
            (head, (dims.head_size / 2.0 / d).atan().to_degrees())
        }
    }
}

fn snapshot(session: &Session) -> Vec<TargetSample> {
    let eye = session.eye_position();
    session
        .targets()
        .map(|t| {
            let (point, radius_deg) = aim_zone(session, t);
            TargetSample {
                id: t.id,
                aim_point: ViewAngles::from_direction(point - eye),
                radius_deg,
                visible_at_secs: t.visible_at,
                decoy: t.decoy,
            }
        })
        .collect()
}

fn nearest(view: ViewAngles, targets: &[TargetSample]) -> Option<usize> {
    let dir = view.direction();
    targets
        .iter()
        .enumerate()
        .map(|(i, t)| (i, angle_between_deg(dir, t.aim_point.direction())))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

/// Go/no-go outcomes counted so far in `stats`.
fn decision_totals(stats: &Stats) -> DecisionCounts {
    DecisionCounts {
        match_hits: stats.kills,
        match_waits: stats.expired,
        nonmatch_hits: stats.decoy_hits,
        nonmatch_waits: stats.decoy_waits,
    }
}

/// Go/no-go outcomes added to `stats` since `before`.
fn decision_since(stats: &Stats, before: DecisionCounts) -> DecisionCounts {
    let now = decision_totals(stats);
    DecisionCounts {
        match_hits: now.match_hits - before.match_hits,
        match_waits: now.match_waits - before.match_waits,
        nonmatch_hits: now.nonmatch_hits - before.nonmatch_hits,
        nonmatch_waits: now.nonmatch_waits - before.nonmatch_waits,
    }
}

/// Re-simulate `recording` and return one sample per input frame.
pub fn trace(recording: &Recording) -> Vec<Sample> {
    let mut samples = Vec::with_capacity(recording.inputs.len());
    let initial = recording.start();
    let mut previous_targets = snapshot(&initial);
    let mut previous_view = ViewAngles::default();
    let mut previous_shots = initial.stats().shots;
    let mut previous_hits = initial.stats().hits;
    let mut previous_on_target_secs = initial.stats().on_target_secs;
    let mut previous_kills = initial.stats().kills;
    let mut previous_decision = decision_totals(initial.stats());

    recording.replay(|session, input| {
        let stats = session.stats();
        let mut targets = snapshot(session);
        // A target killed by this frame's shot is gone after the update; keep it so the shot
        // can be attributed to the target it was aimed at.
        if stats.kills > previous_kills || stats.decoy_hits > previous_decision.nonmatch_hits {
            for gone in &previous_targets {
                if !targets.iter().any(|t| t.id == gone.id) {
                    targets.push(*gone);
                }
            }
        }
        let dt_secs = input.dt.clamp(0.0, 0.25);
        let step = tangent_offset_deg(previous_view, input.view);
        let (velocity, speed) = if dt_secs > 0.0 {
            let turned = angle_between_deg(previous_view.direction(), input.view.direction());
            ((step.0 / dt_secs, step.1 / dt_secs), turned / dt_secs)
        } else {
            ((0.0, 0.0), 0.0)
        };
        let shot = stats.shots > previous_shots;
        samples.push(Sample {
            time_secs: session.time(),
            dt_secs,
            view: input.view,
            view_velocity_deg_per_sec: velocity,
            angular_speed_deg_per_sec: speed,
            aimed: nearest(input.view, &targets),
            targets,
            fire_held: input.fire_held,
            shot,
            hit: stats.hits > previous_hits,
            on_target: stats.on_target_secs > previous_on_target_secs,
            decision: decision_since(stats, previous_decision),
            player_speed: session.player_speed(),
            accurate: session.is_accurate(),
            score: stats.score,
            kills: stats.kills,
        });
        previous_targets = snapshot(session);
        previous_view = input.view;
        previous_shots = stats.shots;
        previous_hits = stats.hits;
        previous_on_target_secs = stats.on_target_secs;
        previous_kills = stats.kills;
        previous_decision = decision_totals(stats);
    });
    samples
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::test_support::record;
    use crate::sim::AimerProfile;

    #[test]
    fn samples_carry_the_running_score_and_kill_count() {
        for id in ["flick-wide", "track-precise"] {
            let rec = record(id, AimerProfile::PRESETS[2], 3);
            let samples = trace(&rec);
            let result = rec.replay(|_, _| {}).stats().clone();
            let last = samples.last().expect("samples");
            assert_eq!(last.kills, result.kills, "{id}");
            assert_eq!(last.score, result.score, "{id}");
            assert!(
                samples.windows(2).all(|w| w[1].kills >= w[0].kills),
                "{id}: kills never decrease"
            );
            assert!(result.score > 0.0, "{id}");
        }
    }

    #[test]
    fn trace_has_one_sample_per_frame_and_counts_every_shot() {
        let rec = record("flick-wide", AimerProfile::PRESETS[2], 3);
        let samples = trace(&rec);
        assert_eq!(samples.len(), rec.inputs.len());
        let result = rec.replay(|_, _| {}).stats().clone();
        assert_eq!(
            samples.iter().filter(|s| s.shot).count() as u32,
            result.shots
        );
        assert_eq!(samples.iter().filter(|s| s.hit).count() as u32, result.hits);
        assert!(
            samples
                .iter()
                .filter(|s| s.shot)
                .all(|s| s.aimed_target().is_some())
        );
    }

    #[test]
    fn samples_carry_the_player_speed_and_accuracy_of_each_frame() {
        let rec = crate::analytics::moving::tests::strafe_recording();
        let samples = trace(&rec);
        let mut expected = Vec::new();
        rec.replay(|s, _| expected.push((s.player_speed(), s.is_accurate())));
        let got: Vec<(f32, bool)> = samples
            .iter()
            .map(|s| (s.player_speed, s.accurate))
            .collect();
        assert_eq!(got, expected);
        assert!(got.iter().any(|(_, accurate)| !accurate));
        assert!(got.iter().any(|(speed, _)| *speed == 0.0));
    }

    #[test]
    fn scenarios_without_movement_report_a_still_accurate_player() {
        let rec = record("flick-wide", AimerProfile::PRESETS[2], 3);
        let samples = trace(&rec);
        assert!(samples.iter().all(|s| s.player_speed == 0.0 && s.accurate));
    }

    #[test]
    fn tangent_offset_wraps_yaw_and_scales_by_pitch() {
        let (dx, dy) =
            tangent_offset_deg(ViewAngles::new(179.0, 0.0), ViewAngles::new(-179.0, 5.0));
        assert!((dx - 2.0).abs() < 1e-3 && (dy - 5.0).abs() < 1e-6);
        let (dx, _) = tangent_offset_deg(ViewAngles::new(0.0, 60.0), ViewAngles::new(10.0, 60.0));
        assert!((dx - 5.0).abs() < 1e-3);
    }
}
