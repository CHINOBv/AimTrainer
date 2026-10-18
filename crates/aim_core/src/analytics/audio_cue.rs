//! Sound localisation: how fast and how well the player turned to each sound cue.
//!
//! For every cue the traced [`Sample`]s give the player's view when the sound played, the
//! turn reaction time (the first frame the crosshair turns towards the source fast enough)
//! and the angular error left a fixed time after the sound, or at the first shot if that
//! comes sooner. Cues are binned into eight directions relative to the view at cue time.
//! See "Sound localisation" in `docs/training-science.md`.

use super::stats::median;
use super::trace::{CueSample, Sample};
use crate::math::{angle_between_deg, wrap_degrees};

/// Seconds after a cue at which its angular error is measured, unless the player shoots
/// first. An estimate: an auditory reaction (≈0.3 s) plus a fast 180° turn fits in it.
pub const ERROR_AT_SECS: f32 = 1.0;
/// Crosshair speed that counts as the start of a turn, degrees/s. An estimate, well above
/// hand tremor and well below a flick's peak speed.
pub const TURN_ONSET_DEG_PER_SEC: f32 = 60.0;
/// How long after a cue a turn may start and still count as the reaction to it, seconds.
pub const TURN_SEARCH_SECS: f32 = 2.0;

/// Width of each direction bin, degrees.
const BIN_DEG: f32 = 45.0;

/// Where a sound came from relative to the player's view, in eight 45° bins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bearing {
    /// Ahead.
    Front,
    /// Ahead and to the right.
    FrontRight,
    /// To the right.
    Right,
    /// Behind and to the right.
    BackRight,
    /// Behind.
    Back,
    /// Behind and to the left.
    BackLeft,
    /// To the left.
    Left,
    /// Ahead and to the left.
    FrontLeft,
}

impl Bearing {
    /// Every bearing, clockwise from the front. [`AudioCueAnalysis::by_bearing`] uses this
    /// order.
    pub const ALL: [Bearing; 8] = [
        Bearing::Front,
        Bearing::FrontRight,
        Bearing::Right,
        Bearing::BackRight,
        Bearing::Back,
        Bearing::BackLeft,
        Bearing::Left,
        Bearing::FrontLeft,
    ];

    /// Lowercase name, such as "back-left".
    pub fn label(self) -> &'static str {
        match self {
            Bearing::Front => "front",
            Bearing::FrontRight => "front-right",
            Bearing::Right => "right",
            Bearing::BackRight => "back-right",
            Bearing::Back => "back",
            Bearing::BackLeft => "back-left",
            Bearing::Left => "left",
            Bearing::FrontLeft => "front-left",
        }
    }

    /// The bin of a horizontal angle from the view to the source, degrees (positive to the
    /// right, any value; it is wrapped first).
    ///
    /// Each bin is 45° wide, centered on its direction, and includes its counter-clockwise
    /// (left) edge: front is [−22.5°, 22.5°), front-right [22.5°, 67.5°), and back covers
    /// [157.5°, 180°] and (−180°, −157.5°).
    pub fn of_yaw_offset(offset_deg: f32) -> Self {
        let a = wrap_degrees(offset_deg);
        let bin = ((a + BIN_DEG / 2.0) / BIN_DEG).floor() as i32;
        Self::ALL[bin.rem_euclid(8) as usize]
    }
}

/// How the player answered one sound cue.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CueResponse {
    /// Id of the target that made the sound.
    pub id: u64,
    /// Horizontal angle from the view to the source when the sound played, degrees
    /// (positive to the right, in (−180, 180]).
    pub offset_deg: f32,
    /// Bin of `offset_deg`.
    pub bearing: Bearing,
    /// Angle between the view and the source [`ERROR_AT_SECS`] after the sound, or at the
    /// first shot after it if that came sooner, degrees.
    pub error_deg: f32,
    /// Seconds from the sound to the first frame the crosshair turned towards the source
    /// at [`TURN_ONSET_DEG_PER_SEC`] or faster, if that happened within
    /// [`TURN_SEARCH_SECS`].
    pub reaction_secs: Option<f32>,
}

