//! Operating-system specifics: the single-instance IPC channel and global hotkeys.

pub mod frame_stats;
pub mod hotkey;
pub mod hotkey_spec;
pub mod ipc;

/// True when running inside a Wayland session (Linux).
pub fn is_wayland() -> bool {
    cfg!(target_os = "linux") && std::env::var_os("WAYLAND_DISPLAY").is_some()
}
