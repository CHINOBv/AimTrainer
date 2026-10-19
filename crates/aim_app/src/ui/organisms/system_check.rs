//! System check: things that silently ruin aim training (low FPS, vsync, acceleration).

use crate::i18n::{tr, tr_named};
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
    ui.heading(tr("System check"));
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
                tr_named(
                    "{fps} FPS average · {low} FPS 1% low · {ms} ms per frame",
                    &[
                        ("fps", &format!("{fps:.0}")),
                        ("low", &format!("{low:.0}")),
                        ("ms", &format!("{ms:.2}")),
                    ],
                ),
            );
            if low < 144.0 {
                ui.small(tr("1% lows under your monitor's refresh rate add visible stutter; close background apps or lower the window size."));
            }
        }
        None => {
            ui.label(tr("Measuring frame times…"));
        }
    }
    let check = |ui: &mut egui::Ui, ok: bool, text: &str| {
        ui.colored_label(
            if ok { GOOD } else { WARN },
            format!("{} {}", if ok { "✔" } else { "!" }, tr(text)),
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
    ui.label(tr(
        "Mouse input is read raw (unaccelerated counts); confirm with the DPI check above.",
    ));
    if m.windows {
        ui.label(tr("Windows: turn off \"Enhance pointer precision\" for the desktop; the trainer and VALORANT use raw input either way."));
    }
    if m.wayland {
        ui.label(tr("Wayland: raw counts come from the relative-pointer protocol, unaffected by compositor acceleration."));
    }
}