/// Aggregates of the cues from one [`Bearing`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BearingStats {
    /// The direction.
    pub bearing: Bearing,
    /// Cues measured from it.
    pub cues: u32,
    /// Median angular error, degrees.
    pub median_error_deg: Option<f32>,
    /// Median turn reaction time, seconds.
    pub median_reaction_secs: Option<f32>,
}

/// The sound-localisation result of a run.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioCueAnalysis {
    /// Every measured cue, in the order the sounds played.
    pub responses: Vec<CueResponse>,
    /// Per-direction aggregates, in [`Bearing::ALL`] order.
    pub by_bearing: [BearingStats; 8],
    /// Median angular error over every cue, degrees.
    pub median_error_deg: Option<f32>,
    /// Median turn reaction time over every cue that got one, seconds.
    pub median_reaction_secs: Option<f32>,
}

/// Analyze the sound cues of a run, or `None` when no cue could be measured. A cue whose
/// error could not be measured before the run ended is left out.
pub fn analyze(samples: &[Sample]) -> Option<AudioCueAnalysis> {
    let responses: Vec<CueResponse> = samples
        .iter()
        .enumerate()
        .flat_map(|(i, s)| s.cues.iter().map(move |c| (i, c)))
        .filter_map(|(i, cue)| respond(samples, i, cue))
        .collect();
    if responses.is_empty() {
        return None;
    }
    let by_bearing = Bearing::ALL.map(|bearing| {
        let of: Vec<&CueResponse> = responses.iter().filter(|r| r.bearing == bearing).collect();
        BearingStats {
            bearing,
            cues: of.len() as u32,
            median_error_deg: median(&of.iter().map(|r| r.error_deg).collect::<Vec<_>>()),
            median_reaction_secs: median(
                &of.iter()
                    .filter_map(|r| r.reaction_secs)
                    .collect::<Vec<_>>(),
            ),
        }
    });
    Some(AudioCueAnalysis {
        median_error_deg: median(&responses.iter().map(|r| r.error_deg).collect::<Vec<_>>()),
        median_reaction_secs: median(
            &responses
                .iter()
                .filter_map(|r| r.reaction_secs)
                .collect::<Vec<_>>(),
        ),
        responses,
        by_bearing,
    })
}

