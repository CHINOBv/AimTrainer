//! Skill profile per benchmark subcategory: strengths, weaknesses and trends over time.
//!
//! The benchmark's subcategory energies (personal bests from the run history, see
//! [`crate::training::benchmark`]) are compared with the player's own average over the
//! subcategories played. A subcategory clearly above that average is a strength and one
//! clearly below is a weakness, so the profile points at the skill holding the harmonic
//! mean (overall energy) down. The margin is provisional (see "Analytics metrics" in
//! `docs/training-science.md`).

use super::rules::Verdict;
use crate::training::benchmark::BenchmarkDef;
use crate::training::progress::{self, RunRecord};

/// A subcategory more than this share above (below) the average is a strength (weakness).
pub const STANDOUT_MARGIN: f32 = 0.15;
/// Fewer played subcategories than this give no verdicts (there is nothing to compare).
pub const MIN_PLAYED: usize = 2;

/// The standing of one benchmark subcategory.
#[derive(Debug, Clone, PartialEq)]
pub struct SubcategoryStanding {
    /// Subcategory name as defined in the benchmark.
    pub name: String,
    /// The skill the subcategory trains.
    pub skill: String,
    /// Current energy from personal bests; `None` until one of its scenarios was played.
    pub energy: Option<f32>,
    /// Strength or weakness relative to the player's average, if comparable.
    pub verdict: Option<Verdict>,
    /// "Static clicking 150 energy is 40% below your average of 250 …", or why there is no
    /// verdict.
    pub explanation: String,
    /// Energy at the end of each of the last weeks (oldest first).
    pub weekly: Vec<Option<f32>>,
    /// The subcategory's benchmark scenarios: what to practice for a weakness.
    pub scenario_ids: Vec<String>,
}

/// Standing of every benchmark subcategory, with the player's average.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkProfile {
    /// Arithmetic mean of the played subcategories' energy.
    pub average: Option<f32>,
    /// One entry per subcategory, in the benchmark's order.
    pub subcategories: Vec<SubcategoryStanding>,
}

impl BenchmarkProfile {
    /// Profile `benchmark` (any benchmark definition) from the run history at `now`, with
    /// a trend over the last `weeks` weeks.
    pub fn build(benchmark: &BenchmarkDef, records: &[RunRecord], now: u64, weeks: u32) -> Self {
        let current = benchmark.evaluate(&progress::bests_until(records, now));
        let trends = progress::subcategory_trend(benchmark, records, now, weeks);
        let played: Vec<f32> = current
            .subcategories
            .iter()
            .filter_map(|s| s.energy)
            .collect();
        let average =
            (!played.is_empty()).then(|| played.iter().sum::<f32>() / played.len() as f32);
        let comparable = average.filter(|_| played.len() >= MIN_PLAYED);
        let subcategories = benchmark
            .subcategories
            .iter()
            .zip(current.subcategories)
            .zip(trends)
            .map(|((def, result), trend)| {
                let (verdict, explanation) =
                    judge(&def.name, result.energy, comparable, played.len());
                SubcategoryStanding {
                    name: def.name.clone(),
                    skill: def.skill.clone(),
                    energy: result.energy,
                    verdict,
                    explanation,
                    weekly: trend.energies,
                    scenario_ids: def.scenarios.iter().map(|s| s.id.clone()).collect(),
                }
            })
            .collect();
        Self {
            average,
            subcategories,
        }
    }

    /// Weak subcategories, weakest first.
    pub fn weaknesses(&self) -> Vec<&SubcategoryStanding> {
        let mut v: Vec<_> = self
            .subcategories
            .iter()
            .filter(|s| s.verdict == Some(Verdict::Weakness))
            .collect();
        v.sort_by(|a, b| a.energy.unwrap_or(0.0).total_cmp(&b.energy.unwrap_or(0.0)));
        v
    }

    /// Strong subcategories, in benchmark order.
    pub fn strengths(&self) -> impl Iterator<Item = &SubcategoryStanding> {
        self.subcategories
            .iter()
            .filter(|s| s.verdict == Some(Verdict::Strength))
    }
}

