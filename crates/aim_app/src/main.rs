//! CustomAimTrainer: VALORANT-focused aim trainer with a toggleable overlay.
//!
//! Usage: see `cli::USAGE` (`aimtrainer --help`).

mod cli;
mod game;
mod overlay;
mod persistence;
mod platform;
mod ui;

use persistence::{history, library, settings};
use platform::ipc;

use bevy::prelude::*;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::window::{
    CompositeAlphaMode, CursorOptions, MonitorSelection, PresentMode, WindowLevel, WindowMode,
};
use bevy_egui::EguiPlugin;
use std::sync::Mutex;

fn main() -> AppExit {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(exit) = cli::run(&args) {
        return exit;
    }

    let paths = settings::Paths::resolve();
    let settings = settings::Settings::load(&paths);
    let ipc_rx = match ipc::listen() {
        Ok(rx) => rx,
        Err(e) => {
            eprintln!("{e}");
            return AppExit::error();
        }
    };

    let window = Window {
        title: "CustomAimTrainer".into(),
        name: Some("aimtrainer".into()), // Wayland app_id / X11 class, for compositor rules
        // Uncapped frame rate without vsync: lowest input-to-photon latency.
        present_mode: PresentMode::AutoNoVsync,
        mode: if settings.fullscreen {
            WindowMode::BorderlessFullscreen(MonitorSelection::Current)
        } else {
            WindowMode::Windowed
        },
        transparent: true,
        decorations: !settings.fullscreen,
        window_level: if settings.always_on_top {
            WindowLevel::AlwaysOnTop
        } else {
            WindowLevel::Normal
        },
        composite_alpha_mode: if cfg!(target_os = "linux") {
            CompositeAlphaMode::PreMultiplied
        } else {
            CompositeAlphaMode::Auto
        },
        ..default()
    };

    let mut app = App::new();
    app.insert_resource(overlay::background(&settings, true))
        .add_plugins(
            DefaultPlugins
                .build()
                // Pipelined rendering adds a frame of latency; aim training wants it gone.
                .disable::<PipelinedRenderingPlugin>()
                .set(WindowPlugin {
                    primary_window: Some(window),
                    primary_cursor_options: Some(CursorOptions::default()),
                    ..default()
                }),
        )
        .add_plugins(EguiPlugin::default())
        .insert_resource(library::Library::load(&paths))
        .insert_resource(history::History::load(&paths))
        .insert_resource(persistence::method_progress::MethodProgress::load(&paths))
        .insert_resource(persistence::adaptive::AdaptiveLevels::load(&paths))
        .add_plugins((
            overlay::OverlayPlugin {
                hotkeys: vec![
                    (settings.overlay_hotkey.clone(), ipc::Command::Toggle),
                    (
                        settings.warmup_hotkey.clone(),
                        ipc::Command::Warmup(settings.warmup_minutes),
                    ),
                    (settings.micro_hotkey.clone(), ipc::Command::Micro),
                ],
            },
            game::GamePlugin,
            persistence::method_progress::MethodProgressPlugin,
            ui::UiPlugin,
        ))
        .insert_resource(settings)
        .insert_resource(paths);
    if let Some(rx) = ipc_rx {
        app.insert_resource(ipc::IpcReceiver(Mutex::new(rx)));
    }
    app.run()
}
