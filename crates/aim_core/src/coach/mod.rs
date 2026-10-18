//! The AI coach, without I/O: a compact summary of the player's analyses for a language
//! model ([`context`]), a coaching knowledge base ([`knowledge`]) and drill generation
//! ([`drills`]).
//!
//! Network clients, key storage and memory stores are adapters outside the core.

pub mod context;
pub mod drills;
pub mod knowledge;

pub use context::{CoachContext, ContextInputs};
pub use knowledge::{Diagnosis, SubSkill, diagnose};
