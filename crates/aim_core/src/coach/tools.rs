//! The tools a coaching model may call, with their JSON schemas and pure handlers.
//!
//! The same registry serves the in-app coach ([`super::run_turn`]) and the MCP server.
//! Handlers never touch files or the network: they read the [`ToolEnv`] the caller built
//! and write only through the [`CoachMemory`] port.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::context::CoachContext;
use super::drills::{self, ID_PREFIX};
use super::knowledge::{Diagnosis, SubSkill, diagnose};
use super::{CoachMemory, MemoryNote, ToolSpec};
use crate::scenario::ScenarioDef;
use crate::training::benchmark::{BenchmarkDef, BenchmarkResult, SubcategoryResult};
use crate::training::playlist;
use crate::training::routine::RoutineDef;

/// Tool: the player's stats as a [`CoachContext`].
pub const LIST_STATS: &str = "list_stats";
/// Tool: built-in routines and scenarios for the weakest sub-skills.
pub const RECOMMEND_ROUTINE: &str = "recommend_routine";
/// Tool: check scenario TOML.
pub const VALIDATE_SCENARIO: &str = "validate_scenario";
/// Tool: create a validated, playable scenario from TOML or from a sub-skill and level.
pub const CREATE_SCENARIO: &str = "create_scenario";
/// Tool: keep a note about the player.
pub const REMEMBER: &str = "remember";
/// Tool: find notes about the player.
pub const RECALL: &str = "recall";

/// Longest scenario TOML a tool accepts, in bytes.
pub const MAX_TOML_BYTES: usize = 16 * 1024;
/// Longest run a created scenario may have, seconds (the playability check simulates it).
pub const MAX_CREATED_SECS: f32 = 300.0;
/// Longest note `remember` keeps, in characters.
pub const MAX_NOTE_CHARS: usize = 500;
/// Most tags per note.
pub const MAX_TAGS: usize = 5;
/// Most notes `recall` returns.
pub const MAX_RECALL: usize = 20;
/// Weak sub-skills `list_stats` and `recommend_routine` report.
pub const TOP_SKILLS: usize = 3;
/// Default length of the personalized playlist, minutes.
pub const DEFAULT_PLAYLIST_MINUTES: f32 = 30.0;

const MAX_SCENARIO_PICKS: usize = 6;
const MAX_ROUTINE_PICKS: usize = 3;
const FALLBACK_ROUTINES: [&str; 2] = ["fundamentals-hour", "quick-warmup"];

/// What the tools can read: the caller builds it from its own state.
#[derive(Debug, Clone, Copy)]
pub struct ToolEnv<'a> {
    /// The player's summary.
    pub context: &'a CoachContext,
    /// Every scenario the player can play (built-in and custom).
    pub scenarios: &'a [ScenarioDef],
    /// Every routine the player can play.
    pub routines: &'a [RoutineDef],
    /// The benchmark behind `context.benchmark`, for the personalized playlist.
    pub benchmark: Option<&'a BenchmarkDef>,
    /// Unix seconds now, stamped on remembered notes.
    pub now: u64,
}

/// The result of one tool call.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutcome {
    /// The JSON returned to the model.
    pub content: Value,
    /// Whether the call failed; the model may fix its arguments and retry.
    pub is_error: bool,
    /// The scenario `create_scenario` made, for the caller to show and save.
    pub scenario: Option<ScenarioDef>,
}

impl ToolOutcome {
    fn ok(content: Value) -> Self {
        Self {
            content,
            is_error: false,
            scenario: None,
        }
    }

    fn errors(errors: Vec<String>) -> Self {
        Self {
            content: json!({ "ok": false, "errors": errors }),
            is_error: true,
            scenario: None,
        }
    }
}

