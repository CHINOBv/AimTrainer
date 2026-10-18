//! Adapters for the AI coach in `aim_core::coach`.
//!
//! The core defines the ports ([`aim_core::coach::ChatModel`] and
//! [`aim_core::coach::CoachMemory`]); this crate implements them with I/O:
//!
//! - [`config`]: what the player chose (provider, model, base URL, timeouts), validated
//!   without I/O. Off by default.
//! - [`secrets`]: API keys in the OS keyring, with an environment-variable fallback. Keys
//!   are never written to settings files and never logged.
//!
//! All network and process work is blocking: call it off the frame (a worker thread).

pub mod config;
pub mod secrets;

pub use config::{CoachConfig, PromptInput, ProviderKind};
pub use secrets::{
    ApiKey, EnvStore, KeySource, KeyringStore, SecretError, SecretStore, lookup_key,
};
