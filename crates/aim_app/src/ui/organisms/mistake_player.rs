//! Mistake player: step through a run's mistakes one at a time.
//!
//! Each mistake ([`aim_core::analytics::mistakes::Mistake`]) plays in four steps: (1) the
//! whole flick, animated; (2) only the wrong part, in slow motion, with a marker on the exact
//! frame it went wrong; (3) the tip; (4) a realistic corrected example, animated as a ghost
//! over the player's own path. The plot is the replay viewer's: crosshair offset from the
//! target, in degrees, painted with egui's painter.
//!
//! [`MistakePlayerState`] holds the mistakes, the current mistake and step, and the playback
//! clock. Time comes from egui's frame time; the clock, step logic, interpolation and plot
//! extent are pure and tested. Painting has no automated tests.

use super::replay_viewer::{paint_grid, plot_frame, to_screen};
use crate::i18n::{tr, tr_fmt, tr_named};
use crate::ui::atoms::{BAD, GOOD, WARN};
use aim_core::analytics::mistakes::{self, Mistake, MistakeKind, PathPoint};
use aim_core::scenario::Recording;
use bevy_egui::egui::{self, Align2, Color32, FontId, Pos2, Stroke, vec2};
use std::path::{Path, PathBuf};

/// Every string the player shows, in one place.
mod text {
    pub const NO_MISTAKES: &str =
        "No mistakes to replay in this run: every flick was clean. Hold runs are not covered.";
    pub const OPEN_SAVED: &str = "Step through this run's mistakes";
    pub const UNREADABLE: &str = "This replay could not be read.";
    pub const CLOSE: &str = "Close";
    pub const PREVIOUS_MISTAKE: &str = "◀ Previous mistake";
    pub const NEXT_MISTAKE: &str = "Next mistake ▶";
    pub const BACK: &str = "◀ Back";
    pub const NEXT: &str = "Next ▶";
    pub const PLAY: &str = "▶ Play";
    pub const PAUSE: &str = "⏸ Pause";
    pub const RESTART: &str = "⟲ Restart";
    pub const SPEED: &str = "Speed:";
    pub const STEP_TITLES: [&str; 4] = [
        "What you did",
        "Where it went wrong",
        "How to fix it",
        "How it looks done right",
    ];
    pub const STEP_HINTS: [&str; 4] = [
        "Your whole flick, from the moment the target could be aimed at to the shot.",
        "Only the wrong part, in slow motion. Watch for the red marker.",
        "What to change on this exact flick.",
        "A realistic corrected flick (green ghost) over yours (grey). Not perfect, just right.",
    ];
    pub const GHOST: &str = "Green: corrected example. Grey: your flick.";
    pub const HOVER: &str = "Crosshair offset from the target center, in degrees. Yellow \
        circle: the target. Red ring: where it went wrong. Hollow dot: start.";

    /// The marker label at the wrong moment.
    pub fn marker(kind: super::MistakeKind) -> &'static str {
        use super::MistakeKind::*;
        match kind {
            Overflick => "you passed the target here",
            Underflick => "your flick stopped short here",
            ExtraCorrections => "your first movement ended here",
            SlowReaction => "you only started moving here",
            LateClick => "you were on the target here, but did not click",
        }
    }

    use super::{tr_fmt, tr_named};

    pub fn counter(index: usize, count: usize) -> String {
        tr_fmt("Mistake {} of {}", &[&(index + 1), &count])
    }

    pub fn step(number: usize, title: &str) -> String {
        tr_named(
            "Step {number} of 4: {title}",
            &[("number", &number), ("title", &super::tr(title))],
        )
    }
}

/// Playback speeds offered, as multiples of real time.
const SPEEDS: [f32; 3] = [0.25, 0.5, 1.0];
/// Slow-motion speed of step 2.
const SLOW_SPEED: f32 = 0.25;
/// Largest frame time fed to the clock, so a stalled frame does not skip the animation.
const MAX_FRAME_SECS: f32 = 0.1;
/// The plot always shows at least this many target radii around the center.
const MIN_EXTENT_RADII: f32 = 2.0;
/// Margin around the farthest point, as a fraction of the extent.
const EXTENT_MARGIN: f32 = 0.1;
/// Smallest extent, degrees.
const MIN_EXTENT_DEG: f32 = 0.25;

