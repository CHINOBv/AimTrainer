//! Weekly training methods: one routine per weekday, played at a tier's difficulty level.
//!
//! The built-in plan follows the structure of VDIM (Voltaic Daily Improvement Method): one
//! skill per day from Monday to Saturday and a flexible Sunday (benchmark focus, replay the
//! weakest day, or rest). See `docs/training-science.md`.

use super::benchmark::{BenchmarkDef, BenchmarkResult};
use super::routine::RoutineDef;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MethodDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Attribution for the method's structure, shown in the UI.
    #[serde(default)]
    pub credit: String,
    /// Benchmark used to suggest a tier and find the weakest day.
    #[serde(default)]
    pub benchmark: Option<String>,
    pub tiers: Vec<Tier>,
    pub week: Week,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tier {
    pub id: String,
    pub name: String,
    /// Base difficulty level of every run (see `scenario::difficulty`).
    pub level: f32,
    /// Suggested while the overall benchmark energy is below this value.
    #[serde(default)]
    pub up_to_energy: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Week {
    pub monday: DayPlan,
    pub tuesday: DayPlan,
    pub wednesday: DayPlan,
    pub thursday: DayPlan,
    pub friday: DayPlan,
    pub saturday: DayPlan,
    pub sunday: DayPlan,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DayPlan {
    /// Routine played that day.
    #[serde(default)]
    pub routine: Option<String>,
    /// Benchmark subcategories this day trains (used to find the weakest day).
    #[serde(default)]
    pub focus: Vec<String>,
    /// Free day: benchmark focus, replay the weakest day, or rest.
    #[serde(default)]
    pub flex: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weekday {
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
    Sunday,
}

impl Weekday {
    pub const ALL: [Weekday; 7] = [
        Weekday::Monday,
        Weekday::Tuesday,
        Weekday::Wednesday,
        Weekday::Thursday,
        Weekday::Friday,
        Weekday::Saturday,
        Weekday::Sunday,
    ];

    /// 0 = Monday … 6 = Sunday (ISO order).
    pub fn from_monday_index(i: u32) -> Self {
        Self::ALL[(i % 7) as usize]
    }

    pub fn label(self) -> &'static str {
        match self {
            Weekday::Monday => "Monday",
            Weekday::Tuesday => "Tuesday",
            Weekday::Wednesday => "Wednesday",
            Weekday::Thursday => "Thursday",
            Weekday::Friday => "Friday",
            Weekday::Saturday => "Saturday",
            Weekday::Sunday => "Sunday",
        }
    }
}

impl MethodDef {
    pub fn from_toml(src: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(src)
    }

    pub fn day(&self, day: Weekday) -> &DayPlan {
        let w = &self.week;
        match day {
            Weekday::Monday => &w.monday,
            Weekday::Tuesday => &w.tuesday,
            Weekday::Wednesday => &w.wednesday,
            Weekday::Thursday => &w.thursday,
            Weekday::Friday => &w.friday,
            Weekday::Saturday => &w.saturday,
            Weekday::Sunday => &w.sunday,
        }
    }

    pub fn tier(&self, id: &str) -> Option<&Tier> {
        self.tiers.iter().find(|t| t.id == id)
    }

    /// Tier suggested for an overall benchmark energy (`None` = not benchmarked yet, which
    /// suggests the first tier).
    pub fn suggested_tier(&self, energy: Option<f32>) -> &Tier {
        let e = energy.unwrap_or(0.0);
        self.tiers
            .iter()
            .find(|t| t.up_to_energy.is_none_or(|limit| e < limit))
            .or(self.tiers.last())
            .expect("validated methods have at least one tier")
    }

    /// Day whose focus contains the weakest benchmark subcategory (unscored counts as 0).
    pub fn weakest_day(&self, result: &BenchmarkResult) -> Option<Weekday> {
        let weakest = result
            .subcategories
            .iter()
            .filter(|s| {
                Weekday::ALL
                    .iter()
                    .any(|d| self.day(*d).focus.contains(&s.name))
            })
            .min_by(|a, b| a.energy.unwrap_or(0.0).total_cmp(&b.energy.unwrap_or(0.0)))?;
        Weekday::ALL
            .into_iter()
            .find(|d| self.day(*d).focus.contains(&weakest.name))
    }

    pub fn validate(&self, routines: &[RoutineDef], benchmarks: &[BenchmarkDef]) -> Vec<String> {
        let mut errs = Vec::new();
        if self.tiers.is_empty() {
            errs.push("tiers: must not be empty".to_owned());
        }
        for (i, t) in self.tiers.iter().enumerate() {
            if !(t.level.is_finite() && (-1.0..=1.0).contains(&t.level)) {
                errs.push(format!("tiers[{i}].level: must be within [-1, 1]"));
            }
        }
        let limits: Vec<f32> = self.tiers.iter().filter_map(|t| t.up_to_energy).collect();
        if !limits.windows(2).all(|w| w[0] < w[1]) {
            errs.push("tiers: up_to_energy must increase from tier to tier".to_owned());
        }
        let bench = self
            .benchmark
            .as_ref()
            .map(|id| benchmarks.iter().find(|b| &b.id == id));
        if let Some(None) = bench {
            errs.push(format!(
                "benchmark: unknown benchmark '{}'",
                self.benchmark.as_deref().unwrap_or("")
            ));
        }
        for day in Weekday::ALL {
            let plan = self.day(day);
            let label = day.label();
            match (&plan.routine, plan.flex) {
                (None, false) => errs.push(format!("week.{label}: set `routine` or `flex = true`")),
                (Some(_), true) => {
                    errs.push(format!("week.{label}: `routine` and `flex` are exclusive"))
                }
                (Some(r), false) if !routines.iter().any(|x| &x.id == r) => {
                    errs.push(format!("week.{label}: unknown routine '{r}'"));
                }
                _ => {}
            }
            if let Some(Some(b)) = bench {
                for f in &plan.focus {
                    if !b.subcategories.iter().any(|s| &s.name == f) {
                        errs.push(format!(
                            "week.{label}.focus: '{f}' is not a subcategory of '{}'",
                            b.id
                        ));
                    }
                }
            }
        }
        errs
    }
}

pub mod builtin {
    use super::MethodDef;

    pub const SOURCES: &[(&str, &str)] = &[(
        "weekly-skill-plan.toml",
        include_str!("../../../../content/methods/weekly-skill-plan.toml"),
    )];

    pub fn all() -> Vec<MethodDef> {
        SOURCES
            .iter()
            .map(|(file, src)| {
                MethodDef::from_toml(src).unwrap_or_else(|e| panic!("built-in {file}: {e}"))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::training::{benchmark, routine};
    use std::collections::HashMap;

    fn method() -> MethodDef {
        builtin::all().remove(0)
    }

    #[test]
    fn builtin_method_validates_against_builtin_content() {
        let m = method();
        let errs = m.validate(&routine::builtin::all(), &benchmark::builtin::all());
        assert!(errs.is_empty(), "{errs:?}");
        assert!(
            m.credit.contains("VDIM"),
            "the method must credit its source"
        );
    }

    #[test]
    fn week_follows_the_vdim_skill_split() {
        let m = method();
        let routine = |d| m.day(d).routine.clone().unwrap_or_default();
        assert_eq!(routine(Weekday::Monday), "skill-clicking-static");
        assert_eq!(routine(Weekday::Tuesday), "skill-clicking-dynamic");
        assert_eq!(routine(Weekday::Wednesday), "skill-tracking-precise");
        assert_eq!(routine(Weekday::Thursday), "skill-tracking-reactive");
        assert_eq!(routine(Weekday::Friday), "skill-switching-speed");
        assert_eq!(routine(Weekday::Saturday), "skill-switching-evasive");
        assert!(m.day(Weekday::Sunday).flex);
    }

    #[test]
    fn day_routines_build_up_to_the_benchmark() {
        let lib = crate::scenario::builtin::all();
        for r in routine::builtin::all()
            .into_iter()
            .filter(|r| r.id.starts_with("skill-"))
        {
            let plan = r.plan(&lib, 0);
            // Every benchmark run (offset 0) is preceded by its hard variant (offset 0.5).
            for (i, run) in plan.iter().enumerate() {
                if run.difficulty == 0.0 && (i == 0 || plan[i - 1].difficulty != 0.0) {
                    assert_eq!(plan[i - 1].difficulty, 0.5, "{}: run {i}", r.id);
                    assert_eq!(
                        plan[i - 1].scenario_id,
                        run.scenario_id,
                        "{}: run {i}",
                        r.id
                    );
                }
            }
            let minutes = r.minutes(&lib);
            assert!((15.0..=45.0).contains(&minutes), "{}: {minutes} min", r.id);
        }
    }

    #[test]
    fn tier_suggestion_follows_benchmark_energy() {
        let m = method();
        assert_eq!(m.suggested_tier(None).id, "novice");
        assert_eq!(m.suggested_tier(Some(150.0)).id, "novice");
        assert_eq!(m.suggested_tier(Some(250.0)).id, "intermediate");
        assert_eq!(m.suggested_tier(Some(800.0)).id, "advanced");
    }

    #[test]
    fn weakest_subcategory_points_to_its_day() {
        let m = method();
        let b = benchmark::builtin::all().remove(0);
        let mut bests = HashMap::new();
        for sub in &b.subcategories {
            bests.insert(sub.scenarios[0].id.clone(), sub.scenarios[0].thresholds[2]);
        }
        let reactive = b
            .subcategories
            .iter()
            .find(|s| s.name == "Reactive tracking")
            .unwrap();
        bests.insert(
            reactive.scenarios[0].id.clone(),
            reactive.scenarios[0].thresholds[0] / 2.0,
        );
        assert_eq!(m.weakest_day(&b.evaluate(&bests)), Some(Weekday::Thursday));
    }

    #[test]
    fn validation_reports_bad_days_and_tiers() {
        let mut m = method();
        m.week.monday = DayPlan::default();
        m.week.tuesday.flex = true;
        m.week.friday.routine = Some("nope".into());
        m.tiers[0].level = 3.0;
        let errs = m.validate(&routine::builtin::all(), &benchmark::builtin::all());
        assert_eq!(errs.len(), 4, "{errs:?}");
    }

    #[test]
    fn weekday_index_is_iso() {
        assert_eq!(Weekday::from_monday_index(0), Weekday::Monday);
        assert_eq!(Weekday::from_monday_index(6), Weekday::Sunday);
        assert_eq!(Weekday::from_monday_index(7), Weekday::Monday);
    }
}
