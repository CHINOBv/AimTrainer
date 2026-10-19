//! Compound daily missions: AND conditions over one run of a scenario.
//!
//! Each mission names a scenario and two or three conditions that must all hold in the
//! same run, for example "accuracy ≥ 75% and 62 kills". Every mission pairs one volume
//! condition (score, kills or shots hit) with one precision condition (accuracy or
//! headshot rate), so spam clicking (volume without precision) and one careful shot
//! (precision without volume) both fail. Mastery missions add a third condition and a
//! stricter target.
//!
//! Targets come from the player's own recent bests on the scenario, scaled by the
//! commitment level. The factors are estimates (see `docs/training-science.md`).
//! Missions are frozen when a day starts, so bests set during the day do not move them.

use crate::i18n::{Lang, tr_in};
use crate::rng::Rng;
use crate::scenario::{RunKey, RunResult};
use serde::{Deserialize, Serialize};

/// Runs kept per scenario for the recent bests.
pub const RECENT_RUNS: usize = 10;
/// Scenarios kept in the recent window (the most recently played ones).
pub const RECENT_SCENARIOS: usize = 12;
/// Extra target factor for mastery missions (estimate).
pub const MASTERY_BONUS: f32 = 0.08;
/// Highest target for a ratio metric (accuracy, headshot rate), so a perfect past run
/// never asks for 100% (estimate).
pub const MAX_RATIO_TARGET: f32 = 0.95;
/// A scenario counts as having heads when the best headshot rate reaches this share.
pub const MIN_HEADSHOT_RATE: f32 = 0.05;

/// How much daily mission work the player signs up for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Commitment {
    /// Few, easier missions.
    Low,
    /// The default: a few missions, one of them mastery.
    #[default]
    Medium,
    /// More missions, closer to the recent bests, two of them mastery.
    High,
}

impl Commitment {
    /// Every level, lowest first.
    pub const ALL: [Commitment; 3] = [Commitment::Low, Commitment::Medium, Commitment::High];

    /// Name shown to the player.
    pub fn label(self) -> &'static str {
        match self {
            Commitment::Low => "Low",
            Commitment::Medium => "Medium",
            Commitment::High => "High",
        }
    }

    /// [`Commitment::label`] in `lang`.
    pub fn label_in(self, lang: Lang) -> &'static str {
        tr_in(lang, self.label())
    }

    /// Missions offered per day.
    pub fn missions_per_day(self) -> usize {
        match self {
            Commitment::Low => 2,
            Commitment::Medium => 3,
            Commitment::High => 4,
        }
    }

    /// How many of the day's missions are mastery missions (the last ones).
    pub fn mastery_per_day(self) -> usize {
        match self {
            Commitment::Low => 0,
            Commitment::Medium => 1,
            Commitment::High => 2,
        }
    }

    /// Share of the recent best that a regular mission asks for (estimate).
    pub fn target_factor(self) -> f32 {
        match self {
            Commitment::Low => 0.85,
            Commitment::Medium => 0.92,
            Commitment::High => 0.97,
        }
    }
}

/// A run measurement a condition can test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Metric {
    /// Points scored.
    Score,
    /// Hits / shots (click) or time on target / time firing (hold).
    Accuracy,
    /// Targets killed.
    Kills,
    /// Mean seconds per kill; lower is better.
    TimePerKill,
    /// Fraction of hits on the head.
    HeadshotRate,
    /// Shots that hit a target.
    ShotsHit,
}

impl Metric {
    /// Volume metrics grow with how much is fired and hit.
    pub fn is_volume(self) -> bool {
        matches!(self, Metric::Score | Metric::Kills | Metric::ShotsHit)
    }

    /// Precision metrics fall when shots are wasted.
    pub fn is_precision(self) -> bool {
        matches!(self, Metric::Accuracy | Metric::HeadshotRate)
    }

    /// Whether a condition on this metric is an upper bound.
    pub fn lower_is_better(self) -> bool {
        self == Metric::TimePerKill
    }

