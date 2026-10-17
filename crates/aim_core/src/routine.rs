//! Training routines: ordered blocks of scenarios, written as TOML.
//!
//! Design follows the evidence in `docs/training-science.md`: a short warm-up kept in order,
//! then the remaining runs interleaved (contextual interference, Shea & Morgan 1979),
//! and session lengths around 30–60 minutes (Listman et al. 2021).

use crate::rng::Rng;
use crate::scenario::ScenarioDef;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutineDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Blocks from this index on are shuffled run-by-run. `None` keeps the order.
    #[serde(default)]
    pub interleave_from: Option<usize>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub scenario: String,
    /// Exact number of runs. Takes precedence over `minutes`.
    #[serde(default)]
    pub runs: Option<u32>,
    /// Approximate time budget; converted to runs using the scenario's duration.
    #[serde(default)]
    pub minutes: Option<f32>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedRun {
    pub scenario_id: String,
    pub block: usize,
    pub note: String,
}

impl RoutineDef {
    pub fn from_toml(src: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(src)
    }

    /// Problems with this routine given the available scenarios.
    pub fn validate(&self, library: &[ScenarioDef]) -> Vec<String> {
        let mut errs = Vec::new();
        if self.blocks.is_empty() {
            errs.push("blocks: must not be empty".to_owned());
        }
        if self.interleave_from.is_some_and(|i| i >= self.blocks.len()) {
            errs.push("interleave_from: must point at an existing block".to_owned());
        }
        for (i, b) in self.blocks.iter().enumerate() {
            if !library.iter().any(|d| d.id == b.scenario) {
                errs.push(format!(
                    "blocks[{i}].scenario: unknown scenario '{}'",
                    b.scenario
                ));
            }
            match (b.runs, b.minutes) {
                (None, None) => errs.push(format!("blocks[{i}]: set `runs` or `minutes`")),
                (Some(0), _) => errs.push(format!("blocks[{i}].runs: must be >= 1")),
                (None, Some(m)) if m.is_nan() || m <= 0.0 => {
                    errs.push(format!("blocks[{i}].minutes: must be > 0"))
                }
                _ => {}
            }
        }
        errs
    }

    fn runs_for(block: &Block, library: &[ScenarioDef]) -> u32 {
        if let Some(r) = block.runs {
            return r;
        }
        let duration = library
            .iter()
            .find(|d| d.id == block.scenario)
            .map_or(60.0, |d| d.duration_secs);
        ((block.minutes.unwrap_or(1.0) * 60.0 / duration).round() as u32).max(1)
    }

    /// Expand into the concrete list of runs. Interleaved runs are shuffled and then
    /// spread so the same scenario rarely appears twice in a row.
    pub fn plan(&self, library: &[ScenarioDef], seed: u64) -> Vec<PlannedRun> {
        let expand = |range: std::ops::Range<usize>| -> Vec<PlannedRun> {
            let mut runs = Vec::new();
            for i in range {
                let b = &self.blocks[i];
                for _ in 0..Self::runs_for(b, library) {
                    runs.push(PlannedRun {
                        scenario_id: b.scenario.clone(),
                        block: i,
                        note: b.note.clone(),
                    });
                }
            }
            runs
        };
        let split = self.interleave_from.unwrap_or(self.blocks.len());
        let mut plan = expand(0..split);
        let mut rest = expand(split..self.blocks.len());

        let mut rng = Rng::new(seed);
        for i in (1..rest.len()).rev() {
            let j = (rng.next_u64() % (i as u64 + 1)) as usize;
            rest.swap(i, j);
        }
        // Greedy pass: avoid back-to-back repeats when another scenario is available.
        for i in 1..rest.len() {
            if rest[i].scenario_id == rest[i - 1].scenario_id
                && let Some(j) =
                    (i + 1..rest.len()).find(|&j| rest[j].scenario_id != rest[i - 1].scenario_id)
            {
                rest.swap(i, j);
            }
        }
        plan.extend(rest);
        plan
    }

    /// Total planned minutes.
    pub fn minutes(&self, library: &[ScenarioDef]) -> f32 {
        self.plan(library, 0)
            .iter()
            .filter_map(|r| library.iter().find(|d| d.id == r.scenario_id))
            .map(|d| d.duration_secs / 60.0)
            .sum()
    }
}

pub mod builtin {
    use super::RoutineDef;

    pub const SOURCES: &[(&str, &str)] = &[
        (
            "valorant-daily.toml",
            include_str!("../../../content/routines/valorant-daily.toml"),
        ),
        (
            "fundamentals-hour.toml",
            include_str!("../../../content/routines/fundamentals-hour.toml"),
        ),
        (
            "quick-warmup.toml",
            include_str!("../../../content/routines/quick-warmup.toml"),
        ),
    ];

    pub fn all() -> Vec<RoutineDef> {
        SOURCES
            .iter()
            .map(|(file, src)| {
                RoutineDef::from_toml(src).unwrap_or_else(|e| panic!("built-in {file}: {e}"))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario;

    #[test]
    fn builtin_routines_validate_and_respect_session_length_research() {
        let lib = scenario::builtin::all();
        for r in builtin::all() {
            assert!(
                r.validate(&lib).is_empty(),
                "{}: {:?}",
                r.id,
                r.validate(&lib)
            );
            let m = r.minutes(&lib);
            assert!((5.0..=60.0).contains(&m), "{}: {m} min", r.id);
        }
    }

    #[test]
    fn warmup_stays_in_order_and_the_rest_is_interleaved_without_repeats() {
        let lib = scenario::builtin::all();
        let r = builtin::all()
            .into_iter()
            .find(|r| r.id == "valorant-daily")
            .unwrap();
        let plan = r.plan(&lib, 3);
        let split = r.interleave_from.unwrap();
        let warmup = plan.iter().take_while(|p| p.block < split).count();
        assert!(plan[..warmup].windows(2).all(|w| w[0].block <= w[1].block));
        let rest = &plan[warmup..];
        assert!(rest.iter().all(|p| p.block >= split));
        let repeats = rest
            .windows(2)
            .filter(|w| w[0].scenario_id == w[1].scenario_id)
            .count();
        assert_eq!(
            repeats,
            0,
            "{:?}",
            rest.iter().map(|p| &p.scenario_id).collect::<Vec<_>>()
        );
        assert_ne!(plan, r.plan(&lib, 4), "different seeds shuffle differently");
    }

    #[test]
    fn validation_catches_unknown_scenarios_and_missing_amounts() {
        let lib = scenario::builtin::all();
        let r = RoutineDef::from_toml(
            r#"
            id = "x"
            name = "x"
            [[blocks]]
            scenario = "nope"
            [[blocks]]
            scenario = "micro-adjust"
            runs = 0
            "#,
        )
        .unwrap();
        let errs = r.validate(&lib);
        assert_eq!(errs.len(), 3, "{errs:?}");
    }
}
