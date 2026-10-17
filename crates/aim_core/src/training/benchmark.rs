//! Benchmarks: skill measurement with ranks, structured like Voltaic's benchmarks.
//!
//! - A scenario's energy grows by `energy_per_rank` per threshold reached, interpolated
//!   linearly between thresholds and capped at the top rank.
//! - A subcategory's energy is the best energy among its scenarios.
//! - Overall energy is the harmonic mean of every subcategory's energy, so weaknesses
//!   pull it down hard, and it only exists once every subcategory has a score.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// `false` while thresholds are provisional (not derived from real player data).
    #[serde(default)]
    pub calibrated: bool,
    pub ranks: Vec<String>,
    #[serde(default = "default_energy_per_rank")]
    pub energy_per_rank: f32,
    pub subcategories: Vec<Subcategory>,
}

fn default_energy_per_rank() -> f32 {
    100.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subcategory {
    pub skill: String,
    pub name: String,
    pub scenarios: Vec<BenchScenario>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchScenario {
    pub id: String,
    /// Score needed for each rank, ascending, one per entry in `ranks`.
    pub thresholds: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubcategoryResult {
    pub name: String,
    pub skill: String,
    pub energy: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkResult {
    pub subcategories: Vec<SubcategoryResult>,
    pub overall: Option<f32>,
}

impl BenchmarkDef {
    pub fn from_toml(src: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(src)
    }

    pub fn validate(&self, scenario_ids: &[&str]) -> Vec<String> {
        let mut errs = Vec::new();
        if self.ranks.is_empty() {
            errs.push("ranks: must not be empty".to_owned());
        }
        if self.energy_per_rank.is_nan() || self.energy_per_rank <= 0.0 {
            errs.push("energy_per_rank: must be > 0".to_owned());
        }
        for sub in &self.subcategories {
            if sub.scenarios.is_empty() {
                errs.push(format!("{}: needs at least one scenario", sub.name));
            }
            for s in &sub.scenarios {
                if !scenario_ids.contains(&s.id.as_str()) {
                    errs.push(format!("{}: unknown scenario '{}'", sub.name, s.id));
                }
                if s.thresholds.len() != self.ranks.len() {
                    errs.push(format!("{}: needs {} thresholds", s.id, self.ranks.len()));
                }
                if !s.thresholds.windows(2).all(|w| w[0] < w[1])
                    || s.thresholds.first().is_some_and(|t| *t <= 0.0)
                {
                    errs.push(format!(
                        "{}: thresholds must be positive and strictly ascending",
                        s.id
                    ));
                }
            }
        }
        errs
    }

    /// Energy earned by `score` against ascending `thresholds`.
    pub fn energy(&self, thresholds: &[f32], score: f32) -> f32 {
        let step = self.energy_per_rank;
        let score = score.max(0.0);
        let Some(&first) = thresholds.first() else {
            return 0.0;
        };
        if score < first {
            return step * score / first;
        }
        for (i, w) in thresholds.windows(2).enumerate() {
            if score < w[1] {
                return step * (i as f32 + 1.0) + step * (score - w[0]) / (w[1] - w[0]);
            }
        }
        step * thresholds.len() as f32
    }

    /// Rank reached with this much energy (`None` below the first rank).
    pub fn rank_for(&self, energy: f32) -> Option<&str> {
        let idx = (energy / self.energy_per_rank).floor() as usize;
        idx.checked_sub(1)
            .and_then(|i| self.ranks.get(i.min(self.ranks.len() - 1)))
            .map(String::as_str)
    }

    /// Evaluate personal bests (`scenario id -> best score`).
    pub fn evaluate(&self, bests: &HashMap<String, f32>) -> BenchmarkResult {
        let subcategories: Vec<_> = self
            .subcategories
            .iter()
            .map(|sub| SubcategoryResult {
                name: sub.name.clone(),
                skill: sub.skill.clone(),
                energy: sub
                    .scenarios
                    .iter()
                    .filter_map(|s| {
                        bests
                            .get(&s.id)
                            .map(|score| self.energy(&s.thresholds, *score))
                    })
                    .reduce(f32::max),
            })
            .collect();
        let energies: Option<Vec<f32>> = subcategories
            .iter()
            .map(|s| s.energy.filter(|e| *e > 0.0))
            .collect();
        let overall = energies
            .filter(|e| !e.is_empty())
            .map(|e| e.len() as f32 / e.iter().map(|x| 1.0 / x).sum::<f32>());
        BenchmarkResult {
            subcategories,
            overall,
        }
    }
}

pub mod builtin {
    use super::BenchmarkDef;

    pub const SOURCES: &[(&str, &str)] = &[(
        "valorant-benchmark.toml",
        include_str!("../../../../content/benchmarks/valorant-benchmark.toml"),
    )];

    pub fn all() -> Vec<BenchmarkDef> {
        SOURCES
            .iter()
            .map(|(file, src)| {
                BenchmarkDef::from_toml(src).unwrap_or_else(|e| panic!("built-in {file}: {e}"))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario;

    fn bench() -> BenchmarkDef {
        builtin::all().remove(0)
    }

    #[test]
    fn builtin_benchmark_validates_against_builtin_scenarios() {
        let lib = scenario::builtin::all();
        let ids: Vec<_> = lib.iter().map(|d| d.id.as_str()).collect();
        let b = bench();
        assert!(b.validate(&ids).is_empty(), "{:?}", b.validate(&ids));
        assert!(
            !b.calibrated,
            "thresholds are provisional until real data exists"
        );
    }

    #[test]
    fn energy_interpolates_between_thresholds_and_caps() {
        let b = bench();
        let t = [100.0, 200.0, 400.0, 800.0];
        assert_eq!(b.energy(&t, 50.0), 50.0);
        assert_eq!(b.energy(&t, 100.0), 100.0);
        assert_eq!(b.energy(&t, 300.0), 250.0);
        assert_eq!(b.energy(&t, 5000.0), 400.0);
        assert_eq!(b.rank_for(50.0), None);
        assert_eq!(b.rank_for(250.0), Some(b.ranks[1].as_str()));
        assert_eq!(b.rank_for(400.0), Some(b.ranks[3].as_str()));
    }

    #[test]
    fn overall_is_harmonic_mean_and_needs_every_subcategory() {
        let b = bench();
        let mut bests = HashMap::new();
        for sub in &b.subcategories {
            bests.insert(sub.scenarios[0].id.clone(), sub.scenarios[0].thresholds[1]);
        }
        let r = b.evaluate(&bests);
        assert!((r.overall.unwrap() - 200.0).abs() < 1e-3);

        // One weak subcategory drags the harmonic mean below the arithmetic mean.
        let weak = b.subcategories[0].scenarios[0].clone();
        bests.insert(weak.id.clone(), weak.thresholds[0] / 2.0);
        let r = b.evaluate(&bests);
        let arith = r
            .subcategories
            .iter()
            .map(|s| s.energy.unwrap())
            .sum::<f32>()
            / r.subcategories.len() as f32;
        assert!(r.overall.unwrap() < arith);

        bests.remove(&weak.id);
        assert_eq!(b.evaluate(&bests).overall, None);
    }

    #[test]
    fn subcategory_uses_best_scenario() {
        let b = bench();
        let sub = b
            .subcategories
            .iter()
            .find(|s| s.scenarios.len() >= 2)
            .unwrap();
        let mut bests = HashMap::new();
        bests.insert(sub.scenarios[0].id.clone(), sub.scenarios[0].thresholds[0]);
        bests.insert(sub.scenarios[1].id.clone(), sub.scenarios[1].thresholds[2]);
        let r = b.evaluate(&bests);
        let e = r
            .subcategories
            .iter()
            .find(|s| s.name == sub.name)
            .unwrap()
            .energy
            .unwrap();
        assert_eq!(e, 300.0);
    }
}
