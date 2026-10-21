//! Scenario list (search, filters, favourites, recent runs, then every category) and the
//! details/play panel.

use crate::i18n::{tr, tr_fmt};
use crate::persistence::history::History;
use crate::persistence::library::Library;
use crate::ui::UiAction;
use crate::ui::atoms::FAVORITE;
use crate::ui::molecules::{matches_query, scenario_item};
use crate::ui::organisms::history_chart::history_chart;
use aim_core::games::Game;
use aim_core::scenario::{Category, FireMode, ScenarioDef, parse_run_key};
use aim_core::training::progress::RunRecord;
use aim_core::training::store::RunStore;
use bevy_egui::egui;

/// Scenarios listed under "Recent".
pub const RECENT_COUNT: usize = 5;

/// What the scenario list shows. Kept for the session, not saved.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ScenarioFilter {
    /// Words matched against the name, description and category.
    pub query: String,
    pub category: Option<Category>,
    pub fire: Option<FireMode>,
    /// `Some(true)`: movement scenarios only; `Some(false)`: standing only.
    pub movement: Option<bool>,
    /// Only scenarios with phases (waves).
    pub waves_only: bool,
    /// `Some(Some(game))`: scenarios made for that game; `Some(None)`: generic drills only.
    pub game: Option<Option<Game>>,
}

impl ScenarioFilter {
    pub fn matches(&self, def: &ScenarioDef) -> bool {
        // The shown (translated) name and description match too; in English they repeat.
        let text = format!(
            "{} {} {} {} {}",
            def.name,
            def.description,
            def.category.label(),
            tr(&def.name),
            tr(&def.description)
        );
        matches_query(&self.query, &text)
            && self.category.is_none_or(|c| def.category == c)
            && self.fire.is_none_or(|f| def.weapon.fire == f)
            && self.movement.is_none_or(|m| def.player.movement == m)
            && (!self.waves_only || def.phases.is_some())
            && self.game.is_none_or(|g| def.game == g)
    }

    /// True when anything narrows the list.
    pub fn is_active(&self) -> bool {
        *self != Self::default()
    }
}

/// The game filter's choices: every game some scenario is made for, in `Game::ALL` order,
/// then generic (`None`).
pub fn game_choices(scenarios: &[ScenarioDef]) -> Vec<Option<Game>> {
    Game::ALL
        .into_iter()
        .filter(|g| scenarios.iter().any(|d| d.game == Some(*g)))
        .map(Some)
        .chain([None])
        .collect()
}

/// Label of a game filter choice.
pub fn game_label(game: Option<Game>) -> &'static str {
    match game {
        Some(g) => g.profile().display_name,
        None => tr("Generic"),
    }
}

/// Add `id` to the favourites, or remove it when it is already one.
pub fn toggle_favorite(favorites: &mut Vec<String>, id: &str) {
    match favorites.iter().position(|f| f == id) {
        Some(i) => {
            favorites.remove(i);
        }
        None => favorites.push(id.to_owned()),
    }
}

/// The last `count` distinct scenarios played, newest first, keeping only ids `known`
/// accepts (a run key's level and tuning suffixes are dropped).
pub fn recent_scenarios(
    runs: &[RunRecord],
    known: impl Fn(&str) -> bool,
    count: usize,
) -> Vec<String> {
    let mut recent: Vec<String> = Vec::new();
    for run in runs.iter().rev() {
        if recent.len() >= count {
            break;
        }
        let (id, _) = parse_run_key(&run.scenario);
        if known(id) && !recent.iter().any(|r| r == id) {
            recent.push(id.to_owned());
        }
    }
    recent
}