/// The four steps of one mistake.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The whole flick, animated.
    #[default]
    FullMotion,
    /// The wrong part only, in slow motion, with a marker at the wrong frame.
    WrongMoment,
    /// The tip.
    Tip,
    /// The corrected example over the player's path.
    Corrected,
}

impl Step {
    const ALL: [Step; 4] = [
        Step::FullMotion,
        Step::WrongMoment,
        Step::Tip,
        Step::Corrected,
    ];

    /// 1-based position.
    pub fn number(self) -> usize {
        Self::ALL.iter().position(|&s| s == self).unwrap_or(0) + 1
    }

    pub fn next(self) -> Option<Step> {
        Self::ALL.get(self.number()).copied()
    }

    pub fn previous(self) -> Option<Step> {
        self.number().checked_sub(2).map(|i| Self::ALL[i])
    }

    /// Default playback speed of the step.
    fn speed(self) -> f32 {
        match self {
            Step::WrongMoment => SLOW_SPEED,
            Step::Corrected => 0.5,
            Step::FullMotion | Step::Tip => 1.0,
        }
    }
}

/// Playback clock over a time window, seconds since the flick's segment start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Clock {
    pub t: f32,
    pub playing: bool,
    pub speed: f32,
}

impl Clock {
    fn start(window: (f32, f32), speed: f32) -> Self {
        Self {
            t: window.0,
            playing: true,
            speed,
        }
    }

    /// Advance by a frame of `dt` real seconds; stops at the window's end.
    pub fn advance(&mut self, dt: f32, window: (f32, f32)) {
        if !self.playing {
            return;
        }
        self.t = (self.t + dt.clamp(0.0, MAX_FRAME_SECS) * self.speed).max(window.0);
        if self.t >= window.1 {
            self.t = window.1;
            self.playing = false;
        }
    }

    /// Play from the start when at the end, otherwise toggle pause.
    pub fn toggle(&mut self, window: (f32, f32)) {
        if !self.playing && self.t >= window.1 {
            self.t = window.0;
        }
        self.playing = !self.playing;
    }
}

/// The mistakes of one run and where the player is in them.
#[derive(Debug, Clone, PartialEq)]
pub struct MistakePlayerState {
    mistakes: Vec<Mistake>,
    index: usize,
    step: Step,
    clock: Clock,
}

impl MistakePlayerState {
    /// A player over `mistakes`, at the first step of the first one.
    pub fn new(mistakes: Vec<Mistake>) -> Self {
        let mut state = Self {
            mistakes,
            index: 0,
            step: Step::FullMotion,
            clock: Clock::start((0.0, 0.0), 1.0),
        };
        state.set_step(Step::FullMotion);
        state
    }

    /// A player over the mistakes of a recorded run.
    pub fn from_recording(recording: &Recording) -> Self {
        Self::new(mistakes::from_recording(recording))
    }

    pub fn is_empty(&self) -> bool {
        self.mistakes.is_empty()
    }

    pub fn current(&self) -> Option<&Mistake> {
        self.mistakes.get(self.index)
    }

    pub fn step(&self) -> Step {
        self.step
    }

    /// Show mistake `index` (clamped) from its first step.
    pub fn select(&mut self, index: usize) {
        self.index = index.min(self.mistakes.len().saturating_sub(1));
        self.set_step(Step::FullMotion);
    }

    /// Go to `step` of the current mistake and restart its playback.
    pub fn set_step(&mut self, step: Step) {
        self.step = step;
        let window = self.window();
        self.clock = Clock::start(window, step.speed());
        if step == Step::Tip {
            self.clock.t = window.1;
            self.clock.playing = false;
        }
    }

    /// The time window the current step plays, seconds since the segment start.
    pub fn window(&self) -> (f32, f32) {
        self.current()
            .map_or((0.0, 0.0), |m| step_window(m, self.step))
    }

