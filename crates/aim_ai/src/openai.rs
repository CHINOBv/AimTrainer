//! [`ChatModel`] over the OpenAI Chat Completions API, as served by OpenAI, OpenCode's
//! pay-as-you-go endpoint, Ollama and LM Studio (see [`crate::config`] for their URLs).
//!
//! Tools go out as function tools with `tool_choice: "auto"`, without `strict`, because
//! local servers differ in what they accept; the core validates every argument anyway.

use aim_core::coach::{ChatModel, Message, ModelError, Reply, ToolCall, ToolSpec};
use serde_json::{Value, json};

use crate::http::{self, HttpRequest, Transport};
use crate::secrets::ApiKey;

/// A Chat Completions client.
pub struct OpenAiCompatProvider {
    base_url: String,
    model: String,
    key: Option<ApiKey>,
    max_tokens: u32,
    transport: Box<dyn Transport>,
}

impl OpenAiCompatProvider {
    /// A client for `model` at `base_url` (e.g. `https://api.openai.com/v1`). Local
    /// servers take no key.
    pub fn new(
        base_url: &str,
        model: &str,
        key: Option<ApiKey>,
        max_tokens: u32,
        transport: Box<dyn Transport>,
    ) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            model: model.to_owned(),
            key: key.filter(|k| !k.is_empty()),
            max_tokens,
            transport,
        }
    }

    /// The request [`ChatModel::complete`] sends.
    pub fn request(&self, messages: &[Message], tools: &[ToolSpec]) -> HttpRequest {
        let mut headers = Vec::new();
        if let Some(key) = &self.key {
            headers.push((
                "authorization".to_owned(),
                format!("Bearer {}", key.expose()),
            ));
        }
        HttpRequest {
            url: format!("{}/chat/completions", self.base_url),
            headers,
            body: request_body(&self.model, self.max_tokens, messages, tools),
        }
    }
}

impl ChatModel for OpenAiCompatProvider {
    fn complete(&mut self, messages: &[Message], tools: &[ToolSpec]) -> Result<Reply, ModelError> {
        let request = self.request(messages, tools);
        let body = http::send(
            self.transport.as_ref(),
            &request,
            "the model server",
            self.key.as_ref(),
        )?;
        parse_response(&body)
    }
}

/// The Chat Completions body for `messages` and `tools`.
pub fn request_body(
    model: &str,
    max_tokens: u32,
    messages: &[Message],
    tools: &[ToolSpec],
) -> Value {
    let messages: Vec<Value> = messages.iter().map(message_json).collect();
    let mut body = json!({
        "model": model,
        "messages": messages,
        "max_completion_tokens": max_tokens,
    });
    if !tools.is_empty() {
        let tools: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect();
        body["tools"] = Value::Array(tools);
        body["tool_choice"] = json!("auto");
    }
    body
}

fn message_json(message: &Message) -> Value {
    match message {
        Message::System { content } => json!({ "role": "system", "content": content }),
        Message::User { content } => json!({ "role": "user", "content": content }),
        Message::Assistant {
            content,
            tool_calls,
        } => {
            if tool_calls.is_empty() {
                return json!({ "role": "assistant", "content": content });
            }
            let calls: Vec<Value> = tool_calls
                .iter()
                .map(|c| {
                    json!({
                        "id": c.id,
                        "type": "function",
                        "function": { "name": c.name, "arguments": c.arguments.to_string() },
                    })
                })
                .collect();
            let content = if content.is_empty() {
                Value::Null
            } else {
                json!(content)
            };
            json!({ "role": "assistant", "content": content, "tool_calls": calls })
        }
        Message::Tool {
            call_id, content, ..
        } => json!({ "role": "tool", "tool_call_id": call_id, "content": content }),
    }
}

/// The reply in a Chat Completions response body.
pub fn parse_response(body: &str) -> Result<Reply, ModelError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|e| ModelError::InvalidReply(format!("not JSON: {e}")))?;
    let message = value
        .pointer("/choices/0/message")
        .ok_or_else(|| ModelError::InvalidReply("no choices[0].message".to_owned()))?;
    let text = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let mut tool_calls = Vec::new();
    for (i, call) in message
        .get("tool_calls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let function = call
            .get("function")
            .ok_or_else(|| ModelError::InvalidReply("a tool call has no function".to_owned()))?;
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| ModelError::InvalidReply("a tool call has no name".to_owned()))?;
        let id = call
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map_or_else(|| format!("call_{i}"), str::to_owned);
        tool_calls.push(ToolCall {
            id,
            name: name.to_owned(),
            arguments: arguments(function.get("arguments")),
        });
    }
    Ok(Reply { text, tool_calls })
}

