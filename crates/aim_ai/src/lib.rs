//! Adapters for the AI coach in `aim_core::coach`.
//!
//! The core defines the ports ([`aim_core::coach::ChatModel`] and
//! [`aim_core::coach::CoachMemory`]); this crate implements them with I/O:
//!
//! - [`config`]: what the player chose (provider, model, base URL, timeouts), validated
//!   without I/O. Off by default. [`build_model`] turns it into a provider.
//! - [`secrets`]: API keys in the OS keyring, with an environment-variable fallback. Keys
//!   are never written to settings files and never logged.
//! - [`openai`] and [`anthropic`]: chat providers over HTTP, through the [`http`] seam.
//! - [`cli`]: delegates to the player's own installed CLI, text only.
//!
//! All network and process work is blocking: call it off the frame (a worker thread).

pub mod anthropic;
pub mod cli;
pub mod config;
pub mod http;
pub mod openai;
pub mod secrets;

use std::time::Duration;

use aim_core::coach::ChatModel;

pub use anthropic::AnthropicProvider;
pub use cli::CliDelegateProvider;
pub use config::{CoachConfig, PromptInput, ProviderKind};
pub use http::{ReqwestTransport, Transport};
pub use openai::OpenAiCompatProvider;
pub use secrets::{
    ApiKey, EnvStore, KeySource, KeyringStore, SecretError, SecretStore, lookup_key,
};

/// The provider `config` describes, ready to use. `key` is the API key found with
/// [`lookup_key`]; Anthropic requires one, OpenAI-compatible servers take one when they
/// need it, and the CLI delegate ignores it. Every problem is returned at once.
pub fn build_model(
    config: &CoachConfig,
    key: Option<ApiKey>,
) -> Result<Box<dyn ChatModel + Send>, Vec<String>> {
    if !config.enabled {
        return Err(vec![
            "the coach is off; enable it in the coach settings".to_owned(),
        ]);
    }
    config.validate()?;
    let key = key.filter(|k| !k.is_empty());
    let request_timeout = Duration::from_secs(config.request_timeout_secs);
    let transport = || {
        ReqwestTransport::new(
            Duration::from_secs(config.connect_timeout_secs),
            request_timeout,
        )
        .map(|t| Box::new(t) as Box<dyn Transport>)
        .map_err(|e| vec![e])
    };
    let base_url = config.effective_base_url().unwrap_or_default();
    Ok(match config.provider {
        ProviderKind::OpenaiCompatible => Box::new(OpenAiCompatProvider::new(
            &base_url,
            &config.model,
            key,
            config.max_tokens,
            transport()?,
        )),
        ProviderKind::Anthropic => {
            let Some(key) = key else {
                return Err(vec![
                    "no Anthropic API key: add one in the coach settings (it is kept in the \
                     OS keyring) or set the environment variable named in api_key_env"
                        .to_owned(),
                ]);
            };
            Box::new(AnthropicProvider::new(
                &base_url,
                &config.model,
                key,
                config.max_tokens,
                transport()?,
            ))
        }
        ProviderKind::CliDelegate => Box::new(CliDelegateProvider::new(
            config.cli_command.clone(),
            config.cli_prompt,
            request_timeout,
        )),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled(provider: ProviderKind) -> CoachConfig {
        CoachConfig {
            enabled: true,
            provider,
            model: "m".into(),
            base_url: Some(config::OLLAMA_BASE_URL.into()),
            cli_command: cli::claude_command(),
            ..CoachConfig::default()
        }
    }

    fn errors(result: Result<Box<dyn ChatModel + Send>, Vec<String>>) -> Vec<String> {
        match result {
            Err(errors) => errors,
            Ok(_) => panic!("expected errors"),
        }
    }

    #[test]
    fn a_disabled_or_invalid_config_builds_nothing() {
        let off = CoachConfig {
            enabled: false,
            ..enabled(ProviderKind::OpenaiCompatible)
        };
        assert!(errors(build_model(&off, None))[0].contains("off"));
        let invalid = CoachConfig {
            model: String::new(),
            ..enabled(ProviderKind::OpenaiCompatible)
        };
        assert!(errors(build_model(&invalid, None))[0].starts_with("model"));
    }

    #[test]
    fn anthropic_needs_a_key_and_the_others_build_without_one() {
        let anthropic = CoachConfig {
            base_url: None,
            ..enabled(ProviderKind::Anthropic)
        };
        assert!(errors(build_model(&anthropic, None))[0].contains("no Anthropic API key"));
        assert!(
            build_model(&anthropic, Some(ApiKey::new("  "))).is_err(),
            "blank is no key"
        );
        assert!(build_model(&anthropic, Some(ApiKey::new("k"))).is_ok());
        assert!(build_model(&enabled(ProviderKind::OpenaiCompatible), None).is_ok());
        assert!(build_model(&enabled(ProviderKind::CliDelegate), None).is_ok());
    }
}
