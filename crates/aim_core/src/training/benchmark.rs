//! Benchmarks: skill measurement with ranks, structured like Voltaic's benchmarks.
//!
//! - A scenario's energy grows by `energy_per_rank` per threshold reached, interpolated
//!   linearly between thresholds and capped at the top rank.
//! - A subcategory's energy is the best energy among its scenarios.
//! - Overall energy is the harmonic mean of every subcategory's energy, so weaknesses
//!   pull it down hard, and it only exists once every subcategory has a score.
//! - Tiers (novice, intermediate, advanced) play the same scenario families at another
//!   difficulty level, each with its own thresholds. [`BenchmarkDef::at_tier`] turns a tier
//!   into a plain benchmark whose scenario ids are run keys (`id@-0.50`), so tier scores
//!   never mix with scores at other levels.

use crate::scenario::difficulty::{MAX_LEVEL, MIN_LEVEL};
use crate::scenario::run_key;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// A benchmark definition: ranked skill measurement across subcategories, loaded from TOML.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkDef {
    /// Stable identifier; equals the content file name.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Short description shown in the UI.
    #[serde(default)]
    pub description: String,
    /// `false` while thresholds are provisional (not derived from real player data).
    #[serde(default)]
    pub calibrated: bool,
    /// Rank names, lowest first.
    pub ranks: Vec<String>,
    /// Energy gained per rank threshold reached.
    #[serde(default = "default_energy_per_rank")]
    pub energy_per_rank: f32,
    /// Difficulty tiers, easiest first. Empty means one tier: the scenarios as authored.
    #[serde(default)]
    pub tiers: Vec<BenchTier>,
    /// Skill subcategories; overall energy is the harmonic mean of their energies.
    pub subcategories: Vec<Subcategory>,
}

/// A difficulty tier: the same scenarios played at another level with their own thresholds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchTier {
    /// Stable identifier, unique within the benchmark.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Difficulty level every scenario of this tier is played at (see `scenario::difficulty`).
    /// The tier at level 0 uses each scenario's `thresholds`.
    pub level: f32,
}

fn default_energy_per_rank() -> f32 {
    100.0
}

/// A skill subcategory: a group of scenarios whose best energy counts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subcategory {
    /// Skill key this subcategory measures.
    pub skill: String,
    /// Display name.
    pub name: String,
    /// Scenarios of this subcategory; the best energy among them counts.
    pub scenarios: Vec<BenchScenario>,
}

/// A scenario entry in a benchmark with its rank thresholds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchScenario {
    /// Scenario id.
    pub id: String,
    /// Score needed for each rank, ascending, one per entry in `ranks`.
    pub thresholds: Vec<f32>,
    /// Thresholds of every tier whose level is not 0, by tier id.
    #[serde(default)]
    pub tiers: BTreeMap<String, Vec<f32>>,
}

/// Energy result of one subcategory.
#[derive(Debug, Clone, PartialEq)]
pub struct SubcategoryResult {
    /// Display name of the subcategory.
    pub name: String,
    /// Skill key of the subcategory.
    pub skill: String,
    /// Best energy among its scenarios; `None` until one was played.
    pub energy: Option<f32>,
}

/// Energy results for a whole benchmark.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkResult {
    /// Per-subcategory results, in definition order.
    pub subcategories: Vec<SubcategoryResult>,
    /// Harmonic mean of subcategory energies; `None` until every subcategory has a score.
    pub overall: Option<f32>,
}

impl BenchmarkDef {
    /// Parses a benchmark from TOML source.
    pub fn from_toml(src: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(src)
    }

