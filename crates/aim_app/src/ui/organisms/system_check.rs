//! System check: things that silently ruin aim training (low FPS, vsync, acceleration).

use crate::ui::atoms::{BAD, GOOD, WARN};
use bevy_egui::egui;

pub struct SystemModel {
    /// `(average fps, 1% low, frame time ms)`.
    pub frames: Option<(f32, f32, f32)>,
    pub vsync_off: bool,
    pub fullscreen: bool,
    pub wayland: bool,
    pub windows: bool,
}

pub fn system_check(ui: &mut egui::Ui, m: &SystemModel) {
    ui.heading("System check");
    match m.frames {
        Some((fps, low, ms)) => {
            let color = if low >= 144.0 {
                GOOD
            } else if low >= 60.0 {
                WARN
            } else {
                BAD
            };
            ui.colored_label(
                color,
                format!("{fps:.0} FPS average · {low:.0} FPS 1% low · {ms:.2} ms per frame"),
            );
            if low < 144.0 {
                ui.small("1% lows under your monitor's refresh rate add visible stutter; close background apps or lower the window size.");
            }
        }
        None => {
            ui.label("Measuring frame times…");
        }
    }
    let check = |ui: &mut egui::Ui, ok: bool, text: &str| {
        ui.colored_label(
            if ok { GOOD } else { WARN },
            format!("{} {text}", if ok { "✔" } else { "!" }),
        );
    };
    check(
        ui,
        m.vsync_off,
        "Vsync off (uncapped frames, lowest input latency)",
    );
    check(
        ui,
        m.fullscreen,
        "Borderless fullscreen (consistent frame pacing)",
    );
    ui.add_space(4.0);
    ui.label("Mouse input is read raw (unaccelerated counts); confirm with the DPI check above.");
    if m.windows {
        ui.label("Windows: turn off \"Enhance pointer precision\" for the desktop; the trainer and VALORANT use raw input either way.");
    }
    if m.wayland {
        ui.label("Wayland: raw counts come from the relative-pointer protocol, unaffected by compositor acceleration.");
    }
}