/// Every tool, with a strict JSON schema for its arguments.
pub fn specs() -> Vec<ToolSpec> {
    let object = |properties: Value, required: &[&str]| {
        json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        })
    };
    let skills: Vec<&str> = SubSkill::ALL.iter().map(|s| s.id()).collect();
    let spec = |name: &str, description: &str, parameters: Value| ToolSpec {
        name: name.to_owned(),
        description: description.to_owned(),
        parameters,
    };
    vec![
        spec(
            LIST_STATS,
            "The player's stats: findings with value and threshold, per-category flick and \
             tracking aggregates, flick directions, miss compass, pace, potential score, \
             benchmark energy, recent runs, missions, readiness, fatigue, and the weakest \
             sub-skills with their evidence.",
            object(json!({}), &[]),
        ),
        spec(
            RECOMMEND_ROUTINE,
            "Rank built-in routines and scenarios for the player's weakest sub-skills, with \
             the reason for each, plus a personalized benchmark playlist when available.",
            object(
                json!({ "minutes": {
                    "type": "number", "minimum": 10, "maximum": 90,
                    "description": "Playlist length in minutes (default 30)."
                }}),
                &[],
            ),
        ),
        spec(
            VALIDATE_SCENARIO,
            "Check scenario TOML (the format of docs/scenarios.md) and return every error.",
            object(
                json!({ "toml": { "type": "string", "description": "Scenario TOML." } }),
                &["toml"],
            ),
        ),
        spec(
            CREATE_SCENARIO,
            "Create a scenario from TOML, or from a sub-skill and a level (-1 easiest to 1 \
             hardest). It is validated and must be playable by a simulated average player; \
             on errors, fix them and retry. Created scenarios are uncalibrated and wait for \
             the player's approval.",
            object(
                json!({
                    "toml": { "type": "string", "description": "Scenario TOML." },
                    "skill": { "type": "string", "enum": skills },
                    "level": { "type": "number", "minimum": -1, "maximum": 1 }
                }),
                &[],
            ),
        ),
        spec(
            REMEMBER,
            "Keep a short note about the player (goals, habits, preferences) for later \
             sessions. Never store passwords, keys or personal identifiers.",
            object(
                json!({
                    "text": { "type": "string", "maxLength": MAX_NOTE_CHARS },
                    "tags": { "type": "array", "items": { "type": "string" },
                              "maxItems": MAX_TAGS }
                }),
                &["text"],
            ),
        ),
        spec(
            RECALL,
            "Find notes about the player by keywords; an empty query returns the newest.",
            object(
                json!({
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_RECALL }
                }),
                &["query"],
            ),
        ),
    ]
}

/// Run tool `name` with `args`. Unknown tools and bad arguments are error outcomes, never
/// panics, so the model can recover.
pub fn call(name: &str, args: &Value, env: &ToolEnv, memory: &mut dyn CoachMemory) -> ToolOutcome {
    match name {
        LIST_STATS => ToolOutcome::ok(list_stats(env)),
        RECOMMEND_ROUTINE => match parse::<RecommendArgs>(args) {
            Ok(a) => ToolOutcome::ok(json!(recommend_routine(env, a.minutes))),
            Err(e) => ToolOutcome::errors(vec![e]),
        },
        VALIDATE_SCENARIO => match parse::<TomlArgs>(args) {
            Ok(a) => match validate_scenario(&a.toml) {
                Ok(def) => ToolOutcome::ok(json!({ "ok": true, "scenario": brief(&def) })),
                Err(errors) => ToolOutcome::errors(errors),
            },
            Err(e) => ToolOutcome::errors(vec![e]),
        },
        CREATE_SCENARIO => match parse::<CreateArgs>(args).and_then(CreateRequest::try_from) {
            Ok(request) => match create_scenario(&request, env) {
                Ok(def) => created(def),
                Err(errors) => ToolOutcome::errors(errors),
            },
            Err(e) => ToolOutcome::errors(vec![e]),
        },
        REMEMBER => match parse::<RememberArgs>(args) {
            Ok(a) => match remember(a.text, a.tags, env.now, memory) {
                Ok(()) => ToolOutcome::ok(json!({ "ok": true })),
                Err(e) => ToolOutcome::errors(vec![e]),
            },
            Err(e) => ToolOutcome::errors(vec![e]),
        },
        RECALL => match parse::<RecallArgs>(args) {
            Ok(a) => {
                let notes = recall(&a.query, a.limit.unwrap_or(5), memory);
                ToolOutcome::ok(json!({ "ok": true, "notes": notes }))
            }
            Err(e) => ToolOutcome::errors(vec![e]),
        },
        other => {
            let names: Vec<String> = specs().into_iter().map(|s| s.name).collect();
            ToolOutcome::errors(vec![format!(
                "unknown tool {other:?}; available: {}",
                names.join(", ")
            )])
        }
    }
}

