//! Personalized playlists: a routine built from the player's benchmark result that spends
//! more runs on weak subcategories and interleaves everything (contextual interference,
//! Shea & Morgan 1979), within a time budget (Listman et al. 2021: ~30–50 min/day).

use super::benchmark::{BenchmarkDef, BenchmarkResult};
use super::routine::{Block, RoutineDef};
use crate::scenario::ScenarioDef;

/// Id of the generated routine.
pub const PERSONALIZED_ID: &str = "personalized";
/// Energy assumed for a subcategory that has never been played (weak, but not ignored).
const UNPLAYED_ENERGY: f32 = 50.0;

/// Build a routine of about `minutes` from `benchmark` scenarios. Each subcategory gets at
/// least one run; the rest is shared in proportion to `1 / energy`, so the weakest
/// subcategory gets the most practice. A short warm-up run opens the session.
pub fn personalized(
    benchmark: &BenchmarkDef,
    result: &BenchmarkResult,
    scenarios: &[ScenarioDef],
    minutes: f32,
) -> RoutineDef {
    let duration = |id: &str| {
        scenarios
            .iter()
            .find(|d| d.id == id)
            .map_or(60.0, |d| d.duration_secs)
    };
    let subs: Vec<(&str, f32)> = benchmark
        .subcategories
        .iter()
        .filter_map(|sub| {
            let first = sub.scenarios.first()?;
            let energy = result
                .subcategories
                .iter()
                .find(|r| r.name == sub.name)
                .and_then(|r| r.energy)
                .unwrap_or(UNPLAYED_ENERGY)
                .max(1.0);
            Some((first.id.as_str(), energy))
        })
        .collect();

    let avg_secs = subs.iter().map(|(id, _)| duration(id)).sum::<f32>() / subs.len().max(1) as f32;
    let total_runs = ((minutes * 60.0 / avg_secs.max(1.0)).round() as usize).max(subs.len());
    let weights: Vec<f32> = subs.iter().map(|(_, e)| 1.0 / e).collect();
    let weight_sum: f32 = weights.iter().sum();
    let extra = total_runs - subs.len();

    // Largest-remainder apportionment of the extra runs.
    let exact: Vec<f32> = weights
        .iter()
        .map(|w| extra as f32 * w / weight_sum)
        .collect();
    let mut runs: Vec<usize> = exact.iter().map(|x| 1 + x.floor() as usize).collect();
    let mut left = total_runs - runs.iter().sum::<usize>();
    let mut order: Vec<usize> = (0..subs.len()).collect();
    order.sort_by(|a, b| {
        (exact[*b] - exact[*b].floor()).total_cmp(&(exact[*a] - exact[*a].floor()))
    });
    for i in order {
        if left == 0 {
            break;
        }
        runs[i] += 1;
        left -= 1;
    }

    let warmup = Block {
        scenario: "track-precise".to_owned(),
        runs: Some(1),
        minutes: None,
        note: "Warm-up: smooth and relaxed.".to_owned(),
        difficulty: -0.25,
        duration_secs: None,
    };
    let mut blocks = vec![warmup];
    for ((id, energy), n) in subs.iter().zip(runs) {
        blocks.push(Block {
            scenario: (*id).to_owned(),
            runs: Some(n as u32),
            minutes: None,
            note: format!("Focus weight from your benchmark energy ({energy:.0})."),
            difficulty: 0.0,
            duration_secs: None,
        });
    }
    RoutineDef {
        id: PERSONALIZED_ID.to_owned(),
        name: "Personalized playlist".to_owned(),
        description: format!(
            "Built from your {} result: weaker skills get more runs, everything is interleaved.",
            benchmark.name
        ),
        interleave_from: Some(1),
        blocks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::builtin;
    use crate::training::benchmark;
    use std::collections::HashMap;

    fn setup() -> (BenchmarkDef, Vec<ScenarioDef>) {
        (benchmark::builtin::all().remove(0), builtin::all())
    }

    #[test]
    fn weakest_subcategory_gets_the_most_runs_and_every_one_is_covered() {
        let (b, lib) = setup();
        let mut bests = HashMap::new();
        for sub in &b.subcategories {
            bests.insert(sub.scenarios[0].id.clone(), sub.scenarios[0].thresholds[2]);
        }
        let weak = &b.subcategories[3];
        bests.insert(
            weak.scenarios[0].id.clone(),
            weak.scenarios[0].thresholds[0] * 0.5,
        );
        let r = personalized(&b, &b.evaluate(&bests), &lib, 30.0);
        assert!(r.validate(&lib).is_empty(), "{:?}", r.validate(&lib));
        let runs_for = |id: &str| {
            r.blocks
                .iter()
                .filter(|x| x.scenario == id)
                .map(|x| x.runs.unwrap())
                .sum::<u32>()
        };
        let weak_runs = runs_for(&weak.scenarios[0].id);
        for sub in &b.subcategories {
            let n = runs_for(&sub.scenarios[0].id);
            assert!(n >= 1, "{} uncovered", sub.name);
            if sub.name != weak.name {
                assert!(weak_runs > n, "{}: {n} vs weak {weak_runs}", sub.name);
            }
        }
    }

    #[test]
    fn respects_the_time_budget_and_interleaves_after_the_warmup() {
        let (b, lib) = setup();
        let r = personalized(&b, &b.evaluate(&HashMap::new()), &lib, 30.0);
        let minutes = r.minutes(&lib);
        assert!((24.0..=36.0).contains(&minutes), "{minutes}");
        assert_eq!(r.interleave_from, Some(1));
        assert_eq!(r.plan(&lib, 1)[0].scenario_id, "track-precise");
    }
}
