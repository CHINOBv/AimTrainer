//! The AI coach, without I/O: a compact summary of the player's analyses for a language
//! model ([`context`]), a coaching knowledge base ([`knowledge`]), drill generation
//! ([`drills`]), the tools a model may call ([`tools`]), the loop that runs them
//! ([`run_turn`]) and the one-shot run and mistake debriefs ([`debrief`]).
//!
//! The model and the memory store are ports ([`ChatModel`], [`CoachMemory`]): network
//! clients, key storage and databases are adapters outside the core.

pub mod context;
pub mod debrief;
pub mod drills;
pub mod knowledge;
pub mod tools;

pub use context::{CoachContext, ContextInputs};
pub use knowledge::{Diagnosis, SubSkill, diagnose};
pub use tools::{ToolEnv, ToolOutcome};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::scenario::ScenarioDef;

/// A tool call the model asked for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Provider-assigned id, echoed back in the [`Message::Tool`] result.
    pub id: String,
    /// Tool name, one of [`tools::specs`].
    pub name: String,
    /// Arguments as a JSON object.
    pub arguments: Value,
}

/// One message of a conversation, provider-neutral.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum Message {
    /// Instructions for the model.
    System {
        /// The instructions.
        content: String,
    },
    /// What the player wrote.
    User {
        /// The text.
        content: String,
    },
    /// What the model answered, possibly with tool calls.
    Assistant {
        /// The text (may be empty when the model only calls tools).
        content: String,
        /// Tool calls to run before the model continues.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ToolCall>,
    },
    /// The result of one tool call.
    Tool {
        /// Id of the [`ToolCall`] this answers.
        call_id: String,
        /// Tool name.
        name: String,
        /// The result as JSON text.
        content: String,
        /// Whether the call failed (bad arguments, invalid scenario, unknown tool).
        is_error: bool,
    },
}

/// A model reply: final text, or tool calls to run first.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    /// The text.
    pub text: String,
    /// Tool calls; empty means the reply is final.
    pub tool_calls: Vec<ToolCall>,
}

/// A tool the model may call.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ToolSpec {
    /// Tool name.
    pub name: String,
    /// What the tool does and when to use it.
    pub description: String,
    /// JSON Schema of the arguments object.
    pub parameters: Value,
}

/// Why a model call failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ModelError {
    /// The request did not complete (network, authentication, rate limit, timeout).
    #[error("the model request failed: {0}")]
    Request(String),
    /// The provider answered with something that is not a usable reply.
    #[error("the model reply could not be read: {0}")]
    InvalidReply(String),
}

/// A language model, behind whatever provider an adapter talks to. Synchronous: an adapter
/// over an async client blocks internally, off the frame.
pub trait ChatModel {
    /// Continue `messages`, offering `tools`.
    fn complete(&mut self, messages: &[Message], tools: &[ToolSpec]) -> Result<Reply, ModelError>;
}

/// Something the coach remembers about the player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryNote {
    /// The note.
    pub text: String,
    /// Keywords for recall.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Unix seconds when it was kept.
    pub timestamp: u64,
}

/// Why a memory write failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MemoryError {
    /// The store could not keep the note.
    #[error("the coach memory could not be written: {0}")]
    Storage(String),
}

/// Long-term memory of the player, behind a store an adapter provides.
pub trait CoachMemory {
    /// Keep `note`.
    fn retain(&mut self, note: MemoryNote) -> Result<(), MemoryError>;
    /// Up to `limit` notes relevant to `query`, most relevant first.
    fn recall(&self, query: &str, limit: usize) -> Vec<MemoryNote>;
}

/// A [`CoachMemory`] that keeps notes in memory: keyword overlap ranks them, newer first
/// on ties, and an empty query returns the newest.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InMemoryCoachMemory {
    /// Every note, oldest first.
    pub notes: Vec<MemoryNote>,
}

impl CoachMemory for InMemoryCoachMemory {
    fn retain(&mut self, note: MemoryNote) -> Result<(), MemoryError> {
        self.notes.push(note);
        Ok(())
    }

