//! The trainer's MCP server: the AI coach's tool registry ([`aim_core::coach::tools`]) and
//! the built-in scenarios, served over stdio to coding agents (Claude Code, Codex, OpenCode).
//!
//! An adapter: it reads the player's files, builds the core's [`aim_core::coach::ToolEnv`]
//! and forwards every tool call to the core, so the tools behave exactly as in the app.

pub mod paths;
pub mod player;
pub mod server;

pub use paths::Paths;
pub use server::CoachServer;
