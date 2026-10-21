//! Difficulty scaling: derives easier or harder versions of a scenario from one definition.
//!
//! A level of `0` is the scenario as authored; `+1` is the hardest and `-1` the easiest
//! supported variant. Each knob is the fraction of change at level −1 (`easy`) and +1
//! (`hard`); a plain number sets both. A scenario tunes or disables knobs in its
//! `[difficulty]` table, e.g. `size = { easy = 0.5, hard = 0.1 }`. Training methods use fixed
//! offsets (drill, variant, "hard"); adaptive training moves the level continuously.

use super::def::{Motion, ScenarioDef, Shape, SpawnArea, TargetsDef};
use crate::math::Range;
use serde::{Deserialize, Serialize};

/// Lowest supported difficulty level (easiest).
pub const MIN_LEVEL: f32 = -1.0;
/// Highest supported difficulty level (hardest).
pub const MAX_LEVEL: f32 = 1.0;

/// How far one parameter moves at the ends of the level range.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged, try_from = "RawKnob")]
pub enum Knob {
    /// The same fraction at level −1 and +1.
    Symmetric(f32),
    /// Fraction of change at level −1 (`easy`) and at +1 (`hard`).
    Bounds {
        /// Fraction of change at level -1.
        easy: f32,
        /// Fraction of change at level +1.
        hard: f32,
    },
}

/// What a knob may be written as. The table form rejects unknown keys, which an untagged
/// struct variant alone would silently drop.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawKnob {
    Symmetric(f32),
    Bounds(KnobBounds),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KnobBounds {
    easy: f32,
    hard: f32,
}

impl From<RawKnob> for Knob {
    fn from(raw: RawKnob) -> Self {
        match raw {
            RawKnob::Symmetric(c) => Knob::Symmetric(c),
            RawKnob::Bounds(KnobBounds { easy, hard }) => Knob::Bounds { easy, hard },
        }
    }
}

impl Knob {
    /// Knob that changes by the same fraction at both ends of the level range.
    pub const fn symmetric(fraction: f32) -> Self {
        Knob::Symmetric(fraction)
    }

    /// Signed fraction of change at `level` (negative when easier).
    pub fn at(self, level: f32) -> f32 {
        let (easy, hard) = self.bounds();
        if level >= 0.0 {
            hard * level
        } else {
            easy * level
        }
    }

    fn bounds(self) -> (f32, f32) {
        match self {
            Knob::Symmetric(c) => (c, c),
            Knob::Bounds { easy, hard } => (easy, hard),
        }
    }
}

/// How each scenario parameter scales with the difficulty level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct DifficultyDef {
    /// Sphere radius shrinks by this fraction when harder (grows when easier).
    pub size: Knob,
    /// Drift speed grows, and direction changes come sooner, by this fraction when harder.
    pub speed: Knob,
    /// Target lifetime shrinks by this fraction when harder.
    pub lifetime: Knob,
    /// Humanoid spawn distance grows by this fraction when harder (smaller heads).
    pub distance: Knob,
    /// Crosshair-relative flick offsets grow by this fraction when harder.
    pub offset: Knob,
    /// Targets alive at once grow by this fraction when harder (rounded, at least one).
    /// Off by default: more targets is not harder in every scenario.
    pub count: Knob,
}

impl Default for DifficultyDef {
    fn default() -> Self {
        Self {
            size: Knob::symmetric(0.3),
            speed: Knob::symmetric(0.3),
            lifetime: Knob::symmetric(0.25),
            distance: Knob::symmetric(0.25),
            offset: Knob::symmetric(0.2),
            count: Knob::symmetric(0.0),
        }
    }
}

impl DifficultyDef {
    /// Whether every knob is finite.
    pub fn is_valid(&self) -> bool {
        [
            self.size,
            self.speed,
            self.lifetime,
            self.distance,
            self.offset,
            self.count,
        ]
        .iter()
        .all(|k| {
            let (easy, hard) = k.bounds();
            [easy, hard]
                .iter()
                .all(|c| c.is_finite() && (0.0..0.95).contains(c))
        })
    }
}

fn scale(r: Range, k: f32) -> Range {
    Range::new(r.min * k, r.max * k)
}

/// Key used to store results: plain id at level 0, `id@+0.50` otherwise, so scores at
/// different difficulties never mix.
pub fn run_key(id: &str, level: f32) -> String {
    if level.abs() < 1e-3 {
        id.to_owned()
    } else {
        format!("{id}@{level:+.2}")
    }
}

/// Inverse of [`run_key`] (and of [`ScenarioDef::with_duration`]): base scenario id and
/// level (`0` when the key has no level). A `~20s` duration suffix is dropped.
pub fn parse_run_key(key: &str) -> (&str, f32) {
    let (base, level, _) = parse_run_key_full(key);
    (base, level)
}

/// Base id, level and shortened duration (`~20s`) encoded in a run key. Tuning and
/// floor-height suffixes (`%s80d120v150`, `^+1.50m`) are dropped; [`RunKey::parse`] reads
/// them.
pub fn parse_run_key_full(key: &str) -> (&str, f32, Option<f32>) {
    parse_level_and_secs(split_tuning(split_elevation(key).0).0)
}

fn parse_level_and_secs(key: &str) -> (&str, f32, Option<f32>) {
    let key = split_end(key).0;
    let (rest, secs) = match key.rsplit_once('~') {
        Some((rest, d)) => match d.strip_suffix('s').and_then(|n| n.parse::<f32>().ok()) {
            Some(secs) if secs.is_finite() && secs > 0.0 => (rest, Some(secs)),
            _ => (key, None),
        },
        None => (key, None),
    };
    match rest.rsplit_once('@') {
        Some((id, level)) => match level.parse::<f32>() {
            Ok(l) if l.is_finite() => (id, l, secs),
            _ => (rest, 0.0, secs),
        },
        None => (rest, 0.0, secs),
    }
}

/// How the player chose a run to end, in place of the scenario's own end
/// ([`ScenarioDef::with_end_choice`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndChoice {
    /// A race to this many kills (`#100k`).
    Kills(u32),
    /// A timed run of this many seconds (`#60s`, or `~60s` for a scenario that already ends
    /// by time).
    Time(u32),
}

