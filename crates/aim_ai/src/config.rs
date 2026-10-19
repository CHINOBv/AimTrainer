//! The player's coach settings: which provider, model and endpoint to use. Pure data and
//! validation; the provider modules build a client from it.
//!
//! API keys are deliberately not a field: they live in a [`crate::SecretStore`]. With
//! `deny_unknown_fields`, a settings file that contains `api_key` fails to load instead of
//! silently keeping a secret in plain text.

use serde::{Deserialize, Serialize};

/// OpenAI's Chat Completions endpoint.
pub const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
/// OpenCode's pay-as-you-go (Console key) OpenAI-compatible endpoint.
pub const OPENCODE_BASE_URL: &str = "https://opencode.ai/inference/openai/v1";
/// A local Ollama server.
pub const OLLAMA_BASE_URL: &str = "http://localhost:11434/v1";
/// A local LM Studio server.
pub const LM_STUDIO_BASE_URL: &str = "http://localhost:1234/v1";
/// Anthropic's API.
pub const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";

/// Longest accepted request timeout, in seconds.
pub const MAX_REQUEST_TIMEOUT_SECS: u64 = 600;
/// Longest accepted connect timeout, in seconds.
pub const MAX_CONNECT_TIMEOUT_SECS: u64 = 60;
/// Largest accepted `max_tokens` (the output cap of current Claude models).
pub const MAX_OUTPUT_TOKENS: u32 = 128_000;

/// Which kind of provider answers the coach.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    /// Any OpenAI-compatible Chat Completions server (OpenAI, OpenCode, Ollama, LM Studio).
    #[default]
    OpenaiCompatible,
    /// Anthropic's Messages API.
    Anthropic,
    /// The player's own installed CLI (`codex`, `claude`, `opencode`), text only.
    CliDelegate,
}

/// How the CLI delegate receives the prompt.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PromptInput {
    /// Written to the process's standard input (no command-line length limit).
    #[default]
    Stdin,
    /// Appended as the last command-line argument.
    Argument,
}

/// The coach settings one player chose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct CoachConfig {
    /// Whether the coach may send anything at all. Off until the player opts in.
    pub enabled: bool,
    /// Which provider answers.
    pub provider: ProviderKind,
    /// Endpoint root, e.g. [`OPENAI_BASE_URL`]. Required for `openai-compatible`;
    /// `anthropic` defaults to [`ANTHROPIC_BASE_URL`]; unused by `cli-delegate`.
    pub base_url: Option<String>,
    /// Model id as the provider names it. Unused by `cli-delegate`.
    pub model: String,
    /// Program and arguments for `cli-delegate`, e.g. `["claude", "-p", "--output-format",
    /// "json"]`.
    pub cli_command: Vec<String>,
    /// How `cli-delegate` receives the prompt.
    pub cli_prompt: PromptInput,
    /// Environment variable holding the API key when the keyring has none.
    pub api_key_env: Option<String>,
    /// Seconds to wait for a connection.
    pub connect_timeout_secs: u64,
    /// Seconds to wait for a whole reply (or for the CLI to exit).
    pub request_timeout_secs: u64,
    /// Output cap per reply, where the provider takes one.
    pub max_tokens: u32,
}

impl Default for CoachConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: ProviderKind::OpenaiCompatible,
            base_url: None,
            model: String::new(),
            cli_command: Vec::new(),
            cli_prompt: PromptInput::Stdin,
            api_key_env: None,
            connect_timeout_secs: 10,
            request_timeout_secs: 120,
            max_tokens: 16_000,
        }
    }
}

impl CoachConfig {
    /// The endpoint root to call: the configured one, or the provider's default.
    pub fn effective_base_url(&self) -> Option<String> {
        match (&self.base_url, self.provider) {
            (Some(url), _) => Some(url.trim_end_matches('/').to_owned()),
            (None, ProviderKind::Anthropic) => Some(ANTHROPIC_BASE_URL.to_owned()),
            (None, _) => None,
        }
    }

