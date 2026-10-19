//! Movement Lab: configure a generated movement drill (enemy motion, count, spawns, the
//! player's area, weapon) and start it. Edits the settings in place; the container saves
//! them like any other setting.

use crate::i18n::tr;
use crate::ui::UiAction;
use aim_core::scenario::lab::{
    APPEAR_DELAY_SECS, AREA_DEPTH_M, AREA_HALF_WIDTH_M, CHANGE_SECS, DISTANCE_M, DURATION_SECS,
    DodgePreset, LAB_WEAPONS, LIFETIME_SECS, LabMotion, LabSettings, MAX_ALIVE, SPEED_PCT,
    SPREAD_M,
};
use bevy_egui::egui;

const TITLE: &str = "Movement Lab";
const INTRO: &str = "Build your own movement drill: how enemies move, how many, where and how often they appear, and how much room you have. Lab runs keep their own history and personal best for each set of settings; they never replace a built-in scenario's best.";
const ENEMIES: &str = "Enemies";
const SPAWNS: &str = "Spawns";
const PLAYER: &str = "You";
const WEAPON: &str = "Weapon and run";
const MOTION: &str = "Motion";
const SPEED: &str = "Speed";
const SPEED_HINT: &str = "Percent of the weapon's VALORANT run speed.";
const CHANGE: &str = "Direction change every";
const CHANGE_HINT: &str =
    "Mean time between direction decisions; each one waits a random 60–140% of it.";
const DODGE: &str = "Dodge style";
const DODGE_HINT: &str = "Light: few jukes, rare crouches. Medium: about Juke Heads. Heavy: mostly jukes, frequent crouches.";
const COUNT: &str = "At once";
const SHAPE: &str = "Shape";
const HEAD_ONLY: &str = "Head only";
const FULL_BODY: &str = "Full body";
const DISTANCE_MIN: &str = "Nearest";
const DISTANCE_MAX: &str = "Farthest";
const SPREAD: &str = "Lateral spread ±";
const DELAY: &str = "Appear delay up to";
const DELAY_HINT: &str = "Each enemy appears after a random delay between half of this and this.";
const UNTIL_KILLED: &str = "Stay until killed";
const LIFETIME: &str = "Leave after";
const MOVEMENT: &str = "Move with WASD";
const AREA_WIDTH: &str = "Area width ±";
const AREA_DEPTH: &str = "Area depth";
const SIMULATE_SPREAD: &str = "Simulate spread";
const SPREAD_HINT: &str = "First-shot spread and movement inaccuracy: shots fired while moving go wide (and lose points when you can move).";
const LENGTH: &str = "Run length";
const START: &str = "▶  Start";
const RESET: &str = "Reset";
const PB: &str = "Best with these settings";

/// The lab panel. `best` is the personal best of the drill the settings describe.
pub fn movement_lab(ui: &mut egui::Ui, s: &mut LabSettings, best: Option<f32>) -> Option<UiAction> {
    let mut action = None;
    egui::ScrollArea::vertical()
        .id_salt("movement-lab")
        .show(ui, |ui| {
            ui.heading(tr(TITLE));
            ui.label(tr(INTRO));
            ui.add_space(6.0);
            enemies(ui, s);
            spawns(ui, s);
            player(ui, s);
            weapon(ui, s);
            ui.separator();
            ui.label(egui::RichText::new(s.summary()).strong());
            if let Some(b) = best {
                ui.label(format!("{}: {b:.0}", tr(PB)));
            }
            ui.horizontal(|ui| {
                if crate::audio::ui_click::clicked(ui.button(tr(START))).clicked() {
                    action = Some(UiAction::StartLab(s.clamped()));
                }
                if ui.button(tr(RESET)).clicked() {
                    *s = LabSettings::default();
                }
            });
        });
    action
}

fn grid(ui: &mut egui::Ui, heading: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.strong(tr(heading));
    egui::Grid::new(heading)
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, add);
    ui.add_space(6.0);
}

fn f_slider<'a>(v: &'a mut f32, (lo, hi): (f32, f32), unit: &str) -> egui::Slider<'a> {
    egui::Slider::new(v, lo..=hi).suffix(unit)
}