/// Filter controls and the list. `favorites` is saved in the settings.
pub fn scenario_list(
    ui: &mut egui::Ui,
    lib: &Library,
    history: &History,
    selected: &mut Option<String>,
    filter: &mut ScenarioFilter,
    favorites: &mut Vec<String>,
) {
    filter_controls(ui, filter, &game_choices(&lib.scenarios));
    ui.separator();
    let mut toggled: Option<String> = None;
    egui::ScrollArea::vertical()
        .id_salt("scenario_list")
        .show(ui, |ui| {
            let mut row = |ui: &mut egui::Ui, d: &ScenarioDef| {
                ui.horizontal(|ui| {
                    let fav = favorites.contains(&d.id);
                    let star = egui::RichText::new("★").color(if fav {
                        FAVORITE
                    } else {
                        ui.visuals().weak_text_color()
                    });
                    let hint = tr(if fav {
                        "Remove from favourites"
                    } else {
                        "Add to favourites"
                    });
                    if ui
                        .add(egui::Button::new(star).frame(false))
                        .on_hover_text(hint)
                        .clicked()
                    {
                        toggled = Some(d.id.clone());
                    }
                    let is_selected = selected.as_deref() == Some(d.id.as_str());
                    if scenario_item(ui, tr(&d.name), history.best(&d.id), is_selected) {
                        *selected = Some(d.id.clone());
                    }
                });
            };
            let group = |ids: &[String]| -> Vec<&ScenarioDef> {
                ids.iter()
                    .filter_map(|id| lib.scenario(id))
                    .filter(|d| filter.matches(d))
                    .collect()
            };
            let favs = group(favorites);
            if !favs.is_empty() {
                ui.push_id("favourites", |ui| {
                    ui.heading(tr("★ Favourites"));
                    for d in favs {
                        row(ui, d);
                    }
                });
                ui.add_space(6.0);
            }
            let recent = group(&recent_scenarios(
                history.runs(),
                |id| lib.scenario(id).is_some(),
                RECENT_COUNT,
            ));
            if !recent.is_empty() {
                ui.push_id("recent", |ui| {
                    ui.heading(tr("Recent"));
                    for d in recent {
                        row(ui, d);
                    }
                });
                ui.add_space(6.0);
            }
            let mut any = false;
            for cat in Category::ALL {
                let items: Vec<&ScenarioDef> = lib
                    .scenarios
                    .iter()
                    .filter(|d| d.category == cat && filter.matches(d))
                    .collect();
                if items.is_empty() {
                    continue;
                }
                any = true;
                ui.push_id(cat.label(), |ui| {
                    ui.heading(cat.label());
                    for d in items {
                        row(ui, d);
                    }
                });
                ui.add_space(6.0);
            }
            if !any {
                ui.label(tr("No scenario matches the filters."));
            }
        });
    if let Some(id) = toggled {
        toggle_favorite(favorites, &id);
    }
}

fn filter_controls(ui: &mut egui::Ui, filter: &mut ScenarioFilter, games: &[Option<Game>]) {
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut filter.query)
                .hint_text(tr("Search scenarios"))
                .desired_width(160.0),
        );
        if filter.is_active() && ui.small_button(tr("Clear")).clicked() {
            *filter = ScenarioFilter::default();
        }
    });
    ui.horizontal_wrapped(|ui| {
        option_combo(
            ui,
            "filter_category",
            &mut filter.category,
            tr("Any category"),
            &Category::ALL,
            |c| tr(c.label()),
        );
        option_combo(
            ui,
            "filter_fire",
            &mut filter.fire,
            tr("Click or hold"),
            &[FireMode::Click, FireMode::Hold],
            |f| {
                tr(match f {
                    FireMode::Click => "Click",
                    FireMode::Hold => "Hold to fire",
                })
            },
        );
        option_combo(
            ui,
            "filter_movement",
            &mut filter.movement,
            tr("Any movement"),
            &[true, false],
            |m| tr(if m { "Moving" } else { "Standing" }),
        );
        option_combo(
            ui,
            "filter_game",
            &mut filter.game,
            tr("Any game"),
            games,
            game_label,
        );
        ui.checkbox(&mut filter.waves_only, tr("Waves"))
            .on_hover_text(tr("Only scenarios with phases (waves)."));
    });
}

/// A combo box whose first entry, `any`, clears the choice.
fn option_combo<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    salt: &str,
    value: &mut Option<T>,
    any: &'static str,
    all: &[T],
    label: impl Fn(T) -> &'static str,
) {
    egui::ComboBox::from_id_salt(salt)
        .selected_text(value.map_or(any, &label))
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, any);
            for &option in all {
                ui.selectable_value(value, Some(option), label(option));
            }
        });
}

/// Runs drawn in the history chart, and listed as numbers under it.
const CHART_RUNS: usize = 30;
const LISTED_RUNS: usize = 10;

pub fn scenario_details(
    ui: &mut egui::Ui,
    def: &ScenarioDef,
    history: &History,
    adaptive_level: f32,
) -> Option<UiAction> {
    ui.heading(tr(&def.name));
    let fire = tr(if def.weapon.fire == FireMode::Hold {
        "hold to fire"
    } else {
        "click"
    });
    ui.label(format!(
        "{} · {} · {:.0}s · {} · {fire}",
        game_label(def.game),
        tr(def.category.label()),
        def.duration_secs,
        def.weapon.kind.stats().display_name,
    ));
    ui.add_space(6.0);
    ui.label(tr(&def.description));
    if def.player.movement {
        ui.label(tr(
            "Movement: WASD (Shift = walk). Counter-strafe before shooting.",
        ));
    }
    let charted = history.recent(&def.id, CHART_RUNS);
    let recent = &charted[charted.len().saturating_sub(LISTED_RUNS)..];
    if !recent.is_empty() {
        ui.add_space(6.0);
        history_chart(ui, &charted);
        let list: Vec<String> = recent.iter().map(|s| format!("{s:.0}")).collect();
        ui.label(tr_fmt("Last runs: {}", &[&list.join(", ")]));
    }
    ui.add_space(10.0);
    let mut action = None;
    ui.horizontal(|ui| {
        if ui
            .add_sized([160.0, 36.0], egui::Button::new(tr("▶  Play")))
            .clicked()
        {
            action = Some(UiAction::StartScenario(def.id.clone()));
        }
        let adaptive = egui::Button::new(tr_fmt(
            "▶  Adaptive ({})",
            &[&format!("{adaptive_level:+.2}")],
        ));
        if ui
            .add_sized([200.0, 36.0], adaptive)
            .on_hover_text(tr(
                "Difficulty follows you to keep success near 80%, where learning is fastest.",
            ))
            .clicked()
        {
            action = Some(UiAction::StartAdaptive(def.id.clone()));
        }
    });
    action
}