    /// Problems with this benchmark given the available scenario ids; empty when valid.
    pub fn validate(&self, scenario_ids: &[&str]) -> Vec<String> {
        let mut errs = Vec::new();
        if self.ranks.is_empty() {
            errs.push("ranks: must not be empty".to_owned());
        }
        if self.energy_per_rank.is_nan() || self.energy_per_rank <= 0.0 {
            errs.push("energy_per_rank: must be > 0".to_owned());
        }
        for (i, t) in self.tiers.iter().enumerate() {
            if !(t.level.is_finite() && (MIN_LEVEL..=MAX_LEVEL).contains(&t.level)) {
                errs.push(format!(
                    "tiers[{i}].level: must be within [{MIN_LEVEL}, {MAX_LEVEL}]"
                ));
            }
        }
        if !self.tiers.windows(2).all(|w| w[0].level < w[1].level) {
            errs.push("tiers: levels must increase from tier to tier (easiest first)".to_owned());
        }
        let mut seen = std::collections::HashSet::new();
        if !self.tiers.iter().all(|t| seen.insert(t.id.as_str())) {
            errs.push("tiers: tier ids must be unique".to_owned());
        }
        if !self.tiers.is_empty() && !self.tiers.iter().any(|t| t.level == 0.0) {
            errs.push("tiers: one tier must have level 0 (it uses `thresholds`)".to_owned());
        }
        let ranks = self.ranks.len();
        let check = |errs: &mut Vec<String>, what: String, t: &[f32]| {
            if t.len() != ranks {
                errs.push(format!("{what}: needs {ranks} thresholds"));
            }
            if !t.windows(2).all(|w| w[0] < w[1]) || t.first().is_some_and(|t| *t <= 0.0) {
                errs.push(format!(
                    "{what}: thresholds must be positive and strictly ascending"
                ));
            }
        };
        for sub in &self.subcategories {
            if sub.scenarios.is_empty() {
                errs.push(format!("{}: needs at least one scenario", sub.name));
            }
            for s in &sub.scenarios {
                if !scenario_ids.contains(&s.id.as_str()) {
                    errs.push(format!("{}: unknown scenario '{}'", sub.name, s.id));
                }
                check(&mut errs, s.id.clone(), &s.thresholds);
                for tier in self.tiers.iter().filter(|t| t.level != 0.0) {
                    match s.tiers.get(&tier.id) {
                        Some(t) => check(&mut errs, format!("{}.tiers.{}", s.id, tier.id), t),
                        None => errs.push(format!("{}: missing tiers.{}", s.id, tier.id)),
                    }
                }
                for id in s.tiers.keys() {
                    if !self.tiers.iter().any(|t| &t.id == id && t.level != 0.0) {
                        errs.push(format!(
                            "{}.tiers.{id}: not a tier of this benchmark with a level other than 0",
                            s.id
                        ));
                    }
                }
            }
        }
        errs
    }

    /// The tier with this id, if any.
    pub fn tier(&self, id: &str) -> Option<&BenchTier> {
        self.tiers.iter().find(|t| t.id == id)
    }

    /// Tier played at `level`, if this benchmark has one.
    pub fn tier_at_level(&self, level: f32) -> Option<&BenchTier> {
        self.tiers.iter().find(|t| (t.level - level).abs() < 1e-3)
    }