fn enemies(ui: &mut egui::Ui, s: &mut LabSettings) {
    grid(ui, ENEMIES, |ui| {
        ui.label(tr(MOTION));
        ui.horizontal(|ui| {
            for m in LabMotion::ALL {
                ui.selectable_value(&mut s.motion, m, m.label());
            }
        });
        ui.end_row();
        let moving = s.motion != LabMotion::Static;
        ui.label(tr(SPEED)).on_hover_text(tr(SPEED_HINT));
        ui.add_enabled(
            moving,
            egui::Slider::new(&mut s.speed_pct, SPEED_PCT.0..=SPEED_PCT.1).suffix("%"),
        );
        ui.end_row();
        ui.label(tr(CHANGE)).on_hover_text(tr(CHANGE_HINT));
        ui.add_enabled(moving, f_slider(&mut s.change_secs, CHANGE_SECS, " s"));
        ui.end_row();
        ui.label(tr(DODGE)).on_hover_text(tr(DODGE_HINT));
        ui.add_enabled_ui(s.motion == LabMotion::Dodge, |ui| {
            ui.horizontal(|ui| {
                for d in DodgePreset::ALL {
                    ui.selectable_value(&mut s.dodge, d, d.label());
                }
            });
        });
        ui.end_row();
        ui.label(tr(COUNT));
        ui.add(egui::Slider::new(
            &mut s.max_alive,
            MAX_ALIVE.0..=MAX_ALIVE.1,
        ));
        ui.end_row();
        ui.label(tr(SHAPE));
        ui.horizontal(|ui| {
            ui.selectable_value(&mut s.head_only, true, tr(HEAD_ONLY));
            ui.selectable_value(&mut s.head_only, false, tr(FULL_BODY));
        });
        ui.end_row();
    });
}

fn spawns(ui: &mut egui::Ui, s: &mut LabSettings) {
    grid(ui, SPAWNS, |ui| {
        ui.label(tr(DISTANCE_MIN));
        ui.add(f_slider(&mut s.distance_m.min, DISTANCE_M, " m"));
        ui.end_row();
        ui.label(tr(DISTANCE_MAX));
        ui.add(f_slider(&mut s.distance_m.max, DISTANCE_M, " m"));
        ui.end_row();
        // Keep the range ordered while dragging either end.
        if s.distance_m.min > s.distance_m.max {
            s.distance_m.max = s.distance_m.min;
        }
        ui.label(tr(SPREAD));
        ui.add(f_slider(&mut s.spread_m, SPREAD_M, " m"));
        ui.end_row();
        ui.label(tr(DELAY)).on_hover_text(tr(DELAY_HINT));
        ui.add(f_slider(&mut s.appear_delay_secs, APPEAR_DELAY_SECS, " s"));
        ui.end_row();
        ui.label(tr(LIFETIME));
        ui.horizontal(|ui| {
            ui.add_enabled(
                !s.until_killed,
                f_slider(&mut s.lifetime_secs, LIFETIME_SECS, " s"),
            );
            ui.checkbox(&mut s.until_killed, tr(UNTIL_KILLED));
        });
        ui.end_row();
    });
}

fn player(ui: &mut egui::Ui, s: &mut LabSettings) {
    grid(ui, PLAYER, |ui| {
        ui.label("");
        ui.checkbox(&mut s.player_movement, tr(MOVEMENT));
        ui.end_row();
        ui.label(tr(AREA_WIDTH));
        ui.add_enabled(
            s.player_movement,
            f_slider(&mut s.area_half_width_m, AREA_HALF_WIDTH_M, " m"),
        );
        ui.end_row();
        ui.label(tr(AREA_DEPTH));
        ui.add_enabled(
            s.player_movement,
            f_slider(&mut s.area_depth_m, AREA_DEPTH_M, " m"),
        );
        ui.end_row();
    });
}

fn weapon(ui: &mut egui::Ui, s: &mut LabSettings) {
    grid(ui, WEAPON, |ui| {
        ui.label(tr(WEAPON));
        egui::ComboBox::from_id_salt("lab-weapon")
            .selected_text(s.weapon.stats().display_name)
            .show_ui(ui, |ui| {
                for w in LAB_WEAPONS {
                    ui.selectable_value(&mut s.weapon, w, w.stats().display_name);
                }
            });
        ui.end_row();
        ui.label("");
        ui.checkbox(&mut s.simulate_spread, tr(SIMULATE_SPREAD))
            .on_hover_text(tr(SPREAD_HINT));
        ui.end_row();
        ui.label(tr(LENGTH));
        ui.add(f_slider(&mut s.duration_secs, DURATION_SECS, " s"));
        ui.end_row();
    });
}
