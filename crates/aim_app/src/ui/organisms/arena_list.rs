//! Arena catalogue: deathmatch and angle-clearing drills on original maps.

use crate::i18n::{tr, tr_fmt, tr_named};
use crate::ui::UiAction;
use aim_world::arena::{ArenaDef, ArenaMode};
use bevy_egui::egui;

const COACHING_LABEL: &str = "Live coaching overlay";
const COACHING_HINT: &str = "While you play an arena: your crosshair height against head level at the nearest corner an enemy can appear from, and whether you are still enough to shoot accurately.";

pub fn arena_list(
    ui: &mut egui::Ui,
    arenas: &[ArenaDef],
    best: impl Fn(&str) -> Option<f32>,
    coaching: &mut bool,
) -> Option<UiAction> {
    let mut action = None;
    egui::ScrollArea::vertical().id_salt("arenas").show(ui, |ui| {
        ui.label(tr(
            "Bots move with VALORANT movement and shoot back. Maps are original training layouts.",
        ));
        ui.checkbox(coaching, tr(COACHING_LABEL))
            .on_hover_text(tr(COACHING_HINT));
        ui.add_space(6.0);
        for a in arenas {
            ui.heading(tr(&a.name));
            ui.label(tr(&a.description));
            let mode = match &a.mode {
                ArenaMode::Deathmatch {
                    bot_skill,
                    objective: Some(o),
                    ..
                } => tr_named(
                    "Site hold · {attackers} {skill} attackers",
                    &[("attackers", &o.attackers), ("skill", bot_skill)],
                ),
                ArenaMode::Deathmatch {
                    bots, bot_skill, ..
                } => tr_named(
                    "Deathmatch · {bots} {skill} bots",
                    &[("bots", bots), ("skill", bot_skill)],
                ),
                ArenaMode::AngleClear { angles, .. } => {
                    tr_fmt("Angle clearing · {} angles", &[&angles.len()])
                }
            };
            let pb = best(&a.id).map_or(String::new(), |b| format!(" · PB {b:.0}"));
            ui.label(format!(
                "{mode} · {:.0} s · {}{pb}",
                a.duration_secs,
                a.weapon.stats().display_name
            ));
            ui.horizontal(|ui| {
                if crate::audio::ui_click::clicked(ui.button(tr("▶  Play"))).clicked() {
                    action = Some(UiAction::StartArena(a.id.clone()));
                }
                if ui
                    .button(tr("Practice"))
                    .on_hover_text(tr(
                        "Bots move and peek as usual but never shoot: learn the map and the angles.",
                    ))
                    .clicked()
                {
                    action = Some(UiAction::StartArenaPractice(a.id.clone()));
                }
            });
            ui.separator();
        }
    });
    action
}
