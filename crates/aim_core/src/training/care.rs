//! Guided hand care: wrist and hand mobility before playing, stretches after playing and an
//! optional daily posture routine, followed one timed step at a time.
//!
//! Idea credit: Kastor, "Wrist stretching and posture guide" (2020, shared by Voltaic). The
//! exercises follow that guide; the instructions are written in our own words. Timings are
//! the guide's suggestions, picked inside its ranges (estimates). Like the break stretches,
//! this is general guidance, not medical advice ([`super::ergonomics::STRETCH_DISCLAIMER`]),
//! and no trial has shown that it prevents complaints in esports
//! ([`super::ergonomics::EVIDENCE_NOTE`]). See "Breaks and wrist care" in
//! `docs/training-science.md`.

/// When a routine is meant to be done.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CareWhen {
    /// Before playing: moving the joints warms them up.
    BeforePlaying,
    /// After playing: static stretches, which can lower performance right after them.
    AfterPlaying,
    /// Once a day, when your posture needs it.
    Daily,
}

impl CareWhen {
    /// Text shown in the UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::BeforePlaying => "Before playing",
            Self::AfterPlaying => "After playing",
            Self::Daily => "Once a day",
        }
    }
}

/// One exercise of a routine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CareStep {
    /// Short name.
    pub name: &'static str,
    /// How to do it.
    pub how: &'static str,
    /// Time for the exercise, seconds; per side when `per_side`.
    pub secs: f32,
    /// Done once with each arm or to each side, `secs` each.
    pub per_side: bool,
}

impl CareStep {
    /// Time for the whole exercise, both sides included, seconds.
    pub fn total_secs(&self) -> f32 {
        if self.per_side {
            2.0 * self.secs
        } else {
            self.secs
        }
    }

    /// Timed parts of the exercise: 2 with sides, else 1.
    fn parts(&self) -> usize {
        if self.per_side { 2 } else { 1 }
    }
}

/// A guided routine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CareRoutine {
    /// Stable identifier.
    pub id: &'static str,
    /// Name shown in the UI.
    pub name: &'static str,
    /// When it is meant to be done.
    pub when: CareWhen,
    /// What it is for, shown under the name.
    pub purpose: &'static str,
    /// Its exercises, in order.
    pub steps: &'static [CareStep],
}

impl CareRoutine {
    /// Time for the whole routine, seconds.
    pub fn total_secs(&self) -> f32 {
        self.steps.iter().map(CareStep::total_secs).sum()
    }
}

/// Credit shown with the routines.
pub const CREDIT: &str = "Exercises from Kastor's \"Wrist stretching and posture guide\" \
(2020), in our own words.";

/// Mobility before playing: keep each movement smooth and slow.
const MOBILITY: [CareStep; 6] = [
    CareStep {
        name: "Hand circles",
        how: "Lace your fingers together, palms touching, and circle your hands around each \
              other: slowly at first, then a little faster. Halfway through, change direction.",
        secs: 45.0,
        per_side: false,
    },
    CareStep {
        name: "Hand waves",
        how: "Fingers laced, palms down and elbows out. Bend one wrist and let the movement \
              roll through the other wrist and along the forearms, like a wave.",
        secs: 45.0,
        per_side: false,
    },
    CareStep {
        name: "Prayer press",
        how: "Palms together at face height, fingers up. Lower the hands, elbows out, until \
              you feel the stretch, then lift them back. Go a little lower each time.",
        secs: 45.0,
        per_side: false,
    },
    CareStep {
        name: "Reverse prayer press",
        how: "Backs of the hands together, fingers down. Raise the hands towards elbow \
              height with the elbows out, then press the elbows in. Go a little further each \
              time.",
        secs: 45.0,
        per_side: false,
    },
    CareStep {
        name: "Thumb-and-pinky twists",
        how: "Stretch out the thumb and little finger, curl the other three, and keep turning \
              the wrists back and forth.",
        secs: 45.0,
        per_side: false,
    },
    CareStep {
        name: "Thumb-and-pinky tilts",
        how: "Same hand shape, palms facing each other. Tilt the hand towards the little \
              finger, then towards the thumb, back and forth.",
        secs: 45.0,
        per_side: false,
    },
];

