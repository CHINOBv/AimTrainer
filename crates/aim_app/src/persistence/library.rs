//! Loads built-in content plus the user's TOML files. A user file with the same id as a
//! built-in replaces it.

use crate::persistence::settings::Paths;
use aim_core::benchmark::{self, BenchmarkDef};
use aim_core::routine::{self, RoutineDef};
use aim_core::scenario::{self, ScenarioDef};
use bevy::prelude::Resource;
use std::fs;
use std::path::Path;

#[derive(Resource, Debug, Default)]
pub struct Library {
    pub scenarios: Vec<ScenarioDef>,
    pub routines: Vec<RoutineDef>,
    pub benchmarks: Vec<BenchmarkDef>,
    /// Human-readable load problems, shown in the menu.
    pub errors: Vec<String>,
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

impl Library {
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
        let ids: Vec<&str> = lib.scenarios.iter().map(|d| d.id.as_str()).collect();
        let mut bad_routines = Vec::new();
        for r in &lib.routines {
            for p in r.validate(&lib.scenarios) {
                lib.errors.push(format!("routine {}: {p}", r.id));
                bad_routines.push(r.id.clone());
            }
        }
        let mut bad_benchmarks = Vec::new();
        for b in &lib.benchmarks {
            for p in b.validate(&ids) {
                lib.errors.push(format!("benchmark {}: {p}", b.id));
                bad_benchmarks.push(b.id.clone());
            }
        }
        lib.routines.retain(|r| !bad_routines.contains(&r.id));
        lib.benchmarks.retain(|b| !bad_benchmarks.contains(&b.id));
        lib.scenarios
            .sort_by(|a, b| (a.category, &a.name).cmp(&(b.category, &b.name)));
        lib
    }

    pub fn scenario(&self, id: &str) -> Option<&ScenarioDef> {
        self.scenarios.iter().find(|d| d.id == id)
    }
}
