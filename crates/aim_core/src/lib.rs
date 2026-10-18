//! Engine-agnostic core of the aim trainer.
//!
//! Everything that decides how training *behaves* lives here (sensitivity, FOV, VALORANT
//! movement, scenarios, scoring) so it can be unit-tested without a window or GPU.
//! Front-ends only translate input into [`scenario::FrameInput`] and draw the targets.

#![warn(missing_docs)]

pub mod activity;
pub mod analytics;
pub mod coach;
pub mod crosshair;
pub mod fov;
pub mod games;
pub mod math;
pub mod movement;
pub mod profile;
pub mod rng;
pub mod scenario;
pub mod sensitivity;
pub mod sim;
pub mod training;
