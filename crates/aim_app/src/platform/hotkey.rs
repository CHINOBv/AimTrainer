//! Global hotkeys for showing/hiding the overlay and quick sessions on Windows and X11.
//! Wayland has no global shortcuts for clients; there the compositor runs
//! `aimtrainer toggle` (see `ipc`).
//!
//! The registered set follows [`Settings`] live: changing a hotkey in the menu unregisters
//! the old combination and registers the new one, no restart needed.

use crate::persistence::settings::Settings;
use crate::platform::ipc::Command;
use bevy::prelude::*;

/// Which hotkey slot the settings UI is recording, if any. While set, every global hotkey
/// is unregistered so the OS hands the pressed combination to the window instead.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyCapture {
    pub recording: Option<usize>,
}

/// The hotkey specs in `settings` (empty means disabled) and the command each one sends,
/// in the order the settings UI lists them.
pub fn specs(settings: &Settings) -> [(&str, Command); 3] {
    [
        (settings.overlay_hotkey.as_str(), Command::Toggle),
        (
            settings.warmup_hotkey.as_str(),
            Command::Warmup(settings.warmup_minutes),
        ),
        (settings.micro_hotkey.as_str(), Command::Micro),
    ]
}

#[cfg(any(windows, target_os = "linux"))]
mod imp {
    use super::{HotkeyCapture, specs};
    use crate::overlay::{Overlay, QuickLaunch};
    use crate::persistence::settings::Settings;
    use crate::platform::ipc::Command;
    use crate::platform::is_wayland;
    use bevy::prelude::*;
    use global_hotkey::hotkey::HotKey;
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

    struct Registered {
        manager: GlobalHotKeyManager,
        /// Registered hotkeys and the command each one sends.
        active: Vec<(HotKey, Command)>,
        /// The wanted set `active` was built from; `None` before the first sync.
        applied: Option<Vec<(String, Command)>>,
    }

    /// True when the `global-hotkey` crate accepts `spec`.
    pub fn is_known(spec: &str) -> bool {
        spec.parse::<HotKey>().is_ok()
    }

    pub fn install(app: &mut App) {
        app.init_resource::<HotkeyCapture>();
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
        app.insert_non_send(Registered {
            manager,
            active: Vec::new(),
            applied: None,
        })
        .add_systems(PreUpdate, (sync, poll).chain());
    }

    /// Re-register when the wanted hotkeys change (settings edited, recording started or
    /// stopped).
    fn sync(mut reg: NonSendMut<Registered>, settings: Res<Settings>, capture: Res<HotkeyCapture>) {
        let wanted: Vec<(String, Command)> = if capture.recording.is_some() {
            Vec::new()
        } else {
            specs(&settings)
                .into_iter()
                .filter(|(s, _)| !s.trim().is_empty())
                .map(|(s, cmd)| (s.to_owned(), cmd))
                .collect()
        };
        if reg.applied.as_ref() == Some(&wanted) {
            return;
        }
        let reg = &mut *reg;
        for (hotkey, _) in reg.active.drain(..) {
            if let Err(e) = reg.manager.unregister(hotkey) {
                warn!("could not unregister hotkey {hotkey}: {e}");
            }
        }
        for (spec, cmd) in &wanted {
            match spec.parse::<HotKey>() {
                Ok(hotkey) => match reg.manager.register(hotkey) {
                    Ok(()) => {
                        info!("hotkey {spec} → {cmd:?}");
                        reg.active.push((hotkey, *cmd));
                    }
                    Err(e) => warn!("could not register hotkey '{spec}': {e}"),
                },
                Err(e) => warn!("invalid hotkey '{spec}': {e}"),
            }
        }
        reg.applied = Some(wanted);
    }

    fn poll(
        registered: NonSend<Registered>,
        capture: Res<HotkeyCapture>,
        mut overlay: ResMut<Overlay>,
        mut quick: ResMut<QuickLaunch>,
    ) {
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            // Drain stale events while recording; the combination being recorded must
            // not toggle the overlay.
            if event.state != HotKeyState::Pressed || capture.recording.is_some() {
                continue;
            }
            if let Some((_, cmd)) = registered
                .active
                .iter()
                .find(|(hotkey, _)| hotkey.id() == event.id)
            {
                overlay.apply(*cmd);
                quick.request(*cmd);
            }
        }
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    use super::HotkeyCapture;

    pub fn is_known(spec: &str) -> bool {
        crate::platform::hotkey_spec::Combo::parse(spec).is_ok()
    }

    pub fn install(app: &mut bevy::prelude::App) {
        app.init_resource::<HotkeyCapture>();
    }
}

pub use imp::{install, is_known};
