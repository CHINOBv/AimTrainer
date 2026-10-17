//! Global hotkey for showing/hiding the overlay on Windows and X11. Wayland has no global
//! shortcuts for clients; there the compositor runs `aimtrainer toggle` (see `ipc`).

#[cfg(any(windows, target_os = "linux"))]
mod imp {
    use crate::overlay::Overlay;
    use crate::platform::ipc::Command;
    use crate::platform::is_wayland;
    use bevy::prelude::*;
    use global_hotkey::hotkey::HotKey;
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

    struct Registered {
        _manager: GlobalHotKeyManager,
        id: u32,
    }

    pub fn install(app: &mut App, spec: &str) {
        if is_wayland() {
            info!(
                "Wayland session: bind `aimtrainer toggle` in your compositor to show/hide the overlay"
            );
            return;
        }
        let hotkey: HotKey = match spec.parse() {
            Ok(h) => h,
            Err(e) => {
                warn!("invalid overlay hotkey '{spec}': {e}");
                return;
            }
        };
        let manager = match GlobalHotKeyManager::new() {
            Ok(m) => m,
            Err(e) => {
                warn!("global hotkeys unavailable: {e}");
                return;
            }
        };
        if let Err(e) = manager.register(hotkey) {
            warn!("could not register overlay hotkey '{spec}': {e}");
            return;
        }
        info!("overlay hotkey: {spec}");
        app.insert_non_send(Registered {
            _manager: manager,
            id: hotkey.id(),
        })
        .add_systems(PreUpdate, poll);
    }

    fn poll(registered: NonSend<Registered>, mut overlay: ResMut<Overlay>) {
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.id == registered.id && event.state == HotKeyState::Pressed {
                overlay.apply(Command::Toggle);
            }
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    pub fn install(_app: &mut bevy::prelude::App, _spec: &str) {}
}

pub use imp::install;
