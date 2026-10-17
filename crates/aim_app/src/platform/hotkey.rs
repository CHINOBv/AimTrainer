//! Global hotkey for showing/hiding the overlay on Windows and X11. Wayland has no global
//! shortcuts for clients; there the compositor runs `aimtrainer toggle` (see `ipc`).

#[cfg(any(windows, target_os = "linux"))]
mod imp {
    use crate::overlay::{Overlay, QuickLaunch};
    use crate::platform::ipc::Command;
    use crate::platform::is_wayland;
    use bevy::prelude::*;
    use global_hotkey::hotkey::HotKey;
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

    struct Registered {
        _manager: GlobalHotKeyManager,
        /// Hotkey id and the command it sends.
        ids: Vec<(u32, Command)>,
    }

    pub fn install(app: &mut App, specs: &[(String, Command)]) {
        if is_wayland() {
            info!(
                "Wayland session: bind `aimtrainer toggle`, `aimtrainer warmup` and `aimtrainer micro` in your compositor"
            );
            return;
        }
        let manager = match GlobalHotKeyManager::new() {
            Ok(m) => m,
            Err(e) => {
                warn!("global hotkeys unavailable: {e}");
                return;
            }
        };
        let mut ids = Vec::new();
        for (spec, cmd) in specs.iter().filter(|(s, _)| !s.trim().is_empty()) {
            match spec.parse::<HotKey>() {
                Ok(hotkey) => match manager.register(hotkey) {
                    Ok(()) => {
                        info!("hotkey {spec} → {cmd:?}");
                        ids.push((hotkey.id(), *cmd));
                    }
                    Err(e) => warn!("could not register hotkey '{spec}': {e}"),
                },
                Err(e) => warn!("invalid hotkey '{spec}': {e}"),
            }
        }
        if ids.is_empty() {
            return;
        }
        app.insert_non_send(Registered {
            _manager: manager,
            ids,
        })
        .add_systems(PreUpdate, poll);
    }

    fn poll(
        registered: NonSend<Registered>,
        mut overlay: ResMut<Overlay>,
        mut quick: ResMut<QuickLaunch>,
    ) {
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state != HotKeyState::Pressed {
                continue;
            }
            if let Some((_, cmd)) = registered.ids.iter().find(|(id, _)| *id == event.id) {
                overlay.apply(*cmd);
                quick.request(*cmd);
            }
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    pub fn install(
        _app: &mut bevy::prelude::App,
        _specs: &[(String, crate::platform::ipc::Command)],
    ) {
    }
}

pub use imp::install;
