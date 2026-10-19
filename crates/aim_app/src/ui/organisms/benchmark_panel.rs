//! Benchmark energies per subcategory, with a tier selector and a play button per scenario.

use crate::i18n::{tr, tr_fmt};
use crate::ui::UiAction;
use crate::ui::atoms::WARN;
use crate::ui::molecules::energy_text;
use aim_core::scenario::parse_run_key;
use aim_core::training::benchmark::BenchmarkDef;
use bevy_egui::egui;
use std::collections::HashMap;

pub fn benchmark_panel(
    ui: &mut egui::Ui,
    benchmarks: &[BenchmarkDef],
    bests: &HashMap<String, f32>,
) -> Option<UiAction> {
    let mut action = None;
    egui::ScrollArea::vertical()
        .id_salt("benchmark")
        .show(ui, |ui| {
            for b in benchmarks {
                ui.heading(tr(&b.name));
                ui.label(tr(&b.description));
                if !b.calibrated {
                    ui.colored_label(WARN, tr("Provisional thresholds (simulated players)."));
                }
                let view = tier_selector(ui, b);
                let b = view.as_ref().unwrap_or(b);
                let rank = |e: f32| b.rank_for(e).map(|r| tr(r).to_owned());
                let res = b.evaluate(bests);
                let overall = match res.overall {
                    Some(e) => energy_text(Some(e), tr("energy"), rank),
                    None => tr("— (score every subcategory)").to_owned(),
                };
                ui.strong(tr_fmt("Overall: {}", &[&overall]));
                egui::Grid::new(&b.id)
                    .striped(true)
                    .num_columns(4)
                    .show(ui, |ui| {
                        for (sub, r) in b.subcategories.iter().zip(&res.subcategories) {
                            ui.label(format!("{} / {}", tr(&sub.skill), tr(&sub.name)));
                            ui.label(energy_text(r.energy, "", rank));
                            for s in &sub.scenarios {
                                let best = bests
                                    .get(&s.id)
                                    .map_or("—".to_owned(), |v| format!("{v:.0}"));
                                let name = parse_run_key(&s.id).0;
                                // A tier's ids are run keys, so this starts it at the tier's level.
                                if ui.button(format!("▶ {name} ({best})")).clicked() {
                                    action = Some(UiAction::StartScenario(s.id.clone()));
                                }
                            }
                            ui.end_row();
                        }
                    });
                ui.separator();
            }
        });
    action
}

/// Tier buttons for a tiered benchmark; returns the selected tier's benchmark. The choice
/// lives in egui memory, so the page needs no extra state. Defaults to the level-0 tier.
fn tier_selector(ui: &mut egui::Ui, b: &BenchmarkDef) -> Option<BenchmarkDef> {
    let default = b.tier_at_level(0.0).or(b.tiers.first())?;
    let key = egui::Id::new(("benchmark-tier", &b.id));
    let mut selected = ui
        .data(|d| d.get_temp::<String>(key))
        .filter(|id| b.tier(id).is_some())
        .unwrap_or_else(|| default.id.clone());
    ui.horizontal(|ui| {
        ui.label(tr("Tier:"));
        for t in &b.tiers {
            ui.selectable_value(&mut selected, t.id.clone(), tr(&t.name))
                .on_hover_text(tr_fmt(
                    "Difficulty level {}",
                    &[&format!("{:+.1}", t.level)],
                ));
        }
    });
    ui.data_mut(|d| d.insert_temp(key, selected.clone()));
    b.at_tier(&selected)
}