pub fn empty_selection(ui: &mut egui::Ui) {
    ui.heading(tr("Pick a scenario"));
    ui.label(tr(
        "Esc pauses a run. Right mouse = ADS zoom for weapons with a scope.",
    ));
    ui.label(tr("Click ★ next to a scenario to pin it to Favourites."));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(id: &str) -> ScenarioDef {
        aim_core::scenario::builtin::all()
            .into_iter()
            .find(|d| d.id == id)
            .unwrap()
    }

    #[test]
    fn search_matches_name_description_and_category_words() {
        let flick = def("flick-headshot");
        let by = |query: &str| ScenarioFilter {
            query: query.into(),
            ..ScenarioFilter::default()
        };
        assert!(by("").matches(&flick));
        assert!(by(&flick.name.to_uppercase()).matches(&flick));
        assert!(by("flicking").matches(&flick), "category label");
        assert!(!by("zzz-no-such-word").matches(&flick));
    }

    #[test]
    fn filters_narrow_by_category_fire_mode_movement_and_waves() {
        let flick = def("flick-headshot");
        let track = def("track-precise");
        let strafe = def("counter-strafe");
        let waves = def("phase-kill-race");

        let category = ScenarioFilter {
            category: Some(Category::Tracking),
            ..ScenarioFilter::default()
        };
        assert!(category.matches(&track) && !category.matches(&flick));

        let hold = ScenarioFilter {
            fire: Some(FireMode::Hold),
            ..ScenarioFilter::default()
        };
        assert_eq!(hold.matches(&track), track.weapon.fire == FireMode::Hold);
        assert!(!hold.matches(&flick));

        let moving = ScenarioFilter {
            movement: Some(true),
            ..ScenarioFilter::default()
        };
        assert!(moving.matches(&strafe) && !moving.matches(&flick));
        let standing = ScenarioFilter {
            movement: Some(false),
            ..ScenarioFilter::default()
        };
        assert!(standing.matches(&flick) && !standing.matches(&strafe));

        let wave = ScenarioFilter {
            waves_only: true,
            ..ScenarioFilter::default()
        };
        assert!(wave.matches(&waves) && !wave.matches(&flick));
        assert!(wave.is_active() && !ScenarioFilter::default().is_active());
    }

    #[test]
    fn the_game_filter_keeps_one_game_or_only_generic_drills() {
        let valorant = def("counter-strafe");
        let generic = def("six-pin");
        let only = |game: Option<Game>| ScenarioFilter {
            game: Some(game),
            ..ScenarioFilter::default()
        };
        assert!(only(Some(Game::Valorant)).matches(&valorant));
        assert!(!only(Some(Game::Valorant)).matches(&generic));
        assert!(only(None).matches(&generic) && !only(None).matches(&valorant));
        assert!(!only(Some(Game::Cs2)).matches(&valorant));
        assert!(only(None).is_active());
        assert!(ScenarioFilter::default().matches(&valorant));
        assert!(ScenarioFilter::default().matches(&generic));
    }

    #[test]
    fn game_choices_list_the_games_in_the_library_then_generic() {
        let lib = [def("six-pin"), def("counter-strafe"), def("flick-headshot")];
        assert_eq!(game_choices(&lib), [Some(Game::Valorant), None]);
        assert_eq!(game_choices(&lib[..1]), [None]);
    }

    #[test]
    fn favourites_toggle_on_and_off_without_duplicates() {
        let mut favs = vec!["a".to_owned()];
        toggle_favorite(&mut favs, "b");
        assert_eq!(favs, ["a", "b"]);
        toggle_favorite(&mut favs, "a");
        assert_eq!(favs, ["b"]);
        toggle_favorite(&mut favs, "b");
        assert!(favs.is_empty());
    }

    #[test]
    fn recent_lists_distinct_known_scenarios_newest_first() {
        let run = |t: u64, key: &str| RunRecord {
            timestamp: t,
            scenario: key.into(),
            score: 1.0,
            duration_secs: 60.0,
        };
        let runs = [
            run(1, "a"),
            run(2, "b@+0.50"),
            run(3, "gone"),
            run(4, "a~20s"),
            run(5, "c"),
        ];
        let known = |id: &str| id != "gone";
        assert_eq!(recent_scenarios(&runs, known, 5), ["c", "a", "b"]);
        assert_eq!(recent_scenarios(&runs, known, 2), ["c", "a"]);
        assert!(recent_scenarios(&[], known, 5).is_empty());
    }
}