/// The response to `cue`, which played in frame `at`.
fn respond(samples: &[Sample], at: usize, cue: &CueSample) -> Option<CueResponse> {
    let heard = &samples[at];
    let source = cue.source.direction();
    let error_to = |s: &Sample| angle_between_deg(s.view.direction(), source);
    let after = &samples[at + 1..];
    // The cue plays at the end of its frame, so shots from the next frame on answer it.
    let measured = after
        .iter()
        .find(|s| s.shot || s.time_secs - heard.time_secs >= ERROR_AT_SECS - 1e-4)?;
    let mut previous = error_to(heard);
    let mut reaction_secs = None;
    for s in after
        .iter()
        .take_while(|s| s.time_secs - heard.time_secs <= TURN_SEARCH_SECS + 1e-4)
    {
        let error = error_to(s);
        if s.angular_speed_deg_per_sec >= TURN_ONSET_DEG_PER_SEC && error < previous {
            reaction_secs = Some(s.time_secs - heard.time_secs);
            break;
        }
        previous = error;
    }
    let offset_deg = wrap_degrees(cue.source.yaw_deg - heard.view.yaw_deg);
    Some(CueResponse {
        id: cue.id,
        offset_deg,
        bearing: Bearing::of_yaw_offset(offset_deg),
        error_deg: error_to(measured),
        reaction_secs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::ViewAngles;

    const DT: f32 = 0.05;

    #[test]
    fn bearings_are_45_degree_bins_centered_on_their_direction() {
        let cases = [
            (0.0, Bearing::Front),
            (-22.5, Bearing::Front),
            (22.4, Bearing::Front),
            (22.5, Bearing::FrontRight),
            (67.4, Bearing::FrontRight),
            (67.5, Bearing::Right),
            (90.0, Bearing::Right),
            (112.5, Bearing::BackRight),
            (157.4, Bearing::BackRight),
            (157.5, Bearing::Back),
            (180.0, Bearing::Back),
            (-180.0, Bearing::Back),
            (-157.6, Bearing::Back),
            (-157.5, Bearing::BackLeft),
            (-112.6, Bearing::BackLeft),
            (-112.5, Bearing::Left),
            (-67.6, Bearing::Left),
            (-67.5, Bearing::FrontLeft),
            (-22.6, Bearing::FrontLeft),
            (360.0 + 90.0, Bearing::Right),
        ];
        for (offset, expected) in cases {
            assert_eq!(Bearing::of_yaw_offset(offset), expected, "{offset}");
        }
        let labels: Vec<_> = Bearing::ALL.iter().map(|b| b.label()).collect();
        assert_eq!(labels[4], "back");
        assert_eq!(labels[7], "front-left");
    }

    /// Frames of a run whose view yaw in frame `i` is `yaws[i]`; a cue from `source_yaw`
    /// plays in frame `cue_at` and the player shoots in frame `shot_at`, if any.
    fn run(yaws: &[f32], cue_at: usize, source_yaw: f32, shot_at: Option<usize>) -> Vec<Sample> {
        let mut previous = ViewAngles::new(yaws[0], 0.0);
        yaws.iter()
            .enumerate()
            .map(|(i, &yaw)| {
                let view = ViewAngles::new(yaw, 0.0);
                let turned = angle_between_deg(previous.direction(), view.direction());
                previous = view;
                Sample {
                    time_secs: (i + 1) as f32 * DT,
                    dt_secs: DT,
                    view,
                    angular_speed_deg_per_sec: turned / DT,
                    shot: shot_at == Some(i),
                    cues: if i == cue_at {
                        vec![CueSample {
                            id: 7,
                            source: ViewAngles::new(source_yaw, 0.0),
                        }]
                    } else {
                        Vec::new()
                    },
                    ..Default::default()
                }
            })
            .collect()
    }

    /// Still for `wait` frames after the cue in frame 0, then turning right at
    /// `deg_per_frame` until `stop_yaw`, then still; 60 frames in all.
    fn turn_right(wait: usize, deg_per_frame: f32, stop_yaw: f32) -> Vec<f32> {
        (0..60usize)
            .map(|i| {
                let moving = i.saturating_sub(wait) as f32;
                (moving * deg_per_frame).min(stop_yaw)
            })
            .collect()
    }

    #[test]
    fn a_turn_to_a_sound_on_the_right_gives_its_reaction_time_and_remaining_error() {
        // Still for 6 frames (0.3 s) after the cue, then 10° per frame (200°/s) up to 85°.
        let samples = run(&turn_right(6, 10.0, 85.0), 0, 90.0, None);
        let a = analyze(&samples).expect("one cue");
        let r = a.responses[0];
        assert_eq!((r.id, r.bearing), (7, Bearing::Right));
        assert!((r.offset_deg - 90.0).abs() < 1e-4);
        // Frame 7 is the first that turns; it ends 0.35 s after the cue frame.
        assert!((r.reaction_secs.unwrap() - 0.35).abs() < 1e-4, "{r:?}");
        assert!((r.error_deg - 5.0).abs() < 1e-3, "{r:?}");
        let right = a.by_bearing[2];
        assert_eq!((right.bearing, right.cues), (Bearing::Right, 1));
        assert_eq!(right.median_error_deg, Some(r.error_deg));
        assert!(a.by_bearing.iter().filter(|b| b.cues == 0).count() == 7);
        assert_eq!(a.median_reaction_secs, r.reaction_secs);
    }

    #[test]
    fn a_shot_before_the_error_time_measures_the_error_at_the_shot() {
        // Shooting in frame 9 (0.45 s after the cue), when the view is at 30°.
        let samples = run(&turn_right(6, 10.0, 85.0), 0, 90.0, Some(9));
        let r = analyze(&samples).unwrap().responses[0];
        assert!((r.error_deg - 60.0).abs() < 1e-3, "{r:?}");
        // A shot in the cue's own frame was fired before the sound and does not count.
        let samples = run(&turn_right(6, 10.0, 85.0), 0, 90.0, Some(0));
        let r = analyze(&samples).unwrap().responses[0];
        assert!((r.error_deg - 5.0).abs() < 1e-3, "{r:?}");
    }

    #[test]
    fn turning_away_or_too_slowly_is_not_a_reaction() {
        // Turning left, away from a sound on the right.
        let away: Vec<f32> = turn_right(2, -10.0, 0.0)
            .iter()
            .enumerate()
            .map(|(i, _)| -(i.saturating_sub(2) as f32 * 10.0).min(80.0))
            .collect();
        let r = analyze(&run(&away, 0, 90.0, None)).unwrap().responses[0];
        assert_eq!(r.reaction_secs, None);
        assert!(r.error_deg > 160.0, "{r:?}");
        // Drifting towards it at 2° per frame (40°/s), below the onset speed.
        let slow = run(&turn_right(0, 2.0, 85.0), 0, 90.0, None);
        assert_eq!(analyze(&slow).unwrap().responses[0].reaction_secs, None);
    }

    #[test]
    fn bearings_are_relative_to_the_view_when_the_sound_played() {
        // Looking at yaw 100 when a sound comes from yaw -80: straight behind.
        let yaws = vec![100.0; 30];
        let r = analyze(&run(&yaws, 2, -80.0, None)).unwrap().responses[0];
        assert_eq!(r.bearing, Bearing::Back);
        assert!((r.offset_deg - 180.0).abs() < 1e-3, "{r:?}");
    }

    #[test]
    fn a_recorded_sound_turn_run_measures_every_cue_around_the_player() {
        use crate::analytics::{analyze as analyze_run, test_support::record};
        use crate::sim::AimerProfile;
        let rec = record("sound-turn", AimerProfile::PRESETS[2], 4);
        let mut heard = Vec::new();
        rec.replay(|s, _| {
            for t in s.heard_targets() {
                if !heard.contains(&t.id) {
                    heard.push(t.id);
                }
            }
        });
        let a = analyze_run(&rec).audio_cue.expect("a sound-cue run");
        let ids: Vec<u64> = a.responses.iter().map(|r| r.id).collect();
        // Every cue is measured except, at most, one cut short by the end of the run.
        assert!(
            ids.len() + 1 >= heard.len() && ids.len() <= heard.len(),
            "{ids:?}"
        );
        assert!(ids.iter().all(|id| heard.contains(id)));
        let binned: u32 = a.by_bearing.iter().map(|b| b.cues).sum();
        assert_eq!(binned as usize, ids.len());
        assert!(a.by_bearing[4].cues > 0, "sounds come from behind too");
        assert!(a.median_reaction_secs.is_some() && a.median_error_deg.is_some());
        let plain = record("flick-wide", AimerProfile::PRESETS[2], 4);
        assert_eq!(analyze_run(&plain).audio_cue, None);
    }

    #[test]
    fn cues_cut_short_by_the_end_of_the_run_and_runs_without_cues_give_nothing() {
        let yaws = vec![0.0; 30];
        assert_eq!(analyze(&run(&yaws, 25, 90.0, None)), None);
        let mut silent = run(&yaws, 0, 90.0, None);
        silent[0].cues.clear();
        assert_eq!(analyze(&silent), None);
    }
}