    /// The benchmark of tier `id`: every scenario id becomes its [`run_key`] at the tier's
    /// level and its thresholds become the tier's. The result has no tiers of its own.
    pub fn at_tier(&self, id: &str) -> Option<BenchmarkDef> {
        let tier = self.tier(id)?;
        let mut out = self.clone();
        out.tiers.clear();
        for s in out
            .subcategories
            .iter_mut()
            .flat_map(|sub| &mut sub.scenarios)
        {
            if let Some(t) = s.tiers.remove(&tier.id) {
                s.thresholds = t;
            }
            s.tiers.clear();
            s.id = run_key(&s.id, tier.level);
        }
        Some(out)
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

/// Benchmarks shipped with the application.
pub mod builtin {
    use super::BenchmarkDef;

    /// Embedded `(file name, TOML source)` pairs of the built-in benchmarks.
    pub const SOURCES: &[(&str, &str)] = &[(
        "valorant-benchmark.toml",
        include_str!("../../../../content/benchmarks/valorant-benchmark.toml"),
    )];

    /// Parses every built-in benchmark; panics if a bundled file is invalid.
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

    #[test]
    fn builtin_benchmark_has_novice_intermediate_and_advanced_tiers() {
        let b = bench();
        let tiers: Vec<_> = b.tiers.iter().map(|t| (t.id.as_str(), t.level)).collect();
        assert_eq!(
            tiers,
            [("novice", -0.5), ("intermediate", 0.0), ("advanced", 0.5)]
        );
    }

    #[test]
    fn a_tier_plays_the_same_family_at_its_level_with_its_own_thresholds() {
        let b = bench();
        let base = &b.subcategories[0].scenarios[0];
        let novice = b.at_tier("novice").unwrap();
        let s = &novice.subcategories[0].scenarios[0];
        assert_eq!(s.id, scenario::run_key(&base.id, -0.5));
        assert_eq!(s.thresholds, base.tiers["novice"]);
        assert_ne!(s.thresholds, base.thresholds);
        assert!(novice.tiers.is_empty());
        // The as-authored tier is the benchmark itself.
        let mid = b.at_tier("intermediate").unwrap();
        assert_eq!(mid.subcategories[0].scenarios[0].id, base.id);
        assert_eq!(
            mid.subcategories[0].scenarios[0].thresholds,
            base.thresholds
        );
        assert_eq!(b.at_tier("nope"), None);
    }

    #[test]
    fn tier_scores_never_mix_with_other_levels() {
        let b = bench();
        let advanced = b.at_tier("advanced").unwrap();
        let mut bests = HashMap::new();
        for sub in &b.subcategories {
            bests.insert(sub.scenarios[0].id.clone(), 1e6);
        }
        // Level-0 scores give the advanced tier no energy at all.
        assert!(
            advanced
                .evaluate(&bests)
                .subcategories
                .iter()
                .all(|s| s.energy.is_none())
        );
        for sub in &advanced.subcategories {
            let s = &sub.scenarios[0];
            bests.insert(s.id.clone(), s.thresholds[1]);
        }
        let r = advanced.evaluate(&bests);
        assert!((r.overall.unwrap() - 200.0).abs() < 1e-3);
    }

    #[test]
    fn untiered_benchmarks_still_load_and_validate() {
        let b = BenchmarkDef::from_toml(
            r#"
            id = "mine"
            name = "Mine"
            ranks = ["A", "B"]
            [[subcategories]]
            skill = "clicking"
            name = "Clicks"
            scenarios = [{ id = "micro-adjust", thresholds = [100, 200] }]
            "#,
        )
        .unwrap();
        assert!(b.tiers.is_empty());
        assert!(b.validate(&["micro-adjust"]).is_empty());
        assert_eq!(b.at_tier("novice"), None);
    }

    #[test]
    fn validation_reports_bad_tiers() {
        let lib = scenario::builtin::all();
        let ids: Vec<_> = lib.iter().map(|d| d.id.as_str()).collect();
        let errs = |f: &dyn Fn(&mut BenchmarkDef)| {
            let mut b = bench();
            f(&mut b);
            b.validate(&ids)
        };
        let missing = errs(&|b| {
            b.subcategories[0].scenarios[0].tiers.remove("advanced");
        });
        assert_eq!(missing.len(), 1, "{missing:?}");
        let unknown = errs(&|b| {
            let s = &mut b.subcategories[0].scenarios[0];
            s.tiers.insert("intermediate".into(), s.thresholds.clone());
            s.tiers.insert("expert".into(), s.thresholds.clone());
        });
        assert_eq!(unknown.len(), 2, "{unknown:?}");
        let descending = errs(&|b| {
            b.subcategories[0].scenarios[0]
                .tiers
                .get_mut("novice")
                .unwrap()
                .reverse();
        });
        assert_eq!(descending.len(), 1, "{descending:?}");
        let unordered = errs(&|b| b.tiers.swap(0, 2));
        assert_eq!(unordered.len(), 1, "{unordered:?}");
        let out_of_range = errs(&|b| b.tiers[2].level = 1.5);
        assert_eq!(out_of_range.len(), 1, "{out_of_range:?}");
        let no_base = errs(&|b| b.tiers.retain(|t| t.level != 0.0));
        assert!(!no_base.is_empty(), "{no_base:?}");
        let duplicate = errs(&|b| b.tiers[2].id = b.tiers[0].id.clone());
        assert!(
            duplicate
                .iter()
                .any(|e| e.contains("tier ids must be unique")),
            "{duplicate:?}"
        );
    }
}
