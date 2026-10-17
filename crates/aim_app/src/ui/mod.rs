//! egui front-end, organised by atomic design: `atoms` → `molecules` → `organisms` → `pages`.
//!
//! Pages are containers: Bevy systems that read ECS state, hand plain data to the views and
//! apply the [`UiAction`]s they return through [`navigation::Navigator`]. Everything below
//! `pages` is presentational and never touches the ECS.

mod atoms;
mod molecules;
mod navigation;
mod organisms;
mod pages;

use bevy::prelude::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            pages::menu::MenuPage,
            pages::hud::HudPage,
            pages::results::ResultsPage,
        ));
    }
}

/// Something the player asked for in a view. Pages apply it.
#[derive(Debug, Clone, PartialEq)]
pub enum UiAction {
    StartScenario(String),
    StartRoutine(String),
    /// Start a routine at a difficulty level, from a given run of the plan made with `seed`
    /// (resuming a split method day).
    StartRoutineAt {
        routine: String,
        level: f32,
        index: usize,
        seed: u64,
    },
    NextRoutineRun,
    Retry(String),
    Resume,
    BackToMenu,
    Quit,
}
