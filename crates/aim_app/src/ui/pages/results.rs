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
use crate::ui::organisms::coach_take::{
    CoachTake, CoachWhy, TakeAction, WhyAction, coach_take, coach_why,
};
use crate::ui::organisms::mistake_player::{MistakePlayerState, PlayerAction, mistake_player_with};
use crate::ui::organisms::replay_viewer::{ReplayViewerState, replay_viewer};
use crate::ui::organisms::results_card::{ResultsContext, results_card};
use crate::ui::organisms::results_visuals::{accuracy_gauge, score_hero};
use aim_core::activity::ActivityKind;
use aim_core::analytics::path::replay_path;
use aim_core::analytics::{RunAnalysis, analyze};
use aim_core::coach::debrief::{MistakeDigest, RunDigest, drill_candidates, previous_runs};
use aim_core::scenario::Recording;
use aim_core::sensitivity::Sensitivity;
use aim_core::training::progress::RunRecord;
use aim_core::training::routine::PlannedRun;
use aim_core::training::store::RunStore;
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

/// The AI coach's take on the result shown and its per-mistake answers, kept until the
/// next result so reopening the card or a mistake sends nothing again.
#[derive(Default)]
struct CoachCache {
    take: Option<CoachTake>,
    whys: std::collections::HashMap<usize, CoachWhy>,
}

impl CoachCache {
    fn for_run(
        analysis: &RunAnalysis,
        last: &LastResult,
        records: &[RunRecord],
        lib: &Library,
    ) -> Self {
        let previous = previous_runs(records, &last.result);
        let digest = RunDigest::build(analysis, &last.result, &previous);
        let drills = drill_candidates(&lib.scenarios, analysis.category);
        Self {
            take: Some(CoachTake::new(digest, drills)),
            whys: Default::default(),
        }
    }

    fn poll(&mut self) {
        if let Some(take) = &mut self.take {
            take.poll();
        }
        self.whys.values_mut().for_each(CoachWhy::poll);
    }
}

/// Save the replay and analyze the run the first time a result is seen. Arena results
/// have no scenario recording, so there is nothing to save or analyze.
fn refresh(
    cache: &mut ReportCache,
    coach: &mut CoachCache,
    last: &LastResult,
    paths: &Paths,
    lib: &Library,
    records: &[RunRecord],
) {
    let Some(recording) = &last.recording else {
        cache.0 = None;
        *coach = CoachCache::default();
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
    let analysis = analyze(recording);
    let report = RunReport::new(&analysis, |id| lib.scenario(id).map(|d| d.name.clone()));
    *coach = CoachCache::for_run(&analysis, last, records, lib);
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

/// Inputs, paths and what deciding the Retry action needs beyond the navigator.
#[derive(SystemParam)]
struct RetryContext<'w> {
    finder: Res<'w, FinderState>,
    settings: Res<'w, Settings>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    paths: Res<'w, Paths>,
    history: Option<Res<'w, crate::persistence::history::History>>,
}

fn results(
    mut contexts: EguiContexts,
    last: Option<Res<LastResult>>,
    overlay: Res<Overlay>,
    mut cache: Local<ReportCache>,
    mut windows: Local<(WatchWindow, CoachCache)>,
    retry_ctx: RetryContext,
    mut nav: Navigator,
) -> Result {
    let (watch, coach) = &mut *windows;
    // Before the visibility check, so a run finished while the overlay is hidden is still
    // saved as soon as its result exists.
    if let Some(last) = &last {
        let records = retry_ctx.history.as_ref().map_or(&[][..], |h| h.runs());
        refresh(
            &mut cache,
            coach,
            last,
            &retry_ctx.paths,
            nav.library(),
            records,
        );
    }
    coach.poll();
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
    // The coach is offered only while it is enabled; nothing is drawn or sent otherwise.
    let config = &retry_ctx.settings.coach;
    let library = nav.library();
    let mut take_section = |ui: &mut egui::Ui| -> Option<UiAction> {
        let take = coach.take.as_mut()?;
        match coach_take(ui, take, |id| library.scenario(id).map(|d| d.name.clone()))? {
            TakeAction::Ask => take.ask(config),
            TakeAction::Cancel => take.cancel(),
            TakeAction::Play(id) => return Some(UiAction::StartScenario(id)),
        }
        None
    };
    let section = config
        .enabled
        .then_some(&mut take_section as &mut dyn FnMut(&mut egui::Ui) -> Option<UiAction>);
    let clicked = results_card(ctx, &last, &rc, section);
    match clicked {
        Some(UiAction::WatchMistakes) => watch.open(last.recording.as_ref(), &retry_ctx.paths),
        Some(action) => {
            watch.open = false;
            nav.apply(action);
        }
        None => {}
    }
    let why = config.enabled.then(|| WhyContext {
        config,
        whys: &mut coach.whys,
        scenario_id: &last.result.scenario_id,
        category: last.recording.as_ref().map(|r| r.def.category),
    });
    watch.show(ctx, why);
    Ok(())
}

const WATCH_TITLE: &str = "Replay";
const MISTAKES_TITLE: &str = "Mistake replays";
const NO_MISTAKES: &str = "No flick mistakes to replay in this run. Here is the full replay.";

/// What "Ask the coach why" needs in the mistake player.
struct WhyContext<'a> {
    config: &'a aim_ai::CoachConfig,
    whys: &'a mut std::collections::HashMap<usize, CoachWhy>,
    scenario_id: &'a str,
    category: Option<aim_core::scenario::Category>,
}

impl WhyContext<'_> {
    /// Draw "Ask the coach why" for mistake `index` and act on its clicks.
    fn section(
        &mut self,
        ui: &mut egui::Ui,
        index: usize,
        mistake: &aim_core::analytics::mistakes::Mistake,
    ) {
        let Some(category) = self.category else {
            return;
        };
        let digest = MistakeDigest::build(mistake, self.scenario_id, category);
        let why = self.whys.entry(index).or_default();
        match coach_why(ui, why, &digest) {
            Some(WhyAction::Ask) => why.ask(self.config, digest),
            Some(WhyAction::Cancel) => why.cancel(),
            None => {}
        }
    }
}

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

    fn show(&mut self, ctx: &egui::Context, mut why: Option<WhyContext>) {
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
                    closed = mistake_player_with(ui, state, |ui, index, mistake| {
                        if let Some(why) = why.as_mut() {
                            why.section(ui, index, mistake);
                        }
                    }) == Some(PlayerAction::Close);
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
    use super::{arena_retry_for, planned_name};
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