fn judge(
    name: &str,
    energy: Option<f32>,
    average: Option<f32>,
    played: usize,
) -> (Option<Verdict>, String) {
    let Some(energy) = energy else {
        return (None, format!("{name}: not played yet."));
    };
    let Some(average) = average.filter(|a| *a > 0.0) else {
        return (
            None,
            format!(
                "{name}: {energy:.0} energy. Play at least {MIN_PLAYED} subcategories to compare them."
            ),
        );
    };
    let ratio = energy / average;
    let verdict = if ratio < 1.0 - STANDOUT_MARGIN {
        Some(Verdict::Weakness)
    } else if ratio > 1.0 + STANDOUT_MARGIN {
        Some(Verdict::Strength)
    } else {
        None
    };
    let relation = if ratio < 1.0 {
        format!("{:.0}% below", (1.0 - ratio) * 100.0)
    } else {
        format!("{:.0}% above", (ratio - 1.0) * 100.0)
    };
    (
        verdict,
        format!(
            "{name}: {energy:.0} energy is {relation} your average of {average:.0} across {played} played subcategories."
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::training::benchmark;

    const DAY: u64 = 86_400;
    const NOW: u64 = 1_800_000_000;

    fn rec(ts: u64, scenario: &str, score: f32) -> RunRecord {
        RunRecord {
            timestamp: ts,
            scenario: scenario.into(),
            score,
            duration_secs: 60.0,
        }
    }

    /// One run per subcategory at the given threshold index (`None` = not played).
    fn runs(b: &BenchmarkDef, ranks: &[Option<usize>], ts: u64) -> Vec<RunRecord> {
        b.subcategories
            .iter()
            .zip(ranks)
            .filter_map(|(sub, r)| {
                let s = &sub.scenarios[0];
                r.map(|i| rec(ts, &s.id, s.thresholds[i]))
            })
            .collect()
    }

    #[test]
    fn subcategories_far_from_the_average_are_strengths_and_weaknesses() {
        let b = benchmark::builtin::all().remove(0);
        assert!(b.subcategories.len() >= 4);
        // Energies 100, 300, 200, 200, … (threshold index i gives (i + 1) * 100 energy).
        let mut ranks = vec![Some(1); b.subcategories.len()];
        ranks[0] = Some(0);
        ranks[1] = Some(2);
        let p = BenchmarkProfile::build(&b, &runs(&b, &ranks, NOW - 60), NOW, 4);
        assert_eq!(p.subcategories.len(), b.subcategories.len());
        assert!((p.average.unwrap() - 200.0).abs() < 1e-3);
        let weak = p.weaknesses();
        assert_eq!(weak.len(), 1);
        assert_eq!(weak[0].name, b.subcategories[0].name);
        assert_eq!(
            weak[0].explanation,
            format!(
                "{}: 100 energy is 50% below your average of 200 across {} played subcategories.",
                b.subcategories[0].name,
                b.subcategories.len()
            )
        );
        assert_eq!(
            weak[0].scenario_ids,
            b.subcategories[0]
                .scenarios
                .iter()
                .map(|s| s.id.clone())
                .collect::<Vec<_>>()
        );
        let strong: Vec<_> = p.strengths().map(|s| s.name.as_str()).collect();
        assert_eq!(strong, vec![b.subcategories[1].name.as_str()]);
        assert!(p.subcategories[2..].iter().all(|s| s.verdict.is_none()));
    }

    #[test]
    fn unplayed_subcategories_and_a_single_one_get_no_verdict() {
        let b = benchmark::builtin::all().remove(0);
        let mut ranks = vec![None; b.subcategories.len()];
        ranks[0] = Some(0);
        let p = BenchmarkProfile::build(&b, &runs(&b, &ranks, NOW - 60), NOW, 2);
        assert!(p.subcategories.iter().all(|s| s.verdict.is_none()));
        assert_eq!(p.subcategories[1].energy, None);
        assert!(p.subcategories[1].explanation.ends_with("not played yet."));
        assert!(
            p.subcategories[0].explanation.contains("Play at least 2"),
            "{}",
            p.subcategories[0].explanation
        );
        assert_eq!(BenchmarkProfile::build(&b, &[], NOW, 2).average, None);
    }

    #[test]
    fn standings_carry_their_weekly_trend() {
        let b = benchmark::builtin::all().remove(0);
        let s = &b.subcategories[0].scenarios[0];
        let r = vec![
            rec(NOW - 7 * DAY - 60, &s.id, s.thresholds[0]),
            rec(NOW - 60, &s.id, s.thresholds[2]),
        ];
        let p = BenchmarkProfile::build(&b, &r, NOW, 3);
        assert_eq!(
            p.subcategories[0].weekly,
            vec![None, Some(100.0), Some(300.0)]
        );
        assert_eq!(p.subcategories[0].energy, Some(300.0));
    }
}