/// `list_stats`: the bounded context plus the weakest sub-skills.
pub fn list_stats(env: &ToolEnv) -> Value {
    let context: Value = serde_json::from_str(&env.context.to_prompt_json())
        .expect("to_prompt_json returns valid JSON");
    let mut weak = diagnose(env.context);
    weak.truncate(TOP_SKILLS);
    json!({ "ok": true, "context": context, "weak_skills": weak })
}

/// A ranked routine or scenario with the reason it was picked.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RankedItem {
    /// Routine or scenario id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Why it was picked, with the evidence.
    pub reason: String,
}

/// `recommend_routine`'s answer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoutineRecommendation {
    /// The weakest sub-skills, weakest first.
    pub weak_skills: Vec<Diagnosis>,
    /// Routines, best first.
    pub routines: Vec<RankedItem>,
    /// Scenarios, best first.
    pub scenarios: Vec<RankedItem>,
    /// The personalized benchmark playlist, when a benchmark and its energy are known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist: Option<RoutineDef>,
}

/// `recommend_routine`: for each of the weakest sub-skills, the scenarios and routines its
/// weakness rules recommend, then the knowledge base's own picks, keeping only what the
/// player has; plus [`playlist::personalized`] over the benchmark energy.
pub fn recommend_routine(env: &ToolEnv, minutes: Option<f32>) -> RoutineRecommendation {
    let mut weak = diagnose(env.context);
    weak.truncate(TOP_SKILLS);
    let mut routines: Vec<RankedItem> = Vec::new();
    let mut scenarios: Vec<RankedItem> = Vec::new();
    for d in &weak {
        let entry = d.skill.entry();
        let reason = match d.evidence.first() {
            Some(e) => format!("{}: {e}", entry.name),
            None => entry.name.to_owned(),
        };
        let fired = env.context.findings.iter().filter(|f| {
            entry
                .signals
                .iter()
                .any(|s| s.metric == f.metric && s.direction == f.direction)
        });
        let mut scenario_ids: Vec<&str> = Vec::new();
        let mut routine_ids: Vec<&str> = Vec::new();
        for f in fired {
            scenario_ids.extend(f.scenarios.iter().map(String::as_str));
            routine_ids.extend(f.routine.as_deref());
        }
        scenario_ids.extend(entry.scenarios);
        routine_ids.extend(entry.routines);
        for id in scenario_ids {
            let name = env.scenarios.iter().find(|s| s.id == id).map(|s| &s.name);
            push_unique(&mut scenarios, id, name, &reason, MAX_SCENARIO_PICKS);
        }
        for id in routine_ids {
            let name = env.routines.iter().find(|r| r.id == id).map(|r| &r.name);
            push_unique(&mut routines, id, name, &reason, MAX_ROUTINE_PICKS);
        }
    }
    if weak.is_empty() {
        let reason = "No weakness found yet; play these to build a profile.";
        for id in FALLBACK_ROUTINES {
            let name = env.routines.iter().find(|r| r.id == id).map(|r| &r.name);
            push_unique(&mut routines, id, name, reason, MAX_ROUTINE_PICKS);
        }
    }
    let minutes = minutes
        .unwrap_or(DEFAULT_PLAYLIST_MINUTES)
        .clamp(10.0, 90.0);
    let playlist = env
        .benchmark
        .zip(env.context.benchmark.as_ref())
        .map(|(def, energy)| {
            let result = BenchmarkResult {
                subcategories: energy
                    .subcategories
                    .iter()
                    .map(|s| SubcategoryResult {
                        name: s.name.clone(),
                        skill: s.skill.clone(),
                        energy: s.energy,
                    })
                    .collect(),
                overall: None,
            };
            playlist::personalized(def, &result, env.scenarios, minutes)
        });
    RoutineRecommendation {
        weak_skills: weak,
        routines,
        scenarios,
        playlist,
    }
}

