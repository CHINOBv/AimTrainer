//! Global hotkeys for showing/hiding the overlay and quick sessions on Windows and X11.
//! Wayland has no global shortcuts for clients; there the compositor runs
//! `aimtrainer toggle` (see `ipc`).
//!
//! The registered set follows [`Settings`] live: changing a hotkey in the menu unregisters
//! the old combination and registers the new one, no restart needed.

use crate::persistence::settings::Settings;
use crate::platform::ipc::Command;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// Which hotkey slot the settings UI is recording, if any. While set, every global hotkey
/// is unregistered so the OS hands the pressed combination to the window instead.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct HotkeyCapture {
    pub recording: Option<usize>,
    /// Seconds the current recording has been waiting.
    waited_secs: f32,
}

impl HotkeyCapture {
    /// Longest wait for a key press before recording gives up.
    pub const TIMEOUT_SECS: f32 = 10.0;

    /// Advance the recording clock by `dt` seconds. Recording ends after
    /// [`Self::TIMEOUT_SECS`] or as soon as the window loses focus, so global hotkeys are
    /// never left unregistered.
    pub fn tick(&mut self, dt: f32, focused: bool) {
        if self.recording.is_none() {
            self.waited_secs = 0.0;
            return;
        }
        self.waited_secs += dt;
        if !focused || self.waited_secs >= Self::TIMEOUT_SECS {
            *self = Self::default();
        }
    }
}

/// End a recording that timed out or whose window lost focus.
fn expire_recording(
    mut capture: ResMut<HotkeyCapture>,
    time: Res<Time>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    if capture.recording.is_none() && capture.waited_secs == 0.0 {
        return;
    }
    let focused = windows.single().is_ok_and(|w| w.focused);
    capture.tick(time.delta_secs(), focused);
}

/// Shared part of `install`: the capture state and its expiry run on every platform.
fn install_capture(app: &mut App) {
    app.init_resource::<HotkeyCapture>()
        .add_systems(PreUpdate, expire_recording);
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
    use super::{HotkeyCapture, expire_recording, install_capture, specs};
    use crate::overlay::{Overlay, QuickLaunch};
    use crate::persistence::settings::Settings;
    use crate::platform::hotkey_spec;
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
        install_capture(app);
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
        .add_systems(PreUpdate, (sync, poll).chain().after(expire_recording));
    }

    /// Re-register when the wanted hotkeys change (settings edited, recording started or
    /// stopped).
    fn sync(mut reg: NonSendMut<Registered>, settings: Res<Settings>, capture: Res<HotkeyCapture>) {
        let wanted =
            hotkey_spec::registrable(&specs(&settings), capture.recording.is_some(), is_known);
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
    use super::install_capture;

    pub fn is_known(spec: &str) -> bool {
        crate::platform::hotkey_spec::Combo::parse(spec).is_ok()
    }

    pub fn install(app: &mut bevy::prelude::App) {
        install_capture(app);
    }
}

pub use imp::{install, is_known};

#[cfg(test)]
mod tests {
    use super::*;

    fn recording(slot: usize) -> HotkeyCapture {
        HotkeyCapture {
            recording: Some(slot),
            ..default()
        }
    }

    #[test]
    fn recording_times_out() {
        let mut c = recording(1);
        c.tick(HotkeyCapture::TIMEOUT_SECS - 0.5, true);
        assert_eq!(c.recording, Some(1));
        c.tick(1.0, true);
        assert_eq!(c.recording, None);
        // A new recording starts with a fresh clock.
        c.recording = Some(2);
        c.tick(1.0, true);
        assert_eq!(c.recording, Some(2));
    }

    #[test]
    fn recording_ends_when_the_window_loses_focus() {
        let mut c = recording(0);
        c.tick(0.016, false);
        assert_eq!(c.recording, None);
    }

    #[test]
    fn idle_clock_stays_at_zero() {
        let mut c = HotkeyCapture::default();
        c.tick(5.0, false);
        assert_eq!(c, HotkeyCapture::default());
    }
}
