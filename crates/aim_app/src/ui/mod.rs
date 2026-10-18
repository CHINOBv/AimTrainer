//! egui front-end, organised by atomic design: `atoms` → `molecules` → `organisms` → `pages`.
//!
//! Pages are containers: Bevy systems that read ECS state, hand plain data to the views and
//! apply the [`UiAction`]s they return through [`navigation::Navigator`]. Everything below
//! `pages` is presentational and never touches the ECS.

mod atoms;
mod launcher;
mod molecules;
mod navigation;
mod organisms;
mod pages;

use aim_core::sensitivity::Sensitivity;
use aim_core::sensitivity::finder::Slot;
use aim_core::training::routine::RoutineDef;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};

pub use organisms::progress_panel::{
    fatigue_text as progress_fatigue_text, goal_text as progress_goal_text,
};

pub struct UiPlugin;

/// Scenario or arena to open as soon as the trainer starts (`aimtrainer play <id>`).
#[derive(Resource, Default)]
pub struct StartupPlay(pub Option<String>);

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StartupPlay>()
            .init_resource::<crate::platform::frame_stats::FrameStats>()
            .add_systems(Update, crate::platform::frame_stats::track_frames)
            .add_systems(EguiPrimaryContextPass, install_fonts)
            .add_systems(
                Update,
                (launcher::launch_requested, launcher::launch_startup),
            )
            .add_plugins((
                pages::menu::MenuPage,
                pages::hud::HudPage,
                pages::results::ResultsPage,
            ));
    }
}

/// Installs the UI fonts on the primary egui context, once.
fn install_fonts(mut contexts: EguiContexts, mut done: Local<bool>) -> Result {
    if !*done {
        contexts
            .ctx_mut()?
            .set_fonts(atoms::fonts::font_definitions());
        *done = true;
    }
    Ok(())
}

/// Something the player asked for in a view. Pages apply it.
#[derive(Debug, Clone, PartialEq)]
pub enum UiAction {
    StartScenario(String),
    StartRoutine(String),
    /// Play a scenario at its adaptive difficulty level; the result moves the level.
    StartAdaptive(String),
    /// Play a routine that is not in the library (e.g. the personalized playlist).
    StartRoutineDef(Box<RoutineDef>),
    /// Play a built-in arena (deathmatch or angle clearing on an original map).
    StartArena(String),
    /// Play a built-in arena as practice: bots move as usual but never shoot.
    StartArenaPractice(String),
    /// One sensitivity-finder test at `sens` (computed by the page from the finder).
    StartFinderTest {
        slot: Slot,
        sens: Sensitivity,
    },
    /// Today's daily challenge: fixed seed and a history key of its own.
    StartChallenge {
        scenario: String,
        seed: u64,
        key: String,
    },
    /// Play a scenario shortened to `secs`; `auto_hide` hides the overlay when it ends
    /// (buy-phase micro drills hand the mouse straight back to the game).
    StartTimed {
        scenario: String,
        secs: f32,
        auto_hide: bool,
    },
    /// Start a routine at a difficulty level, from a given run of the plan made with `seed`
    /// (resuming a split method day).
    StartRoutineAt {
        routine: String,
        level: f32,
        index: usize,
        seed: u64,
    },
    NextRoutineRun,
    /// Restart the current scenario run with the player's floor at this height (meters),
    /// keeping its difficulty level, duration and routine position.
    RestartAtElevation(f32),
    Retry(String),
    Resume,
    BackToMenu,
    Quit,
}