    /// Name shown to the player.
    pub fn label(self) -> &'static str {
        match self {
            Metric::Score => "score",
            Metric::Accuracy => "accuracy",
            Metric::Kills => "kills",
            Metric::TimePerKill => "time per kill",
            Metric::HeadshotRate => "headshot rate",
            Metric::ShotsHit => "shots hit",
        }
    }

    /// [`Metric::label`] in `lang`.
    pub fn label_in(self, lang: Lang) -> &'static str {
        tr_in(lang, self.label())
    }

    /// `v` in this metric's display unit.
    pub fn format(self, v: f32) -> String {
        match self {
            Metric::Accuracy | Metric::HeadshotRate => format!("{:.0}%", v * 100.0),
            Metric::TimePerKill => format!("{v:.2} s"),
            Metric::Score | Metric::Kills | Metric::ShotsHit => format!("{v:.0}"),
        }
    }

    /// A target rounded to what the player reads: whole percent, whole counts, 0.01 s.
    fn round(self, v: f32) -> f32 {
        match self {
            Metric::Accuracy | Metric::HeadshotRate => (v * 100.0).round() / 100.0,
            Metric::TimePerKill => (v * 100.0).round() / 100.0,
            Metric::Score | Metric::Kills | Metric::ShotsHit => v.round().max(1.0),
        }
    }
}

/// What a mission needs from one run.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct RunMetrics {
    /// Points scored.
    pub score: f32,
    /// Accuracy, 0–1.
    pub accuracy: f32,
    /// Targets killed.
    pub kills: u32,
    /// Mean seconds per kill, if any target was killed.
    pub time_per_kill: Option<f32>,
    /// Fraction of hits on the head, 0–1.
    pub headshot_rate: f32,
    /// Shots that hit a target.
    pub shots_hit: u32,
}

impl RunMetrics {
    /// The metrics of a finished run. `shots_hit` comes from the session's stats
    /// (`Stats::hits`), which the result does not carry.
    pub fn of(result: &RunResult, shots_hit: u32) -> Self {
        Self {
            score: result.score,
            accuracy: result.accuracy,
            kills: result.kills,
            time_per_kill: result.avg_time_to_kill,
            headshot_rate: result.headshot_rate,
            shots_hit,
        }
    }

    /// The value of `metric`; `None` when the run has no kill to time.
    pub fn value(&self, metric: Metric) -> Option<f32> {
        Some(match metric {
            Metric::Score => self.score,
            Metric::Accuracy => self.accuracy,
            Metric::Kills => self.kills as f32,
            Metric::TimePerKill => return self.time_per_kill,
            Metric::HeadshotRate => self.headshot_rate,
            Metric::ShotsHit => self.shots_hit as f32,
        })
    }

    /// The better of two values per metric (each metric can come from a different run).
    fn best_with(self, o: RunMetrics) -> RunMetrics {
        RunMetrics {
            score: self.score.max(o.score),
            accuracy: self.accuracy.max(o.accuracy),
            kills: self.kills.max(o.kills),
            time_per_kill: match (self.time_per_kill, o.time_per_kill) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            },
            headshot_rate: self.headshot_rate.max(o.headshot_rate),
            shots_hit: self.shots_hit.max(o.shots_hit),
        }
    }
}

/// One condition: `metric ≥ target`, or `≤` for time per kill.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    /// What is measured.
    pub metric: Metric,
    /// Threshold in the metric's unit (ratios 0–1, seconds, counts or points).
    pub target: f32,
}

impl Condition {
    /// Whether the run reaches the target.
    pub fn met(&self, m: &RunMetrics) -> bool {
        match m.value(self.metric) {
            Some(v) if self.metric.lower_is_better() => v <= self.target,
            Some(v) => v >= self.target,
            None => false,
        }
    }

    /// For example "accuracy ≥ 75%".
    pub fn describe(&self) -> String {
        self.describe_in(Lang::En)
    }

    /// [`Condition::describe`] in `lang`: the metric name is translated; the operator
    /// and the value are the same in every language.
    pub fn describe_in(&self, lang: Lang) -> String {
        let op = if self.metric.lower_is_better() {
            "≤"
        } else {
            "≥"
        };
        format!(
            "{} {op} {}",
            self.metric.label_in(lang),
            self.metric.format(self.target)
        )
    }
}

/// A daily mission on one scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mission {
    /// Scenario id to play (a plain library id, no difficulty or tuning suffix).
    pub scenario: String,
    /// All must hold in the same run.
    pub conditions: Vec<Condition>,
    /// A harder mission: a stricter target and a third condition.
    pub mastery: bool,
    /// Completed today.
    pub done: bool,
    /// Most conditions met in a single run today.
    #[serde(default)]
    pub best_met: usize,
}

impl Mission {
    /// Whether one run of the mission's scenario meets every condition.
    pub fn is_met_by(&self, m: &RunMetrics) -> bool {
        self.met_count(m) == self.conditions.len()
    }

