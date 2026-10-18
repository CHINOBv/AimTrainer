//! [`ChatModel`] over Anthropic's Messages API (`POST /v1/messages`).
//!
//! - Tools are sent with `strict: true` and `tool_choice: auto`. Forced tool choice
//!   (`any`/`tool`) is rejected by current models, so the coach's prompt steers instead.
//!   Strict mode does not accept numeric, string-length or array-size constraints, so they
//!   are removed from the schema sent; the core still enforces them on every call.
//! - `tool_use` and `tool_result` blocks map to and from [`Message`]. Consecutive tool
//!   results go back in one user message, as the API expects.
//! - Replies can carry blocks the core [`Message`] has no room for (thinking). The
//!   provider remembers the raw content of its recent replies and sends it back unchanged
//!   when the same assistant turn reappears in the history, because current models reject
//!   a tool-use continuation whose thinking was stripped.

use aim_core::coach::{ChatModel, Message, ModelError, Reply, ToolCall, ToolSpec};
use serde_json::{Map, Value, json};

use crate::http::{self, HttpRequest, Transport};
use crate::secrets::ApiKey;

/// The API version header value.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
/// How many recent replies keep their raw content for replay.
pub const MAX_REPLAYS: usize = 32;

/// Schema keywords strict tool use does not accept.
const UNSUPPORTED_KEYWORDS: [&str; 8] = [
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "minLength",
    "maxLength",
    "maxItems",
];

/// A Messages API client.
pub struct AnthropicProvider {
    base_url: String,
    model: String,
    key: ApiKey,
    max_tokens: u32,
    transport: Box<dyn Transport>,
    replays: Vec<(String, Value)>,
}

impl AnthropicProvider {
    /// A client for `model` (for example `claude-sonnet-5-5`) at `base_url`
    /// (normally [`crate::config::ANTHROPIC_BASE_URL`]).
    pub fn new(
        base_url: &str,
        model: &str,
        key: ApiKey,
        max_tokens: u32,
        transport: Box<dyn Transport>,
    ) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            model: model.to_owned(),
            key,
            max_tokens,
            transport,
            replays: Vec::new(),
        }
    }

    /// The request [`ChatModel::complete`] sends.
    pub fn request(&self, messages: &[Message], tools: &[ToolSpec]) -> HttpRequest {
        HttpRequest {
            url: format!("{}/v1/messages", self.base_url),
            headers: vec![
                ("x-api-key".to_owned(), self.key.expose().to_owned()),
                ("anthropic-version".to_owned(), ANTHROPIC_VERSION.to_owned()),
            ],
            body: request_body(&self.model, self.max_tokens, messages, tools, &self.replays),
        }
    }

    fn remember_raw(&mut self, reply: &Reply, content: Value) {
        let plain = content.as_array().is_some_and(|blocks| {
            blocks
                .iter()
                .all(|b| matches!(b["type"].as_str(), Some("text" | "tool_use")))
        });
        if plain {
            return;
        }
        if self.replays.len() == MAX_REPLAYS {
            self.replays.remove(0);
        }
        self.replays
            .push((replay_key(&reply.text, &reply.tool_calls), content));
    }
}

impl ChatModel for AnthropicProvider {
    fn complete(&mut self, messages: &[Message], tools: &[ToolSpec]) -> Result<Reply, ModelError> {
        let request = self.request(messages, tools);
        let body = http::send(
            self.transport.as_ref(),
            &request,
            "Anthropic",
            Some(&self.key),
        )?;
        let (reply, content) = parse_response(&body)?;
        self.remember_raw(&reply, content);
        Ok(reply)
    }
}

/// Identifies an assistant turn by what the core keeps of it.
fn replay_key(text: &str, calls: &[ToolCall]) -> String {
    json!([text, calls]).to_string()
}

