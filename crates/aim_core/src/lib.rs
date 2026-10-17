//! Engine-agnostic core of the aim trainer.
//!
//! Everything that decides how training *behaves* lives here (sensitivity, FOV, VALORANT
//! movement, scenarios, scoring) so it can be unit-tested without a window or GPU.
//! Front-ends only translate input into [`scenario::FrameInput`] and draw the targets.

pub mod benchmark;
pub mod fov;
pub mod math;
pub mod movement;
pub mod rng;
pub mod routine;
pub mod scenario;
pub mod sensitivity;
pub mod sim;
pub mod valorant;
