//! Show/hide overlay. Hidden means: nothing is drawn, the window is click-through (or
//! hidden where the platform allows it), the cursor is released and the run is paused,
//! so the game or desktop underneath gets all input.
//!
//! Toggle sources:
//! - Windows / X11: a global hotkey (`global-hotkey` crate).
//! - Linux (any session, required on Wayland): `aimtrainer toggle` over IPC, typically
//!   bound in the compositor, e.g. Hyprland `bind = CTRL SHIFT, F10, exec, aimtrainer toggle`.

use crate::persistence::settings::Settings;
use crate::platform::ipc::{Command, IpcReceiver};
use crate::platform::{self, is_wayland};
use bevy::prelude::*;
use bevy::window::{CursorOptions, PrimaryWindow};

#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Overlay {
    pub visible: bool,
}

impl Overlay {
    pub fn apply(&mut self, cmd: Command) {
        self.visible = match cmd {
            Command::Toggle => !self.visible,
            Command::Show => true,
            Command::Hide => false,
        };
    }
}

pub struct OverlayPlugin {
    pub hotkey: String,
}

impl Plugin for OverlayPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Overlay { visible: true })
            .add_systems(PreUpdate, poll_ipc)
            .add_systems(
                PostUpdate,
                apply_overlay.run_if(resource_changed::<Overlay>),
            );
        platform::hotkey::install(app, &self.hotkey);
    }
}

fn poll_ipc(rx: Option<Res<IpcReceiver>>, mut overlay: ResMut<Overlay>) {
    let Some(rx) = rx else { return };
    let Ok(rx) = rx.0.lock() else { return };
    while let Ok(cmd) = rx.try_recv() {
        overlay.apply(cmd);
    }
}

fn apply_overlay(
    overlay: Res<Overlay>,
    settings: Res<Settings>,
    mut clear: ResMut<ClearColor>,
    mut windows: Query<(&mut Window, &mut CursorOptions), With<PrimaryWindow>>,
    mut cameras: Query<&mut Camera>,
) {
    for mut cam in &mut cameras {
        cam.is_active = overlay.visible;
    }
    *clear = background(&settings, overlay.visible);
    let Ok((mut window, mut cursor)) = windows.single_mut() else {
        return;
    };
    cursor.hit_test = overlay.visible;
    if !overlay.visible {
        cursor.grab_mode = bevy::window::CursorGrabMode::None;
        cursor.visible = true;
    }
    // Wayland clients cannot hide or raise themselves; there the window stays mapped,
    // fully transparent and click-through.
    if !is_wayland() {
        window.visible = overlay.visible;
    }
}

pub fn background(settings: &Settings, visible: bool) -> ClearColor {
    let alpha = if visible {
        settings.background_opacity.clamp(0.0, 1.0)
    } else {
        0.0
    };
    // Premultiplied alpha: scale the color with its alpha.
    let base = 0.06 * alpha;
    ClearColor(Color::srgba(base, base, base * 1.2, alpha))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_drive_visibility() {
        let mut o = Overlay { visible: true };
        o.apply(Command::Toggle);
        assert!(!o.visible);
        o.apply(Command::Toggle);
        assert!(o.visible);
        o.apply(Command::Hide);
        o.apply(Command::Hide);
        assert!(!o.visible);
        o.apply(Command::Show);
        assert!(o.visible);
    }

    #[test]
    fn hidden_background_is_fully_transparent() {
        let s = Settings::default();
        assert_eq!(background(&s, false).0.alpha(), 0.0);
        assert!((background(&s, true).0.alpha() - s.background_opacity).abs() < 1e-6);
    }
}
