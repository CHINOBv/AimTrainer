//! The server's handlers, called directly (no transport): each MCP tool must behave exactly
//! like the core registry, plus the opt-in `dry_run = false` save of `create_scenario`.

use std::fs;
use std::path::PathBuf;

use aim_core::coach::tools;
use aim_core::scenario::{ScenarioDef, builtin};
use aim_mcp::{CoachServer, Paths};
use rmcp::model::{CallToolResult, JsonObject, ResourceContents};
use serde_json::{Value, json};

/// A scratch config/data folder outside the repository, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("aimtrainer-mcp-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn paths(&self) -> Paths {
        Paths::in_dirs(&self.0.join("config"), &self.0.join("data"))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn args(value: Value) -> Option<JsonObject> {
    match value {
        Value::Object(map) => Some(map),
        other => panic!("arguments must be an object: {other}"),
    }
}

fn structured(result: &CallToolResult) -> &Value {
    result
        .structured_content
        .as_ref()
        .expect("every tool returns structured content")
}

fn is_error(result: &CallToolResult) -> bool {
    result.is_error == Some(true)
}

#[test]
fn the_tools_mirror_the_core_registry_with_a_dry_run_flag_on_create_scenario() {
    let listed = CoachServer::tools();
    let core = tools::specs();
    let names: Vec<&str> = listed.iter().map(|t| t.name.as_ref()).collect();
    let core_names: Vec<&str> = core.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, core_names);
    assert_eq!(
        names,
        [
            "list_stats",
            "recommend_routine",
            "validate_scenario",
            "create_scenario",
            "remember",
            "recall"
        ]
    );
    for (tool, spec) in listed.iter().zip(&core) {
        let description = tool.description.as_deref().unwrap_or_default();
        assert!(description.starts_with(&spec.description), "{description}");
        let schema = Value::Object((*tool.input_schema).clone());
        if spec.name == tools::CREATE_SCENARIO {
            let dry_run = &schema["properties"]["dry_run"];
            assert_eq!(dry_run["type"], "boolean");
            assert_eq!(dry_run["default"], true);
            let mut without = schema.clone();
            without["properties"]
                .as_object_mut()
                .unwrap()
                .remove("dry_run");
            assert_eq!(without, spec.parameters, "only dry_run is added");
        } else {
            assert_eq!(description, spec.description);
            assert_eq!(schema, spec.parameters);
        }
    }
}

#[test]
fn list_stats_summarizes_the_history_file() {
    let scratch = Scratch::new("stats");
    let paths = scratch.paths();
    fs::create_dir_all(paths.history_file.parent().unwrap()).unwrap();
    fs::write(
        &paths.history_file,
        "timestamp,scenario,score,accuracy,kills,kills_per_min,avg_time_to_kill,headshot_rate,moving_shots,duration_secs\n\
         1000,micro-adjust,100.0,0.5,1,1.0,,0.0,0,60.0\n\
         2000,micro-adjust,120.0,0.6,2,2.0,0.5,0.1,0,60.0\n\
         not,a,row\n",
    )
    .unwrap();
    let server = CoachServer::new(paths);
    let out = server.call(tools::LIST_STATS, None);
    assert!(!is_error(&out), "{out:?}");
    let body = structured(&out);
    assert_eq!(body["ok"], true);
    let recent = body["context"]["recent"].as_array().unwrap();
    assert_eq!(recent.len(), 1, "{body}");
}

#[test]
fn recommend_routine_answers_from_the_built_in_library() {
    let scratch = Scratch::new("recommend");
    let server = CoachServer::new(scratch.paths());
    let out = server.call(tools::RECOMMEND_ROUTINE, args(json!({ "minutes": 20 })));
    assert!(!is_error(&out), "{out:?}");
    assert!(!structured(&out)["routines"].as_array().unwrap().is_empty());

    let bad = server.call(tools::RECOMMEND_ROUTINE, args(json!({ "hours": 1 })));
    assert!(
        is_error(&bad),
        "unknown arguments are refused as in the core"
    );
}

#[test]
fn validate_scenario_accepts_valid_toml_and_reports_invalid_toml() {
    let scratch = Scratch::new("validate");
    let server = CoachServer::new(scratch.paths());
    let (_, src) = builtin::SOURCES[0];
    let ok = server.call(tools::VALIDATE_SCENARIO, args(json!({ "toml": src })));
    assert!(!is_error(&ok), "{ok:?}");
    assert_eq!(structured(&ok)["ok"], true);

    let broken = server.call(tools::VALIDATE_SCENARIO, args(json!({ "toml": "id = [" })));
    assert!(is_error(&broken));
    assert_eq!(structured(&broken)["ok"], false);
    assert_eq!(structured(&broken)["errors"].as_array().unwrap().len(), 1);
}

#[test]
fn create_scenario_is_a_dry_run_by_default_and_writes_nothing() {
    let scratch = Scratch::new("dry");
    let paths = scratch.paths();
    let server = CoachServer::new(paths.clone());
    let out = server.call(
        tools::CREATE_SCENARIO,
        args(json!({ "skill": "micro-adjustment", "level": 0.0 })),
    );
    assert!(!is_error(&out), "{out:?}");
    let body = structured(&out);
    assert_eq!(body["ok"], true);
    assert_eq!(body["dry_run"], true);
    assert!(body["saved"].is_null(), "{body}");
    assert!(
        body["toml"]
            .as_str()
            .unwrap()
            .contains("coach-micro-adjustment-1")
    );
    assert!(!paths.scenarios_dir.exists(), "a dry run creates no folder");

    let explicit = server.call(
        tools::CREATE_SCENARIO,
        args(json!({ "skill": "micro-adjustment", "level": 0.0, "dry_run": true })),
    );
    assert_eq!(structured(&explicit)["dry_run"], true);
    assert!(!paths.scenarios_dir.exists());
}

