//! The player's content and history, read the way the trainer reads them, and the
//! [`CoachContext`] built from them.
//!
//! Content loading mirrors `Library::load` in `crates/aim_app/src/persistence/library.rs`
//! and the history reader mirrors `History::load` in
//! `crates/aim_app/src/persistence/history.rs` (the app is a binary, so neither can be
//! shared). Everything here only reads; broken files are skipped and reported, never fatal.

use std::fs;
use std::path::Path;

use aim_core::analytics::BenchmarkProfile;
use aim_core::coach::{CoachContext, ContextInputs};
use aim_core::scenario::{self, ScenarioDef};
use aim_core::training::benchmark::{self, BenchmarkDef};
use aim_core::training::progress::{self, RunRecord};
use aim_core::training::routine::{self, RoutineDef};

use crate::paths::Paths;

/// Rows written before `duration_secs` existed were full-length runs (as in the app).
const LEGACY_DURATION_SECS: f32 = 60.0;
/// Weeks of benchmark trend, the same window the app's progress page shows.
const TREND_WEEKS: u32 = 8;

/// Built-in content plus the player's valid TOML files; a custom file with a built-in's id
/// replaces it.
#[derive(Debug, Default)]
pub struct Library {
    /// Every playable scenario.
    pub scenarios: Vec<ScenarioDef>,
    /// Every playable routine.
    pub routines: Vec<RoutineDef>,
    /// Every valid benchmark; the first is the one the app profiles.
    pub benchmarks: Vec<BenchmarkDef>,
    /// Files that failed to load, with the reason.
    pub errors: Vec<String>,
}

impl Library {
    /// Load the built-ins and the custom files under `paths`.
    pub fn load(paths: &Paths) -> Self {
        let mut lib = Self {
            scenarios: scenario::builtin::all(),
            routines: routine::builtin::all(),
            benchmarks: benchmark::builtin::all(),
            errors: Vec::new(),
        };
        for (file, src) in toml_files(&paths.scenarios_dir) {
            match ScenarioDef::from_toml(&src) {
                Ok(def) => {
                    let problems = def.validate();
                    if problems.is_empty() {
                        upsert(&mut lib.scenarios, def, |d| &d.id);
                    } else {
                        for p in problems {
                            lib.errors.push(format!("scenarios/{file}: {p}"));
                        }
                    }
                }
                Err(e) => lib.errors.push(format!("scenarios/{file}: {e}")),
            }
        }
        for (file, src) in toml_files(&paths.routines_dir) {
            match RoutineDef::from_toml(&src) {
                Ok(r) => upsert(&mut lib.routines, r, |r| &r.id),
                Err(e) => lib.errors.push(format!("routines/{file}: {e}")),
            }
        }
        for (file, src) in toml_files(&paths.benchmarks_dir) {
            match BenchmarkDef::from_toml(&src) {
                Ok(b) => upsert(&mut lib.benchmarks, b, |b| &b.id),
                Err(e) => lib.errors.push(format!("benchmarks/{file}: {e}")),
            }
        }
        // Routines and benchmarks can only be checked once every scenario is known.
        let arenas: Vec<String> = aim_world::arena::builtin::all()
            .into_iter()
            .map(|a| a.id)
            .collect();
        let arenas: Vec<&str> = arenas.iter().map(String::as_str).collect();
        let mut errors = Vec::new();
        lib.routines.retain(|r| {
            let problems: Vec<String> = r
                .validate(&lib.scenarios)
                .into_iter()
                .chain(r.validate_arenas(&arenas))
                .collect();
            errors.extend(problems.iter().map(|p| format!("routine {}: {p}", r.id)));
            problems.is_empty()
        });
        let ids: Vec<&str> = lib.scenarios.iter().map(|d| d.id.as_str()).collect();
        lib.benchmarks.retain(|b| {
            let problems = b.validate(&ids);
            errors.extend(problems.iter().map(|p| format!("benchmark {}: {p}", b.id)));
            problems.is_empty()
        });
        lib.errors.extend(errors);
        lib
    }
}

/// The runs in `history.csv`, oldest first; a missing file is an empty history and
/// unreadable rows are skipped.
pub fn read_history(path: &Path) -> Vec<RunRecord> {
    let Ok(src) = fs::read_to_string(path) else {
        return Vec::new();
    };
    src.lines()
        .skip(1)
        .filter_map(|line| {
            let f: Vec<&str> = line.split(',').collect();
            Some(RunRecord {
                timestamp: f.first()?.parse().ok()?,
                scenario: f.get(1)?.to_string(),
                score: f.get(2)?.parse().ok()?,
                duration_secs: f
                    .get(9)
                    .and_then(|d| d.parse().ok())
                    .unwrap_or(LEGACY_DURATION_SECS),
            })
        })
        .collect()
}

/// The coach's summary of what the history alone supports: recent runs, benchmark energy
/// and fatigue. Analyses that need replays (skill profile, directions, miss compass, pace)
/// stay empty: the server does not read replay traces.
pub fn context(lib: &Library, records: &[RunRecord], now: u64) -> CoachContext {
    let profile = lib
        .benchmarks
        .first()
        .map(|b| BenchmarkProfile::build(b, records, now, TREND_WEEKS));
    let fatigue = progress::fatigue(progress::current_session(records));
    CoachContext::build(&ContextInputs {
        benchmark: profile.as_ref(),
        records,
        fatigue: &fatigue,
        now,
        ..ContextInputs::default()
    })
}

fn toml_files(dir: &Path) -> Vec<(String, String)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .filter_map(|p| {
            Some((
                p.file_name()?.to_string_lossy().into_owned(),
                fs::read_to_string(&p).ok()?,
            ))
        })
        .collect();
    files.sort();
    files
}

fn upsert<T>(items: &mut Vec<T>, item: T, id: impl Fn(&T) -> &str) {
    match items.iter().position(|x| id(x) == id(&item)) {
        Some(i) => items[i] = item,
        None => items.push(item),
    }
}
