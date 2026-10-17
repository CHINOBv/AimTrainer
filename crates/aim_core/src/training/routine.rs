//! Training routines: ordered blocks of activities (scenarios or arenas), written as TOML.
//!
//! Design follows the evidence in `docs/training-science.md`: a short warm-up kept in order,
//! then the remaining runs interleaved (contextual interference, Shea & Morgan 1979),
//! and session lengths around 30–60 minutes (Listman et al. 2021).

use crate::activity::ActivityKind;
use crate::rng::Rng;
use crate::scenario::ScenarioDef;
use serde::{Deserialize, Serialize};

/// A routine: an ordered list of blocks of scenario runs, loaded from TOML.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutineDef {
    /// Stable identifier; equals the content file name.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Short description shown in the UI.
    #[serde(default)]
    pub description: String,
    /// Blocks from this index on are shuffled run-by-run. `None` keeps the order.
    #[serde(default)]
    pub interleave_from: Option<usize>,
    /// Blocks that make up the routine, in play order.
    pub blocks: Vec<Block>,
}

/// One block of a routine: a scenario or arena played for some runs or minutes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Block {
    /// Scenario id. A block names either a `scenario` or an `arena`.
    #[serde(default)]
    pub scenario: String,
    /// Arena id (deathmatch or angle clearing, defined in `aim_world`). Arena blocks set
    /// `runs`, because their length is only known to `aim_world`.
    #[serde(default)]
    pub arena: Option<String>,
    /// Exact number of runs. Takes precedence over `minutes`.
    #[serde(default)]
    pub runs: Option<u32>,
    /// Approximate time budget; converted to runs using the scenario's duration.
    #[serde(default)]
    pub minutes: Option<f32>,
    /// Free-text hint shown to the player during the block.
    #[serde(default)]
    pub note: String,
    /// Difficulty offset relative to the level the routine is played at (see
    /// `scenario::difficulty`): e.g. `-0.25` for a drill, `0.25` for a variant, `0.5` for
    /// a "hard" variant, `0` for the benchmark itself.
    #[serde(default)]
    pub difficulty: f32,
    /// Shorter (or longer) runs than the scenario's own duration, e.g. 20 s micro drills.
    #[serde(default)]
    pub duration_secs: Option<f32>,
}

/// One run in an expanded routine plan.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedRun {
    /// Scenario id, or arena id when `kind` is [`ActivityKind::Arena`].
    pub scenario_id: String,
    /// Whether the activity is a scenario or an arena.
    pub kind: ActivityKind,
    /// Index of the routine block this run came from.
    pub block: usize,
    /// Free-text hint copied from the block.
    pub note: String,
    /// Offset relative to the routine's base level.
    pub difficulty: f32,
    /// Duration override for this run.
    pub duration_secs: Option<f32>,
}

impl Block {
    /// The activity this block plays and its id.
    pub fn activity(&self) -> (ActivityKind, &str) {
        match &self.arena {
            Some(a) => (ActivityKind::Arena, a),
            None => (ActivityKind::Scenario, &self.scenario),
        }
    }
}

impl PlannedRun {
    /// Absolute difficulty level when the routine is played at `base`.
    pub fn level(&self, base: f32) -> f32 {
        (base + self.difficulty).clamp(
            crate::scenario::difficulty::MIN_LEVEL,
            crate::scenario::difficulty::MAX_LEVEL,
        )
    }
}

