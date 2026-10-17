//! Scenarios: data definitions, the runtime session, and the built-in library.

pub mod builtin;
pub mod def;
pub mod hit;
mod motion;
mod scoring;
pub mod session;
mod spawn;
mod weapon;

pub use def::{Category, FireMode, Motion, ScenarioDef, Shape, SpawnArea, ValidationError};
pub use hit::HitZone;
pub use session::{FrameInput, RunResult, Session, SessionEvent, Stats, Target};