    fn tick(&mut self, dt: f32) {
        let window = self.window();
        self.clock.advance(dt, window);
    }
}

/// The time window a step plays.
pub fn step_window(m: &Mistake, step: Step) -> (f32, f32) {
    let end = |p: &[PathPoint]| p.last().map_or(0.0, |p| p.time_secs);
    let player_end = end(&m.player);
    match step {
        Step::FullMotion | Step::Tip => (0.0, player_end),
        Step::WrongMoment => m.focus_secs(),
        Step::Corrected => (0.0, player_end.max(end(&m.corrected))),
    }
}

/// The path's offset at time `t`, interpolated between frames and held past either end.
pub fn point_at(path: &[PathPoint], t: f32) -> Option<(f32, f32)> {
    let first = path.first()?;
    if t <= first.time_secs {
        return Some(first.offset_deg);
    }
    let Some(j) = path.iter().position(|p| p.time_secs >= t) else {
        return path.last().map(|p| p.offset_deg);
    };
    let (a, b) = (path[j - 1], path[j]);
    let s = (t - a.time_secs) / (b.time_secs - a.time_secs).max(1e-6);
    Some((
        a.offset_deg.0 + (b.offset_deg.0 - a.offset_deg.0) * s,
        a.offset_deg.1 + (b.offset_deg.1 - a.offset_deg.1) * s,
    ))
}

/// Number of path points at or before `t`.
pub fn trail_len(path: &[PathPoint], t: f32) -> usize {
    path.iter().take_while(|p| p.time_secs <= t).count()
}

/// Half-width of the square plot, degrees: both paths fit with a margin, and the target is
/// never shown smaller than [`MIN_EXTENT_RADII`] radii.
pub fn extent_deg(m: &Mistake) -> f32 {
    let far = m.player.iter().chain(&m.corrected).fold(0.0_f32, |acc, p| {
        acc.max(p.offset_deg.0.abs()).max(p.offset_deg.1.abs())
    });
    (far * (1.0 + EXTENT_MARGIN))
        .max(m.radius_deg * MIN_EXTENT_RADII)
        .max(MIN_EXTENT_DEG)
}

/// What the user asked the player to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerAction {
    Close,
}

/// Draw the player. Returns [`PlayerAction::Close`] when the user closes it.
pub fn mistake_player(ui: &mut egui::Ui, state: &mut MistakePlayerState) -> Option<PlayerAction> {
    mistake_player_with(ui, state, |_, _, _| {})
}

/// [`mistake_player`] with `below_tip` drawn under the template tip, given the shown
/// mistake and its index (the results page adds "Ask the coach why" there).
pub fn mistake_player_with(
    ui: &mut egui::Ui,
    state: &mut MistakePlayerState,
    below_tip: impl FnOnce(&mut egui::Ui, usize, &Mistake),
) -> Option<PlayerAction> {
    let mut action = None;
    let count = state.mistakes.len();
    ui.horizontal(|ui| {
        if !state.is_empty() {
            if ui
                .add_enabled(
                    state.index > 0,
                    egui::Button::new(tr(text::PREVIOUS_MISTAKE)),
                )
                .clicked()
            {
                state.select(state.index - 1);
            }
            ui.strong(text::counter(state.index, count));
            if ui
                .add_enabled(
                    state.index + 1 < count,
                    egui::Button::new(tr(text::NEXT_MISTAKE)),
                )
                .clicked()
            {
                state.select(state.index + 1);
            }
        }
        if ui.button(tr(text::CLOSE)).clicked() {
            action = Some(PlayerAction::Close);
        }
    });
    let Some(m) = state.current().cloned() else {
        ui.small(tr(text::NO_MISTAKES));
        return action;
    };
    ui.colored_label(BAD, m.kind.label());

    let step = state.step();
    let i = step.number() - 1;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(step.previous().is_some(), egui::Button::new(tr(text::BACK)))
            .clicked()
            && let Some(s) = step.previous()
        {
            state.set_step(s);
        }
        ui.strong(text::step(step.number(), text::STEP_TITLES[i]));
        if ui
            .add_enabled(step.next().is_some(), egui::Button::new(tr(text::NEXT)))
            .clicked()
            && let Some(s) = step.next()
        {
            state.set_step(s);
        }
    });
    ui.small(tr(text::STEP_HINTS[i]));

    let step = state.step();
    if step == Step::Tip {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(&m.tip);
        });
        below_tip(ui, state.index, &m);
    } else {
        playback_controls(ui, state);
    }
    let dt = ui.input(|input| input.stable_dt);
    state.tick(dt);
    if state.clock.playing {
        ui.ctx().request_repaint();
    }
    plot(ui, &m, step, state.clock.t);
    if step == Step::Corrected {
        ui.small(tr(text::GHOST));
    }
    action
}

