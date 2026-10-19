//! The coach's [`CoachContext`], assembled from what the app already keeps: the run
//! history, the benchmark, saved replays, missions, readiness and fatigue.
//!
//! [`build_context`] is pure; [`load_context`] reads and analyses the newest replays, so
//! it runs on a worker thread and the coach page caches its result.

use aim_core::analytics::miss_compass::MissCompass;
use aim_core::analytics::pace::{PaceAnalysis, PaceSummary};
use aim_core::analytics::{self, BenchmarkProfile, DirectionSummary, RunAnalysis, SkillProfile};
use aim_core::coach::{CoachContext, ContextInputs};
use aim_core::training::benchmark::BenchmarkDef;
use aim_core::training::missions::MissionBook;
use aim_core::training::progress::{self, RunRecord};

use crate::persistence::replays;
use crate::persistence::settings::Paths;

/// Newest replays analysed for the skill profile, flick directions, miss compass and pace.
pub const CONTEXT_REPLAYS: usize = 20;
/// Weeks of benchmark trend, the same window as the Progress page.
const TREND_WEEKS: u32 = 8;

/// Everything the context is built from, owned so it can move to a worker thread.
#[derive(Debug, Clone, Default)]
pub struct ContextData {
    /// Run history, oldest first.
    pub records: Vec<RunRecord>,
    /// Analyses of the newest replays, newest first.
    pub analyses: Vec<RunAnalysis>,
    /// The benchmark the Progress page profiles.
    pub benchmark: Option<BenchmarkDef>,
    /// Daily missions and the recent runs behind them.
    pub missions: Option<MissionBook>,
    /// Scenarios checked for warm-up readiness, in order; the first with a reading wins.
    pub readiness_scenarios: Vec<String>,
    /// Unix seconds now.
    pub now: u64,
    /// Seconds east of UTC of the player's clock (days start at local midnight).
    pub utc_offset_secs: i64,
}

/// The coach context for `data`.
pub fn build_context(data: &ContextData) -> CoachContext {
    let records = &data.records;
    let analyses = &data.analyses;
    let profile = (!analyses.is_empty()).then(|| SkillProfile::build(analyses));
    let mut flicks = analyses
        .iter()
        .flat_map(|a| a.classified_flicks())
        .peekable();
    let directions = flicks
        .peek()
        .is_some()
        .then(|| DirectionSummary::from_flicks(flicks));
    // Miss counts add up across runs; the summary applies its own minimum-sample guards.
    let miss_compass = analyses
        .iter()
        .filter_map(|a| a.miss_compass.as_ref())
        .fold(None, |acc: Option<MissCompass>, c| {
            let mut sum = acc.unwrap_or_default();
            sum.merge(c);
            Some(sum)
        });
    let paces: Vec<&PaceAnalysis> = analyses.iter().filter_map(|a| a.pace.as_ref()).collect();
    let pace = PaceSummary::from_runs(&paces);
    let benchmark = data
        .benchmark
        .as_ref()
        .map(|b| BenchmarkProfile::build(b, records, data.now, TREND_WEEKS));
    let readiness = data
        .readiness_scenarios
        .iter()
        .find_map(|id| progress::readiness(records, id, data.now, data.utc_offset_secs));
    let fatigue = progress::fatigue(progress::current_session(records));
    CoachContext::build(&ContextInputs {
        profile: profile.as_ref(),
        directions: directions.as_ref(),
        miss_compass: miss_compass.as_ref(),
        pace: pace.as_ref(),
        // The tip of the latest run only, as the results card shows it.
        potential: analyses.first().and_then(|a| a.potential.as_ref()),
        benchmark: benchmark.as_ref(),
        records,
        missions: data.missions.as_ref(),
        readiness: readiness.as_ref(),
        fatigue: &fatigue,
        now: data.now,
    })
}

/// [`build_context`] after analysing the newest [`CONTEXT_REPLAYS`] replays under `paths`
/// (unreadable files are skipped). Blocking: call it off the frame.
pub fn load_context(paths: &Paths, mut data: ContextData) -> CoachContext {
    data.analyses = replays::list(paths)
        .iter()
        .take(CONTEXT_REPLAYS)
        .filter_map(|e| replays::load(&e.path).ok())
        .map(|r| analytics::analyze(&r))
        .collect();
    build_context(&data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::training::benchmark;

    const DAY: u64 = 86_400;
    const NOW: u64 = 200 * DAY + 12 * 3_600;

    fn run(days_ago: u64, scenario: &str, score: f32) -> RunRecord {
        RunRecord {
            timestamp: NOW - days_ago * DAY,
            scenario: scenario.into(),
            score,
            duration_secs: 60.0,
        }
    }

    fn fixture() -> ContextData {
        let bench = benchmark::builtin::all().into_iter().next().unwrap();
        let first = bench.subcategories[0].scenarios[0].id.clone();
        let mut records: Vec<RunRecord> = (1..=4)
            .rev()
            .map(|d| run(d, "track-precise", 1_000.0))
            .collect();
        records.push(run(0, &first, 500.0));
        records.push(run(0, "track-precise", 1_100.0));
        ContextData {
            records,
            benchmark: Some(bench),
            readiness_scenarios: vec!["flick-headshot".into(), "track-precise".into()],
            now: NOW,
            ..ContextData::default()
        }
    }

    #[test]
    fn history_benchmark_and_readiness_reach_the_context() {
        let data = fixture();
        let ctx = build_context(&data);
        let recent: Vec<&str> = ctx.recent.iter().map(|r| r.scenario.as_str()).collect();
        assert_eq!(recent[0], "track-precise", "most recently played first");
        assert!(
            recent.contains(
                &data.benchmark.as_ref().unwrap().subcategories[0].scenarios[0]
                    .id
                    .as_str()
            )
        );
        assert!(
            ctx.benchmark.is_some(),
            "the benchmark energy is summarised"
        );
        let readiness = ctx
            .readiness
            .expect("track-precise has a baseline and a run today");
        assert_eq!(readiness.scenario, "track-precise");
        assert!((readiness.ratio - 1.1).abs() < 1e-3, "{}", readiness.ratio);
        assert!(
            ctx.findings.is_empty() && ctx.directions.is_none() && ctx.pace.is_none(),
            "no replays: no replay analyses"
        );
    }

    #[test]
    fn an_empty_history_gives_an_empty_context_and_missions_are_passed_through() {
        let empty = build_context(&ContextData {
            now: NOW,
            ..ContextData::default()
        });
        assert!(empty.recent.is_empty() && empty.benchmark.is_none() && empty.readiness.is_none());

        let mut book = MissionBook::default();
        book.refresh(
            "2026-10-18",
            1,
            aim_core::training::missions::Commitment::Medium,
        );
        let with_missions = build_context(&ContextData {
            missions: Some(book),
            now: NOW,
            ..ContextData::default()
        });
        assert_eq!(
            with_missions.missions.map(|m| m.date),
            Some("2026-10-18".to_owned())
        );
    }

    #[test]
    fn a_missing_replays_folder_still_builds_the_history_context() {
        let dir = std::env::temp_dir().join(format!("aimtrainer-coach-ctx-{}", std::process::id()));
        let paths = Paths {
            settings_file: dir.join("settings.toml"),
            scenarios_dir: dir.clone(),
            routines_dir: dir.clone(),
            benchmarks_dir: dir.clone(),
            history_file: dir.join("history.csv"),
            coach_memory_file: dir.join("coach.db"),
        };
        let ctx = load_context(&paths, fixture());
        assert!(!ctx.recent.is_empty());
        assert!(!dir.exists(), "loading reads only");
    }
}