#[test]
fn create_scenario_without_dry_run_saves_the_validated_toml_once() {
    let scratch = Scratch::new("save");
    let paths = scratch.paths();
    let server = CoachServer::new(paths.clone());
    let out = server.call(
        tools::CREATE_SCENARIO,
        args(json!({ "skill": "micro-adjustment", "level": 0.0, "dry_run": false })),
    );
    assert!(!is_error(&out), "{out:?}");
    let body = structured(&out);
    assert_eq!(body["dry_run"], false);
    let file = paths.scenarios_dir.join("coach-micro-adjustment-1.toml");
    assert_eq!(body["saved"], file.display().to_string());
    let saved = fs::read_to_string(&file).unwrap();
    assert_eq!(saved, body["toml"].as_str().unwrap());
    let def = ScenarioDef::from_toml(&saved).unwrap();
    assert_eq!(def.id, "coach-micro-adjustment-1");
    assert!(def.validate().is_empty());

    // The saved drill is now in the library, so the next one is numbered after it.
    let next = server.call(
        tools::CREATE_SCENARIO,
        args(json!({ "skill": "micro-adjustment", "level": 0.0, "dry_run": false })),
    );
    assert!(!is_error(&next), "{next:?}");
    assert!(
        paths
            .scenarios_dir
            .join("coach-micro-adjustment-2.toml")
            .exists()
    );

    // TOML reusing a saved id is refused, and the file is left as it was.
    let taken = server.call(
        tools::CREATE_SCENARIO,
        args(json!({ "toml": saved, "dry_run": false })),
    );
    assert!(is_error(&taken), "{taken:?}");
    assert_eq!(fs::read_to_string(&file).unwrap(), saved);
}

#[test]
fn create_scenario_with_invalid_toml_returns_errors_and_writes_nothing() {
    let scratch = Scratch::new("invalid");
    let paths = scratch.paths();
    let server = CoachServer::new(paths.clone());
    let out = server.call(
        tools::CREATE_SCENARIO,
        args(json!({ "toml": "id = [", "dry_run": false })),
    );
    assert!(is_error(&out));
    assert_eq!(structured(&out)["ok"], false);
    assert!(!paths.scenarios_dir.exists());

    let bad_flag = server.call(
        tools::CREATE_SCENARIO,
        args(json!({ "skill": "micro-adjustment", "dry_run": "no" })),
    );
    assert!(is_error(&bad_flag));
    let errors = structured(&bad_flag)["errors"].to_string();
    assert!(errors.contains("dry_run"), "{errors}");
    assert!(!paths.scenarios_dir.exists());
}

#[test]
fn remembered_notes_are_recalled_across_calls() {
    let scratch = Scratch::new("memory");
    let server = CoachServer::new(scratch.paths());
    let kept = server.call(
        tools::REMEMBER,
        args(json!({ "text": "Wants to fix tracking on strafes", "tags": ["tracking"] })),
    );
    assert!(!is_error(&kept), "{kept:?}");
    let found = server.call(tools::RECALL, args(json!({ "query": "tracking" })));
    let notes = structured(&found)["notes"].as_array().unwrap().clone();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0]["text"], "Wants to fix tracking on strafes");
}

#[test]
fn unknown_tools_are_error_results() {
    let scratch = Scratch::new("unknown");
    let server = CoachServer::new(scratch.paths());
    let out = server.call("delete_everything", None);
    assert!(is_error(&out));
    assert!(
        structured(&out)["errors"][0]
            .as_str()
            .unwrap()
            .contains("unknown tool")
    );
}

#[test]
fn resources_serve_every_built_in_scenario_and_the_schema_doc() {
    let listed = CoachServer::resources();
    let uris: Vec<&str> = listed.iter().map(|r| r.uri.as_str()).collect();
    assert_eq!(uris.len(), builtin::SOURCES.len() + 1);
    assert!(uris.contains(&"aimtrainer://docs/scenarios.md"));

    let (file, src) = builtin::SOURCES[0];
    let id = file.trim_end_matches(".toml");
    let uri = format!("aimtrainer://scenarios/{id}");
    assert!(uris.contains(&uri.as_str()));
    let read = CoachServer::read(&uri).unwrap();
    let ResourceContents::TextResourceContents { text, .. } = &read.contents[0] else {
        panic!("scenarios are text");
    };
    assert_eq!(text, src);

    let doc = CoachServer::read("aimtrainer://docs/scenarios.md").unwrap();
    let ResourceContents::TextResourceContents { text, .. } = &doc.contents[0] else {
        panic!("the doc is text");
    };
    assert!(text.starts_with("# Writing scenarios and routines"));

    assert!(CoachServer::read("aimtrainer://scenarios/nope").is_err());
    assert!(CoachServer::read("file:///etc/passwd").is_err());
}
