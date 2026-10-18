//! The MCP surface: the core's coach tools, read-only resources, and the rmcp
//! [`ServerHandler`] that routes requests to them.
//!
//! Every tool call is forwarded to [`tools::call`], so there is one registry and the agent
//! sees exactly what the in-app coach sees. The only addition is `create_scenario`'s
//! `dry_run` flag: when it is `false`, the validated scenario is saved to the player's
//! custom scenarios folder.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use aim_core::coach::tools::{self, ToolOutcome};
use aim_core::coach::{InMemoryCoachMemory, ToolEnv};
use aim_core::scenario::{ScenarioDef, builtin};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, JsonObject,
    ListResourcesResult, ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams,
    ReadResourceResponse, ReadResourceResult, Resource, ResourceContents, ServerCapabilities,
    ServerConfig, Tool,
};
use rmcp::service::{MaybeSendFuture, RequestContext};
use rmcp::{ErrorData, RoleServer, ServerHandler};
use serde_json::{Value, json};

use crate::paths::Paths;
use crate::player::{self, Library};

/// The `create_scenario` argument this server adds to the core's schema.
pub const DRY_RUN: &str = "dry_run";
/// URI prefix of the built-in scenario resources (`aimtrainer://scenarios/<id>`).
pub const SCENARIO_URI: &str = "aimtrainer://scenarios/";
/// URI of the scenario format reference (`docs/scenarios.md`).
pub const SCHEMA_DOC_URI: &str = "aimtrainer://docs/scenarios.md";

const SCHEMA_DOC: &str = include_str!("../../../docs/scenarios.md");

/// The coach's MCP server. Content and history are re-read on every call, so the agent
/// always sees what the trainer would load now.
#[derive(Debug)]
pub struct CoachServer {
    paths: Paths,
    // The SQLite store from `aim_ai` replaces this at the `CoachMemory` seam; until then,
    // notes last as long as the server process.
    memory: Mutex<InMemoryCoachMemory>,
}

impl CoachServer {
    /// A server over the player's files at `paths`.
    pub fn new(paths: Paths) -> Self {
        Self {
            paths,
            memory: Mutex::new(InMemoryCoachMemory::default()),
        }
    }

    /// Every tool of [`tools::specs`], in order, with `dry_run` added to `create_scenario`.
    pub fn tools() -> Vec<Tool> {
        tools::specs()
            .into_iter()
            .map(|spec| {
                let mut description = spec.description;
                let mut schema = match spec.parameters {
                    Value::Object(map) => map,
                    other => unreachable!("tool schemas are JSON objects: {other}"),
                };
                if spec.name == tools::CREATE_SCENARIO {
                    description.push_str(
                        " With dry_run false (the default is true), the validated scenario is \
                         also saved to the player's custom scenarios folder.",
                    );
                    if let Some(Value::Object(properties)) = schema.get_mut("properties") {
                        properties.insert(
                            DRY_RUN.to_owned(),
                            json!({
                                "type": "boolean",
                                "default": true,
                                "description": "Only validate and return the TOML (true), or \
                                                also save it as a custom scenario (false).",
                            }),
                        );
                    }
                }
                Tool::new(spec.name, description, schema)
            })
            .collect()
    }

    /// Run tool `name` with `arguments` through the core registry. Failures are error
    /// results the agent can read and retry, never protocol errors.
    pub fn call(&self, name: &str, arguments: Option<JsonObject>) -> CallToolResult {
        let mut args = arguments.unwrap_or_default();
        let dry_run = if name == tools::CREATE_SCENARIO {
            match args.remove(DRY_RUN) {
                None | Some(Value::Null) => true,
                Some(Value::Bool(b)) => b,
                Some(other) => {
                    return errors(vec![format!(
                        "arguments: {DRY_RUN} must be true or false, got {other}"
                    )]);
                }
            }
        } else {
            true
        };

        let lib = Library::load(&self.paths);
        for problem in &lib.errors {
            eprintln!("aimtrainer-mcp: skipped {problem}");
        }
        let now = now_secs();
        let records = player::read_history(&self.paths.history_file);
        let context = player::context(&lib, &records, now);
        let env = ToolEnv {
            context: &context,
            scenarios: &lib.scenarios,
            routines: &lib.routines,
            benchmark: lib.benchmarks.first(),
            now,
        };
        let outcome = {
            let mut memory = self.memory.lock().unwrap_or_else(PoisonError::into_inner);
            tools::call(name, &Value::Object(args), &env, &mut *memory)
        };

        if name != tools::CREATE_SCENARIO || outcome.is_error {
            return result(outcome);
        }
        let mut outcome = outcome;
        let saved = match (&outcome.scenario, dry_run) {
            (Some(def), false) => match self.save(def, &outcome.content) {
                Ok(path) => Value::String(path),
                Err(e) => return errors(vec![e]),
            },
            _ => Value::Null,
        };
        if let Value::Object(body) = &mut outcome.content {
            body.insert(DRY_RUN.to_owned(), Value::Bool(dry_run));
            body.insert("saved".to_owned(), saved);
        }
        result(outcome)
    }

