//! CustomAimTrainer: VALORANT-focused aim trainer with a toggleable overlay.
//!
//! Usage:
//!   aimtrainer            start the trainer
//!   aimtrainer toggle     show/hide a running trainer (Linux; bind it in your compositor)
//!   aimtrainer show|hide

mod game;
mod history;
mod ipc;
mod library;
mod overlay;
mod settings;
mod ui;

use bevy::prelude::*;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::window::{
    CompositeAlphaMode, CursorOptions, MonitorSelection, PresentMode, WindowLevel, WindowMode,
};
use bevy_egui::EguiPlugin;
use std::sync::Mutex;

const USAGE: &str = "usage: aimtrainer [toggle|show|hide]";

fn main() -> AppExit {
    if let Some(arg) = std::env::args().nth(1) {
        return match ipc::Command::parse(&arg) {
            Some(cmd) => match ipc::send(cmd) {
                Ok(()) => AppExit::Success,
                Err(e) => {
                    eprintln!("{e}");
                    AppExit::error()
                }
            },
            None if arg == "-h" || arg == "--help" => {
                println!("{USAGE}");
                AppExit::Success
            }
            None => {
                eprintln!("unknown command '{arg}'\n{USAGE}");
                AppExit::error()
            }
        };
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
        .add_plugins((
            overlay::OverlayPlugin {
                hotkey: settings.overlay_hotkey.clone(),
            },
            game::GamePlugin,
            ui::UiPlugin,
        ))
        .insert_resource(settings)
        .insert_resource(paths);
    if let Some(rx) = ipc_rx {
        app.insert_resource(ipc::IpcReceiver(Mutex::new(rx)));
    }
    app.run()
}
