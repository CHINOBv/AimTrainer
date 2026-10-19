//! Results screen shown after a run finishes. When a new result appears, the page saves
//! its replay and analyzes it once; both are cached until the next result.

use crate::game::{AppState, FinderState, LastResult};
use crate::overlay::Overlay;
use crate::persistence::library::Library;
use crate::persistence::replays;
use crate::persistence::settings::{Paths, Settings};
use crate::ui::UiAction;
use crate::ui::navigation::Navigator;
use crate::ui::organisms::analysis_panel::RunReport;
use crate::ui::organisms::mistake_player::{MistakePlayerState, PlayerAction, mistake_player};
use crate::ui::organisms::replay_viewer::{ReplayViewerState, replay_viewer};
use crate::ui::organisms::results_card::{ResultsContext, results_card};
use crate::ui::organisms::results_visuals::{accuracy_gauge, score_hero};
use aim_core::activity::ActivityKind;
use aim_core::analytics::analyze;
use aim_core::analytics::path::replay_path;
use aim_core::scenario::Recording;
use aim_core::scenario::lab::{LabSettings, is_lab_id};
use aim_core::sensitivity::Sensitivity;
use aim_core::training::routine::PlannedRun;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

pub struct ResultsPage;

impl Plugin for ResultsPage {
    fn build(&self, app: &mut App) {
        app.add_systems(
            EguiPrimaryContextPass,
            results.run_if(in_state(AppState::Results)),
        );
    }
}

/// Identifies one result: a new run has a new seed or input log.
type ResultKey = (String, u64, usize);

fn result_key(rec: &Recording) -> ResultKey {
    (rec.def.id.clone(), rec.seed, rec.inputs.len())
}

/// The analysis of the result currently shown.
#[derive(Default)]
struct ReportCache(Option<(ResultKey, RunReport)>);

/// Save the replay and analyze the run the first time a result is seen. Arena results
/// have no scenario recording, so there is nothing to save or analyze.
fn refresh(cache: &mut ReportCache, last: &LastResult, paths: &Paths, lib: &Library) {
    let Some(recording) = &last.recording else {
        cache.0 = None;
        return;
    };
    let key = result_key(recording);
    if cache.0.as_ref().is_some_and(|(k, _)| *k == key) {
        return;
    }
    if !recording.inputs.is_empty()
        && let Err(e) = replays::save(paths, recording)
    {
        bevy::log::error!("could not save replay: {e}");
    }
    let report = RunReport::new(&analyze(recording), |id| {
        lib.scenario(id).map(|d| d.name.clone())
    });
    cache.0 = Some((key, report));
}

/// Display name of a planned routine run, or `None` when its scenario or arena is unknown.
fn planned_name(run: &PlannedRun, library: &Library) -> Option<String> {
    match run.kind {
        ActivityKind::Scenario => library.scenario(&run.scenario_id).map(|d| d.name.clone()),
        ActivityKind::Arena => aim_world::arena::builtin::get(&run.scenario_id).map(|a| a.name),
    }
}

/// The arena an arena result came from (a routine may have shortened it to `id~90s`).
fn arena_retry(last: &LastResult, id: &str) -> Option<crate::ui::UiAction> {
    last.recording
        .is_none()
        .then(|| arena_retry_for(id))
        .flatten()
}

/// The action that replays the arena behind run key `id` (possibly shortened to
/// `id~90s` by a routine, possibly practice), or `None` when it is not an arena.
fn arena_retry_for(id: &str) -> Option<crate::ui::UiAction> {
    let (key, practice) = aim_world::arena::split_practice(id);
    let (base, _, _) = aim_core::scenario::parse_run_key_full(key);
    aim_world::arena::builtin::get(base)?;
    Some(if practice {
        crate::ui::UiAction::StartArenaPractice(base.to_owned())
    } else {
        crate::ui::UiAction::StartArena(base.to_owned())
    })
}

/// The action that replays the Movement Lab drill behind run key `id`: the saved lab
/// settings when they still describe that drill, otherwise `None`.
fn lab_retry_for(id: &str, lab: &LabSettings) -> Option<UiAction> {
    (is_lab_id(id) && lab.to_def().id == id).then(|| UiAction::StartLab(lab.clone()))
}

/// Inputs, paths and what deciding the Retry action needs beyond the navigator.
#[derive(SystemParam)]
struct RetryContext<'w> {
    finder: Res<'w, FinderState>,
    settings: Res<'w, Settings>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    paths: Res<'w, Paths>,
}

