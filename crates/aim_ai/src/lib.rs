//! Adapters for the AI coach in `aim_core::coach`.
//!
//! The core defines the ports ([`aim_core::coach::ChatModel`] and
//! [`aim_core::coach::CoachMemory`]); this crate implements them with I/O:
//!
//! - [`config`]: what the player chose (provider, model, base URL, timeouts), validated
//!   without I/O. Off by default.
//! - [`secrets`]: API keys in the OS keyring, with an environment-variable fallback. Keys
//!   are never written to settings files and never logged.
//! - [`openai`] and [`anthropic`]: chat providers over HTTP, through the [`http`] seam.
//!
//! All network and process work is blocking: call it off the frame (a worker thread).

pub mod anthropic;
pub mod config;
pub mod http;
pub mod openai;
pub mod secrets;

pub use anthropic::AnthropicProvider;
pub use config::{CoachConfig, PromptInput, ProviderKind};
pub use http::{ReqwestTransport, Transport};
pub use openai::OpenAiCompatProvider;
pub use secrets::{
    ApiKey, EnvStore, KeySource, KeyringStore, SecretError, SecretStore, lookup_key,
};
