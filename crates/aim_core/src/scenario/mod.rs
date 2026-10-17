//! Scenarios: data definitions, the runtime session, and the built-in library.

pub mod builtin;
pub mod def;
pub mod hit;
pub mod session;

pub use def::{Category, FireMode, Motion, ScenarioDef, Shape, SpawnArea, ValidationError};
pub use hit::HitZone;
pub use session::{FrameInput, RunResult, Session, SessionEvent, Stats, Target};