    fn met_count(&self, m: &RunMetrics) -> usize {
        self.conditions.iter().filter(|c| c.met(m)).count()
    }
}

/// Recent runs of one scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioRuns {
    /// Plain scenario id.
    pub scenario: String,
    /// Oldest first, at most [`RECENT_RUNS`].
    pub runs: Vec<RunMetrics>,
}

impl ScenarioRuns {
    /// Best value per metric over the kept runs.
    pub fn bests(&self) -> RunMetrics {
        self.runs
            .iter()
            .copied()
            .reduce(RunMetrics::best_with)
            .unwrap_or_default()
    }
}

/// The missions of one day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MissionDay {
    /// Local date, `YYYY-MM-DD`.
    pub date: String,
    /// Commitment the missions were generated for.
    pub commitment: Commitment,
    /// The day's missions; mastery ones last.
    pub missions: Vec<Mission>,
}

/// Everything missions need to persist: the seed, today's missions and recent runs.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MissionBook {
    /// Player seed mixed with the day, so each player gets their own missions.
    pub seed: u64,
    /// Today's missions, once generated.
    pub today: Option<MissionDay>,
    /// Recent runs per scenario, least recently played first.
    pub recent: Vec<ScenarioRuns>,
}

/// Whether `key` is a plain scenario id (no difficulty, length, tuning, floor height or
/// daily-challenge suffix), so missions compare like with like.
pub fn is_plain_key(key: &str) -> bool {
    !key.contains('#') && RunKey::parse(key).base == key
}

impl MissionBook {
    /// Make sure today's missions are for `date` (day number `day`) and `commitment`;
    /// generate them otherwise. Returns whether they changed. A commitment change
    /// regenerates the day and drops its progress.
    pub fn refresh(&mut self, date: &str, day: i64, commitment: Commitment) -> bool {
        if self
            .today
            .as_ref()
            .is_some_and(|d| d.date == date && d.commitment == commitment)
        {
            return false;
        }
        self.today = Some(MissionDay {
            date: date.to_owned(),
            commitment,
            missions: generate(day, self.seed, commitment, &self.recent),
        });
        true
    }

    /// Count a finished run: update today's missions on its scenario and keep it in the
    /// recent window. Returns the indices of missions it completed.
    pub fn record(&mut self, result: &RunResult, shots_hit: u32) -> Vec<usize> {
        let key = result.scenario_id.as_str();
        if !is_plain_key(key) {
            return Vec::new();
        }
        let m = RunMetrics::of(result, shots_hit);
        let mut completed = Vec::new();
        if let Some(day) = self.today.as_mut() {
            for (i, mission) in day.missions.iter_mut().enumerate() {
                if mission.scenario != key {
                    continue;
                }
                mission.best_met = mission.best_met.max(mission.met_count(&m));
                if !mission.done && mission.is_met_by(&m) {
                    mission.done = true;
                    completed.push(i);
                }
            }
        }
        let mut entry = match self.recent.iter().position(|r| r.scenario == key) {
            Some(i) => self.recent.remove(i),
            None => ScenarioRuns {
                scenario: key.to_owned(),
                runs: Vec::new(),
            },
        };
        entry.runs.push(m);
        let skip = entry.runs.len().saturating_sub(RECENT_RUNS);
        entry.runs.drain(..skip);
        self.recent.push(entry);
        let skip = self.recent.len().saturating_sub(RECENT_SCENARIOS);
        self.recent.drain(..skip);
        completed
    }
}