/// Stretches after playing: hold each one gently, never to pain.
const STRETCHING: [CareStep; 10] = [
    CareStep {
        name: "Wrist extension stretch",
        how: "Arm out in front at shoulder height, palm up. Bend the hand back towards you \
              with the other hand until the palm and forearm feel a light stretch.",
        secs: 20.0,
        per_side: true,
    },
    CareStep {
        name: "Wrist flexion stretch",
        how: "Arm out in front, palm down. Bend the hand down towards you with the other hand; \
              the stretch is on the back of the forearm.",
        secs: 20.0,
        per_side: true,
    },
    CareStep {
        name: "Fist stretch",
        how: "Like the last one, but with the fingers curled into a fist, to stretch the back \
              of the hand.",
        secs: 20.0,
        per_side: true,
    },
    CareStep {
        name: "Forearm turn, palm up",
        how: "Elbow tucked against your side. With the other hand, turn the palm up and out.",
        secs: 20.0,
        per_side: true,
    },
    CareStep {
        name: "Forearm turn, palm down",
        how: "Elbow tucked against your side. With the other hand, turn the back of the hand \
              in and down.",
        secs: 20.0,
        per_side: true,
    },
    CareStep {
        name: "Thumb extensor stretch",
        how: "Fold the fingers over the thumb into a fist and tilt the hand down, towards the \
              little finger. Help gently with the other hand.",
        secs: 20.0,
        per_side: true,
    },
    CareStep {
        name: "Thumb flexor stretch",
        how: "Palm up. With the other hand, ease the thumb down and back towards the elbow.",
        secs: 20.0,
        per_side: true,
    },
    CareStep {
        name: "Thumb web stretch",
        how: "Gently pull the thumb away from the other fingers, opening the space between the \
              thumb and the index finger.",
        secs: 20.0,
        per_side: true,
    },
    CareStep {
        name: "Finger flexor stretch",
        how: "Hold the other fingers down and bend one finger back, towards the back of the \
              hand. Go through every finger.",
        secs: 20.0,
        per_side: true,
    },
    CareStep {
        name: "Finger extensor stretch",
        how: "Hold the other fingers straight and bend one finger into the palm. Go through \
              every finger.",
        secs: 20.0,
        per_side: true,
    },
];

/// Posture work against a forward head and rounded shoulders: stretch what is tight and
/// strengthen what is weak.
const POSTURE: [CareStep; 7] = [
    CareStep {
        name: "Side of the neck stretch",
        how: "Back straight, one hand pressing lightly on the chest. Turn the head to one side \
              and tilt it slightly back until the side of the neck stretches.",
        secs: 30.0,
        per_side: true,
    },
    CareStep {
        name: "Neck and upper back stretch",
        how: "One hand on the chest. Bring the ear towards the shoulder on that side, turn the \
              face away, and lift the chin slightly.",
        secs: 30.0,
        per_side: true,
    },
    CareStep {
        name: "Base of the skull release",
        how: "Lying down, rest the back of the neck on a tennis ball and slowly nod the chin up \
              and down, 15 to 20 times.",
        secs: 60.0,
        per_side: false,
    },
    CareStep {
        name: "Chin tucks",
        how: "Lying down or against a wall, tuck the chin in (a double chin), hold 2 seconds and \
              let go. 15 to 20 times.",
        secs: 60.0,
        per_side: false,
    },
    CareStep {
        name: "Upper back lift",
        how: "Face down, arms out at shoulder height, elbows bent 90° and thumbs up. Lift the \
              chest a little and squeeze the shoulder blades together and down. Hold; three \
              rounds of 30 s with a short rest.",
        secs: 90.0,
        per_side: false,
    },
    CareStep {
        name: "Doorway chest stretch",
        how: "Forearm on a door frame, elbow at shoulder height and bent 90°. Step forward \
              until the chest stretches lightly.",
        secs: 30.0,
        per_side: true,
    },
    CareStep {
        name: "High doorway chest stretch",
        how: "The same, with the elbow above shoulder height.",
        secs: 30.0,
        per_side: true,
    },
];

/// Every guided routine, in the order they are shown.
pub const ROUTINES: [CareRoutine; 3] = [
    CareRoutine {
        id: "mobility",
        name: "Wrist and hand mobility",
        when: CareWhen::BeforePlaying,
        purpose: "Smooth, easy movements that warm up the wrists and hands before you aim.",
        steps: &MOBILITY,
    },
    CareRoutine {
        id: "stretching",
        name: "Hand and forearm stretches",
        when: CareWhen::AfterPlaying,
        purpose: "Gentle holds for the wrists, thumbs and fingers. Save them for after playing: \
                  long static stretches can lower performance right after them.",
        steps: &STRETCHING,
    },
    CareRoutine {
        id: "posture",
        name: "Posture reset",
        when: CareWhen::Daily,
        purpose: "For a head that drifts forward or rounded shoulders: stretches for the neck \
                  and chest, and exercises for the upper back. Needs a tennis ball, a floor \
                  and a doorway.",
        steps: &POSTURE,
    },
];