    fn recall(&self, query: &str, limit: usize) -> Vec<MemoryNote> {
        let words: Vec<String> = keywords(query);
        let mut scored: Vec<(usize, usize, &MemoryNote)> = self
            .notes
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let haystack = format!("{} {}", n.text, n.tags.join(" ")).to_lowercase();
                let hits = words.iter().filter(|w| haystack.contains(*w)).count();
                (hits, i, n)
            })
            .filter(|(hits, _, _)| words.is_empty() || *hits > 0)
            .collect();
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then(b.2.timestamp.cmp(&a.2.timestamp))
                .then(b.1.cmp(&a.1))
        });
        scored
            .into_iter()
            .take(limit)
            .map(|(_, _, n)| n.clone())
            .collect()
    }
}

fn keywords(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 1)
        .map(str::to_owned)
        .collect()
}

/// How a turn ended.
#[derive(Debug, Clone, PartialEq)]
pub enum TurnEnd {
    /// The model answered.
    Answer(String),
    /// The model was still calling tools after the step limit.
    StepLimit,
}

/// The result of [`run_turn`].
#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    /// The messages this turn added, in order (the system prompt first when the history
    /// had none, then the player's message); append them to the history.
    pub messages: Vec<Message>,
    /// How the turn ended.
    pub end: TurnEnd,
    /// Scenarios created by `create_scenario` this turn, validated and playable, for the
    /// player to approve.
    pub scenarios: Vec<ScenarioDef>,
}

/// Who the coach is; every coach prompt opens with it.
pub(crate) const ROLE: &str = "You are an aim coach inside a VALORANT-focused aim trainer.";
/// The external-focus rule (docs/training-science.md), shared by every coach prompt.
pub(crate) const CUE_RULE: &str =
    "Give one external-focus cue at a time (for example \"land on the head in one motion\").";
/// The game-integrity rule, shared by every coach prompt.
pub(crate) const NO_GAME_RULE: &str = "Never suggest touching the game process.";

/// The coach's standing instructions.
pub fn system_prompt() -> String {
    format!(
        "{ROLE} Use the tools: call list_stats before judging the player, recommend_routine \
         to pick practice from the built-in library, create_scenario to generate a drill (fix \
         the errors it returns and retry), and remember/recall for what the player tells you \
         about goals and habits. Explain every recommendation with the metric and threshold \
         behind it. {CUE_RULE} Never invent VALORANT constants; cite \
         docs/valorant-mechanics.md. Treat drill numbers as estimates and say generated \
         scenarios are uncalibrated. {NO_GAME_RULE}"
    )
}

