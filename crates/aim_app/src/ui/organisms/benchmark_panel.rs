//! Benchmark energies per subcategory, with a play button per scenario.

use crate::ui::UiAction;
use crate::ui::atoms::WARN;
use crate::ui::molecules::energy_text;
use aim_core::benchmark::BenchmarkDef;
use bevy_egui::egui;
use std::collections::HashMap;

pub fn benchmark_panel(
    ui: &mut egui::Ui,
    benchmarks: &[BenchmarkDef],
    bests: &HashMap<String, f32>,
) -> Option<UiAction> {
    let mut action = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        for b in benchmarks {
            ui.heading(&b.name);
            ui.label(&b.description);
            if !b.calibrated {
                ui.colored_label(WARN, "Provisional thresholds (simulated players).");
            }
            let rank = |e: f32| b.rank_for(e).map(str::to_owned);
            let res = b.evaluate(bests);
            let overall = match res.overall {
                Some(e) => energy_text(Some(e), rank).replacen(" · ", " energy · ", 1),
                None => "— (score every subcategory)".to_owned(),
            };
            ui.strong(format!("Overall: {overall}"));
            egui::Grid::new(&b.id)
                .striped(true)
                .num_columns(4)
                .show(ui, |ui| {
                    for (sub, r) in b.subcategories.iter().zip(&res.subcategories) {
                        ui.label(format!("{} / {}", sub.skill, sub.name));
                        ui.label(energy_text(r.energy, rank));
                        for s in &sub.scenarios {
                            let best = bests
                                .get(&s.id)
                                .map_or("—".to_owned(), |v| format!("{v:.0}"));
                            if ui.button(format!("▶ {} ({best})", s.id)).clicked() {
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