impl EndChoice {
    fn suffix(self) -> String {
        match self {
            Self::Kills(n) => format!("#{n}k"),
            Self::Time(s) => format!("#{s}s"),
        }
    }
}

/// Splits a trailing `#100k` or `#60s` end-choice suffix off a run key (without tuning or
/// height).
fn split_end(key: &str) -> (&str, Option<EndChoice>) {
    let Some((rest, e)) = key.rsplit_once('#') else {
        return (key, None);
    };
    let n = |unit: char| e.strip_suffix(unit).and_then(|n| n.parse::<u32>().ok());
    match (n('k'), n('s')) {
        (Some(k), _) if k > 0 => (rest, Some(EndChoice::Kills(k))),
        (_, Some(s)) if s > 0 => (rest, Some(EndChoice::Time(s))),
        _ => (key, None),
    }
}

/// Seconds allowed per kill when a player turns a scenario into a kill goal
/// ([`ScenarioDef::with_kill_goal`]): the run fails if the goal takes longer. A design
/// choice (estimate): about three times the 1–1.5 s per kill of the movement drills.
pub const KILL_GOAL_SECS_PER_KILL: f32 = 4.0;
/// Points per second left under the time cap when a kill goal is reached, so faster runs
/// score more (estimate: one second saved is worth about one hit).
pub const KILL_GOAL_FINISH_BONUS: f32 = 100.0;
/// Largest kill goal a player can pick.
pub const MAX_KILL_GOAL: u32 = 500;
/// Shortest and longest time limit a player can pick, seconds.
pub const TIME_LIMIT_SECS: (u32, u32) = (10, 1200);

/// Every part of a run key: `id@+0.50~20s%s80d120v150^+1.50m` is the base id, the
/// difficulty level, the shortened duration (or a `#100k` / `#60s` end choice in its
/// place), the in-run tuning and the player's floor height, each optional and in that
/// order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunKey<'a> {
    /// Base scenario id.
    pub base: &'a str,
    /// Difficulty level (`0` when the key has none).
    pub level: f32,
    /// Shortened duration in seconds (`~20s`), if any.
    pub secs: Option<f32>,
    /// The player's end choice (`#100k` or `#60s`), if any.
    pub end: Option<EndChoice>,
    /// Target size, distance and speed tuning (`%s80d120v150`); default when absent.
    pub tuning: Tuning,
    /// Player floor height in meters (`^+1.50m`), when it differs from the scenario file.
    pub elevation: Option<f32>,
}

impl<'a> RunKey<'a> {
    /// Splits a run key made by [`run_key`], [`ScenarioDef::with_duration`],
    /// [`ScenarioDef::with_tuning`] and [`ScenarioDef::with_elevation`].
    pub fn parse(key: &'a str) -> Self {
        let (rest, elevation) = split_elevation(key);
        let (rest, tuning) = split_tuning(rest);
        let end = split_end(rest).1;
        let (base, level, secs) = parse_level_and_secs(rest);
        RunKey {
            base,
            level,
            secs,
            end,
            tuning: tuning.unwrap_or_default(),
            elevation,
        }
    }
}

/// In-run tuning of a scenario's targets, in percent of the scenario's own values. Tuned
/// runs are kept apart in the history by a `%s80d120v150` run-key suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tuning {
    /// Sphere radius. Humanoids keep their VALORANT size (see [`Tuning::size_applies`]).
    pub size_pct: u16,
    /// Spawn distance from the player.
    pub distance_pct: u16,
    /// Target movement speed (drift speed; strafe run speed and acceleration).
    pub speed_pct: u16,
}

impl Default for Tuning {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Tuning {
    /// The scenario as authored.
    pub const DEFAULT: Tuning = Tuning {
        size_pct: 100,
        distance_pct: 100,
        speed_pct: 100,
    };
    /// Lowest percentage of any knob.
    pub const MIN_PCT: u16 = 50;
    /// Highest percentage of any knob.
    pub const MAX_PCT: u16 = 200;
    /// Step the in-run controls use.
    pub const STEP_PCT: u16 = 10;

    /// Whether this is the scenario as authored.
    pub fn is_default(&self) -> bool {
        *self == Self::DEFAULT
    }

    /// Whether the size knob changes `def`: only spheres and capsules scale; humanoids keep the
    /// VALORANT agent size (move them farther or closer with the distance knob instead).
    pub fn size_applies(def: &ScenarioDef) -> bool {
        matches!(
            def.targets.shape,
            Shape::Sphere { .. } | Shape::Capsule { .. }
        )
    }

    fn clamped(self) -> Self {
        let c = |p: u16| p.clamp(Self::MIN_PCT, Self::MAX_PCT);
        Tuning {
            size_pct: c(self.size_pct),
            distance_pct: c(self.distance_pct),
            speed_pct: c(self.speed_pct),
        }
    }

    fn suffix(&self) -> String {
        format!(
            "%s{}d{}v{}",
            self.size_pct, self.distance_pct, self.speed_pct
        )
    }

    /// Inverse of [`Tuning::suffix`] (without the `%`).
    fn parse(s: &str) -> Option<Self> {
        let s = s.strip_prefix('s')?;
        let (size, s) = s.split_once('d')?;
        let (distance, speed) = s.split_once('v')?;
        Some(Tuning {
            size_pct: size.parse().ok()?,
            distance_pct: distance.parse().ok()?,
            speed_pct: speed.parse().ok()?,
        })
    }
}

/// Splits a trailing `%s80d120v150` tuning suffix off a run key (without its height).
fn split_tuning(key: &str) -> (&str, Option<Tuning>) {
    match key.rsplit_once('%') {
        Some((rest, t)) => match Tuning::parse(t) {
            Some(t) => (rest, Some(t)),
            None => (key, None),
        },
        None => (key, None),
    }
}

/// Splits a trailing `^+1.50m` floor-height suffix off a run key.
fn split_elevation(key: &str) -> (&str, Option<f32>) {
    match key.rsplit_once('^') {
        Some((rest, e)) => match e.strip_suffix('m').and_then(|n| n.parse::<f32>().ok()) {
            Some(m) if m.is_finite() => (rest, Some(m)),
            _ => (key, None),
        },
        None => (key, None),
    }
}

impl ScenarioDef {
    /// This scenario with the player's floor `m` meters above the targets' floor. Apply it
    /// last: the id gains a `^+1.50m` suffix (replacing an earlier one) so scores at
    /// different heights never mix. The height the definition already has keeps the id.
    pub fn with_elevation(&self, m: f32) -> ScenarioDef {
        let mut out = self.clone();
        if (m - self.player.elevation).abs() >= 5e-3 {
            out.player.elevation = m;
            out.id = format!("{}^{m:+.2}m", split_elevation(&self.id).0);
        }
        out
    }