fn playback_controls(ui: &mut egui::Ui, state: &mut MistakePlayerState) {
    let window = state.window();
    ui.horizontal(|ui| {
        let label = if state.clock.playing {
            text::PAUSE
        } else {
            text::PLAY
        };
        if ui.button(tr(label)).clicked() {
            state.clock.toggle(window);
        }
        if ui.button(tr(text::RESTART)).clicked() {
            state.clock = Clock::start(window, state.clock.speed);
        }
        ui.label(tr(text::SPEED));
        for speed in SPEEDS {
            ui.selectable_value(&mut state.clock.speed, speed, format!("{speed}×"));
        }
    });
}

fn polyline(painter: &egui::Painter, points: &[Pos2], stroke: Stroke) {
    for pair in points.windows(2) {
        painter.line_segment([pair[0], pair[1]], stroke);
    }
}

fn crosshair(painter: &egui::Painter, p: Pos2, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    painter.circle_stroke(p, 4.0, stroke);
    painter.line_segment([p + vec2(-7.0, 0.0), p + vec2(7.0, 0.0)], stroke);
    painter.line_segment([p + vec2(0.0, -7.0), p + vec2(0.0, 7.0)], stroke);
}

fn plot(ui: &mut egui::Ui, m: &Mistake, step: Step, t: f32) {
    let (response, painter, area) = plot_frame(ui);
    let extent = extent_deg(m);
    let center = area.center();
    let px_per_deg = paint_grid(&painter, area, extent, "°");
    let screen = |path: &[PathPoint]| -> Vec<Pos2> {
        path.iter()
            .map(|p| to_screen(p.offset_deg, center, px_per_deg))
            .collect()
    };
    painter.circle_stroke(
        center,
        (m.radius_deg * px_per_deg).max(2.0),
        Stroke::new(1.5, WARN),
    );
    let player = screen(&m.player);
    let dim = Stroke::new(1.0, Color32::from_gray(90));
    let bright = Stroke::new(2.0, Color32::from_gray(230));
    if let Some(start) = player.first() {
        painter.circle_stroke(*start, 3.0, Stroke::new(1.0, Color32::from_gray(150)));
    }
    let shown = trail_len(&m.player, t);
    match step {
        Step::FullMotion => polyline(&painter, &player[..shown], bright),
        Step::WrongMoment => {
            polyline(&painter, &player, dim);
            let from = m.local(m.focus_start);
            polyline(&painter, &player[from..shown.max(from)], bright);
        }
        Step::Tip => polyline(&painter, &player, bright),
        Step::Corrected => {
            polyline(&painter, &player, dim);
            let ghost = screen(&m.corrected);
            polyline(
                &painter,
                &ghost[..trail_len(&m.corrected, t)],
                Stroke::new(2.0, GOOD),
            );
            if let Some(p) = point_at(&m.corrected, t) {
                crosshair(&painter, to_screen(p, center, px_per_deg), GOOD);
            }
        }
    }
    if step != Step::Corrected {
        if let Some(p) = point_at(&m.player, t) {
            crosshair(&painter, to_screen(p, center, px_per_deg), Color32::WHITE);
        }
        let reached = step == Step::Tip || t >= m.error_secs();
        if step != Step::FullMotion && reached {
            let p = player[m.local(m.error_at)];
            painter.circle_stroke(p, 9.0, Stroke::new(2.5, BAD));
            painter.text(
                p + vec2(0.0, -14.0),
                Align2::CENTER_BOTTOM,
                tr(text::marker(m.kind)),
                FontId::proportional(13.0),
                BAD,
            );
        }
    }
    response.on_hover_text(tr(text::HOVER));
}