/// Posture while playing, as `(body part, tip)`.
pub const POSTURE_TIPS: [(&str, &str); 7] = [
    (
        "Neck",
        "Look straight ahead without twisting; your eyes level with the top of the screen.",
    ),
    ("Shoulders", "Relaxed and down, not shrugged."),
    (
        "Arms",
        "Forearms supported, elbows close to the body at about 90–100°.",
    ),
    (
        "Wrists",
        "Relaxed and in line with the forearm, not bent to the side.",
    ),
    (
        "Spine",
        "Against the backrest, keeping its natural curve; a chair with lower-back support helps.",
    ),
    (
        "Hips and knees",
        "Thighs level with the floor, knees at about 90°.",
    ),
    (
        "Feet",
        "Flat on the floor or on a footrest, with room to move.",
    ),
];

/// What a guided session just did, for sounds and screen cues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CareCue {
    /// The other side of the same exercise starts.
    SwitchSides,
    /// The next exercise starts.
    NextStep,
    /// The routine is over.
    Finished,
}

/// A routine being followed: the current exercise and side, and the time left on it.
#[derive(Debug, Clone, PartialEq)]
pub struct CareSession {
    routine: &'static CareRoutine,
    step: usize,
    /// 0 for the first side (or the only part), 1 for the second side.
    part: usize,
    left_secs: f32,
    paused: bool,
    finished: bool,
}

impl CareSession {
    /// Starts `routine` at its first exercise, running.
    pub fn new(routine: &'static CareRoutine) -> Self {
        Self {
            routine,
            step: 0,
            part: 0,
            left_secs: routine.steps[0].secs,
            paused: false,
            finished: false,
        }
    }