/// Run one player turn: send the conversation and the tools to `model`, run every tool
/// call it makes against `env` and `memory`, and repeat until it answers or `max_steps`
/// model calls were made. A failed tool call is returned to the model as an error result
/// so it can retry; only a failed model call ends the turn with an error.
pub fn run_turn(
    model: &mut dyn ChatModel,
    memory: &mut dyn CoachMemory,
    env: &ToolEnv,
    history: &[Message],
    user_message: &str,
    max_steps: usize,
) -> Result<Turn, ModelError> {
    let specs = tools::specs();
    let mut conversation = history.to_vec();
    let start = conversation.len();
    if !history.iter().any(|m| matches!(m, Message::System { .. })) {
        conversation.push(Message::System {
            content: system_prompt(),
        });
    }
    conversation.push(Message::User {
        content: user_message.to_owned(),
    });
    let mut scenarios = Vec::new();
    for _ in 0..max_steps {
        let reply = model.complete(&conversation, &specs)?;
        let calls = reply.tool_calls;
        conversation.push(Message::Assistant {
            content: reply.text.clone(),
            tool_calls: calls.clone(),
        });
        if calls.is_empty() {
            return Ok(Turn {
                messages: conversation.split_off(start),
                end: TurnEnd::Answer(reply.text),
                scenarios,
            });
        }
        for call in calls {
            let outcome = tools::call(&call.name, &call.arguments, env, memory);
            scenarios.extend(outcome.scenario);
            conversation.push(Message::Tool {
                call_id: call.id,
                name: call.name,
                content: outcome.content.to_string(),
                is_error: outcome.is_error,
            });
        }
    }
    Ok(Turn {
        messages: conversation.split_off(start),
        end: TurnEnd::StepLimit,
        scenarios,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A model that plays back scripted replies and records what it was sent.
    struct FakeModel {
        replies: Vec<Reply>,
        seen: Vec<Vec<Message>>,
    }

    impl FakeModel {
        fn new(replies: Vec<Reply>) -> Self {
            Self {
                replies,
                seen: Vec::new(),
            }
        }
    }

    impl ChatModel for FakeModel {
        fn complete(
            &mut self,
            messages: &[Message],
            tools: &[ToolSpec],
        ) -> Result<Reply, ModelError> {
            assert!(!tools.is_empty(), "the tools are offered on every call");
            self.seen.push(messages.to_vec());
            if self.replies.is_empty() {
                return Err(ModelError::Request("script exhausted".into()));
            }
            Ok(self.replies.remove(0))
        }
    }

    fn call(id: &str, name: &str, arguments: Value) -> Reply {
        Reply {
            text: String::new(),
            tool_calls: vec![ToolCall {
                id: id.into(),
                name: name.into(),
                arguments,
            }],
        }
    }

    fn answer(text: &str) -> Reply {
        Reply {
            text: text.into(),
            tool_calls: Vec::new(),
        }
    }

    fn env(ctx: &CoachContext) -> ToolEnv<'_> {
        ToolEnv {
            context: ctx,
            scenarios: &[],
            routines: &[],
            benchmark: None,
            now: 1_000,
        }
    }

    #[test]
    fn a_tool_call_is_run_and_its_result_reaches_the_model_before_the_answer() {
        let ctx = CoachContext::default();
        let mut model = FakeModel::new(vec![
            call("c1", tools::LIST_STATS, json!({})),
            answer("Train micro-adjustments."),
        ]);
        let mut memory = InMemoryCoachMemory::default();
        let turn = run_turn(
            &mut model,
            &mut memory,
            &env(&ctx),
            &[],
            "How am I doing?",
            4,
        )
        .unwrap();

        assert_eq!(turn.end, TurnEnd::Answer("Train micro-adjustments.".into()));
        let roles: Vec<&str> = turn
            .messages
            .iter()
            .map(|m| match m {
                Message::System { .. } => "system",
                Message::User { .. } => "user",
                Message::Assistant { .. } => "assistant",
                Message::Tool { .. } => "tool",
            })
            .collect();
        assert_eq!(roles, ["system", "user", "assistant", "tool", "assistant"]);
        assert_eq!(model.seen.len(), 2);
        let Some(Message::Tool {
            call_id,
            content,
            is_error,
            ..
        }) = model.seen[1].last()
        else {
            panic!(
                "the second call ends with the tool result: {:?}",
                model.seen[1]
            );
        };
        assert_eq!(call_id, "c1");
        assert!(!is_error);
        assert!(content.contains("\"findings\""), "{content}");
    }

    #[test]
    fn the_turn_stops_at_the_step_limit() {
        let ctx = CoachContext::default();
        let replies = (0..10)
            .map(|i| call(&format!("c{i}"), tools::LIST_STATS, json!({})))
            .collect();
        let mut model = FakeModel::new(replies);
        let mut memory = InMemoryCoachMemory::default();
        let turn = run_turn(&mut model, &mut memory, &env(&ctx), &[], "Go", 3).unwrap();
        assert_eq!(turn.end, TurnEnd::StepLimit);
        assert_eq!(model.seen.len(), 3, "exactly max_steps model calls");
    }

    #[test]
    fn a_bad_tool_call_goes_back_to_the_model_as_an_error() {
        let ctx = CoachContext::default();
        let mut model = FakeModel::new(vec![
            call("c1", "delete_everything", json!({})),
            answer("Sorry."),
        ]);
        let mut memory = InMemoryCoachMemory::default();
        let turn = run_turn(&mut model, &mut memory, &env(&ctx), &[], "Hi", 4).unwrap();
        assert!(turn.messages.iter().any(|m| matches!(
            m,
            Message::Tool { is_error: true, content, .. } if content.contains("unknown tool")
        )));
        assert_eq!(turn.end, TurnEnd::Answer("Sorry.".into()));
    }

    #[test]
    fn created_scenarios_are_returned_with_the_turn() {
        let ctx = CoachContext::default();
        let mut model = FakeModel::new(vec![
            call(
                "c1",
                tools::CREATE_SCENARIO,
                json!({ "skill": "micro-adjustment", "level": 0.0 }),
            ),
            answer("Here is a drill."),
        ]);
        let mut memory = InMemoryCoachMemory::default();
        let turn = run_turn(&mut model, &mut memory, &env(&ctx), &[], "Make one", 4).unwrap();
        let ids: Vec<&str> = turn.scenarios.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, ["coach-micro-adjustment-1"]);
    }

    #[test]
    fn an_existing_system_prompt_is_not_repeated_and_model_errors_end_the_turn() {
        let ctx = CoachContext::default();
        let history = [Message::System {
            content: "custom".into(),
        }];
        let mut model = FakeModel::new(vec![answer("ok")]);
        let mut memory = InMemoryCoachMemory::default();
        let turn = run_turn(&mut model, &mut memory, &env(&ctx), &history, "Hi", 2).unwrap();
        assert!(matches!(turn.messages[0], Message::User { .. }));
        assert_eq!(model.seen[0].len(), 2);

        let err = run_turn(&mut model, &mut memory, &env(&ctx), &history, "Again", 2);
        assert_eq!(err, Err(ModelError::Request("script exhausted".into())));
    }

    #[test]
    fn memory_recalls_by_keyword_newest_first_within_the_limit() {
        let mut m = InMemoryCoachMemory::default();
        for (text, ts) in [
            ("Wants to reach Immortal this act", 1),
            ("Struggles with tracking strafes", 2),
            ("Plays Jett; tracking warm-up before ranked", 3),
            ("Prefers short sessions", 4),
        ] {
            m.retain(MemoryNote {
                text: text.into(),
                tags: Vec::new(),
                timestamp: ts,
            })
            .unwrap();
        }
        let hits = m.recall("tracking", 5);
        let ts: Vec<u64> = hits.iter().map(|n| n.timestamp).collect();
        assert_eq!(ts, [3, 2]);
        assert_eq!(m.recall("tracking warm-up", 1)[0].timestamp, 3);
        assert_eq!(m.recall("", 2).len(), 2);
        assert_eq!(m.recall("", 2)[0].timestamp, 4, "empty query: newest first");
        assert!(m.recall("operator", 5).is_empty());
    }

    #[test]
    fn the_system_prompt_text_is_unchanged() {
        assert_eq!(
            system_prompt(),
            "You are an aim coach inside a VALORANT-focused aim trainer. Use the tools: call \
             list_stats before judging the player, recommend_routine to pick practice from the \
             built-in library, create_scenario to generate a drill (fix the errors it returns and \
             retry), and remember/recall for what the player tells you about goals and habits. \
             Explain every recommendation with the metric and threshold behind it. Give one \
             external-focus cue at a time (for example \"land on the head in one motion\"). Never \
             invent VALORANT constants; cite docs/valorant-mechanics.md. Treat drill numbers as \
             estimates and say generated scenarios are uncalibrated. Never suggest touching the game \
             process."
        );
    }

    #[test]
    fn messages_round_trip_as_tagged_json() {
        let msg = Message::Assistant {
            content: String::new(),
            tool_calls: vec![ToolCall {
                id: "1".into(),
                name: "recall".into(),
                arguments: json!({ "query": "goals" }),
            }],
        };
        let text = serde_json::to_string(&msg).unwrap();
        assert!(text.contains("\"role\":\"assistant\""), "{text}");
        assert_eq!(serde_json::from_str::<Message>(&text).unwrap(), msg);
    }
}