    /// This scenario with its targets tuned (each knob clamped to
    /// [`Tuning::MIN_PCT`]..=[`Tuning::MAX_PCT`]): sphere radius, spawn distance and motion
    /// speed scale by their percentage. The id gains a `%s80d120v150` suffix, before any
    /// height suffix, so tuned scores never mix; the default tuning keeps the id.
    pub fn with_tuning(&self, tuning: Tuning) -> ScenarioDef {
        let t = tuning.clamped();
        if t.is_default() {
            return self.clone();
        }
        let mut out = self.clone();
        let (rest, elevation) = match self.id.rsplit_once('^') {
            Some((rest, e)) if split_elevation(&self.id).1.is_some() => (rest, format!("^{e}")),
            _ => (self.id.as_str(), String::new()),
        };
        out.id = format!("{rest}{}{elevation}", t.suffix());
        out.transform_targets(|targets| tune_targets(targets, t));
        out
    }

    /// This scenario shortened (or lengthened) to `secs`. The id gains a `~20s` suffix so
    /// short drills never mix with full-length scores.
    pub fn with_duration(&self, secs: f32) -> ScenarioDef {
        let mut out = self.clone();
        if (secs - self.duration_secs).abs() >= 0.5 && secs > 0.0 {
            out.duration_secs = secs;
            out.id = format!("{}~{}s", self.id, secs.round() as u32);
        }
        out
    }

    /// This scenario as a race to `kills` kills (clamped to 1..=[`MAX_KILL_GOAL`]): the run
    /// ends at the goal and fails if it takes longer than [`KILL_GOAL_SECS_PER_KILL`] per
    /// kill (never less than the scenario's own length), and every second left scores
    /// [`KILL_GOAL_FINISH_BONUS`]. Apply it where [`Self::with_duration`] goes: the id gains
    /// a `#100k` suffix so kill-goal scores never mix with timed ones.
    pub fn with_kill_goal(&self, kills: u32) -> ScenarioDef {
        let n = kills.clamp(1, MAX_KILL_GOAL);
        let mut out = self.clone();
        out.end = super::RunEnd::Kills { kills: n };
        out.duration_secs = self.duration_secs.max(n as f32 * KILL_GOAL_SECS_PER_KILL);
        out.scoring.finish_bonus_per_sec = KILL_GOAL_FINISH_BONUS;
        out.id = format!("{}{}", self.id, EndChoice::Kills(n).suffix());
        out
    }

    /// This scenario as a timed run of `secs` seconds (clamped to [`TIME_LIMIT_SECS`]). A
    /// scenario that already ends by time is just lengthened or shortened
    /// ([`Self::with_duration`], `~60s`); one that ends by kills or never ends becomes a
    /// timed run with a `#60s` suffix.
    pub fn with_time_limit(&self, secs: u32) -> ScenarioDef {
        let s = secs.clamp(TIME_LIMIT_SECS.0, TIME_LIMIT_SECS.1);
        if self.end.is_time() {
            return self.with_duration(s as f32);
        }
        let mut out = self.clone();
        out.end = super::RunEnd::Time;
        out.duration_secs = s as f32;
        out.id = format!("{}{}", self.id, EndChoice::Time(s).suffix());
        out
    }

    /// This scenario ending the way the player chose.
    pub fn with_end_choice(&self, choice: EndChoice) -> ScenarioDef {
        match choice {
            EndChoice::Kills(n) => self.with_kill_goal(n),
            EndChoice::Time(s) => self.with_time_limit(s),
        }
    }