    /// Write the TOML `create_scenario` returned as `<scenarios_dir>/<id>.toml`, never
    /// replacing an existing file.
    fn save(&self, def: &ScenarioDef, content: &Value) -> Result<String, String> {
        // The core validated the id; checking again keeps the path inside the folder.
        let id_ok = !def.id.is_empty()
            && def
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
        if !id_ok {
            return Err(format!("id {:?} cannot be used as a file name", def.id));
        }
        let toml = content["toml"]
            .as_str()
            .ok_or("create_scenario returned no TOML to save")?;
        let dir = &self.paths.scenarios_dir;
        let path = dir.join(format!("{}.toml", def.id));
        fs::create_dir_all(dir)
            .and_then(|()| OpenOptions::new().write(true).create_new(true).open(&path))
            .and_then(|mut f| f.write_all(toml.as_bytes()))
            .map_err(|e| format!("could not save {}: {e}", path.display()))?;
        Ok(path.display().to_string())
    }

    /// The built-in scenarios (`aimtrainer://scenarios/<id>`) and the format reference.
    pub fn resources() -> Vec<Resource> {
        let mut out: Vec<Resource> = builtin::SOURCES
            .iter()
            .map(|(file, src)| {
                let id = file.trim_end_matches(".toml");
                Resource::new(format!("{SCENARIO_URI}{id}"), id)
                    .with_description(format!("Built-in scenario {id} (TOML)."))
                    .with_mime_type("application/toml")
                    .with_size(src.len() as u64)
            })
            .collect();
        out.push(
            Resource::new(SCHEMA_DOC_URI, "scenarios.md")
                .with_title("Writing scenarios and routines")
                .with_description("The scenario and routine TOML format, every field.")
                .with_mime_type("text/markdown")
                .with_size(SCHEMA_DOC.len() as u64),
        );
        out
    }

    /// Read resource `uri`; anything but a listed resource is not found.
    pub fn read(uri: &str) -> Result<ReadResourceResult, ErrorData> {
        let found = if uri == SCHEMA_DOC_URI {
            Some((SCHEMA_DOC, "text/markdown"))
        } else {
            uri.strip_prefix(SCENARIO_URI).and_then(|id| {
                builtin::SOURCES
                    .iter()
                    .find(|(file, _)| file.strip_suffix(".toml") == Some(id))
                    .map(|(_, src)| (*src, "application/toml"))
            })
        };
        match found {
            Some((text, mime)) => Ok(ReadResourceResult::new(vec![
                ResourceContents::text(text, uri).with_mime_type(mime),
            ])),
            None => Err(ErrorData::resource_not_found(
                format!("no resource {uri:?}"),
                None,
            )),
        }
    }
}

impl ServerHandler for CoachServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
        .with_server_info(Implementation::new("aimtrainer", env!("CARGO_PKG_VERSION")))
        .with_instructions(aim_core::coach::system_prompt())
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListToolsResult, ErrorData>> + MaybeSendFuture + '_ {
        std::future::ready(Ok(ListToolsResult::with_all_items(Self::tools())))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<CallToolResponse, ErrorData>> + MaybeSendFuture + '_ {
        std::future::ready(Ok(self.call(&request.name, request.arguments).into()))
    }

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourcesResult, ErrorData>> + MaybeSendFuture + '_ {
        std::future::ready(Ok(ListResourcesResult::with_all_items(Self::resources())))
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ReadResourceResponse, ErrorData>> + MaybeSendFuture + '_ {
        std::future::ready(Self::read(&request.uri).map(Into::into))
    }
}

fn result(outcome: ToolOutcome) -> CallToolResult {
    if outcome.is_error {
        CallToolResult::structured_error(outcome.content)
    } else {
        CallToolResult::structured(outcome.content)
    }
}

fn errors(errors: Vec<String>) -> CallToolResult {
    CallToolResult::structured_error(json!({ "ok": false, "errors": errors }))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
