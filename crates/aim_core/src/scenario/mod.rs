//! Scenarios: data definitions, the runtime session, and the built-in library.

pub mod builtin;
pub mod def;
pub mod difficulty;
pub mod hit;
mod motion;
mod recoil;
pub mod recording;
mod scoring;
pub mod session;
mod spawn;
mod weapon;

/// Lowest height a sphere target's center may reach, shared by spawning and motion.
const MIN_SPHERE_HEIGHT: f32 = 0.3;

pub use def::{Category, FireMode, Motion, ScenarioDef, Shape, SpawnArea, ValidationError};
pub use difficulty::{
    DifficultyDef, Knob, RunKey, Tuning, parse_run_key, parse_run_key_full, run_key,
};
pub use hit::HitZone;
pub use recording::Recording;
pub use session::{FrameInput, RunResult, Session, SessionEvent, Stats, Target};
pub use weapon::shot_direction;