fn results(
    mut contexts: EguiContexts,
    last: Option<Res<LastResult>>,
    overlay: Res<Overlay>,
    mut cache: Local<ReportCache>,
    mut watch: Local<WatchWindow>,
    retry_ctx: RetryContext,
    mut nav: Navigator,
) -> Result {
    // Before the visibility check, so a run finished while the overlay is hidden is still
    // saved as soon as its result exists.
    if let Some(last) = &last {
        refresh(&mut cache, last, &retry_ctx.paths, nav.library());
    }
    if !overlay.visible {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let Some(last) = last else {
        nav.apply(crate::ui::UiAction::BackToMenu);
        return Ok(());
    };
    let report = cache.0.as_ref().map(|(_, r)| r);
    let r = &last.result;
    let next_run = nav
        .routine()
        .and_then(|p| p.plan.get(p.index + 1))
        .and_then(|p| planned_name(p, nav.library()));
    // Arena results retry the arena; scenario results retry the scenario (with its level).
    let id = &last.result.scenario_id;
    let challenge =
        crate::ui::pages::menu::todays_challenge(nav.library()).filter(|c| c.run_key() == *id);
    let finder_next = retry_ctx
        .finder
        .0
        .as_ref()
        .filter(|_| last.finder_test)
        .and_then(|f| {
            let slot = f.next_slot()?;
            let s = retry_ctx.settings.sensitivity;
            Some((
                slot,
                Sensitivity::from_cm_per_360(s.game, f.cm_for(slot), s.dpi),
            ))
        });
    let retry = if let Some(c) = challenge {
        crate::ui::UiAction::StartChallenge {
            key: c.run_key(),
            scenario: c.scenario_id,
            seed: c.seed,
        }
    } else if let Some((slot, sens)) = finder_next {
        crate::ui::UiAction::StartFinderTest { slot, sens }
    } else if let Some(arena) = arena_retry(&last, id) {
        arena
    } else if let Some(lab) = lab_retry_for(id, &retry_ctx.settings.movement_lab) {
        lab
    } else {
        crate::ui::UiAction::Retry(id.clone())
    };
    let rc = ResultsContext {
        retry,
        routine: nav.routine(),
        next_run: next_run.as_deref(),
        space: retry_ctx.keys.just_pressed(KeyCode::Space),
        report,
        hero: score_hero(r.score, last.previous_best, r.kills, r.kills_per_min),
        gauges: std::iter::once(accuracy_gauge(r.accuracy))
            .chain(report.iter().flat_map(|rep| rep.gauges.iter().cloned()))
            .collect(),
    };
    match results_card(ctx, &last, &rc) {
        Some(UiAction::WatchMistakes) => watch.open(last.recording.as_ref(), &retry_ctx.paths),
        Some(action) => {
            watch.open = false;
            nav.apply(action);
        }
        None => {}
    }
    watch.show(ctx);
    Ok(())
}

const WATCH_TITLE: &str = "Replay";
const MISTAKES_TITLE: &str = "Mistake replays";
const NO_MISTAKES: &str = "No flick mistakes to replay in this run. Here is the full replay.";

/// The window "Watch it" opens: the mistake player for the run just finished, one mistake
/// at a time. Runs without flick mistakes (tracking, arenas, clean runs) fall back to the
/// replay viewer on the newest saved replay.
#[derive(Default)]
struct WatchWindow {
    open: bool,
    player: Option<MistakePlayerState>,
    viewer: ReplayViewerState,
    /// Saved replays for the fallback viewer, listed once when the window opens.
    entries: Vec<replays::ReplayEntry>,
}

impl WatchWindow {
    fn open(&mut self, recording: Option<&Recording>, paths: &Paths) {
        let player = recording
            .map(MistakePlayerState::from_recording)
            .filter(|p| !p.is_empty());
        *self = Self {
            open: true,
            entries: if player.is_some() {
                Vec::new()
            } else {
                replays::list(paths)
            },
            player,
            viewer: ReplayViewerState::default(),
        };
    }

    fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let Self {
            open,
            player,
            viewer,
            entries,
        } = self;
        let title = if player.is_some() {
            MISTAKES_TITLE
        } else {
            WATCH_TITLE
        };
        let mut closed = false;
        egui::Window::new(title)
            .open(open)
            .default_size([460.0, 600.0])
            .show(ctx, |ui| match player {
                Some(state) => {
                    closed = mistake_player(ui, state) == Some(PlayerAction::Close);
                }
                None => {
                    ui.label(NO_MISTAKES);
                    replay_viewer(ui, viewer, entries, |e| {
                        replays::load(&e.path).ok().map(|r| replay_path(&r))
                    });
                }
            });
        if closed {
            *open = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{arena_retry_for, lab_retry_for, planned_name};
    use crate::ui::UiAction;

    #[test]
    fn arena_results_retry_the_same_arena_in_the_same_mode() {
        assert_eq!(
            arena_retry_for("yard-deathmatch+practice"),
            Some(UiAction::StartArenaPractice("yard-deathmatch".into()))
        );
        assert_eq!(
            arena_retry_for("yard-deathmatch~90s"),
            Some(UiAction::StartArena("yard-deathmatch".into()))
        );
        assert_eq!(arena_retry_for("micro-adjust"), None);
    }

    #[test]
    fn lab_results_retry_the_same_lab_drill() {
        use aim_core::scenario::lab::LabSettings;
        let lab = LabSettings {
            max_alive: 4,
            ..LabSettings::default()
        };
        let id = lab.to_def().id;
        assert_eq!(
            lab_retry_for(&id, &lab),
            Some(UiAction::StartLab(lab.clone()))
        );
        let other = LabSettings::default();
        assert_eq!(
            lab_retry_for(&id, &other),
            None,
            "settings changed since the run: no other drill is started"
        );
        assert_eq!(lab_retry_for("micro-adjust", &lab), None);
    }

    #[test]
    fn planned_runs_are_named_from_their_scenario_or_arena() {
        let library = crate::persistence::library::Library {
            scenarios: aim_core::scenario::builtin::all(),
            routines: Vec::new(),
            benchmarks: Vec::new(),
            methods: Vec::new(),
            errors: Vec::new(),
        };
        let run = |kind, id: &str| aim_core::training::routine::PlannedRun {
            scenario_id: id.into(),
            kind,
            block: 0,
            note: String::new(),
            difficulty: 0.0,
            duration_secs: None,
        };
        use aim_core::activity::ActivityKind::{Arena, Scenario};
        assert_eq!(
            planned_name(&run(Arena, "yard-deathmatch"), &library).as_deref(),
            Some("Yard Deathmatch")
        );
        assert!(planned_name(&run(Scenario, "micro-adjust"), &library).is_some());
        assert_eq!(planned_name(&run(Arena, "nope"), &library), None);
    }

    use bevy::ecs::system::assert_is_system;
    use bevy::prelude::Result;

    #[test]
    fn systems_have_no_conflicting_access() {
        assert_is_system::<(), Result, _>(super::results);
    }
}