/// Arguments arrive as a JSON string (some local servers send an object). Text that is not
/// JSON is passed on as a string, so the tool reports bad arguments and the model retries.
fn arguments(raw: Option<&Value>) -> Value {
    match raw {
        Some(Value::String(text)) if text.trim().is_empty() => json!({}),
        Some(Value::String(text)) => {
            serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.clone()))
        }
        Some(Value::Null) | None => json!({}),
        Some(other) => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::tests::FakeTransport;

    const TOOL_CALL: &str = include_str!("../tests/fixtures/openai_tool_call.json");
    const ANSWER: &str = include_str!("../tests/fixtures/openai_answer.json");

    fn provider(transport: &FakeTransport, key: Option<&str>) -> OpenAiCompatProvider {
        OpenAiCompatProvider::new(
            "https://api.openai.com/v1/",
            "gpt-test",
            key.map(ApiKey::new),
            1_000,
            Box::new(transport.clone()),
        )
    }

    fn specs() -> Vec<ToolSpec> {
        aim_core::coach::tools::specs()
    }

    #[test]
    fn the_request_carries_messages_tools_and_the_bearer_key() {
        let transport = FakeTransport::ok(ANSWER);
        let p = provider(&transport, Some("sk-test"));
        let request = p.request(
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
        assert_eq!(request.url, "https://api.openai.com/v1/chat/completions");
        assert_eq!(request.header("Authorization"), Some("Bearer sk-test"));
        let body = &request.body;
        assert_eq!(body["model"], "gpt-test");
        assert_eq!(body["max_completion_tokens"], 1_000);
        assert_eq!(
            body["messages"][0],
            json!({ "role": "system", "content": "coach" })
        );
        assert_eq!(body["tool_choice"], "auto");
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "list_stats");
        assert!(body["tools"][0]["function"]["parameters"].is_object());
        assert!(
            !body.to_string().contains("sk-test"),
            "the key stays in the header"
        );
    }

    #[test]
    fn a_local_server_gets_no_authorization_header() {
        let transport = FakeTransport::ok(ANSWER);
        let p = OpenAiCompatProvider::new(
            crate::config::OLLAMA_BASE_URL,
            "llama",
            None,
            100,
            Box::new(transport.clone()),
        );
        assert_eq!(p.request(&[], &[]).header("authorization"), None);
        assert!(p.request(&[], &[]).body.get("tools").is_none());
    }

    #[test]
    fn a_tool_call_round_trips_through_the_history() {
        let transport = FakeTransport::ok(TOOL_CALL);
        let mut p = provider(&transport, Some("k"));
        let reply = p.complete(&[], &specs()).unwrap();
        assert_eq!(reply.text, "");
        assert_eq!(
            reply.tool_calls,
            [ToolCall {
                id: "call_abc".into(),
                name: "recall".into(),
                arguments: json!({ "query": "goals", "limit": 3 }),
            }]
        );

        let history = [
            Message::Assistant {
                content: reply.text,
                tool_calls: reply.tool_calls,
            },
            Message::Tool {
                call_id: "call_abc".into(),
                name: "recall".into(),
                content: "{\"ok\":true}".into(),
                is_error: false,
            },
        ];
        let body = p.request(&history, &specs()).body;
        assert_eq!(
            body["messages"][0],
            json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_abc",
                    "type": "function",
                    "function": { "name": "recall", "arguments": "{\"limit\":3,\"query\":\"goals\"}" }
                }]
            })
        );
        assert_eq!(
            body["messages"][1],
            json!({ "role": "tool", "tool_call_id": "call_abc", "content": "{\"ok\":true}" })
        );
    }

    #[test]
    fn a_text_answer_is_final() {
        let transport = FakeTransport::ok(ANSWER);
        let reply = provider(&transport, None).complete(&[], &[]).unwrap();
        assert_eq!(
            transport.last().body["model"],
            "gpt-test",
            "complete sends the built request"
        );
        assert_eq!(reply.text, "Warm up with micro-adjustments first.");
        assert!(reply.tool_calls.is_empty());
    }

    #[test]
    fn loose_local_server_replies_are_tolerated() {
        let body = json!({ "choices": [{ "message": { "content": null, "tool_calls": [
            { "function": { "name": "list_stats", "arguments": "" } },
            { "id": "x", "function": { "name": "recall", "arguments": { "query": "a" } } },
            { "id": "y", "function": { "name": "recall", "arguments": "not json" } }
        ]}}]})
        .to_string();
        let reply = parse_response(&body).unwrap();
        let ids: Vec<&str> = reply.tool_calls.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["call_0", "x", "y"]);
        assert_eq!(reply.tool_calls[0].arguments, json!({}));
        assert_eq!(reply.tool_calls[1].arguments, json!({ "query": "a" }));
        assert_eq!(reply.tool_calls[2].arguments, json!("not json"));
    }

    #[test]
    fn unusable_replies_and_http_errors_are_errors() {
        assert!(matches!(
            parse_response("nope"),
            Err(ModelError::InvalidReply(_))
        ));
        assert!(matches!(
            parse_response(r#"{"choices":[]}"#),
            Err(ModelError::InvalidReply(_))
        ));
        let transport = FakeTransport::status(401, r#"{"error":{"message":"bad key sk-x"}}"#);
        let err = provider(&transport, Some("sk-x"))
            .complete(&[], &[])
            .unwrap_err();
        assert!(matches!(&err, ModelError::Request(t) if t.contains("401") && !t.contains("sk-x")));
    }
}