    /// This scenario at difficulty `level` (clamped to [`MIN_LEVEL`], [`MAX_LEVEL`]).
    /// The id becomes [`run_key`] so history keeps difficulties apart.
    pub fn at_difficulty(&self, level: f32) -> ScenarioDef {
        let d = level.clamp(MIN_LEVEL, MAX_LEVEL);
        let mut out = self.clone();
        if d == 0.0 {
            return out;
        }
        out.id = run_key(&self.id, d);
        let c = &self.difficulty;
        out.transform_targets(|targets| scale_targets(targets, c, d));
        out
    }
}

/// Applies `tuning` to `targets`: sphere radius, spawn distance and motion speed.
fn tune_targets(targets: &mut TargetsDef, t: Tuning) {
    let k = |pct: u16| f32::from(pct) / 100.0;
    scale_size(&mut targets.shape, k(t.size_pct));
    let dist = k(t.distance_pct);
    match &mut targets.spawn {
        SpawnArea::Angular { distance, .. } | SpawnArea::AroundCrosshair { distance, .. } => {
            *distance = scale(*distance, dist)
        }
        // Floor boxes extend away from the player along -Z.
        SpawnArea::Floor { z, .. } => *z = scale(*z, dist),
    }
    targets.speed_scale *= k(t.speed_pct);
}

/// Scales a sphere or capsule by `k` (every length of a capsule, so it keeps its
/// proportions); humanoids keep the VALORANT agent size.
fn scale_size(shape: &mut Shape, k: f32) {
    match shape {
        Shape::Sphere { radius } => *radius *= k,
        Shape::Capsule {
            radius,
            height,
            head,
        } => {
            *radius *= k;
            *height *= k;
            if let Some(h) = head {
                h.radius *= k;
                h.offset *= k;
            }
        }
        Shape::Humanoid { .. } => {}
    }
}

/// Scales `motion` by the speed knob's fraction `s` at this level: faster movement and
/// sooner decisions for `s > 0`.
fn scale_motion(motion: &mut Motion, s: f32) {
    let sooner = |r: &mut Range| *r = scale(*r, 1.0 - s);
    if let Some(depth) = motion_depth_mut(motion) {
        sooner(&mut depth.change_secs);
    }
    match motion {
        Motion::Static => {}
        Motion::Strafe { change_secs, .. } => sooner(change_secs),
        Motion::Dodge {
            change_secs,
            juke_secs,
            ..
        } => {
            sooner(change_secs);
            sooner(juke_secs);
        }
        Motion::Drift {
            speed, change_secs, ..
        } => {
            *speed = scale(*speed, 1.0 + s);
            sooner(change_secs);
        }
        Motion::Ballistic { speed, gravity, .. } => {
            // The same arcs, flown faster: speed × k and gravity × k².
            *speed = scale(*speed, 1.0 + s);
            *gravity *= (1.0 + s) * (1.0 + s);
        }
        Motion::LinearBounce { speed_deg, .. } => *speed_deg = scale(*speed_deg, 1.0 + s),
        Motion::Orbit {
            speed_deg,
            change_secs,
            ..
        } => {
            *speed_deg = scale(*speed_deg, 1.0 + s);
            if let Some(c) = change_secs {
                sooner(c);
            }
        }
        Motion::Wave {
            speed,
            frequency_hz,
            ..
        } => {
            // The same path, travelled faster.
            *speed = scale(*speed, 1.0 + s);
            *frequency_hz = scale(*frequency_hz, 1.0 + s);
        }
        Motion::Path {
            speed, pause_secs, ..
        } => {
            *speed = scale(*speed, 1.0 + s);
            sooner(pause_secs);
        }
    }
}

/// The forward/back block of a motion, mutably, when it has one.
fn motion_depth_mut(m: &mut Motion) -> Option<&mut super::def::DepthDef> {
    match m {
        Motion::Strafe { depth, .. }
        | Motion::Dodge { depth, .. }
        | Motion::Drift { depth, .. } => depth.as_mut(),
        _ => None,
    }
}

/// Scales `targets` by the difficulty coefficients `c` at level `d` (not 0).
fn scale_targets(targets: &mut TargetsDef, c: &DifficultyDef, d: f32) {
    let humanoid = matches!(targets.shape, Shape::Humanoid { .. });

    scale_size(&mut targets.shape, 1.0 - c.size.at(d));
    if let Some(l) = &mut targets.lifetime_secs {
        *l *= 1.0 - c.lifetime.at(d);
    }
    scale_motion(&mut targets.motion, c.speed.at(d));
    for p in &mut targets.motion_profiles {
        scale_motion(&mut p.motion, c.speed.at(d));
    }
    let count = c.count.at(d);
    if count != 0.0 {
        let alive = targets.max_alive as f32 * (1.0 + count);
        targets.max_alive = (alive.round() as u32).clamp(1, 32);
    }
    let dist_k = if humanoid {
        1.0 + c.distance.at(d)
    } else {
        1.0
    };
    match &mut targets.spawn {
        SpawnArea::Angular { distance, .. } => *distance = scale(*distance, dist_k),
        SpawnArea::AroundCrosshair {
            offset_deg,
            distance,
            on_screen,
            ..
        } => {
            *distance = scale(*distance, dist_k);
            let k = 1.0 + c.offset.at(d);
            let cap = if *on_screen {
                crate::fov::valorant_on_screen_half_h_deg() as f32
            } else {
                180.0
            };
            *offset_deg = Range::new((offset_deg.min * k).min(cap), (offset_deg.max * k).min(cap));
        }
        SpawnArea::Floor { z, .. } => {
            // Floor boxes extend away from the player along -Z.
            *z = scale(*z, dist_k);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::builtin;

    fn find(id: &str) -> ScenarioDef {
        builtin::all().into_iter().find(|d| d.id == id).unwrap()
    }

    #[test]
    fn level_zero_is_identity_and_keys_encode_level() {
        let d = find("micro-adjust");
        assert_eq!(d.at_difficulty(0.0), d);
        assert_eq!(run_key("x", 0.0), "x");
        assert_eq!(run_key("x", 0.5), "x@+0.50");
        assert_eq!(run_key("x", -0.25), "x@-0.25");
        assert_eq!(d.at_difficulty(0.5).id, "micro-adjust@+0.50");
        assert_eq!(parse_run_key("micro-adjust@+0.50"), ("micro-adjust", 0.5));
        assert_eq!(parse_run_key("micro-adjust@-0.25"), ("micro-adjust", -0.25));
        assert_eq!(parse_run_key("micro-adjust"), ("micro-adjust", 0.0));
        assert_eq!(parse_run_key("odd@name"), ("odd@name", 0.0));
        let short = d.at_difficulty(0.25).with_duration(20.0);
        assert_eq!(short.id, "micro-adjust@+0.25~20s");
        assert_eq!(short.duration_secs, 20.0);
        assert_eq!(
            parse_run_key_full(&short.id),
            ("micro-adjust", 0.25, Some(20.0))
        );
        assert_eq!(
            parse_run_key_full("micro-adjust~20s"),
            ("micro-adjust", 0.0, Some(20.0))
        );
        assert_eq!(
            d.with_duration(d.duration_secs),
            d,
            "same duration keeps the id"
        );
    }

    #[test]
    fn harder_on_screen_flicks_stay_inside_the_screen() {
        let limit = crate::fov::valorant_on_screen_half_h_deg() as f32;
        for id in ["flick-wide", "flick-headshot"] {
            let d = find(id);
            let h = d.at_difficulty(MAX_LEVEL);
            let SpawnArea::AroundCrosshair {
                offset_deg,
                on_screen,
                ..
            } = h.targets.spawn
            else {
                panic!("{id} must spawn around the crosshair")
            };
            assert!(on_screen, "{id}");
            assert!(offset_deg.max <= limit, "{id}: {offset_deg:?}");
            // The run key (`id@+1.00`) is not a valid id; every other field must pass.
            let errs: Vec<_> = h
                .validate()
                .into_iter()
                .filter(|e| e.field != "id")
                .collect();
            assert!(errs.is_empty(), "{id}: {errs:?}");
        }
    }

    #[test]
    fn a_kill_goal_ends_the_run_at_n_kills_and_keeps_its_scores_apart() {
        use crate::scenario::RunEnd;
        let d = find("counter-strafe");
        let goal = d.at_difficulty(0.5).with_kill_goal(100);
        assert_eq!(goal.id, "counter-strafe@+0.50#100k");
        assert_eq!(goal.end, RunEnd::Kills { kills: 100 });
        assert_eq!(
            goal.duration_secs,
            100.0 * KILL_GOAL_SECS_PER_KILL,
            "a time cap"
        );
        assert_eq!(goal.scoring.finish_bonus_per_sec, KILL_GOAL_FINISH_BONUS);
        let errs: Vec<_> = goal
            .validate()
            .into_iter()
            .filter(|e| e.field != "id")
            .collect();
        assert!(errs.is_empty(), "{errs:?}");
        // A short goal never cuts the scenario's own length.
        assert_eq!(d.with_kill_goal(3).duration_secs, d.duration_secs);

        let full = goal
            .with_tuning(Tuning {
                size_pct: 80,
                ..Tuning::DEFAULT
            })
            .with_elevation(1.5);
        assert_eq!(full.id, "counter-strafe@+0.50#100k%s80d100v100^+1.50m");
        let key = RunKey::parse(&full.id);
        assert_eq!(
            (key.base, key.level, key.secs, key.end),
            ("counter-strafe", 0.5, None, Some(EndChoice::Kills(100)))
        );
        assert_eq!(parse_run_key_full(&full.id), ("counter-strafe", 0.5, None));
        assert_eq!(parse_run_key("counter-strafe#30k"), ("counter-strafe", 0.0));
        assert_eq!(RunKey::parse("counter-strafe").end, None);
        assert_eq!(RunKey::parse("odd#name").base, "odd#name", "not a goal");
        let lib = builtin::all();
        assert_eq!(
            crate::analytics::proof::variant_for_key(&lib, &full.id),
            Some(full)
        );
    }

    #[test]
    fn a_time_limit_turns_any_scenario_into_a_timed_run() {
        use crate::scenario::RunEnd;
        // Already timed: just a duration, with the usual `~` key.
        let timed = find("counter-strafe");
        assert_eq!(timed.with_time_limit(30), timed.with_duration(30.0));
        assert_eq!(timed.with_time_limit(30).id, "counter-strafe~30s");
        // A kill race becomes a timed run, kept apart by `#60s`.
        let race = find("phase-kill-race");
        let t = race.at_difficulty(0.25).with_time_limit(90);
        assert_eq!(t.id, "phase-kill-race@+0.25#90s");
        assert_eq!((t.end, t.duration_secs), (RunEnd::Time, 90.0));
        let key = RunKey::parse(&t.id);
        assert_eq!(
            (key.base, key.end),
            ("phase-kill-race", Some(EndChoice::Time(90)))
        );
        assert_eq!(parse_run_key_full(&t.id), ("phase-kill-race", 0.25, None));
        let lib = builtin::all();
        assert_eq!(
            crate::analytics::proof::variant_for_key(&lib, &t.id),
            Some(t)
        );
        assert_eq!(race.with_time_limit(1).duration_secs, 10.0, "clamped");
        assert_eq!(
            race.with_end_choice(EndChoice::Kills(5)),
            race.with_kill_goal(5)
        );
        assert_eq!(
            race.with_end_choice(EndChoice::Time(60)),
            race.with_time_limit(60)
        );
    }

    #[test]
    fn elevation_keys_keep_heights_apart_and_parse_back() {
        let d = find("counter-strafe");
        assert_eq!(d.with_elevation(0.0), d, "the authored height keeps the id");
        let up = d.at_difficulty(0.5).with_duration(20.0).with_elevation(1.5);
        assert_eq!(up.id, "counter-strafe@+0.50~20s^+1.50m");
        assert_eq!(up.player.elevation, 1.5);
        assert_eq!(up.duration_secs, 20.0);
        assert_eq!(
            parse_run_key_full(&up.id),
            ("counter-strafe", 0.5, Some(20.0)),
            "existing parsers ignore the height"
        );
        assert_eq!(parse_run_key(&up.id), ("counter-strafe", 0.5));
        let key = RunKey::parse(&up.id);
        assert_eq!(key.base, "counter-strafe");
        assert_eq!(key.level, 0.5);
        assert_eq!(key.secs, Some(20.0));
        assert_eq!(key.elevation, Some(1.5));
        let down = d.with_elevation(-2.0);
        assert_eq!(down.id, "counter-strafe^-2.00m");
        assert_eq!(RunKey::parse(&down.id).elevation, Some(-2.0));
        assert_eq!(RunKey::parse("counter-strafe").elevation, None);
        assert_eq!(
            up.with_elevation(3.0).id,
            "counter-strafe@+0.50~20s^+3.00m",
            "a new height replaces the old suffix"
        );
        assert_eq!(RunKey::parse("odd^name").base, "odd^name");
    }

    fn tuning(size_pct: u16, distance_pct: u16, speed_pct: u16) -> Tuning {
        Tuning {
            size_pct,
            distance_pct,
            speed_pct,
        }
    }

    #[test]
    fn default_tuning_changes_nothing_and_adds_no_suffix() {
        let d = find("micro-adjust").at_difficulty(0.5).with_duration(20.0);
        assert!(Tuning::DEFAULT.is_default());
        assert_eq!(d.with_tuning(Tuning::DEFAULT), d);
        assert_eq!(RunKey::parse(&d.id).tuning, Tuning::DEFAULT);
    }

    #[test]
    fn tuning_scales_sphere_size_spawn_distance_and_target_speed() {
        let d = find("track-reactive-micro");
        let t = d.with_tuning(tuning(150, 50, 120));
        assert!((radius(&t) / radius(&d) - 1.5).abs() < 1e-5);
        let (SpawnArea::Angular { distance: d0, .. }, SpawnArea::Angular { distance: d1, .. }) =
            (&d.targets.spawn, &t.targets.spawn)
        else {
            panic!("angular spawn")
        };
        assert!((d1.min / d0.min - 0.5).abs() < 1e-5);
        assert!((d1.max / d0.max - 0.5).abs() < 1e-5);
        assert!((t.targets.speed_scale - 1.2).abs() < 1e-5);
        assert_eq!(
            t.targets.motion, d.targets.motion,
            "motion speed is a multiplier"
        );
        assert_eq!(t.id, "track-reactive-micro%s150d50v120");
    }

    #[test]
    fn humanoids_keep_their_valorant_size_but_take_distance() {
        let d = find("dynamic-strafe-heads");
        let t = d.with_tuning(tuning(200, 150, 100));
        assert_eq!(
            t.targets.shape, d.targets.shape,
            "agent size stays accurate"
        );
        assert_ne!(t.targets.spawn, d.targets.spawn);
        assert!(!Tuning::size_applies(&d));
        assert!(Tuning::size_applies(&find("micro-adjust")));
    }

    #[test]
    fn tuning_clamps_to_its_range_and_keeps_every_builtin_valid() {
        let d = find("micro-adjust");
        let t = d.with_tuning(tuning(10, 999, 0));
        assert_eq!(
            RunKey::parse(&t.id).tuning,
            tuning(Tuning::MIN_PCT, Tuning::MAX_PCT, Tuning::MIN_PCT)
        );
        for d in builtin::all() {
            for t in [tuning(50, 50, 50), tuning(200, 200, 200)] {
                let errs: Vec<_> = d
                    .with_tuning(t)
                    .validate()
                    .into_iter()
                    .filter(|e| e.field != "id")
                    .collect();
                assert!(errs.is_empty(), "{} {t:?}: {errs:?}", d.id);
            }
        }
    }

    #[test]
    fn tuning_keys_round_trip_with_every_other_suffix() {
        let d = find("counter-strafe");
        let full = d
            .at_difficulty(0.5)
            .with_duration(20.0)
            .with_tuning(tuning(80, 120, 150))
            .with_elevation(1.5);
        assert_eq!(full.id, "counter-strafe@+0.50~20s%s80d120v150^+1.50m");
        let key = RunKey::parse(&full.id);
        assert_eq!(key.base, "counter-strafe");
        assert_eq!(key.level, 0.5);
        assert_eq!(key.secs, Some(20.0));
        assert_eq!(key.elevation, Some(1.5));
        assert_eq!(key.tuning, tuning(80, 120, 150));
        assert_eq!(
            parse_run_key_full(&full.id),
            ("counter-strafe", 0.5, Some(20.0)),
            "existing parsers ignore the tuning"
        );
        let raised = d.with_elevation(1.5).with_tuning(tuning(100, 100, 50));
        assert_eq!(
            raised.id, "counter-strafe%s100d100v50^+1.50m",
            "the tuning goes before a height suffix"
        );
        assert_eq!(RunKey::parse("odd%name").base, "odd%name");
        assert_eq!(RunKey::parse("odd%name").tuning, Tuning::DEFAULT);
    }

    /// Fastest target speed over `frames` frames at 120 fps, m/s.
    fn top_speed(def: &ScenarioDef, frames: usize) -> f32 {
        use crate::scenario::{FrameInput, Session};
        let dt = 1.0 / 120.0;
        let positions = |s: &Session| -> std::collections::HashMap<u64, crate::math::Vec3> {
            s.targets().map(|t| (t.id, t.position)).collect()
        };
        let mut s = Session::new(def.clone(), 7);
        let mut last = positions(&s);
        let mut top: f32 = 0.0;
        for _ in 0..frames {
            s.update(FrameInput {
                dt,
                view: Default::default(),
                fire_pressed: false,
                fire_held: false,
                movement: Default::default(),
            });
            let now = positions(&s);
            for (id, b) in &now {
                if let Some(a) = last.get(id) {
                    top = top.max((*b - *a).length() / dt);
                }
            }
            last = now;
        }
        top
    }

    #[test]
    fn the_speed_multiplier_scales_drift_strafe_and_dodge_motion() {
        for id in [
            "track-reactive-micro",
            "dynamic-strafe-heads",
            "dodge-juke-heads",
        ] {
            let d = find(id);
            let base = top_speed(&d, 600);
            let fast = top_speed(&d.with_tuning(tuning(100, 100, 150)), 600);
            assert!(base > 0.0, "{id} moves");
            assert!((fast / base - 1.5).abs() < 0.1, "{id}: {base} → {fast}");
        }
    }

    #[test]
    fn harder_shrinks_spheres_speeds_up_motion_and_shortens_lifetimes() {
        let d = find("track-reactive-micro");
        let h = d.at_difficulty(1.0);
        let (Shape::Sphere { radius: r0 }, Shape::Sphere { radius: r1 }) =
            (&d.targets.shape, &h.targets.shape)
        else {
            panic!()
        };
        assert!((r1 / r0 - 0.7).abs() < 1e-5);
        let (Motion::Drift { speed: s0, .. }, Motion::Drift { speed: s1, .. }) =
            (&d.targets.motion, &h.targets.motion)
        else {
            panic!()
        };
        assert!((s1.max / s0.max - 1.3).abs() < 1e-5);
        let dynamic = find("dynamic-strafe-heads");
        let hard = dynamic.at_difficulty(1.0);
        assert!(hard.targets.lifetime_secs.unwrap() < dynamic.targets.lifetime_secs.unwrap());
        assert!(
            dynamic.at_difficulty(-1.0).targets.lifetime_secs.unwrap()
                > dynamic.targets.lifetime_secs.unwrap()
        );
    }

    #[test]
    fn harder_makes_dodging_targets_decide_and_juke_sooner() {
        let d = find("dodge-juke-heads");
        let h = d.at_difficulty(1.0);
        let (
            Motion::Dodge {
                change_secs: c0,
                juke_secs: j0,
                ..
            },
            Motion::Dodge {
                change_secs: c1,
                juke_secs: j1,
                ..
            },
        ) = (&d.targets.motion, &h.targets.motion)
        else {
            panic!()
        };
        assert!((c1.max / c0.max - 0.7).abs() < 1e-5);
        assert!((j1.min / j0.min - 0.7).abs() < 1e-5);
    }

    #[test]
    fn levels_clamp_and_every_builtin_stays_valid_across_the_range() {
        for d in builtin::all() {
            for level in [-1.0, -0.5, 0.25, 0.5, 1.0, 3.0] {
                let v = d.at_difficulty(level);
                // Derived ids carry the level (`id@+0.50`), so only the id rule may fail.
                let other: Vec<_> = v
                    .validate()
                    .into_iter()
                    .filter(|e| e.field != "id")
                    .collect();
                assert!(other.is_empty(), "{} at {level}: {other:?}", d.id);
            }
            assert_eq!(d.at_difficulty(3.0), d.at_difficulty(1.0));
        }
    }

    #[test]
    fn simulated_scores_drop_as_difficulty_rises() {
        use crate::sim::{AimerProfile, simulate};
        let p = AimerProfile::PRESETS[1];
        for id in ["micro-adjust", "switch-speed", "track-reactive-micro"] {
            let d = find(id);
            let easy = simulate(&d.at_difficulty(-1.0), p, 3).score;
            let base = simulate(&d, p, 3).score;
            let hard = simulate(&d.at_difficulty(1.0), p, 3).score;
            assert!(easy > base && base > hard, "{id}: {easy} / {base} / {hard}");
        }
    }

    #[test]
    fn zero_coefficients_disable_a_knob() {
        let mut d = find("micro-adjust");
        d.difficulty.size = Knob::symmetric(0.0);
        assert_eq!(d.at_difficulty(1.0).targets.shape, d.targets.shape);
    }

    fn radius(d: &ScenarioDef) -> f32 {
        match d.targets.shape {
            Shape::Sphere { radius } => radius,
            _ => panic!("sphere scenario"),
        }
    }

    #[test]
    fn knobs_take_separate_easy_and_hard_bounds() {
        let src = r#"
            [difficulty]
            size = { easy = 0.5, hard = 0.1 }
        "#;
        let k: DifficultyDef = toml::from_str::<toml::Table>(src).unwrap()["difficulty"]
            .clone()
            .try_into()
            .unwrap();
        assert_eq!(
            k.size,
            Knob::Bounds {
                easy: 0.5,
                hard: 0.1
            }
        );
        assert_eq!(
            k.speed,
            DifficultyDef::default().speed,
            "unset knobs keep defaults"
        );

        let mut d = find("micro-adjust");
        d.difficulty = k;
        let r = radius(&d);
        assert!((radius(&d.at_difficulty(1.0)) - r * 0.9).abs() < 1e-6);
        assert!((radius(&d.at_difficulty(-1.0)) - r * 1.5).abs() < 1e-6);
        assert!((radius(&d.at_difficulty(-0.5)) - r * 1.25).abs() < 1e-6);
    }

    #[test]
    fn a_plain_number_is_the_same_bound_both_ways() {
        let k: Knob = toml::Value::Float(0.3).try_into().unwrap();
        assert_eq!(k, Knob::symmetric(0.3));
        assert_eq!(k.at(1.0), 0.3);
        assert_eq!(k.at(-0.5), -0.15);
    }

    #[test]
    fn the_count_knob_adds_targets_when_harder_and_is_off_by_default() {
        let mut d = find("micro-adjust");
        assert_eq!(d.at_difficulty(1.0).targets.max_alive, d.targets.max_alive);
        d.targets.max_alive = 3;
        d.difficulty.count = Knob::Bounds {
            easy: 0.4,
            hard: 0.7,
        };
        assert_eq!(d.at_difficulty(1.0).targets.max_alive, 5, "3 × 1.7 rounded");
        assert_eq!(
            d.at_difficulty(-1.0).targets.max_alive,
            2,
            "3 × 0.6 rounded"
        );
        d.targets.max_alive = 1;
        d.difficulty.count = Knob::symmetric(0.9);
        assert_eq!(
            d.at_difficulty(-1.0).targets.max_alive,
            1,
            "never below one"
        );
    }

    #[test]
    fn misspelled_keys_inside_a_knob_are_rejected() {
        let typo = toml::Value::Table(toml::from_str("easy = 0.5\nhrad = 0.1").unwrap());
        assert!(typo.try_into::<Knob>().is_err());
        let extra =
            toml::Value::Table(toml::from_str("easy = 0.5\nhard = 0.1\nmid = 0.2").unwrap());
        assert!(extra.try_into::<Knob>().is_err());
    }

    #[test]
    fn invalid_bounds_are_rejected() {
        let mut k = DifficultyDef::default();
        assert!(k.is_valid());
        k.lifetime = Knob::Bounds {
            easy: 0.2,
            hard: 0.95,
        };
        assert!(!k.is_valid());
        k.lifetime = Knob::Bounds {
            easy: -0.1,
            hard: 0.2,
        };
        assert!(!k.is_valid());
    }

    #[test]
    fn difficulty_and_tuning_keep_the_target_behaviour_blocks() {
        for id in ["spread-flicks", "regen-track", "react-duel"] {
            let d = find(id);
            let tuned = Tuning {
                size_pct: 150,
                distance_pct: 80,
                speed_pct: 130,
            };
            for v in [
                d.at_difficulty(-1.0),
                d.at_difficulty(1.0),
                d.with_tuning(tuned),
            ] {
                assert_eq!(v.targets.spawn_rules, d.targets.spawn_rules, "{id}");
                assert_eq!(v.targets.regen, d.targets.regen, "{id}");
                assert_eq!(v.scoring.decay, d.scoring.decay, "{id}");
                assert_eq!(v.targets.reactions, d.targets.reactions, "{id}");
                assert_eq!(v.targets.behaviors, d.targets.behaviors, "{id}");
                let errs: Vec<_> = v
                    .validate()
                    .into_iter()
                    .filter(|e| e.field != "id")
                    .collect();
                assert!(errs.is_empty(), "{id}: {errs:?}");
            }
        }
    }

    #[test]
    fn difficulty_and_tuning_keep_decoys_and_score_rings() {
        for id in ["go-no-go", "precision-rings"] {
            let d = find(id);
            let tuned = Tuning {
                size_pct: 150,
                distance_pct: 80,
                speed_pct: 100,
            };
            for v in [
                d.at_difficulty(-1.0),
                d.at_difficulty(1.0),
                d.with_tuning(tuned),
            ] {
                assert_eq!(v.targets.decoy_chance, d.targets.decoy_chance, "{id}");
                assert_eq!(v.scoring.decoy_penalty, d.scoring.decoy_penalty, "{id}");
                assert_eq!(v.scoring.rings, d.scoring.rings, "{id}");
                let errs: Vec<_> = v
                    .validate()
                    .into_iter()
                    .filter(|e| e.field != "id")
                    .collect();
                assert!(errs.is_empty(), "{id}: {errs:?}");
            }
        }
    }

    #[test]
    fn difficulty_and_tuning_scale_each_phase_like_the_base_targets() {
        let d = ScenarioDef::from_toml(
            r#"
            id = "phased"
            name = "Phased"
            category = "custom"
            [difficulty]
            count = 0.5
            [targets]
            shape = { kind = "sphere", radius = 0.3 }
            spawn = { kind = "angular", yaw_deg = [-30, 30], pitch_deg = [0, 5], distance = [10, 12] }
            lifetime_secs = 2.0
            [[phases]]
            targets = { shape = { kind = "sphere", radius = 0.5 }, max_alive = 2 }
            advance = { kind = "kills", kills = 3 }
            [[phases]]
            targets = { spawn = { kind = "floor", x = [-3, 3], z = [-20, -15] }, lifetime_secs = 1.0, motion = { kind = "drift", speed = [1, 2], change_secs = [0.5, 1.0] } }
            "#,
        )
        .unwrap();
        let tuned = Tuning {
            size_pct: 150,
            distance_pct: 80,
            speed_pct: 120,
        };
        type Variant = fn(&ScenarioDef, Tuning) -> ScenarioDef;
        let variants: [(&str, Variant); 3] = [
            ("easy", |d, _| d.at_difficulty(-1.0)),
            ("hard", |d, _| d.at_difficulty(1.0)),
            ("tuned", |d, t| d.with_tuning(t)),
        ];
        for (name, variant) in variants {
            let scaled = variant(&d, tuned);
            assert_ne!(scaled.phases, d.phases, "{name}: phases scale");
            for (i, phase) in d.phases.as_ref().unwrap().iter().enumerate() {
                // The phase played as a plain scenario, then scaled.
                let mut alone = d.clone();
                alone.targets = phase.targets.apply(&d.targets);
                alone.phases = None;
                let expected = variant(&alone, tuned).targets;
                let got = scaled.phases.as_ref().unwrap()[i]
                    .targets
                    .apply(&scaled.targets);
                assert_eq!(got, expected, "{name}: phase {i}");
            }
            let errs: Vec<_> = scaled
                .validate()
                .into_iter()
                .filter(|e| e.field != "id")
                .collect();
            assert!(errs.is_empty(), "{name}: {errs:?}");
        }
    }

    #[test]
    fn the_speed_knob_scales_the_new_motions() {
        let hard = |id: &str| {
            (
                find(id).targets.motion,
                find(id).at_difficulty(1.0).targets.motion,
            )
        };
        let (
            Motion::Ballistic {
                speed: s0,
                gravity: g0,
                ..
            },
            Motion::Ballistic {
                speed: s1,
                gravity: g1,
                ..
            },
        ) = hard("demo-ballistic")
        else {
            panic!("ballistic")
        };
        assert!((s1.max / s0.max - 1.3).abs() < 1e-5);
        assert!((g1 / g0 - 1.69).abs() < 1e-5, "the same arc, flown faster");
        let (
            Motion::Orbit {
                speed_deg: o0,
                change_secs: c0,
                ..
            },
            Motion::Orbit {
                speed_deg: o1,
                change_secs: c1,
                ..
            },
        ) = hard("demo-orbit")
        else {
            panic!("orbit")
        };
        assert!((o1.min / o0.min - 1.3).abs() < 1e-5);
        assert!((c1.unwrap().max / c0.unwrap().max - 0.7).abs() < 1e-5);
        let (
            Motion::LinearBounce { speed_deg: b0, .. },
            Motion::LinearBounce { speed_deg: b1, .. },
        ) = hard("demo-bounce")
        else {
            panic!("bounce")
        };
        assert!((b1.max / b0.max - 1.3).abs() < 1e-5);
        let depth = |m: &Motion| match m {
            Motion::Strafe { depth: Some(d), .. } => d.change_secs,
            _ => panic!("strafe with depth"),
        };
        let (d0, d1) = hard("demo-depth");
        assert!((depth(&d1).max / depth(&d0).max - 0.7).abs() < 1e-5);
    }

    #[test]
    fn the_speed_knob_plays_a_wave_faster_with_the_same_amplitude() {
        let def = ScenarioDef::from_toml(
            r#"
            id = "w"
            name = "w"
            category = "custom"
            [targets]
            shape = { kind = "sphere", radius = 0.2 }
            spawn = { kind = "angular", yaw_deg = [0, 0], pitch_deg = [5, 5], distance = [10, 10] }
            motion = { kind = "wave", speed = [2, 3], amplitude = [0.4, 0.6], frequency_hz = [1, 2] }
            "#,
        )
        .unwrap();
        let wave = |d: &ScenarioDef| match d.targets.motion {
            Motion::Wave {
                speed,
                amplitude,
                frequency_hz,
                ..
            } => (speed, amplitude, frequency_hz),
            _ => panic!("wave"),
        };
        let ((s0, a0, f0), (s1, a1, f1)) = (wave(&def), wave(&def.at_difficulty(1.0)));
        assert!((s1.max / s0.max - 1.3).abs() < 1e-5);
        assert!((f1.min / f0.min - 1.3).abs() < 1e-5);
        assert_eq!(a1, a0);
    }
}