/// The Messages API body. `replays` maps assistant turns to the raw content to send back.
pub fn request_body(
    model: &str,
    max_tokens: u32,
    messages: &[Message],
    tools: &[ToolSpec],
    replays: &[(String, Value)],
) -> Value {
    let mut system = Vec::new();
    let mut turns: Vec<(&str, Vec<Value>)> = Vec::new();
    for message in messages {
        let (role, blocks) = match message {
            Message::System { content } => {
                system.push(content.as_str());
                continue;
            }
            Message::User { content } => ("user", text_block(content)),
            Message::Assistant {
                content,
                tool_calls,
            } => {
                let key = replay_key(content, tool_calls);
                let raw = replays.iter().rev().find(|(k, _)| *k == key);
                let blocks = match raw.and_then(|(_, raw)| raw.as_array()) {
                    Some(raw) => raw.clone(),
                    None => {
                        let mut blocks = text_block(content);
                        blocks.extend(tool_calls.iter().map(|c| {
                            json!({
                                "type": "tool_use",
                                "id": c.id,
                                "name": c.name,
                                "input": c.arguments,
                            })
                        }));
                        blocks
                    }
                };
                ("assistant", blocks)
            }
            Message::Tool {
                call_id,
                content,
                is_error,
                ..
            } => (
                "user",
                vec![json!({
                    "type": "tool_result",
                    "tool_use_id": call_id,
                    "content": content,
                    "is_error": is_error,
                })],
            ),
        };
        if blocks.is_empty() {
            continue;
        }
        match turns.last_mut() {
            Some((last, existing)) if *last == role => existing.extend(blocks),
            _ => turns.push((role, blocks)),
        }
    }
    let messages: Vec<Value> = turns
        .into_iter()
        .map(|(role, content)| json!({ "role": role, "content": content }))
        .collect();
    let mut body = json!({
        "model": model,
        "max_tokens": max_tokens,
        "messages": messages,
    });
    if !system.is_empty() {
        body["system"] = json!(system.join("\n\n"));
    }
    if !tools.is_empty() {
        let tools: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": strict_schema(&t.parameters),
                    "strict": true,
                })
            })
            .collect();
        body["tools"] = Value::Array(tools);
        body["tool_choice"] = json!({ "type": "auto" });
    }
    body
}

/// An empty text block is rejected by the API, so empty text sends nothing.
fn text_block(text: &str) -> Vec<Value> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![json!({ "type": "text", "text": text })]
    }
}

/// `schema` without the keywords strict tool use rejects. Property names are kept, even
/// one called `minimum`: only schema keywords are removed.
pub fn strict_schema(schema: &Value) -> Value {
    let mut schema = schema.clone();
    strip(&mut schema);
    schema
}

fn strip(schema: &mut Value) {
    let Value::Object(map) = schema else {
        return;
    };
    for keyword in UNSUPPORTED_KEYWORDS {
        map.remove(keyword);
    }
    if map
        .get("minItems")
        .and_then(Value::as_u64)
        .is_some_and(|n| n > 1)
    {
        map.remove("minItems");
    }
    for key in ["properties", "$defs", "definitions"] {
        if let Some(Value::Object(children)) = map.get_mut(key) {
            children.values_mut().for_each(strip);
        }
    }
    if let Some(items) = map.get_mut("items") {
        strip(items);
    }
    for key in ["anyOf", "allOf", "oneOf"] {
        if let Some(Value::Array(options)) = map.get_mut(key) {
            options.iter_mut().for_each(strip);
        }
    }
}

