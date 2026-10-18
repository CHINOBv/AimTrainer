//! Live smoke tests against real providers. Ignored by default; they spend a few tokens.
//!
//! ```sh
//! ANTHROPIC_API_KEY=... cargo test -p aim_ai --test live_smoke -- --ignored
//! OPENAI_API_KEY=... OPENAI_MODEL=... cargo test -p aim_ai --test live_smoke -- --ignored
//! ```
//!
//! Each test returns early, passing, when its key is not set.

use aim_ai::config::{ANTHROPIC_BASE_URL, OPENAI_BASE_URL};
use aim_ai::{ApiKey, CoachConfig, ProviderKind, build_model};
use aim_core::coach::{Message, tools};

fn ask(config: &CoachConfig, key: ApiKey) {
    let mut model = build_model(config, Some(key)).expect("valid settings");
    let reply = model
        .complete(
            &[
                Message::System {
                    content: "You are an aim coach. Call list_stats before judging.".into(),
                },
                Message::User {
                    content: "How am I doing?".into(),
                },
            ],
            &tools::specs(),
        )
        .expect("the provider answers");
    assert!(
        !reply.text.is_empty() || !reply.tool_calls.is_empty(),
        "{reply:?}"
    );
}

#[test]
#[ignore = "calls the Anthropic API; needs ANTHROPIC_API_KEY"]
fn anthropic_answers_or_calls_a_tool() {
    let Ok(key) = std::env::var("ANTHROPIC_API_KEY") else {
        return;
    };
    let config = CoachConfig {
        enabled: true,
        provider: ProviderKind::Anthropic,
        base_url: Some(ANTHROPIC_BASE_URL.into()),
        model: std::env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-haiku-5-5".into()),
        max_tokens: 2_000,
        ..CoachConfig::default()
    };
    ask(&config, ApiKey::new(key));
}

#[test]
#[ignore = "calls the OpenAI API; needs OPENAI_API_KEY and OPENAI_MODEL"]
fn openai_answers_or_calls_a_tool() {
    let (Ok(key), Ok(model)) = (
        std::env::var("OPENAI_API_KEY"),
        std::env::var("OPENAI_MODEL"),
    ) else {
        return;
    };
    let config = CoachConfig {
        enabled: true,
        provider: ProviderKind::OpenaiCompatible,
        base_url: Some(std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| OPENAI_BASE_URL.into())),
        model,
        max_tokens: 2_000,
        ..CoachConfig::default()
    };
    ask(&config, ApiKey::new(key));
}