impl RoutineDef {
    /// Parses a routine from TOML source.
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
            if !(b.difficulty.is_finite() && (-2.0..=2.0).contains(&b.difficulty)) {
                errs.push(format!("blocks[{i}].difficulty: must be within [-2, 2]"));
            }
            match (&b.arena, b.scenario.is_empty()) {
                (Some(_), false) => errs.push(format!(
                    "blocks[{i}]: set either `scenario` or `arena`, not both"
                )),
                (Some(_), true) if b.runs.is_none() => {
                    errs.push(format!("blocks[{i}]: arena blocks need `runs`"))
                }
                (Some(_), true) => {}
                (None, _) if !library.iter().any(|d| d.id == b.scenario) => errs.push(format!(
                    "blocks[{i}].scenario: unknown scenario '{}'",
                    b.scenario
                )),
                (None, _) => {}
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

    /// Arena blocks whose arena is not in `arenas` (arenas live in `aim_world`, so the
    /// caller that knows them runs this check next to [`Self::validate`]).
    pub fn validate_arenas(&self, arenas: &[&str]) -> Vec<String> {
        self.blocks
            .iter()
            .enumerate()
            .filter_map(|(i, b)| {
                let a = b.arena.as_deref()?;
                (!arenas.contains(&a)).then(|| format!("blocks[{i}].arena: unknown arena '{a}'"))
            })
            .collect()
    }

    fn runs_for(block: &Block, library: &[ScenarioDef]) -> u32 {
        if let Some(r) = block.runs {
            return r;
        }
        let duration = block.duration_secs.unwrap_or_else(|| {
            library
                .iter()
                .find(|d| d.id == block.scenario)
                .map_or(60.0, |d| d.duration_secs)
        });
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
                    let (kind, id) = b.activity();
                    runs.push(PlannedRun {
                        scenario_id: id.to_owned(),
                        kind,
                        block: i,
                        note: b.note.clone(),
                        difficulty: b.difficulty,
                        duration_secs: b.duration_secs,
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

    /// Number of runs the routine plans. Shuffling never changes it, so no seed is needed.
    pub fn run_count(&self, library: &[ScenarioDef]) -> usize {
        self.blocks
            .iter()
            .map(|b| Self::runs_for(b, library) as usize)
            .sum()
    }

    /// Total planned minutes. Arena runs count only when their block sets `duration_secs`.
    pub fn minutes(&self, library: &[ScenarioDef]) -> f32 {
        self.plan(library, 0)
            .iter()
            .filter_map(|r| {
                if r.kind == ActivityKind::Arena {
                    return r.duration_secs.map(|s| s / 60.0);
                }
                let d = library.iter().find(|d| d.id == r.scenario_id)?;
                Some(r.duration_secs.unwrap_or(d.duration_secs) / 60.0)
            })
            .sum()
    }
}

/// Routines shipped with the application.
pub mod builtin {
    use super::RoutineDef;

    /// Embedded `(file name, TOML source)` pairs of the built-in routines.
    pub const SOURCES: &[(&str, &str)] = &[
        (
            "aim-and-duel.toml",
            include_str!("../../../../content/routines/aim-and-duel.toml"),
        ),
        (
            "fundamentals-hour.toml",
            include_str!("../../../../content/routines/fundamentals-hour.toml"),
        ),
        (
            "quick-warmup.toml",
            include_str!("../../../../content/routines/quick-warmup.toml"),
        ),
        (
            "skill-clicking-dynamic.toml",
            include_str!("../../../../content/routines/skill-clicking-dynamic.toml"),
        ),
        (
            "skill-clicking-static.toml",
            include_str!("../../../../content/routines/skill-clicking-static.toml"),
        ),
        (
            "skill-switching-evasive.toml",
            include_str!("../../../../content/routines/skill-switching-evasive.toml"),
        ),
        (
            "skill-switching-speed.toml",
            include_str!("../../../../content/routines/skill-switching-speed.toml"),
        ),
        (
            "skill-tracking-precise.toml",
            include_str!("../../../../content/routines/skill-tracking-precise.toml"),
        ),
        (
            "skill-tracking-reactive.toml",
            include_str!("../../../../content/routines/skill-tracking-reactive.toml"),
        ),
        (
            "valorant-daily.toml",
            include_str!("../../../../content/routines/valorant-daily.toml"),
        ),
    ];

    /// Parses every built-in routine; panics if a bundled file is invalid.
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
        assert_eq!(r.run_count(&lib), plan.len());
        assert_eq!(r.run_count(&lib), r.plan(&lib, 99).len());
    }

    #[test]
    fn block_difficulty_reaches_planned_runs_and_levels_clamp() {
        let lib = scenario::builtin::all();
        let r = RoutineDef::from_toml(
            r#"
            id = "x"
            name = "x"
            [[blocks]]
            scenario = "micro-adjust"
            runs = 1
            difficulty = 0.5
            [[blocks]]
            scenario = "micro-adjust"
            runs = 1
            "#,
        )
        .unwrap();
        let plan = r.plan(&lib, 1);
        assert_eq!(plan[0].difficulty, 0.5);
        assert_eq!(plan[1].difficulty, 0.0);
        assert_eq!(plan[0].level(0.75), 1.0);
        assert_eq!(plan[1].level(-0.5), -0.5);
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

    #[test]
    fn arena_blocks_plan_arena_runs_and_are_validated() {
        let lib = scenario::builtin::all();
        let r = RoutineDef::from_toml(
            r#"
            id = "mixed"
            name = "Mixed"
            [[blocks]]
            scenario = "micro-adjust"
            runs = 1
            [[blocks]]
            arena = "yard-deathmatch"
            runs = 2
            duration_secs = 90.0
            "#,
        )
        .unwrap();
        assert!(r.validate(&lib).is_empty(), "{:?}", r.validate(&lib));
        let plan = r.plan(&lib, 1);
        let kinds: Vec<_> = plan
            .iter()
            .map(|p| (p.kind, p.scenario_id.as_str()))
            .collect();
        assert_eq!(
            kinds,
            [
                (ActivityKind::Scenario, "micro-adjust"),
                (ActivityKind::Arena, "yard-deathmatch"),
                (ActivityKind::Arena, "yard-deathmatch"),
            ]
        );
        let micro = lib
            .iter()
            .find(|d| d.id == "micro-adjust")
            .unwrap()
            .duration_secs;
        assert!((r.minutes(&lib) - (micro / 60.0 + 3.0)).abs() < 1e-4);
        assert!(r.validate_arenas(&["yard-deathmatch"]).is_empty());
        assert_eq!(
            r.validate_arenas(&["other"]),
            ["blocks[1].arena: unknown arena 'yard-deathmatch'"]
        );
    }

    #[test]
    fn arena_blocks_need_runs_and_exclude_a_scenario() {
        let lib = scenario::builtin::all();
        let r = RoutineDef::from_toml(
            r#"
            id = "x"
            name = "x"
            [[blocks]]
            arena = "yard-deathmatch"
            minutes = 4.0
            [[blocks]]
            arena = "yard-deathmatch"
            scenario = "micro-adjust"
            runs = 1
            "#,
        )
        .unwrap();
        assert_eq!(
            r.validate(&lib),
            [
                "blocks[0]: arena blocks need `runs`",
                "blocks[1]: set either `scenario` or `arena`, not both",
            ]
        );
    }

    #[test]
    fn scenario_blocks_still_plan_scenario_runs() {
        let lib = scenario::builtin::all();
        let r = builtin::all()
            .into_iter()
            .find(|r| r.id == "valorant-daily")
            .unwrap();
        assert!(
            r.plan(&lib, 3)
                .iter()
                .all(|p| p.kind == ActivityKind::Scenario)
        );
    }
}