/// The reply in a Messages API response body, plus its raw `content` for replay.
pub fn parse_response(body: &str) -> Result<(Reply, Value), ModelError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|e| ModelError::InvalidReply(format!("not JSON: {e}")))?;
    let stop_reason = value["stop_reason"].as_str().unwrap_or_default();
    if stop_reason == "refusal" {
        return Err(ModelError::InvalidReply(
            "Claude declined this request; rephrase it or try another model".to_owned(),
        ));
    }
    let content = value
        .get("content")
        .filter(|c| c.is_array())
        .cloned()
        .ok_or_else(|| ModelError::InvalidReply("no content array".to_owned()))?;
    let mut reply = Reply::default();
    for block in content.as_array().into_iter().flatten() {
        match block["type"].as_str() {
            Some("text") => reply
                .text
                .push_str(block["text"].as_str().unwrap_or_default()),
            Some("tool_use") => {
                let field = |name: &str| {
                    block[name].as_str().map(str::to_owned).ok_or_else(|| {
                        ModelError::InvalidReply(format!("a tool_use block has no {name}"))
                    })
                };
                reply.tool_calls.push(ToolCall {
                    id: field("id")?,
                    name: field("name")?,
                    arguments: block
                        .get("input")
                        .cloned()
                        .unwrap_or_else(|| Value::Object(Map::new())),
                });
            }
            // Thinking and any future block types are kept only in the raw content.
            _ => {}
        }
    }
    if stop_reason == "max_tokens" && reply.text.is_empty() && reply.tool_calls.is_empty() {
        return Err(ModelError::InvalidReply(
            "the reply was cut off before any text; raise max_tokens".to_owned(),
        ));
    }
    Ok((reply, content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::tests::FakeTransport;

    const TOOL_USE: &str = include_str!("../tests/fixtures/anthropic_tool_use.json");
    const ANSWER: &str = include_str!("../tests/fixtures/anthropic_answer.json");
    const REFUSAL: &str = include_str!("../tests/fixtures/anthropic_refusal.json");

    fn provider(transport: &FakeTransport) -> AnthropicProvider {
        AnthropicProvider::new(
            crate::config::ANTHROPIC_BASE_URL,
            "claude-sonnet-5-5",
            ApiKey::new("sk-ant-test"),
            16_000,
            Box::new(transport.clone()),
        )
    }

    fn specs() -> Vec<ToolSpec> {
        aim_core::coach::tools::specs()
    }

    #[test]
    fn the_request_has_the_headers_system_prompt_and_strict_tools() {
        let transport = FakeTransport::ok(ANSWER);
        let request = provider(&transport).request(
            &[
                Message::System {
                    content: "coach".into(),
                },
                Message::User {
                    content: "hi".into(),
                },
            ],
            &specs(),
        );
        assert_eq!(request.url, "https://api.anthropic.com/v1/messages");
        assert_eq!(request.header("x-api-key"), Some("sk-ant-test"));
        assert_eq!(request.header("anthropic-version"), Some("2023-06-01"));
        let body = &request.body;
        assert_eq!(body["model"], "claude-sonnet-5-5");
        assert_eq!(body["max_tokens"], 16_000);
        assert_eq!(body["system"], "coach");
        assert_eq!(
            body["messages"],
            json!([{ "role": "user", "content": [{ "type": "text", "text": "hi" }] }])
        );
        assert_eq!(body["tool_choice"], json!({ "type": "auto" }));
        let tool = &body["tools"][1];
        assert_eq!(tool["name"], "recommend_routine");
        assert_eq!(tool["strict"], true);
        assert_eq!(tool["input_schema"]["additionalProperties"], false);
        assert!(
            tool["input_schema"]["properties"]["minutes"]
                .get("minimum")
                .is_none(),
            "{tool}"
        );
        assert!(!body.to_string().contains("sk-ant-test"));
    }

    #[test]
    fn strict_schemas_keep_property_names_and_drop_only_keywords() {
        let schema = json!({
            "type": "object",
            "properties": {
                "minimum": { "type": "integer", "minimum": 1, "maximum": 5 },
                "tags": { "type": "array", "items": { "type": "string", "maxLength": 9 },
                          "maxItems": 5, "minItems": 1 },
                "pick": { "anyOf": [{ "type": "number", "multipleOf": 2 }, { "type": "null" }] }
            },
            "required": ["minimum"],
            "additionalProperties": false
        });
        assert_eq!(
            strict_schema(&schema),
            json!({
                "type": "object",
                "properties": {
                    "minimum": { "type": "integer" },
                    "tags": { "type": "array", "items": { "type": "string" }, "minItems": 1 },
                    "pick": { "anyOf": [{ "type": "number" }, { "type": "null" }] }
                },
                "required": ["minimum"],
                "additionalProperties": false
            })
        );
    }

    #[test]
    fn a_tool_use_round_trip_replays_thinking_and_groups_tool_results() {
        let transport = FakeTransport::ok(TOOL_USE);
        let mut p = provider(&transport);
        let reply = p.complete(&[], &specs()).unwrap();
        assert_eq!(reply.text, "Let me check your stats.");
        assert_eq!(
            reply.tool_calls,
            [
                ToolCall {
                    id: "toolu_01".into(),
                    name: "list_stats".into(),
                    arguments: json!({}),
                },
                ToolCall {
                    id: "toolu_02".into(),
                    name: "recall".into(),
                    arguments: json!({ "query": "goals" }),
                },
            ]
        );

        let history = [
            Message::User {
                content: "How am I doing?".into(),
            },
            Message::Assistant {
                content: reply.text.clone(),
                tool_calls: reply.tool_calls.clone(),
            },
            Message::Tool {
                call_id: "toolu_01".into(),
                name: "list_stats".into(),
                content: "{\"findings\":[]}".into(),
                is_error: false,
            },
            Message::Tool {
                call_id: "toolu_02".into(),
                name: "recall".into(),
                content: "{\"error\":\"bad\"}".into(),
                is_error: true,
            },
        ];
        let body = p.request(&history, &specs()).body;
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3, "{messages:?}");
        let raw: Value = serde_json::from_str(TOOL_USE).unwrap();
        assert_eq!(
            messages[1],
            json!({ "role": "assistant", "content": raw["content"] }),
            "the assistant turn goes back unchanged, thinking block included"
        );
        assert_eq!(
            messages[2],
            json!({ "role": "user", "content": [
                { "type": "tool_result", "tool_use_id": "toolu_01",
                  "content": "{\"findings\":[]}", "is_error": false },
                { "type": "tool_result", "tool_use_id": "toolu_02",
                  "content": "{\"error\":\"bad\"}", "is_error": true }
            ]})
        );
    }

    #[test]
    fn an_assistant_turn_from_elsewhere_is_rebuilt_from_its_text_and_calls() {
        let history = [Message::Assistant {
            content: String::new(),
            tool_calls: vec![ToolCall {
                id: "toolu_9".into(),
                name: "recall".into(),
                arguments: json!({ "query": "x" }),
            }],
        }];
        let body = request_body("m", 10, &history, &[], &[]);
        assert_eq!(
            body["messages"][0]["content"],
            json!([{ "type": "tool_use", "id": "toolu_9", "name": "recall",
                     "input": { "query": "x" } }]),
            "no empty text block"
        );
        assert!(body.get("tools").is_none() && body.get("system").is_none());
    }

    #[test]
    fn a_text_answer_joins_its_blocks() {
        let transport = FakeTransport::ok(ANSWER);
        let mut p = provider(&transport);
        let reply = p.complete(&[], &[]).unwrap();
        assert_eq!(
            reply.text,
            "Your flicks undershoot to the left. Train micro-adjustments for ten minutes."
        );
        assert!(reply.tool_calls.is_empty());
        assert!(p.replays.is_empty(), "plain replies need no replay");
    }

    #[test]
    fn refusals_truncation_and_http_errors_are_errors() {
        let transport = FakeTransport::ok(REFUSAL);
        let err = provider(&transport).complete(&[], &[]).unwrap_err();
        assert!(matches!(&err, ModelError::InvalidReply(t) if t.contains("declined")));
        let cut = r#"{"content":[],"stop_reason":"max_tokens"}"#;
        assert!(
            matches!(parse_response(cut), Err(ModelError::InvalidReply(t)) if t.contains("max_tokens"))
        );
        for (status, needle) in [(401, "API key"), (429, "rate limit"), (529, "unavailable")] {
            let transport = FakeTransport::status(
                status,
                r#"{"type":"error","error":{"type":"x","message":"m"}}"#,
            );
            let err = provider(&transport).complete(&[], &[]).unwrap_err();
            assert!(
                matches!(&err, ModelError::Request(t) if t.contains(needle) && t.contains("Anthropic")),
                "{status}: {err:?}"
            );
        }
    }

    #[test]
    fn replays_are_bounded() {
        let transport = FakeTransport::ok(TOOL_USE);
        let mut p = provider(&transport);
        for _ in 0..MAX_REPLAYS + 5 {
            p.complete(&[], &[]).unwrap();
        }
        assert_eq!(p.replays.len(), MAX_REPLAYS);
    }
}