/// Add `id` unless it is already listed, missing from the player's library (`name` is
/// `None`), or the list is full.
fn push_unique(
    list: &mut Vec<RankedItem>,
    id: &str,
    name: Option<&String>,
    reason: &str,
    max: usize,
) {
    let Some(name) = name else { return };
    if list.len() < max && !list.iter().any(|i| i.id == id) {
        list.push(RankedItem {
            id: id.to_owned(),
            name: name.clone(),
            reason: reason.to_owned(),
        });
    }
}

/// `validate_scenario`: parse and validate `toml`, returning every problem.
pub fn validate_scenario(toml: &str) -> Result<ScenarioDef, Vec<String>> {
    if toml.len() > MAX_TOML_BYTES {
        return Err(vec![format!(
            "the TOML is {} bytes; the limit is {MAX_TOML_BYTES}",
            toml.len()
        )]);
    }
    drills::parse_and_validate(toml)
}

/// What `create_scenario` builds from.
#[derive(Debug, Clone, PartialEq)]
pub enum CreateRequest {
    /// Scenario TOML written by the model.
    Toml(String),
    /// A knowledge-base drill for a sub-skill at a level in [−1, 1].
    Skill {
        /// The sub-skill.
        skill: SubSkill,
        /// The level.
        level: f32,
    },
}

/// `create_scenario`: a valid scenario whose id is free and whose run the simulated
/// contender can score in, or every reason it is not.
pub fn create_scenario(request: &CreateRequest, env: &ToolEnv) -> Result<ScenarioDef, Vec<String>> {
    let def = match request {
        CreateRequest::Toml(toml) => {
            let def = validate_scenario(toml)?;
            let mut errors = Vec::new();
            if env.scenarios.iter().any(|s| s.id == def.id) {
                errors.push(format!(
                    "id: {:?} is taken by an existing scenario; choose another (generated \
                     drills use the {ID_PREFIX:?} prefix)",
                    def.id
                ));
            }
            if def.duration_secs > MAX_CREATED_SECS {
                errors.push(format!(
                    "duration_secs: at most {MAX_CREATED_SECS} s for a created scenario"
                ));
            }
            if !errors.is_empty() {
                return Err(errors);
            }
            def
        }
        CreateRequest::Skill { skill, level } => {
            drills::generate(*skill, *level, next_drill_number(*skill, env.scenarios))?
        }
    };
    drills::check_playable(&def).map_err(|e| vec![e])?;
    Ok(def)
}

