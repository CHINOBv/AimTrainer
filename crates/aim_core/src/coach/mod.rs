//! The AI coach, without I/O: a compact summary of the player's analyses for a language
//! model ([`context`]).
//!
//! Network clients, key storage and memory stores are adapters outside the core.

pub mod context;

pub use context::{CoachContext, ContextInputs};
