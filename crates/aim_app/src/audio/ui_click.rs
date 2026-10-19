//! Menu click sounds. Widgets note a click in the egui context's memory with [`clicked`];
//! the sound system takes the note once per frame and plays one click (see
//! `super::play_ui_clicks`). Keeping the note in egui memory needs no Bevy access from
//! the widgets.

use bevy_egui::egui;

fn key() -> egui::Id {
    egui::Id::new("aimtrainer.ui_click")
}

/// Note that a menu button was clicked this frame.
pub fn note(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(key(), true));
}

/// Whether a click was noted since the last call; clears the note.
pub fn take(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| d.remove_temp::<bool>(key()))
        .unwrap_or(false)
}

/// Pass a widget's response through, noting a click when it was clicked.
pub fn clicked(response: egui::Response) -> egui::Response {
    if response.clicked() {
        note(&response.ctx);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_noted_click_is_taken_once() {
        let ctx = egui::Context::default();
        assert!(!take(&ctx));
        note(&ctx);
        note(&ctx);
        assert!(take(&ctx), "several clicks in a frame play one sound");
        assert!(!take(&ctx));
    }
}