/// One more than the highest `n` of the player's `coach-<skill>-<n>` scenarios.
fn next_drill_number(skill: SubSkill, scenarios: &[ScenarioDef]) -> u32 {
    let prefix = format!("{ID_PREFIX}{}-", skill.id());
    scenarios
        .iter()
        .filter_map(|s| s.id.strip_prefix(&prefix)?.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
        + 1
}

/// `remember`: keep a trimmed note, stamped `now`.
pub fn remember(
    text: String,
    tags: Vec<String>,
    now: u64,
    memory: &mut dyn CoachMemory,
) -> Result<(), String> {
    let text = text.trim().to_owned();
    if text.is_empty() {
        return Err("text: the note is empty".into());
    }
    if text.chars().count() > MAX_NOTE_CHARS {
        return Err(format!("text: at most {MAX_NOTE_CHARS} characters"));
    }
    if tags.len() > MAX_TAGS {
        return Err(format!("tags: at most {MAX_TAGS}"));
    }
    memory
        .retain(MemoryNote {
            text,
            tags,
            timestamp: now,
        })
        .map_err(|e| e.to_string())
}

/// `recall`: up to `limit` (1–[`MAX_RECALL`]) notes for `query`.
pub fn recall(query: &str, limit: usize, memory: &dyn CoachMemory) -> Vec<MemoryNote> {
    memory.recall(query, limit.clamp(1, MAX_RECALL))
}

fn created(def: ScenarioDef) -> ToolOutcome {
    let toml = toml::to_string(&def).expect("a scenario definition serializes to TOML");
    ToolOutcome {
        content: json!({
            "ok": true,
            "uncalibrated": true,
            "scenario": brief(&def),
            "toml": toml,
        }),
        is_error: false,
        scenario: Some(def),
    }
}

fn brief(def: &ScenarioDef) -> Value {
    json!({
        "id": def.id,
        "name": def.name,
        "category": def.category,
        "duration_secs": def.duration_secs,
    })
}

fn parse<T: for<'de> Deserialize<'de>>(args: &Value) -> Result<T, String> {
    // Some providers send no arguments for a tool without parameters.
    let args = if args.is_null() { &json!({}) } else { args };
    serde_json::from_value(args.clone()).map_err(|e| format!("arguments: {e}"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecommendArgs {
    #[serde(default)]
    minutes: Option<f32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlArgs {
    toml: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateArgs {
    #[serde(default)]
    toml: Option<String>,
    #[serde(default)]
    skill: Option<String>,
    #[serde(default)]
    level: Option<f32>,
}

impl TryFrom<CreateArgs> for CreateRequest {
    type Error = String;

    fn try_from(a: CreateArgs) -> Result<Self, String> {
        match (a.toml, a.skill) {
            (Some(toml), None) => Ok(CreateRequest::Toml(toml)),
            (None, Some(id)) => {
                let skill = SubSkill::from_id(&id)
                    .ok_or_else(|| format!("skill: unknown sub-skill {id:?}"))?;
                Ok(CreateRequest::Skill {
                    skill,
                    level: a.level.unwrap_or(0.0),
                })
            }
            _ => Err("arguments: give either toml or skill (with an optional level)".into()),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RememberArgs {
    text: String,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecallArgs {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::rules::{Direction, Metric, Verdict};
    use crate::coach::InMemoryCoachMemory;
    use crate::coach::context::{BenchmarkSummary, FindingSummary, SubcategorySummary};
    use crate::scenario::{Category, builtin};
    use crate::training::{benchmark, routine};

    struct Library {
        scenarios: Vec<ScenarioDef>,
        routines: Vec<RoutineDef>,
        benchmark: BenchmarkDef,
    }

    fn library() -> Library {
        Library {
            scenarios: builtin::all(),
            routines: routine::builtin::all(),
            benchmark: benchmark::builtin::all().remove(0),
        }
    }

    fn env<'a>(ctx: &'a CoachContext, lib: &'a Library) -> ToolEnv<'a> {
        ToolEnv {
            context: ctx,
            scenarios: &lib.scenarios,
            routines: &lib.routines,
            benchmark: Some(&lib.benchmark),
            now: 42,
        }
    }

    fn overshooting() -> CoachContext {
        CoachContext {
            findings: vec![FindingSummary {
                category: Category::Flicking,
                verdict: Verdict::Weakness,
                metric: Metric::OvershootRate,
                value: 0.5,
                threshold: 0.35,
                direction: Direction::Above,
                severity: 0.43,
                explanation: "Overshoot rate 50% is above the 35% threshold.".into(),
                scenarios: vec!["static-headline".into(), "micro-adjust".into()],
                routine: None,
            }],
            ..Default::default()
        }
    }

    fn run(name: &str, args: Value, env: &ToolEnv, mem: &mut InMemoryCoachMemory) -> ToolOutcome {
        call(name, &args, env, mem)
    }

    #[test]
    fn specs_name_every_tool_with_a_strict_object_schema() {
        let specs = specs();
        let names: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            [
                LIST_STATS,
                RECOMMEND_ROUTINE,
                VALIDATE_SCENARIO,
                CREATE_SCENARIO,
                REMEMBER,
                RECALL
            ]
        );
        for s in &specs {
            assert_eq!(s.parameters["type"], "object", "{}", s.name);
            assert_eq!(s.parameters["additionalProperties"], false, "{}", s.name);
        }
    }

    #[test]
    fn list_stats_returns_the_context_and_the_weakest_sub_skills() {
        let ctx = overshooting();
        let lib = library();
        let out = run(
            LIST_STATS,
            json!({}),
            &env(&ctx, &lib),
            &mut Default::default(),
        );
        assert!(!out.is_error);
        assert_eq!(
            out.content["context"]["findings"][0]["metric"],
            "overshoot_rate"
        );
        assert_eq!(out.content["weak_skills"][0]["skill"], "micro-adjustment");
    }

    #[test]
    fn recommendations_follow_the_rules_then_the_knowledge_base_and_add_a_playlist() {
        let ctx = CoachContext {
            benchmark: Some(BenchmarkSummary {
                average: Some(400.0),
                subcategories: vec![SubcategorySummary {
                    name: lib_sub_name(),
                    skill: "clicking".into(),
                    energy: Some(200.0),
                    verdict: Some(Verdict::Weakness),
                }],
            }),
            ..overshooting()
        };
        let lib = library();
        let rec = recommend_routine(&env(&ctx, &lib), Some(20.0));
        assert_eq!(rec.weak_skills[0].skill, SubSkill::MicroAdjustment);
        let ids: Vec<&str> = rec.scenarios.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            &ids[..2],
            ["static-headline", "micro-adjust"],
            "rule order first"
        );
        assert!(rec.scenarios[0].reason.contains("Overshoot rate 50%"));
        assert_eq!(rec.routines[0].id, "skill-clicking-static");
        let playlist = rec.playlist.expect("benchmark energy gives a playlist");
        assert_eq!(playlist.id, playlist::PERSONALIZED_ID);
    }

    fn lib_sub_name() -> String {
        benchmark::builtin::all()[0].subcategories[0].name.clone()
    }

    #[test]
    fn without_weaknesses_the_fundamentals_are_recommended() {
        let ctx = CoachContext::default();
        let lib = library();
        let rec = recommend_routine(&env(&ctx, &lib), None);
        assert!(rec.weak_skills.is_empty() && rec.scenarios.is_empty());
        let ids: Vec<&str> = rec.routines.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, FALLBACK_ROUTINES);
        assert!(rec.playlist.is_none(), "no benchmark energy, no playlist");
    }

    #[test]
    fn validate_scenario_accepts_valid_toml_and_lists_every_error() {
        let ctx = CoachContext::default();
        let lib = library();
        let e = env(&ctx, &lib);
        let mut mem = InMemoryCoachMemory::default();
        let (_, src) = builtin::SOURCES[0];
        let ok = run(VALIDATE_SCENARIO, json!({ "toml": src }), &e, &mut mem);
        assert!(!ok.is_error, "{}", ok.content);
        assert_eq!(ok.content["ok"], true);

        let broken = run(VALIDATE_SCENARIO, json!({ "toml": "id = [" }), &e, &mut mem);
        assert!(broken.is_error);
        assert_eq!(broken.content["errors"].as_array().unwrap().len(), 1);

        let invalid = drills::drill_toml(SubSkill::FlickAccuracy, 0.0, 1)
            .replace("duration_secs = 60.0", "duration_secs = 0.0");
        let out = run(VALIDATE_SCENARIO, json!({ "toml": invalid }), &e, &mut mem);
        assert!(
            out.content["errors"][0]
                .as_str()
                .unwrap()
                .starts_with("duration_secs")
        );

        let huge = "#".repeat(MAX_TOML_BYTES + 1);
        assert!(validate_scenario(&huge).unwrap_err()[0].contains("limit"));
    }

    #[test]
    fn create_scenario_from_a_skill_numbers_drills_after_existing_ones() {
        let ctx = CoachContext::default();
        let mut lib = library();
        lib.scenarios
            .push(drills::generate(SubSkill::MicroAdjustment, 0.0, 4).unwrap());
        let out = run(
            CREATE_SCENARIO,
            json!({ "skill": "micro-adjustment", "level": 0.5 }),
            &env(&ctx, &lib),
            &mut Default::default(),
        );
        assert!(!out.is_error, "{}", out.content);
        assert_eq!(out.scenario.unwrap().id, "coach-micro-adjustment-5");
        assert_eq!(out.content["uncalibrated"], true);
        assert!(
            out.content["toml"]
                .as_str()
                .unwrap()
                .contains("coach-micro-adjustment-5")
        );
    }

    #[test]
    fn create_scenario_refuses_taken_ids_long_runs_and_unplayable_drills() {
        let ctx = CoachContext::default();
        let lib = library();
        let e = env(&ctx, &lib);
        let (_, taken) = builtin::SOURCES[0];
        let errors = create_scenario(&CreateRequest::Toml(taken.into()), &e).unwrap_err();
        assert!(errors[0].contains("is taken"), "{errors:?}");

        let base = drills::drill_toml(SubSkill::FlickAccuracy, 0.0, 9);
        let long = base.replace("duration_secs = 60.0", "duration_secs = 600.0");
        let errors = create_scenario(&CreateRequest::Toml(long), &e).unwrap_err();
        assert!(errors[0].starts_with("duration_secs"), "{errors:?}");

        let tiny = base.replace("[scoring]", "lifetime_secs = 0.05\n\n[scoring]");
        let errors = create_scenario(&CreateRequest::Toml(tiny), &e).unwrap_err();
        assert!(errors[0].contains("not playable"), "{errors:?}");

        let both = run(
            CREATE_SCENARIO,
            json!({ "toml": "x", "skill": "micro-adjustment" }),
            &e,
            &mut Default::default(),
        );
        assert!(both.is_error && both.scenario.is_none());
    }

    #[test]
    fn remember_and_recall_go_through_the_memory_port() {
        let ctx = CoachContext::default();
        let lib = library();
        let e = env(&ctx, &lib);
        let mut mem = InMemoryCoachMemory::default();
        let kept = run(
            REMEMBER,
            json!({ "text": "  Wants to fix overshoot  ", "tags": ["goal"] }),
            &e,
            &mut mem,
        );
        assert!(!kept.is_error);
        assert_eq!(mem.notes[0].text, "Wants to fix overshoot");
        assert_eq!(mem.notes[0].timestamp, 42);

        let found = run(RECALL, json!({ "query": "goal" }), &e, &mut mem);
        assert_eq!(found.content["notes"][0]["text"], "Wants to fix overshoot");

        let empty = run(REMEMBER, json!({ "text": "   " }), &e, &mut mem);
        assert!(empty.is_error);
        let long = run(
            REMEMBER,
            json!({ "text": "x".repeat(MAX_NOTE_CHARS + 1) }),
            &e,
            &mut mem,
        );
        assert!(long.is_error);
        assert_eq!(mem.notes.len(), 1, "rejected notes are not kept");
    }

    #[test]
    fn bad_arguments_and_unknown_tools_are_errors() {
        let ctx = CoachContext::default();
        let lib = library();
        let e = env(&ctx, &lib);
        let mut mem = InMemoryCoachMemory::default();
        assert!(run(RECALL, json!({ "q": "x" }), &e, &mut mem).is_error);
        assert!(run(VALIDATE_SCENARIO, json!({}), &e, &mut mem).is_error);
        assert!(run("rm", json!({}), &e, &mut mem).is_error);
        assert!(!run(LIST_STATS, Value::Null, &e, &mut mem).is_error);
    }
}