    /// Every problem with these settings, so the player can fix them all at once.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        match self.provider {
            ProviderKind::OpenaiCompatible | ProviderKind::Anthropic => {
                if self.model.trim().is_empty() {
                    errors.push("model: choose a model id".to_owned());
                }
                match self.effective_base_url() {
                    None => errors
                        .push("base_url: required for an OpenAI-compatible provider".to_owned()),
                    Some(url) => {
                        if let Err(e) = check_base_url(&url) {
                            errors.push(format!("base_url: {e}"));
                        }
                    }
                }
            }
            ProviderKind::CliDelegate => match self.cli_command.first() {
                None => errors.push(
                    "cli_command: name the program, e.g. [\"claude\", \"-p\", \
                     \"--output-format\", \"json\"]"
                        .to_owned(),
                ),
                Some(program) if program.trim().is_empty() => {
                    errors.push("cli_command: the program name is empty".to_owned())
                }
                Some(_) => {}
            },
        }
        if let Some(var) = &self.api_key_env
            && !is_env_var_name(var)
        {
            errors.push(format!(
                "api_key_env: {var:?} is not an environment variable name (letters, digits \
                 and underscores, not starting with a digit)"
            ));
        }
        if !(1..=MAX_CONNECT_TIMEOUT_SECS).contains(&self.connect_timeout_secs) {
            errors.push(format!(
                "connect_timeout_secs: must be 1 to {MAX_CONNECT_TIMEOUT_SECS}"
            ));
        }
        if !(1..=MAX_REQUEST_TIMEOUT_SECS).contains(&self.request_timeout_secs) {
            errors.push(format!(
                "request_timeout_secs: must be 1 to {MAX_REQUEST_TIMEOUT_SECS}"
            ));
        }
        if !(1..=MAX_OUTPUT_TOKENS).contains(&self.max_tokens) {
            errors.push(format!("max_tokens: must be 1 to {MAX_OUTPUT_TOKENS}"));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// HTTPS anywhere; plain HTTP only to this machine, so a key never crosses a network in
/// clear text.
fn check_base_url(url: &str) -> Result<(), String> {
    if let Some(rest) = url.strip_prefix("https://") {
        return if host(rest).is_empty() {
            Err("missing host".to_owned())
        } else {
            Ok(())
        };
    }
    if let Some(rest) = url.strip_prefix("http://") {
        return match host(rest) {
            "localhost" | "127.0.0.1" | "[::1]" => Ok(()),
            "" => Err("missing host".to_owned()),
            other => Err(format!(
                "plain http is only allowed to localhost; use https for {other}"
            )),
        };
    }
    Err("must start with https:// (or http://localhost for a local server)".to_owned())
}

/// The host of `authority/path` without the port (keeps `[::1]` whole).
fn host(rest: &str) -> &str {
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit('@').next().unwrap_or("");
    if authority.starts_with('[') {
        return authority
            .find(']')
            .map_or(authority, |end| &authority[..=end]);
    }
    authority.split(':').next().unwrap_or("")
}

fn is_env_var_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn openai() -> CoachConfig {
        CoachConfig {
            enabled: true,
            base_url: Some(OPENAI_BASE_URL.into()),
            model: "gpt-test".into(),
            ..CoachConfig::default()
        }
    }

    #[test]
    fn the_default_is_off() {
        let config = CoachConfig::default();
        assert!(!config.enabled);
        let parsed: CoachConfig = toml::from_str("").unwrap();
        assert_eq!(parsed, config, "an empty settings table is the default");
    }

    #[test]
    fn presets_for_every_endpoint_validate() {
        for url in [
            OPENAI_BASE_URL,
            OPENCODE_BASE_URL,
            OLLAMA_BASE_URL,
            LM_STUDIO_BASE_URL,
            "http://127.0.0.1:8080/v1",
            "http://[::1]:1234/v1",
        ] {
            let config = CoachConfig {
                base_url: Some(url.into()),
                ..openai()
            };
            assert_eq!(config.validate(), Ok(()), "{url}");
        }
        let anthropic = CoachConfig {
            provider: ProviderKind::Anthropic,
            base_url: None,
            model: "claude-sonnet-5-5".into(),
            ..CoachConfig::default()
        };
        assert_eq!(anthropic.validate(), Ok(()));
        assert_eq!(
            anthropic.effective_base_url().as_deref(),
            Some(ANTHROPIC_BASE_URL)
        );
        let cli = CoachConfig {
            provider: ProviderKind::CliDelegate,
            cli_command: vec!["codex".into(), "exec".into()],
            ..CoachConfig::default()
        };
        assert_eq!(
            cli.validate(),
            Ok(()),
            "a CLI delegate needs no model or URL"
        );
    }

    #[test]
    fn plain_http_to_another_host_is_refused() {
        let config = CoachConfig {
            base_url: Some("http://example.com/v1".into()),
            ..openai()
        };
        let errors = config.validate().unwrap_err();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert!(
            errors[0].contains("only allowed to localhost"),
            "{errors:?}"
        );
        let tricky = CoachConfig {
            base_url: Some("http://localhost.evil.com/v1".into()),
            ..openai()
        };
        assert!(tricky.validate().is_err(), "a lookalike host is not local");
        let userinfo = CoachConfig {
            base_url: Some("http://localhost@evil.com/v1".into()),
            ..openai()
        };
        assert!(
            userinfo.validate().is_err(),
            "userinfo does not make it local"
        );
        let ftp = CoachConfig {
            base_url: Some("ftp://example.com".into()),
            ..openai()
        };
        assert!(ftp.validate().unwrap_err()[0].contains("https://"));
    }

    #[test]
    fn every_problem_is_reported_at_once() {
        let config = CoachConfig {
            base_url: None,
            model: " ".into(),
            api_key_env: Some("1BAD-NAME".into()),
            connect_timeout_secs: 0,
            request_timeout_secs: 601,
            max_tokens: 0,
            ..CoachConfig::default()
        };
        let errors = config.validate().unwrap_err();
        let fields: Vec<&str> = errors
            .iter()
            .map(|e| e.split(':').next().unwrap())
            .collect();
        assert_eq!(
            fields,
            [
                "model",
                "base_url",
                "api_key_env",
                "connect_timeout_secs",
                "request_timeout_secs",
                "max_tokens"
            ]
        );
    }

    #[test]
    fn a_cli_delegate_needs_a_program() {
        let config = CoachConfig {
            provider: ProviderKind::CliDelegate,
            ..CoachConfig::default()
        };
        assert!(config.validate().unwrap_err()[0].starts_with("cli_command"));
        let blank = CoachConfig {
            cli_command: vec!["  ".into()],
            ..config
        };
        assert!(blank.validate().unwrap_err()[0].contains("empty"));
    }

    #[test]
    fn a_key_in_the_settings_file_is_rejected() {
        let text = "enabled = true\nmodel = \"m\"\napi_key = \"sk-secret\"\n";
        let err = toml::from_str::<CoachConfig>(text).unwrap_err();
        assert!(err.to_string().contains("api_key"), "{err}");
    }

    #[test]
    fn settings_round_trip_with_kebab_case_kinds() {
        let text = "enabled = true\nprovider = \"cli-delegate\"\ncli_command = [\"opencode\", \
                    \"run\", \"--format\", \"json\"]\ncli_prompt = \"argument\"\n";
        let config: CoachConfig = toml::from_str(text).unwrap();
        assert_eq!(config.provider, ProviderKind::CliDelegate);
        assert_eq!(config.cli_prompt, PromptInput::Argument);
        let back: CoachConfig = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
        assert_eq!(back, config);
    }
}