/// Today's missions: deterministic for the same day, seed, commitment and recent runs.
/// Empty when no plain scenario has been played yet.
pub fn generate(
    day: i64,
    seed: u64,
    commitment: Commitment,
    recent: &[ScenarioRuns],
) -> Vec<Mission> {
    let mut pool: Vec<(&str, RunMetrics)> = recent
        .iter()
        .filter(|r| is_plain_key(&r.scenario) && !r.runs.is_empty())
        .map(|r| (r.scenario.as_str(), r.bests()))
        .filter(|(_, b)| !volume_metrics(b).is_empty() && !precision_metrics(b).is_empty())
        .collect();
    if pool.is_empty() {
        return Vec::new();
    }
    // Order by id so the result never depends on play order.
    pool.sort_by(|a, b| a.0.cmp(b.0));
    let mut rng = Rng::new(seed ^ (day as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    // Shuffle, then cycle, so scenarios repeat only when there are fewer than missions.
    for i in (1..pool.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        pool.swap(i, j);
    }
    let n = commitment.missions_per_day();
    let first_mastery = n - commitment.mastery_per_day();
    (0..n)
        .map(|i| {
            let (scenario, bests) = pool[i % pool.len()];
            let mastery = i >= first_mastery;
            mission(scenario, &bests, commitment, mastery, &mut rng)
        })
        .collect()
}

fn volume_metrics(b: &RunMetrics) -> Vec<Metric> {
    let mut v = Vec::new();
    if b.score > 0.0 {
        v.push(Metric::Score);
    }
    if b.kills > 0 {
        v.push(Metric::Kills);
    }
    if b.shots_hit > 0 {
        v.push(Metric::ShotsHit);
    }
    v
}

fn precision_metrics(b: &RunMetrics) -> Vec<Metric> {
    let mut v = Vec::new();
    if b.accuracy > 0.0 {
        v.push(Metric::Accuracy);
    }
    if b.headshot_rate >= MIN_HEADSHOT_RATE {
        v.push(Metric::HeadshotRate);
    }
    v
}

fn pick(v: &[Metric], rng: &mut Rng) -> Metric {
    v[(rng.next_u64() % v.len() as u64) as usize]
}

fn mission(
    scenario: &str,
    bests: &RunMetrics,
    commitment: Commitment,
    mastery: bool,
    rng: &mut Rng,
) -> Mission {
    let factor = commitment.target_factor() + if mastery { MASTERY_BONUS } else { 0.0 };
    let volume = volume_metrics(bests);
    let precision = precision_metrics(bests);
    let mut metrics = vec![pick(&volume, rng), pick(&precision, rng)];
    if mastery {
        // The third condition: speed when kills are timed, else the other precision or
        // volume metric.
        let third = if bests.time_per_kill.is_some() {
            Some(Metric::TimePerKill)
        } else {
            precision
                .iter()
                .chain(volume.iter())
                .copied()
                .find(|m| !metrics.contains(m))
        };
        metrics.extend(third);
    }
    let conditions = metrics
        .into_iter()
        .filter_map(|metric| {
            let best = bests.value(metric)?;
            let target = if metric.lower_is_better() {
                best / factor
            } else if metric.is_precision() {
                (best * factor).min(MAX_RATIO_TARGET)
            } else {
                best * factor
            };
            Some(Condition {
                metric,
                target: metric.round(target),
            })
        })
        .collect();
    Mission {
        scenario: scenario.to_owned(),
        conditions,
        mastery,
        done: false,
        best_met: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(key: &str, score: f32, accuracy: f32, kills: u32) -> RunResult {
        RunResult {
            scenario_id: key.into(),
            score,
            accuracy,
            kills,
            kills_per_min: kills as f32,
            avg_time_to_kill: Some(0.8),
            headshot_rate: 0.4,
            moving_shots: 0,
            expired: 0,
            duration_secs: 60.0,
        }
    }

    /// A book whose recent window holds one good run on each of `ids`.
    fn book_with(ids: &[&str]) -> MissionBook {
        let mut b = MissionBook {
            seed: 42,
            ..MissionBook::default()
        };
        for id in ids {
            b.record(&result(id, 1000.0, 0.8, 60), 70);
        }
        b
    }

    #[test]
    fn all_conditions_must_hold_in_the_same_run() {
        let m = Mission {
            scenario: "s".into(),
            conditions: vec![
                Condition {
                    metric: Metric::Accuracy,
                    target: 0.75,
                },
                Condition {
                    metric: Metric::Kills,
                    target: 62.0,
                },
            ],
            mastery: false,
            done: false,
            best_met: 0,
        };
        let run = |accuracy, kills| RunMetrics {
            accuracy,
            kills,
            ..RunMetrics::default()
        };
        assert!(m.is_met_by(&run(0.75, 62)));
        assert!(!m.is_met_by(&run(0.90, 61)), "kills short");
        assert!(!m.is_met_by(&run(0.74, 80)), "accuracy short");
        assert_eq!(m.conditions[0].describe(), "accuracy ≥ 75%");
        assert_eq!(m.conditions[1].describe(), "kills ≥ 62");
    }

    #[test]
    fn time_per_kill_is_an_upper_bound_and_needs_a_kill() {
        let c = Condition {
            metric: Metric::TimePerKill,
            target: 0.9,
        };
        let with = |t| RunMetrics {
            time_per_kill: t,
            ..RunMetrics::default()
        };
        assert!(c.met(&with(Some(0.85))));
        assert!(!c.met(&with(Some(0.95))));
        assert!(!c.met(&with(None)));
        assert_eq!(c.describe(), "time per kill ≤ 0.90 s");
    }

    #[test]
    fn every_mission_pairs_volume_with_precision_so_spam_or_one_shot_fails() {
        let ids = ["a", "b", "c", "d", "e"];
        let book = book_with(&ids);
        for commitment in Commitment::ALL {
            for day in 0..30 {
                for m in generate(day, 7, commitment, &book.recent) {
                    assert!(m.conditions.iter().any(|c| c.metric.is_volume()));
                    assert!(m.conditions.iter().any(|c| c.metric.is_precision()));
                    // Spam clicking: far more volume than ever, almost no precision.
                    let spam = RunMetrics {
                        score: 1e6,
                        accuracy: 0.03,
                        kills: 10_000,
                        time_per_kill: Some(0.01),
                        headshot_rate: 0.01,
                        shots_hit: 10_000,
                    };
                    assert!(!m.is_met_by(&spam), "spam satisfied {m:?}");
                    // One careful shot: perfect precision, no volume.
                    let one_shot = RunMetrics {
                        score: 1.0,
                        accuracy: 1.0,
                        kills: 0,
                        time_per_kill: None,
                        headshot_rate: 1.0,
                        shots_hit: 1,
                    };
                    assert!(!m.is_met_by(&one_shot), "one shot satisfied {m:?}");
                }
            }
        }
    }

    #[test]
    fn generation_is_deterministic_per_day_and_seed() {
        let book = book_with(&["a", "b", "c", "d", "e"]);
        let g = |day, seed| generate(day, seed, Commitment::High, &book.recent);
        assert_eq!(g(100, 1), g(100, 1));
        let days: Vec<_> = (100..110).map(|d| g(d, 1)).collect();
        assert!(days.windows(2).any(|w| w[0] != w[1]), "days vary");
        assert!((100..110).any(|d| g(d, 1) != g(d, 2)), "seeds vary");
        // Play order does not matter, only the runs.
        let reversed = book_with(&["e", "d", "c", "b", "a"]);
        assert_eq!(
            generate(100, 1, Commitment::High, &reversed.recent),
            g(100, 1)
        );
    }

    #[test]
    fn commitment_scales_count_difficulty_and_mastery() {
        let book = book_with(&["a", "b", "c", "d", "e"]);
        let target = |c: Commitment, mastery: bool| {
            generate(5, 3, c, &book.recent)
                .into_iter()
                .find(|m| m.mastery == mastery)
                .map(|m| {
                    let k = m.conditions.iter().find(|c| c.metric.is_volume()).unwrap();
                    k.target / book.recent[0].bests().value(k.metric).unwrap()
                })
        };
        let counts: Vec<_> = Commitment::ALL
            .iter()
            .map(|&c| generate(5, 3, c, &book.recent).len())
            .collect();
        assert_eq!(counts, [2, 3, 4]);
        let masteries: Vec<_> = Commitment::ALL
            .iter()
            .map(|&c| {
                generate(5, 3, c, &book.recent)
                    .iter()
                    .filter(|m| m.mastery)
                    .count()
            })
            .collect();
        assert_eq!(masteries, [0, 1, 2]);
        let low = target(Commitment::Low, false).unwrap();
        let high = target(Commitment::High, false).unwrap();
        assert!(
            low < high,
            "higher commitment asks for more: {low} vs {high}"
        );
        assert!(
            high <= 1.0,
            "regular missions stay at or under the recent best"
        );
        let mastery = target(Commitment::High, true).unwrap();
        assert!(mastery > high, "mastery is harder");
    }

    #[test]
    fn mastery_missions_have_three_conditions() {
        let book = book_with(&["a", "b"]);
        let day = generate(1, 1, Commitment::High, &book.recent);
        for m in &day {
            assert_eq!(m.conditions.len(), if m.mastery { 3 } else { 2 });
        }
        assert!(day.last().unwrap().mastery);
    }

    #[test]
    fn targets_come_from_recent_bests() {
        let mut b = MissionBook::default();
        b.record(&result("a", 500.0, 0.5, 30), 40);
        b.record(&result("a", 400.0, 0.9, 20), 30);
        let bests = b.recent[0].bests();
        assert_eq!((bests.score, bests.accuracy, bests.kills), (500.0, 0.9, 30));
        assert_eq!(bests.shots_hit, 40);
        let m = &generate(0, 0, Commitment::Low, &b.recent)[0];
        for c in &m.conditions {
            let best = bests.value(c.metric).unwrap();
            if c.metric.lower_is_better() {
                assert!(c.target >= best);
            } else {
                assert!(c.target <= best && c.target >= best * 0.8, "{c:?}");
            }
        }
    }

    #[test]
    fn no_missions_before_any_plain_run() {
        let mut b = MissionBook::default();
        b.record(&result("a@+0.50", 500.0, 0.5, 30), 40);
        b.record(&result("a#daily-3", 500.0, 0.5, 30), 40);
        b.record(&result("a~20s", 500.0, 0.5, 30), 40);
        assert!(b.recent.is_empty(), "only plain scenario runs are kept");
        assert!(b.refresh("2026-10-18", 1, Commitment::Medium));
        assert!(b.today.as_ref().unwrap().missions.is_empty());
    }

    #[test]
    fn recording_completes_matching_missions_once() {
        let mut b = book_with(&["a"]);
        b.refresh("2026-10-18", 1, Commitment::Low);
        let missions = b.today.clone().unwrap().missions;
        assert_eq!(missions.len(), 2);
        assert!(missions.iter().all(|m| m.scenario == "a"));
        // A weak run on the scenario counts towards progress but completes nothing.
        assert!(b.record(&result("a", 10.0, 0.1, 1), 1).is_empty());
        // A run on another scenario touches nothing.
        assert!(b.record(&result("z", 5000.0, 1.0, 999), 999).is_empty());
        let done = b.record(&result("a", 1000.0, 0.8, 60), 70);
        assert_eq!(done, [0, 1]);
        assert!(b.today.as_ref().unwrap().missions.iter().all(|m| m.done));
        assert_eq!(b.today.as_ref().unwrap().missions[0].best_met, 2);
        assert!(b.record(&result("a", 1000.0, 0.8, 60), 70).is_empty());
    }

    #[test]
    fn missions_are_frozen_for_the_day_and_regenerate_on_change() {
        let mut b = book_with(&["a", "b", "c"]);
        assert!(b.refresh("2026-10-18", 1, Commitment::Medium));
        let first = b.today.clone();
        // New bests during the day do not move today's missions.
        b.record(&result("a", 9000.0, 0.99, 600), 700);
        assert!(!b.refresh("2026-10-18", 1, Commitment::Medium));
        let targets = |d: &Option<MissionDay>| {
            d.as_ref()
                .unwrap()
                .missions
                .iter()
                .map(|m| (m.scenario.clone(), m.conditions.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(targets(&b.today), targets(&first));
        assert!(b.refresh("2026-10-18", 1, Commitment::High));
        assert_eq!(b.today.as_ref().unwrap().missions.len(), 4);
        assert!(b.refresh("2026-10-19", 2, Commitment::High));
    }

    #[test]
    fn the_recent_window_is_bounded() {
        let mut b = MissionBook::default();
        for i in 0..(RECENT_RUNS + 5) {
            b.record(&result("a", i as f32, 0.5, 1), 1);
        }
        assert_eq!(b.recent[0].runs.len(), RECENT_RUNS);
        assert_eq!(b.recent[0].runs[0].score, 5.0, "oldest runs drop first");
        for i in 0..(RECENT_SCENARIOS + 3) {
            b.record(&result(&format!("s{i}"), 1.0, 0.5, 1), 1);
        }
        assert_eq!(b.recent.len(), RECENT_SCENARIOS);
    }

    #[test]
    fn the_book_round_trips_through_toml() {
        let mut b = book_with(&["a", "b"]);
        b.refresh("2026-10-18", 1, Commitment::High);
        b.record(&result("a", 1000.0, 0.8, 60), 70);
        let text = toml::to_string_pretty(&b).unwrap();
        assert_eq!(toml::from_str::<MissionBook>(&text).unwrap(), b);
        assert_eq!(
            toml::from_str::<MissionBook>("").unwrap(),
            MissionBook::default()
        );
    }

    #[test]
    fn mission_text_is_shown_in_spanish() {
        assert_eq!(Commitment::High.label_in(Lang::Es), "Alto");
        assert_eq!(Metric::TimePerKill.label_in(Lang::Es), "tiempo por baja");
        let c = Condition {
            metric: Metric::Accuracy,
            target: 0.75,
        };
        assert_eq!(c.describe_in(Lang::En), c.describe());
        assert_eq!(c.describe_in(Lang::Es), "precisión ≥ 75%");
    }
}