/// The mistake player for the replay selected in the replay viewer.
#[derive(Debug, Default)]
pub struct SavedMistakes {
    /// The replay the player was opened for.
    path: Option<PathBuf>,
    /// `None` until opened; `Some(None)` when the replay could not be read.
    player: Option<Option<MistakePlayerState>>,
}

impl SavedMistakes {
    /// Close the player when another replay is selected.
    pub fn sync(&mut self, selected: Option<&Path>) {
        if self.path.as_deref() != selected {
            self.path = selected.map(Path::to_path_buf);
            self.player = None;
        }
    }

    fn open(&mut self, load: impl Fn(&Path) -> Option<Recording>) {
        self.player = Some(
            self.path
                .as_deref()
                .and_then(load)
                .map(|r| MistakePlayerState::from_recording(&r)),
        );
    }
}

/// A button that opens the mistake player for the selected saved replay, and the player.
pub fn saved_replay_mistakes(
    ui: &mut egui::Ui,
    state: &mut SavedMistakes,
    selected: Option<&Path>,
    load: impl Fn(&Path) -> Option<Recording>,
) {
    state.sync(selected);
    if state.path.is_none() {
        return;
    }
    match &mut state.player {
        None => {
            if ui.button(tr(text::OPEN_SAVED)).clicked() {
                state.open(load);
            }
        }
        Some(None) => {
            ui.colored_label(BAD, tr(text::UNREADABLE));
        }
        Some(Some(player)) => {
            if mistake_player(ui, player) == Some(PlayerAction::Close) {
                state.player = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(points: &[(f32, f32, f32)]) -> Vec<PathPoint> {
        points
            .iter()
            .map(|&(t, x, y)| PathPoint {
                time_secs: t,
                offset_deg: (x, y),
            })
            .collect()
    }

    fn mistake(kind: MistakeKind) -> Mistake {
        Mistake {
            kind,
            flick_index: 0,
            target_id: 1,
            radius_deg: 0.5,
            distance_deg: 10.0,
            direction_deg: (10.0, 0.0),
            segment_start: 100,
            focus_start: 102,
            error_at: 103,
            focus_end: 104,
            segment_end: 104,
            error_radii: Some(2.0),
            edge_error_deg: Some(0.5),
            delay_secs: None,
            corrections: 1,
            hit: true,
            tip: "tip".into(),
            player: path(&[
                (0.1, -10.0, 0.0),
                (0.2, -5.0, 0.0),
                (0.3, 0.0, 0.0),
                (0.4, 1.0, 0.0),
                (0.5, 0.0, 0.0),
            ]),
            corrected: path(&[(0.0, -10.0, 0.0), (0.3, -2.0, 0.0), (0.6, -0.2, 0.1)]),
        }
    }

    #[test]
    fn points_are_interpolated_and_held_past_the_ends() {
        let p = path(&[(0.0, 0.0, 0.0), (1.0, 2.0, -4.0)]);
        assert_eq!(point_at(&p, 0.5), Some((1.0, -2.0)));
        assert_eq!(point_at(&p, -1.0), Some((0.0, 0.0)));
        assert_eq!(point_at(&p, 3.0), Some((2.0, -4.0)));
        assert_eq!(point_at(&[], 0.0), None);
        assert_eq!(trail_len(&p, 0.5), 1);
        assert_eq!(trail_len(&p, 1.0), 2);
    }

    #[test]
    fn steps_run_one_to_four() {
        assert_eq!(Step::FullMotion.number(), 1);
        assert_eq!(Step::FullMotion.previous(), None);
        assert_eq!(Step::FullMotion.next(), Some(Step::WrongMoment));
        assert_eq!(Step::Tip.next(), Some(Step::Corrected));
        assert_eq!(Step::Corrected.next(), None);
        assert_eq!(Step::Corrected.previous(), Some(Step::Tip));
    }

    #[test]
    fn each_step_plays_its_own_window() {
        let m = mistake(MistakeKind::Overflick);
        assert_eq!(step_window(&m, Step::FullMotion), (0.0, 0.5));
        assert_eq!(step_window(&m, Step::WrongMoment), (0.3, 0.5));
        assert_eq!(step_window(&m, Step::Corrected), (0.0, 0.6));
        assert!((m.error_secs() - 0.4).abs() < 1e-6);
    }

    #[test]
    fn the_clock_plays_at_its_speed_and_stops_at_the_end() {
        let mut c = Clock::start((0.3, 0.5), 0.25);
        c.advance(0.4, (0.3, 0.5));
        // A stalled frame is clamped to 0.1 s, at quarter speed.
        assert!((c.t - 0.325).abs() < 1e-6, "{c:?}");
        for _ in 0..20 {
            c.advance(0.1, (0.3, 0.5));
        }
        assert_eq!(c.t, 0.5);
        assert!(!c.playing);
        c.toggle((0.3, 0.5));
        assert!(c.playing && c.t == 0.3, "replays from the start: {c:?}");
        c.toggle((0.3, 0.5));
        assert!(!c.playing);
    }

    #[test]
    fn changing_mistake_or_step_restarts_playback() {
        let mut s = MistakePlayerState::new(vec![
            mistake(MistakeKind::Overflick),
            mistake(MistakeKind::LateClick),
        ]);
        assert_eq!(s.step(), Step::FullMotion);
        s.set_step(Step::WrongMoment);
        assert_eq!(s.clock.t, 0.3);
        assert_eq!(s.clock.speed, SLOW_SPEED);
        s.tick(0.1);
        assert!(s.clock.t > 0.3);
        s.set_step(Step::Tip);
        assert!(!s.clock.playing, "the tip is static");
        s.select(1);
        assert_eq!(s.current().unwrap().kind, MistakeKind::LateClick);
        assert_eq!((s.step(), s.clock.t), (Step::FullMotion, 0.0));
        s.select(9);
        assert_eq!(s.index, 1, "clamped");
        assert!(MistakePlayerState::new(vec![]).is_empty());
    }

    #[test]
    fn the_extent_fits_both_paths_and_a_readable_target() {
        let m = mistake(MistakeKind::Overflick);
        assert!((extent_deg(&m) - 11.0).abs() < 1e-4);
        let mut small = m.clone();
        small.player = path(&[(0.1, 0.1, 0.0)]);
        small.corrected = path(&[(0.0, 0.2, 0.0)]);
        assert_eq!(extent_deg(&small), 1.0);
    }

    #[test]
    fn selecting_another_replay_closes_the_player() {
        let mut saved = SavedMistakes::default();
        let a = PathBuf::from("/r/1-x.aimreplay");
        saved.sync(Some(&a));
        saved.open(|_| None);
        assert_eq!(saved.player, Some(None), "unreadable replay");
        saved.sync(Some(&a));
        assert!(saved.player.is_some(), "same replay keeps it");
        saved.sync(Some(Path::new("/r/2-x.aimreplay")));
        assert!(saved.player.is_none());
    }

    #[test]
    fn the_player_chrome_reads_in_spanish() {
        use aim_core::i18n::{Lang, tr_fmt_in, tr_in, tr_named_in};
        let title = tr_in(Lang::Es, text::STEP_TITLES[1]);
        assert_eq!(
            tr_named_in(
                Lang::Es,
                "Step {number} of 4: {title}",
                &[("number", &2), ("title", &title)]
            ),
            "Paso 2 de 4: Dónde salió mal"
        );
        assert_eq!(
            tr_fmt_in(Lang::Es, "Mistake {} of {}", &[&1, &3]),
            "Error 1 de 3"
        );
        assert_eq!(
            tr_in(Lang::Es, text::marker(MistakeKind::LateClick)),
            "aquí estabas sobre el objetivo, pero no hiciste clic"
        );
        assert_eq!(text::counter(0, 3), "Mistake 1 of 3");
        assert_eq!(
            text::step(2, "Where it went wrong"),
            "Step 2 of 4: Where it went wrong"
        );
    }
}