    /// The routine being followed.
    pub fn routine(&self) -> &'static CareRoutine {
        self.routine
    }

    /// Index of the current exercise.
    pub fn step_index(&self) -> usize {
        self.step
    }

    /// The current exercise.
    pub fn step(&self) -> &'static CareStep {
        &self.routine.steps[self.step]
    }

    /// For an exercise done on each side: 1 or 2. `None` otherwise.
    pub fn side(&self) -> Option<usize> {
        self.step().per_side.then_some(self.part + 1)
    }

    /// Seconds left on the current exercise (or side).
    pub fn left_secs(&self) -> f32 {
        self.left_secs
    }

    /// Whether the clock is stopped.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Whether every exercise is done.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Share of the routine's time done, in [0, 1].
    pub fn progress(&self) -> f32 {
        if self.finished {
            return 1.0;
        }
        let steps = self.routine.steps;
        let before: f32 = steps[..self.step].iter().map(CareStep::total_secs).sum();
        let step = self.step();
        let done = before + step.secs * self.part as f32 + (step.secs - self.left_secs);
        (done / self.routine.total_secs()).clamp(0.0, 1.0)
    }

    /// Stops the clock, or starts it again.
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    /// Lets `dt` seconds pass. Time left over when a part ends runs on into the next one.
    /// Returns the last cue reached.
    pub fn tick(&mut self, dt: f32) -> Option<CareCue> {
        if self.paused || self.finished {
            return None;
        }
        let mut dt = dt.max(0.0);
        let mut cue = None;
        while dt >= self.left_secs && !self.finished {
            dt -= self.left_secs;
            cue = Some(self.advance());
        }
        if !self.finished {
            self.left_secs -= dt;
        }
        cue
    }

    /// Skips to the next part (the other side, or the next exercise).
    pub fn skip(&mut self) -> CareCue {
        if self.finished {
            return CareCue::Finished;
        }
        self.advance()
    }

    /// Goes back to the start of the current part, or to the previous part when it has
    /// just started (within 2 s).
    pub fn previous(&mut self) {
        let fresh = self.step().secs - self.left_secs < 2.0;
        if self.finished {
            self.finished = false;
            self.step = self.routine.steps.len() - 1;
            self.part = self.step().parts() - 1;
        } else if fresh {
            if self.part > 0 {
                self.part -= 1;
            } else if self.step > 0 {
                self.step -= 1;
                self.part = self.step().parts() - 1;
            }
        }
        self.left_secs = self.step().secs;
    }

    fn advance(&mut self) -> CareCue {
        if self.part + 1 < self.step().parts() {
            self.part += 1;
            self.left_secs = self.step().secs;
            return CareCue::SwitchSides;
        }
        if self.step + 1 < self.routine.steps.len() {
            self.step += 1;
            self.part = 0;
            self.left_secs = self.step().secs;
            return CareCue::NextStep;
        }
        self.finished = true;
        self.left_secs = 0.0;
        CareCue::Finished
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn routine(id: &str) -> &'static CareRoutine {
        ROUTINES.iter().find(|r| r.id == id).unwrap()
    }

    #[test]
    fn routines_cover_before_after_and_daily_care_in_a_few_minutes() {
        let whens: Vec<CareWhen> = ROUTINES.iter().map(|r| r.when).collect();
        assert_eq!(
            whens,
            [
                CareWhen::BeforePlaying,
                CareWhen::AfterPlaying,
                CareWhen::Daily
            ]
        );
        // The guide aims at about 5 minutes for mobility and for stretching.
        assert_eq!(routine("mobility").total_secs(), 270.0);
        assert_eq!(routine("stretching").total_secs(), 400.0);
        assert_eq!(routine("posture").total_secs(), 450.0);
        for r in &ROUTINES {
            assert!(!r.steps.is_empty());
            for s in r.steps {
                assert!(
                    s.secs > 0.0 && !s.name.is_empty() && !s.how.is_empty(),
                    "{}",
                    s.name
                );
            }
        }
    }

    #[test]
    fn a_session_runs_each_side_then_each_exercise_and_finishes() {
        let mut s = CareSession::new(routine("stretching"));
        assert_eq!(
            (s.step_index(), s.side(), s.left_secs()),
            (0, Some(1), 20.0)
        );
        assert_eq!(s.tick(5.0), None);
        assert_eq!(s.left_secs(), 15.0);
        assert_eq!(s.tick(15.0), Some(CareCue::SwitchSides));
        assert_eq!(
            (s.step_index(), s.side(), s.left_secs()),
            (0, Some(2), 20.0)
        );
        // Leftover time runs on into the next exercise.
        assert_eq!(s.tick(23.0), Some(CareCue::NextStep));
        assert_eq!(
            (s.step_index(), s.side(), s.left_secs()),
            (1, Some(1), 17.0)
        );
        assert_eq!(s.tick(10_000.0), Some(CareCue::Finished));
        assert!(s.is_finished());
        assert_eq!(s.progress(), 1.0);
        assert_eq!(s.tick(1.0), None, "nothing happens after the end");

        let mut m = CareSession::new(routine("mobility"));
        assert_eq!(m.side(), None, "mobility moves both hands together");
        assert_eq!(m.tick(45.0), Some(CareCue::NextStep));
    }

    #[test]
    fn pausing_stops_the_clock() {
        let mut s = CareSession::new(routine("mobility"));
        s.toggle_pause();
        assert_eq!(s.tick(30.0), None);
        assert_eq!(s.left_secs(), 45.0);
        assert!(s.is_paused());
        s.toggle_pause();
        s.tick(30.0);
        assert_eq!(s.left_secs(), 15.0);
    }

    #[test]
    fn next_and_previous_move_between_parts() {
        let mut s = CareSession::new(routine("posture"));
        assert_eq!(s.skip(), CareCue::SwitchSides);
        assert_eq!(s.skip(), CareCue::NextStep);
        assert_eq!((s.step_index(), s.side()), (1, Some(1)));
        s.tick(10.0);
        s.previous();
        assert_eq!(
            (s.step_index(), s.side(), s.left_secs()),
            (1, Some(1), 30.0),
            "restart"
        );
        s.previous();
        assert_eq!(
            (s.step_index(), s.side()),
            (0, Some(2)),
            "just started: go back"
        );
        for _ in 0..30 {
            s.skip();
        }
        assert!(s.is_finished());
        s.previous();
        assert!(!s.is_finished());
        assert_eq!(s.step_index(), routine("posture").steps.len() - 1);
        CareSession::new(routine("posture")).previous();
    }

    #[test]
    fn progress_grows_with_time() {
        let mut s = CareSession::new(routine("stretching"));
        assert_eq!(s.progress(), 0.0);
        s.tick(100.0);
        assert!((s.progress() - 0.25).abs() < 1e-6, "{}", s.progress());
        s.tick(100.0);
        assert!((s.progress() - 0.5).abs() < 1e-6);
    }
}
