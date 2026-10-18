//! Playing a scenario: mouse look with exact degrees-per-count, feeding the core session,
//! and mirroring its targets as meshes. All gameplay rules live in `aim_core`; arenas
//! (maps with bots) are in `arena`.

pub mod arena;
pub mod feedback;
pub mod hit_effect;

use crate::audio::CueHeard;
use crate::overlay::Overlay;
use crate::persistence::adaptive::AdaptiveLevels;
use crate::persistence::history::History;
use crate::persistence::missions::{Missions, MissionsPlugin};
use crate::persistence::settings::{Paths, Settings};
use aim_core::fov;
use aim_core::games::valorant::HumanoidDims;
use aim_core::math::ViewAngles;
use aim_core::movement::MoveInput;
use aim_core::scenario::{
    FrameInput, Recording, RunKey, RunOutcome, RunResult, ScenarioDef, Session, SessionEvent,
    Shape, Target, Tuning, parse_run_key,
};
use aim_core::sensitivity::Sensitivity;
use aim_core::sensitivity::finder::{SensFinder, Slot};
use aim_core::sensitivity::randomizer::SensRandomizer;
use aim_core::training::routine::PlannedRun;
use aim_core::training::store::RunStore;
use bevy::camera::{PerspectiveProjection, Projection};
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use feedback::{ShotFeedback, ShotOutcome};
use hit_effect::{BaseMaterial, EffectAssets, Struck};
use std::collections::HashMap;

const COUNTDOWN_SECS: f32 = 3.0;

#[derive(States, Debug, Default, Clone, PartialEq, Eq, Hash)]
pub enum AppState {
    #[default]
    Menu,
    Playing,
    Results,
}

/// A routine in progress: the planned runs and which one is next.
#[derive(Debug, Clone)]
pub struct RoutineProgress {
    pub routine_id: String,
    pub name: String,
    pub plan: Vec<PlannedRun>,
    pub index: usize,
    /// Difficulty level the routine is played at (a method tier's level, or 0).
    pub base_level: f32,
    /// Seed of the shuffled plan, so a split session can be resumed with the same order.
    pub seed: u64,
}

#[derive(Resource)]
pub struct Run {
    pub session: Session,
    pub view: ViewAngles,
    pub countdown: f32,
    pub paused: bool,
    pub zoomed: bool,
    /// Seconds left on the hit-marker flash.
    pub hit_flash: f32,
    pub kill_flash: f32,
    /// Definition, seed and every input fed to the session, for analysis and replays.
    pub recording: Recording,
    /// Restarts keep the seed (daily challenge: everyone gets the same targets).
    pub fixed_seed: bool,
    /// Played in adaptive mode: the result moves the scenario's difficulty staircase.
    pub adaptive: bool,
    /// Hide the overlay when the run ends (buy-phase micro drills).
    pub auto_hide: bool,
    /// Sensitivity used instead of the player's own (sensitivity finder tests).
    pub sens_override: Option<Sensitivity>,
    /// The sensitivity-finder test this run is, if any.
    pub finder_slot: Option<Slot>,
    entities: HashMap<u64, Entity>,
    /// Targets hit this frame, for their visual effect.
    struck: Vec<Struck>,
}

impl Run {
    pub fn new(def: ScenarioDef) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        Self::with_seed(def, seed)
    }

    /// A run with a fixed seed (daily challenge: everyone gets the same targets).
    pub fn with_seed(def: ScenarioDef, seed: u64) -> Self {
        let recording = Recording::new(def.clone(), seed);
        Self {
            recording,
            fixed_seed: false,
            adaptive: false,
            auto_hide: false,
            sens_override: None,
            finder_slot: None,
            session: Session::new(def, seed),
            view: ViewAngles::default(),
            countdown: COUNTDOWN_SECS,
            paused: false,
            zoomed: false,
            hit_flash: 0.0,
            kill_flash: 0.0,
            entities: HashMap::new(),
            struck: Vec::new(),
        }
    }
}

/// Current routine, if the run was started from one.
#[derive(Resource, Default)]
pub struct ActiveRoutine(pub Option<RoutineProgress>);

/// What the results screen shows.
#[derive(Resource)]
pub struct LastResult {
    pub name: String,
    pub result: RunResult,
    pub previous_best: Option<f32>,
    /// The run's input log, for post-run analysis (scenarios only).
    pub recording: Option<Recording>,
    /// Extra lines for the results screen (e.g. arena K/D and crosshair placement).
    pub notes: Vec<String>,
    /// Next adaptive level, when the run was played in adaptive mode.
    pub adaptive_next: Option<f32>,
    /// The run was a sensitivity-finder test.
    pub finder_test: bool,
}

#[derive(Component)]
pub(crate) struct TargetVisual;

#[derive(Component)]
pub struct MainCamera;

/// The player's play area as drawn: the floor slab at the player's height and, for movement
/// scenarios, low strips along the edges the feet are held inside.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
struct PlayArea {
    bounds_x: aim_core::math::Range,
    bounds_z: aim_core::math::Range,
    elevation: f32,
    edges: bool,
}

impl PlayArea {
    /// What the run's scenario needs drawn, if anything.
    fn of(def: &ScenarioDef) -> Option<Self> {
        let p = &def.player;
        let raised = p.elevation.abs() >= 0.01;
        (p.movement || raised).then_some(PlayArea {
            bounds_x: p.bounds_x,
            bounds_z: p.bounds_z,
            elevation: p.elevation,
            edges: p.movement,
        })
    }
}

/// Height of the edge strips: low enough to stay out of the line of sight.
const EDGE_HEIGHT: f32 = 0.04;
/// Width of the edge strips.
const EDGE_WIDTH: f32 = 0.08;
/// How far the slab reaches past the bounds, so the player never stands on its rim.
const SLAB_MARGIN: f32 = 0.6;
const SLAB_THICKNESS: f32 = 0.04;

#[derive(Resource)]
pub(crate) struct WorldAssets {
    target: Handle<StandardMaterial>,
    head: Handle<StandardMaterial>,
    body: Handle<StandardMaterial>,
    unit_sphere: Handle<Mesh>,
    head_box: Handle<Mesh>,
    body_box: Handle<Mesh>,
    legs_box: Handle<Mesh>,
    /// Unit cube, scaled per play-area piece.
    unit_box: Handle<Mesh>,
    edge: Handle<StandardMaterial>,
    slab: Handle<StandardMaterial>,
    /// Go/no-go decoys: a faint, see-through grey, told apart from targets by brightness
    /// and transparency rather than hue, so it works with colour-vision deficiencies.
    decoy: Handle<StandardMaterial>,
    /// Target outline: an unlit shell drawn from its back faces (inverted hull).
    outline: Handle<StandardMaterial>,
    /// Shell scale relative to the target when the outline is on.
    outline_scale: Option<f32>,
    /// Floor grid lines, tinted by the environment setting.
    grid: Handle<StandardMaterial>,
}

/// Floor grid and play-area slab colours for an environment `tint`; `None` gives the
/// original colours.
fn environment_colors(tint: Option<[f32; 3]>) -> (Color, Color) {
    match tint {
        None => (
            Color::srgba(0.35, 0.4, 0.5, 0.35),
            Color::srgba(0.35, 0.6, 0.85, 0.12),
        ),
        Some([r, g, b]) => (Color::srgba(r, g, b, 0.35), Color::srgba(r, g, b, 0.12)),
    }
}

/// The outline material: unlit, front faces culled so only the rim around the target shows.
fn outline_material(
    materials: &mut Assets<StandardMaterial>,
    rgb: [f32; 3],
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: color(rgb),
        unlit: true,
        cull_mode: Some(bevy::render::render_resource::Face::Front),
        ..default()
    })
}

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .init_resource::<ActiveRoutine>()
            .init_resource::<FinderState>()
            .add_message::<ShotFeedback>()
            .add_message::<CueHeard>()
            .add_systems(Startup, setup_world)
            .add_systems(
                Update,
                (
                    pause_input,
                    look,
                    tick,
                    sync_targets,
                    pose_humanoids,
                    update_camera,
                )
                    .chain()
                    .run_if(in_state(AppState::Playing).and_then(resource_exists::<Run>)),
            )
            .add_systems(Update, sync_play_area)
            .add_plugins((arena::ArenaPlugin, hit_effect::HitEffectPlugin))
            .add_systems(Update, (manage_cursor, refresh_target_color, fit_viewport))
            .add_systems(OnExit(AppState::Playing), clear_targets)
            .add_plugins(MissionsPlugin)
            .init_resource::<SensDrill>()
            .add_systems(Update, tick_sens_drill.before(look));
    }
}

/// The sensitivity randomiser drill while a run is played with it enabled.
#[derive(Resource, Default)]
pub struct SensDrill(pub Option<SensRandomizer>);

impl SensDrill {
    /// Multiplier for the look sensitivity: 1 when the drill is off.
    pub fn multiplier(&self) -> f64 {
        self.0.as_ref().map_or(1.0, SensRandomizer::multiplier)
    }
}

/// Start, advance or stop the drill. It runs only while playing with the setting on, never
/// in sensitivity-finder tests (they measure one exact sensitivity), and pauses with the run.
fn tick_sens_drill(
    time: Res<Time>,
    settings: Res<Settings>,
    state: Res<State<AppState>>,
    run: Option<Res<Run>>,
    arena: Option<Res<arena::ArenaRun>>,
    mut drill: ResMut<SensDrill>,
) {
    let config = settings.sens_randomizer;
    let finder_test = run.as_ref().is_some_and(|r| r.sens_override.is_some());
    if *state.get() != AppState::Playing || !config.enabled || finder_test {
        if drill.0.is_some() {
            drill.0 = None;
        }
        return;
    }
    if drill
        .0
        .as_ref()
        .is_none_or(|d| *d.config() != config.sanitized())
    {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        drill.0 = Some(SensRandomizer::new(config, seed));
    }
    let paused = run.is_some_and(|r| r.paused) || arena.is_some_and(|a| a.paused);
    if !paused && let Some(d) = drill.0.as_mut() {
        d.advance(time.delta_secs());
    }
}

/// Start a run of `def` (used by the menu and the results screen).
pub fn start_run(commands: &mut Commands, next: &mut NextState<AppState>, def: ScenarioDef) {
    start_run_with(commands, next, Run::new(def));
}

/// Start a prepared run (e.g. one flagged as adaptive).
pub fn start_run_with(commands: &mut Commands, next: &mut NextState<AppState>, run: Run) {
    // Only one kind of run is active at a time.
    commands.remove_resource::<arena::ArenaRun>();
    commands.insert_resource(run);
    next.set(AppState::Playing);
}

pub(crate) fn color(rgb: [f32; 3]) -> Color {
    Color::srgb(rgb[0], rgb[1], rgb[2])
}

/// Unlit and alpha-blended, for see-through world markers.
fn translucent(materials: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: c,
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    })
}

pub(crate) fn unlit(
    materials: &mut Assets<StandardMaterial>,
    c: Color,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: c,
        unlit: true,
        ..default()
    })
}

fn setup_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    settings: Res<Settings>,
) {
    commands.spawn((
        MainCamera,
        Camera3d::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: (fov::valorant_vertical_fov_deg() as f32).to_radians(),
            ..default()
        }),
        Transform::from_xyz(0.0, HumanoidDims::default().eye_height, 0.0),
    ));

    // Floor grid: thin unlit strips every 2 m. Cheap and keeps the background see-through.
    let (grid_color, slab_color) = environment_colors(settings.target_visibility.environment_tint);
    let line = unlit(&mut materials, grid_color);
    let along_x = meshes.add(Cuboid::new(120.0, 0.004, 0.02));
    let along_z = meshes.add(Cuboid::new(0.02, 0.004, 120.0));
    for i in -30..=30 {
        let o = i as f32 * 2.0;
        commands.spawn((
            Mesh3d(along_x.clone()),
            MeshMaterial3d(line.clone()),
            Transform::from_xyz(0.0, 0.0, o),
        ));
        commands.spawn((
            Mesh3d(along_z.clone()),
            MeshMaterial3d(line.clone()),
            Transform::from_xyz(o, 0.0, 0.0),
        ));
    }

    let d = HumanoidDims::default();
    let target_color = color(settings.target_color);
    commands.insert_resource(WorldAssets {
        target: unlit(&mut materials, target_color),
        head: unlit(&mut materials, target_color),
        body: unlit(&mut materials, Color::srgb(0.55, 0.2, 0.25)),
        unit_sphere: meshes.add(Sphere::new(1.0).mesh().ico(5).expect("valid icosphere")),
        head_box: meshes.add(Cuboid::new(d.head_size, d.head_size, d.head_size)),
        body_box: meshes.add(Cuboid::new(
            d.body_width,
            d.height - d.head_size - d.waist_height,
            d.body_depth,
        )),
        legs_box: meshes.add(Cuboid::new(
            d.body_width * 0.8,
            d.waist_height,
            d.body_depth,
        )),
        unit_box: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        edge: translucent(&mut materials, Color::srgba(0.35, 0.85, 1.0, 0.7)),
        slab: translucent(&mut materials, slab_color),
        decoy: translucent(&mut materials, Color::srgba(0.8, 0.8, 0.85, 0.28)),
        outline: outline_material(&mut materials, settings.target_visibility.outline_color),
        outline_scale: settings.target_visibility.outline_scale(),
        grid: line,
    });
}

/// Keep the drawn play area in step with the scenario run. A restart or a new run replaces
/// the `Run` resource without leaving `Playing`, so this compares what is drawn with what
/// the current run needs instead of reacting to the resource being added.
fn sync_play_area(
    mut commands: Commands,
    state: Res<State<AppState>>,
    run: Option<Res<Run>>,
    assets: Option<Res<WorldAssets>>,
    drawn: Query<(Entity, &PlayArea)>,
) {
    let Some(assets) = assets else { return };
    let wanted = run
        .filter(|_| *state.get() == AppState::Playing)
        .and_then(|r| PlayArea::of(r.session.def()));
    let mut current = None;
    for (e, area) in &drawn {
        if Some(*area) == wanted && current.is_none() {
            current = Some(e);
        } else {
            commands.entity(e).despawn();
        }
    }
    if let (Some(area), None) = (wanted, current) {
        spawn_play_area(&mut commands, &assets, area);
    }
}

fn spawn_play_area(commands: &mut Commands, assets: &WorldAssets, area: PlayArea) {
    let (bx, bz, y) = (area.bounds_x, area.bounds_z, area.elevation);
    let piece = |material: &Handle<StandardMaterial>, center: Vec3, size: Vec3| {
        (
            Mesh3d(assets.unit_box.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(center).with_scale(size),
        )
    };
    let (cx, cz) = ((bx.min + bx.max) / 2.0, (bz.min + bz.max) / 2.0);
    let (w, d) = (bx.max - bx.min, bz.max - bz.min);
    commands
        .spawn((area, Transform::default(), Visibility::default()))
        .with_children(|p| {
            if y.abs() >= 0.01 {
                p.spawn(piece(
                    &assets.slab,
                    Vec3::new(cx, y - SLAB_THICKNESS / 2.0, cz),
                    Vec3::new(w + 2.0 * SLAB_MARGIN, SLAB_THICKNESS, d + 2.0 * SLAB_MARGIN),
                ));
            }
            if area.edges {
                let h = Vec3::new(0.0, y + EDGE_HEIGHT / 2.0, 0.0);
                let along_x = Vec3::new(w + EDGE_WIDTH, EDGE_HEIGHT, EDGE_WIDTH);
                let along_z = Vec3::new(EDGE_WIDTH, EDGE_HEIGHT, d + EDGE_WIDTH);
                for z in [bz.min, bz.max] {
                    p.spawn(piece(&assets.edge, h + Vec3::new(cx, 0.0, z), along_x));
                }
                for x in [bx.min, bx.max] {
                    p.spawn(piece(&assets.edge, h + Vec3::new(x, 0.0, cz), along_z));
                }
            }
        });
}

fn refresh_target_color(
    settings: Res<Settings>,
    assets: Option<ResMut<WorldAssets>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(mut assets) = assets else { return };
    if !settings.is_changed() {
        return;
    }
    for h in [&assets.target, &assets.head] {
        if let Some(mut m) = materials.get_mut(h) {
            m.base_color = color(settings.target_color);
        }
    }
    // Visibility options; a new outline setting applies to targets spawned from now on.
    let vis = &settings.target_visibility;
    let (grid, slab) = environment_colors(vis.environment_tint);
    for (h, c) in [
        (&assets.outline, color(vis.outline_color)),
        (&assets.grid, grid),
        (&assets.slab, slab),
    ] {
        if let Some(mut m) = materials.get_mut(h)
            && m.base_color != c
        {
            m.base_color = c;
        }
    }
    if assets.outline_scale != vis.outline_scale() {
        assets.outline_scale = vis.outline_scale();
    }
}

fn pause_input(keys: Res<ButtonInput<KeyCode>>, mut run: ResMut<Run>, overlay: Res<Overlay>) {
    if keys.just_pressed(KeyCode::Escape) {
        run.paused = !run.paused;
    }
    if !overlay.visible {
        run.paused = true;
    }
}

/// Degrees per count for the current zoom state.
fn deg_per_count(settings: &Settings, run: &Run) -> f64 {
    let zoom = run.session.def().weapon.kind.stats().zoom as f64;
    let sens = run.sens_override.unwrap_or(settings.sensitivity);
    if run.zoomed && zoom > 1.0 {
        sens.zoomed_deg_per_count(zoom, settings.ads_multiplier)
    } else {
        sens.deg_per_count()
    }
}

fn look(
    motion: Res<AccumulatedMouseMotion>,
    buttons: Res<ButtonInput<MouseButton>>,
    settings: Res<Settings>,
    drill: Res<SensDrill>,
    mut run: ResMut<Run>,
) {
    if run.paused {
        return;
    }
    run.zoomed = buttons.pressed(MouseButton::Right);
    let k = deg_per_count(&settings, &run) * drill.multiplier();
    run.view = apply_look(run.view, &motion, k);
}

/// Rotate `view` by this frame's raw mouse counts at `deg_per_count`. Counts arrive
/// unaccelerated (Raw Input / Wayland relative-pointer); +y is down.
pub(crate) fn apply_look(
    view: ViewAngles,
    motion: &AccumulatedMouseMotion,
    deg_per_count: f64,
) -> ViewAngles {
    let (dx, dy) = (
        motion.delta.x as f64 * deg_per_count,
        motion.delta.y as f64 * deg_per_count,
    );
    view.rotated(dx as f32, -dy as f32)
}

/// Raw player input for one frame.
#[derive(SystemParam)]
pub(crate) struct PlayerInput<'w> {
    time: Res<'w, Time>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    buttons: Res<'w, ButtonInput<MouseButton>>,
}

/// Everything needed to close a finished run: record it and show the results screen.
#[derive(SystemParam)]
struct RunFinisher<'w, 's> {
    commands: Commands<'w, 's>,
    next: ResMut<'w, NextState<AppState>>,
    history: ResMut<'w, History>,
    adaptive: ResMut<'w, AdaptiveLevels>,
    overlay: ResMut<'w, Overlay>,
    settings: Res<'w, Settings>,
    finder: ResMut<'w, FinderState>,
    paths: Res<'w, Paths>,
    missions: ResMut<'w, Missions>,
}

/// The sensitivity finder in progress (session only; not persisted).
#[derive(Resource, Default)]
pub struct FinderState(pub Option<SensFinder>);

/// Scenario and length of a sensitivity-finder test: wide switching mixes flicks and
/// control, which is where a sensitivity that is too fast or too slow shows first.
pub const FINDER_SCENARIO: &str = "switch-wide";
pub const FINDER_SECS: f32 = 30.0;

impl RunFinisher<'_, '_> {
    fn finish(&mut self, run: &Run) {
        let session = &run.session;
        let result = session.result();
        let previous_best = self.history.best(&result.scenario_id);
        let mut notes = progress_notes(&self.history, self.settings.daily_goal_minutes);
        if let (Some(slot), Some(finder)) = (run.finder_slot, self.finder.0.as_mut()) {
            finder.record(slot, result.score);
            notes.insert(0, finder_note(finder));
        } else {
            // Finder test runs do not enter the regular history.
            self.history.record(&result);
            self.missions
                .record(&result, session.stats().hits, &self.paths);
        }
        if let Some(note) = outcome_note(session) {
            notes.insert(0, note);
        }
        let tuning = RunKey::parse(&session.def().id).tuning;
        if !tuning.is_default() {
            let mut note = tuning_text(&tuning);
            if run.adaptive {
                note.push_str(" · tuned runs do not move your adaptive level");
            }
            notes.insert(0, note);
        }
        let adaptive_next = moves_staircase(run).then(|| {
            let (base, _) = parse_run_key(&session.def().id);
            let next = self.adaptive.record(base, session.success_rate());
            self.adaptive.save(&self.paths);
            next
        });
        self.commands.insert_resource(LastResult {
            name: session.def().name.clone(),
            result,
            previous_best,
            recording: Some(run.recording.clone()),
            notes,
            finder_test: run.finder_slot.is_some(),
            adaptive_next,
        });
        self.next.set(AppState::Results);
        if run.auto_hide {
            self.overlay.visible = false;
        }
    }
}

/// Whether a finished `run` moves its scenario's adaptive staircase: adaptive runs played
/// as the scenario is. Tuned runs (target size, distance or speed) are stored under their
/// own key and never move it.
pub fn moves_staircase(run: &Run) -> bool {
    run.adaptive && RunKey::parse(&run.session.def().id).tuning.is_default()
}

/// How a finished run ended, for scenarios with phases, fail conditions or a non-time end,
/// e.g. "Failed in phase 3/5 (Strafing trio): too many misses". `None` for a plain run that
/// reached its time.
pub fn outcome_note(session: &Session) -> Option<String> {
    let def = session.def();
    let rules = def.phases.is_some() || !def.fail.is_empty() || !def.end.is_time();
    let outcome = session.outcome().filter(|_| rules)?;
    let phase = session.phase_index().map(|i| {
        let name = def
            .phases
            .as_ref()
            .map_or_else(String::new, |p| p[i].label(i));
        format!("phase {}/{} ({name})", i + 1, session.phase_count())
    });
    Some(match (outcome, phase) {
        (RunOutcome::Completed, Some(p)) => format!("Completed · reached {p}"),
        (RunOutcome::Completed, None) => "Completed".to_owned(),
        (RunOutcome::Failed(r), Some(p)) => format!("Failed in {p}: {}", r.label()),
        (RunOutcome::Failed(r), None) => format!("Failed: {}", r.label()),
    })
}

/// The tuning of a run, e.g. "Tuned: size 120% · distance 80% · speed 150%".
pub fn tuning_text(t: &Tuning) -> String {
    format!(
        "Tuned: size {}% · distance {}% · speed {}%",
        t.size_pct, t.distance_pct, t.speed_pct
    )
}

/// Where the sensitivity finder stands, for the results screen.
pub(crate) fn finder_note(f: &SensFinder) -> String {
    if f.is_done() {
        format!(
            "Sensitivity finder done: about {:.1} cm/360 suits you best. Apply it in Settings.",
            f.center_cm
        )
    } else {
        format!(
            "Sensitivity finder round {}/{}: comparing {:.1} vs {:.1} cm/360.",
            f.round + 1,
            aim_core::sensitivity::finder::MAX_ROUNDS,
            f.cm_for(Slot::Fast),
            f.cm_for(Slot::Slow)
        )
    }
}

/// Daily-goal and fatigue lines for the results screen.
pub(crate) fn progress_notes(history: &History, goal_minutes: f32) -> Vec<String> {
    use aim_core::training::progress;
    let records = history.runs();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let offset = chrono::Local::now().offset().local_minus_utc() as i64;
    let mut notes = vec![crate::ui::progress_goal_text(&progress::daily_goal(
        records,
        goal_minutes,
        now,
        offset,
    ))];
    notes.extend(
        progress::fatigue(progress::current_session(records))
            .iter()
            .map(crate::ui::progress_fatigue_text),
    );
    notes
}

fn tick(
    input: PlayerInput,
    mut run: ResMut<Run>,
    mut finisher: RunFinisher,
    mut feedback: MessageWriter<ShotFeedback>,
    mut cues: MessageWriter<CueHeard>,
) {
    let PlayerInput {
        time,
        keys,
        buttons,
    } = input;
    if run.paused {
        return;
    }
    let dt = time.delta_secs();
    run.hit_flash = (run.hit_flash - dt).max(0.0);
    run.kill_flash = (run.kill_flash - dt).max(0.0);
    if run.countdown > 0.0 {
        run.countdown -= dt;
        return;
    }
    let input = frame_input(dt, run.view, &keys, &buttons);
    run.recording.inputs.push(input);
    for event in run.session.update(input) {
        if let Some(outcome) = ShotOutcome::from_session(&event) {
            feedback.write(ShotFeedback(outcome));
        }
        match event {
            SessionEvent::Shot {
                hit: Some((id, zone)),
                ..
            } => {
                run.hit_flash = finisher.settings.hit_feedback.marker_secs();
                hit_effect::record(&mut run.struck, id, Some(zone), false);
            }
            SessionEvent::Killed { id, .. } => {
                run.kill_flash = 0.12;
                hit_effect::record(&mut run.struck, id, None, true);
            }
            SessionEvent::Finished => finisher.finish(&run),
            SessionEvent::Cue {
                position, sound, ..
            } => {
                let eye = run.session.eye_position();
                cues.write(CueHeard::new(sound, eye, run.view, position));
            }
            _ => {}
        }
    }
}

/// The core's per-frame input from keys and buttons (WASD, Shift walk, left click).
pub(crate) fn frame_input(
    dt: f32,
    view: ViewAngles,
    keys: &ButtonInput<KeyCode>,
    buttons: &ButtonInput<MouseButton>,
) -> FrameInput {
    let axis = |pos: KeyCode, neg: KeyCode| {
        keys.pressed(pos) as i8 as f32 - keys.pressed(neg) as i8 as f32
    };
    FrameInput {
        dt,
        view,
        fire_pressed: buttons.just_pressed(MouseButton::Left),
        fire_held: buttons.pressed(MouseButton::Left),
        movement: MoveInput {
            right: axis(KeyCode::KeyD, KeyCode::KeyA),
            forward: axis(KeyCode::KeyW, KeyCode::KeyS),
            walk: keys.pressed(KeyCode::ShiftLeft),
        },
    }
}

/// A target mesh's material, remembered so hit effects can restore it.
fn material(h: &Handle<StandardMaterial>) -> (MeshMaterial3d<StandardMaterial>, BaseMaterial) {
    (MeshMaterial3d(h.clone()), BaseMaterial(h.clone()))
}

/// One box of a humanoid target visual, posed from the core hitbox each frame.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HumanoidPart {
    Head,
    Body,
    Legs,
}

impl HumanoidPart {
    /// Local transform that lines the box (meshed at standing size) up with `d`'s hitbox.
    fn transform(self, d: &HumanoidDims) -> Transform {
        let standing = HumanoidDims::default();
        let neck = d.height - d.head_size;
        let standing_torso = standing.height - standing.head_size - standing.waist_height;
        match self {
            Self::Head => Transform::from_xyz(0.0, d.head_center_height(), 0.0),
            Self::Body => {
                let torso = neck - d.waist_height;
                Transform::from_xyz(0.0, d.waist_height + torso / 2.0, 0.0).with_scale(Vec3::new(
                    1.0,
                    torso / standing_torso,
                    1.0,
                ))
            }
            Self::Legs => Transform::from_xyz(0.0, d.waist_height / 2.0, 0.0)
                .with_scale(Vec3::new(1.0, d.waist_height / standing.waist_height, 1.0)),
        }
    }
}

/// Lower crouching humanoid targets: each box follows the target's crouched hitbox.
/// Jumps need nothing here, since the target's feet position already carries them.
fn pose_humanoids(run: Res<Run>, mut parts: Query<(&HumanoidPart, &ChildOf, &mut Transform)>) {
    let targets: HashMap<Entity, &Target> = run
        .session
        .targets()
        .filter_map(|t| run.entities.get(&t.id).map(|e| (*e, t)))
        .collect();
    for (part, parent, mut tf) in &mut parts {
        if let Some(t) = targets.get(&parent.parent()) {
            let posed = part.transform(&run.session.target_dims(t));
            if *tf != posed {
                *tf = posed;
            }
        }
    }
}

pub(crate) fn spawn_visual(commands: &mut Commands, assets: &WorldAssets, shape: &Shape) -> Entity {
    spawn_target_visual(commands, assets, shape, false)
}

/// A target visual; a go/no-go `decoy` uses the decoy material on every part.
fn spawn_target_visual(
    commands: &mut Commands,
    assets: &WorldAssets,
    shape: &Shape,
    decoy: bool,
) -> Entity {
    let pick = |normal| if decoy { &assets.decoy } else { normal };
    // Decoys never get an outline: they must stay faint.
    let outline = assets.outline_scale.filter(|_| !decoy).map(|s| {
        (
            assets.outline.clone(),
            Transform::from_scale(Vec3::splat(s)),
        )
    });
    let mut e = commands.spawn((TargetVisual, Transform::default(), Visibility::default()));
    match *shape {
        Shape::Sphere { radius } => {
            e.insert((
                Mesh3d(assets.unit_sphere.clone()),
                material(pick(&assets.target)),
            ));
            e.insert(Transform::from_scale(Vec3::splat(radius)));
            if let Some((mat, tf)) = outline {
                e.with_child((Mesh3d(assets.unit_sphere.clone()), MeshMaterial3d(mat), tf));
            }
        }
        Shape::Humanoid { .. } => {
            let d = HumanoidDims::default();
            e.with_children(|p| {
                for (part, mesh, mat) in [
                    (HumanoidPart::Head, &assets.head_box, pick(&assets.head)),
                    (HumanoidPart::Body, &assets.body_box, pick(&assets.body)),
                    (HumanoidPart::Legs, &assets.legs_box, pick(&assets.body)),
                ] {
                    let mut part = p.spawn((
                        part,
                        Mesh3d(mesh.clone()),
                        material(mat),
                        part.transform(&d),
                    ));
                    if let Some((mat, tf)) = &outline {
                        part.with_child((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone()), *tf));
                    }
                }
            });
        }
    }
    e.id()
}

fn sync_targets(
    mut commands: Commands,
    mut run: ResMut<Run>,
    assets: Res<WorldAssets>,
    effects: Option<Res<EffectAssets>>,
    settings: Res<Settings>,
    mut transforms: Query<&mut Transform, With<TargetVisual>>,
) {
    let run = &mut *run;
    // The current phase may resize targets.
    let shape = run.session.active_targets().shape.clone();
    hit_effect::apply_struck(
        &mut commands,
        effects.as_deref(),
        settings.hit_feedback.target_effect,
        &mut run.struck,
        &mut run.entities,
        &transforms,
        &shape,
    );
    let shrinking = run.session.def().targets.shrink_per_kill.is_some();
    // Radar decoys look like every other target: only the radar tells them apart.
    let disguised = run
        .session
        .def()
        .targets
        .perception
        .as_ref()
        .is_some_and(|p| p.disguises_decoys());
    let decoys: Vec<u64> = run
        .session
        .targets()
        .filter(|t| t.decoy && !disguised)
        .map(|t| t.id)
        .collect();
    let alive: HashMap<u64, (Vec3, f32)> = run
        .session
        .targets()
        .map(|t| {
            (
                t.id,
                (
                    Vec3::new(t.position.x, t.position.y, t.position.z),
                    t.facing_yaw_deg,
                ),
            )
        })
        .collect();

    run.entities.retain(|id, e| {
        let keep = alive.contains_key(id);
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
    for (id, (pos, facing)) in alive {
        let entity = *run.entities.entry(id).or_insert_with(|| {
            spawn_target_visual(&mut commands, &assets, &shape, decoys.contains(&id))
        });
        let rotation = Quat::from_rotation_y(facing.to_radians());
        if let Ok(mut tf) = transforms.get_mut(entity) {
            tf.translation = pos;
            tf.rotation = rotation;
            // Shrinking spheres (`targets.shrink_per_kill`) follow the active radius.
            if let (true, Shape::Sphere { radius }) = (shrinking, &shape) {
                tf.scale = Vec3::splat(*radius);
            }
        } else {
            // Spawned this frame: commands have not been applied yet.
            let scale = match shape {
                Shape::Sphere { radius } => Vec3::splat(radius),
                Shape::Humanoid { .. } => Vec3::ONE,
            };
            commands.entity(entity).insert(Transform {
                translation: pos,
                rotation,
                scale,
            });
        }
    }
}

fn update_camera(
    run: Res<Run>,
    mut camera: Query<(&mut Transform, &mut Projection), With<MainCamera>>,
) {
    let Ok((mut tf, mut projection)) = camera.single_mut() else {
        return;
    };
    let zoom = run.session.def().weapon.kind.stats().zoom as f64;
    place_camera(
        &mut tf,
        &mut projection,
        run.session.eye_position(),
        run.view,
        run.zoomed,
        zoom,
    );
}

/// Put the camera at `eye` looking along `view`, with VALORANT's FOV for the zoom state.
pub(crate) fn place_camera(
    tf: &mut Transform,
    projection: &mut Projection,
    eye: aim_core::math::Vec3,
    view: ViewAngles,
    zoomed: bool,
    zoom: f64,
) {
    tf.translation = Vec3::new(eye.x, eye.y, eye.z);
    // Core yaw turns right (clockwise from above); Bevy's Y rotation is counter-clockwise.
    tf.rotation = Quat::from_euler(
        EulerRot::YXZ,
        -view.yaw_deg.to_radians(),
        view.pitch_deg.to_radians(),
        0.0,
    );
    let vfov = if zoomed && zoom > 1.0 {
        fov::valorant_zoomed_vertical_fov_deg(zoom)
    } else {
        fov::valorant_vertical_fov_deg()
    };
    if let Projection::Perspective(p) = projection {
        p.fov = (vfov as f32).to_radians();
    }
}

/// Match VALORANT on ultrawide windows: render a centered 16:9 viewport (pillarbox).
fn fit_viewport(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cameras: Query<&mut Camera, With<MainCamera>>,
) {
    let (Ok(window), Ok(mut camera)) = (windows.single(), cameras.single_mut()) else {
        return;
    };
    let (w, h) = (window.physical_width(), window.physical_height());
    let (vw, _) = fov::valorant_viewport(w as f64, h as f64);
    let vw = vw.round() as u32;
    let wanted = (vw < w).then(|| bevy::camera::Viewport {
        physical_position: UVec2::new((w - vw) / 2, 0),
        physical_size: UVec2::new(vw, h),
        ..default()
    });
    let current = camera
        .viewport
        .as_ref()
        .map(|v| (v.physical_position, v.physical_size));
    if current
        != wanted
            .as_ref()
            .map(|v| (v.physical_position, v.physical_size))
    {
        camera.viewport = wanted;
    }
}

fn clear_targets(mut commands: Commands, visuals: Query<Entity, With<TargetVisual>>) {
    for e in &visuals {
        commands.entity(e).despawn();
    }
}

fn manage_cursor(
    state: Res<State<AppState>>,
    overlay: Res<Overlay>,
    run: Option<Res<Run>>,
    arena: Option<Res<arena::ArenaRun>>,
    mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let Ok(mut cursor) = cursors.single_mut() else {
        return;
    };
    let active = run.is_some_and(|r| !r.paused) || arena.is_some_and(|a| !a.paused);
    let capture = overlay.visible && *state.get() == AppState::Playing && active;
    let (grab, visible) = if capture {
        (CursorGrabMode::Locked, false)
    } else {
        (CursorGrabMode::None, true)
    };
    if cursor.grab_mode != grab {
        cursor.grab_mode = grab;
    }
    if cursor.visible != visible {
        cursor.visible = visible;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::assert_is_system;

    /// Plays `def` idle (looking forward, firing every frame) until it ends.
    fn played(def: ScenarioDef) -> Session {
        let mut session = Session::new(def, 1);
        while !session.is_finished() {
            session.update(FrameInput {
                dt: 0.2,
                view: ViewAngles::new(180.0, 0.0),
                fire_pressed: true,
                ..Default::default()
            });
        }
        session
    }

    #[test]
    fn the_results_note_names_the_phase_reached_and_how_the_run_ended() {
        let plain = aim_core::scenario::builtin::all()
            .into_iter()
            .find(|d| d.id == "flick-wide")
            .unwrap()
            .with_duration(2.0);
        assert_eq!(outcome_note(&played(plain)), None);
        let ladder = aim_core::scenario::builtin::all()
            .into_iter()
            .find(|d| d.id == "phase-peek-ladder")
            .unwrap();
        assert_eq!(
            outcome_note(&played(ladder)).as_deref(),
            Some("Failed in phase 1/5 (Close trio): too many misses")
        );
        let race = aim_core::scenario::builtin::all()
            .into_iter()
            .find(|d| d.id == "phase-kill-race")
            .unwrap()
            .with_duration(1.0);
        assert_eq!(
            outcome_note(&played(race)).as_deref(),
            Some("Failed in phase 1/2 (Warm-up): ran out of time")
        );
    }

    #[test]
    fn no_environment_tint_keeps_the_original_grid_and_slab_colours() {
        assert_eq!(
            environment_colors(None),
            (
                Color::srgba(0.35, 0.4, 0.5, 0.35),
                Color::srgba(0.35, 0.6, 0.85, 0.12)
            )
        );
        let (grid, slab) = environment_colors(Some([0.1, 0.2, 0.3]));
        assert_eq!(grid, Color::srgba(0.1, 0.2, 0.3, 0.35));
        assert_eq!(slab, Color::srgba(0.1, 0.2, 0.3, 0.12));
    }

    #[test]
    fn systems_have_no_conflicting_access() {
        assert_is_system(pause_input);
        assert_is_system(look);
        assert_is_system(tick_sens_drill);
        assert_is_system(tick);
        assert_is_system(sync_targets);
        assert_is_system(pose_humanoids);
        assert_is_system(update_camera);
        assert_is_system(manage_cursor);
        assert_is_system(refresh_target_color);
        assert_is_system(clear_targets);
        assert_is_system(fit_viewport);
        assert_is_system(setup_world);
        assert_is_system(sync_play_area);
    }

    #[test]
    fn only_untuned_adaptive_runs_move_the_staircase() {
        let def = aim_core::scenario::builtin::all()
            .into_iter()
            .find(|d| d.id == "micro-adjust")
            .unwrap()
            .at_difficulty(0.25);
        let tuned = def.with_tuning(Tuning {
            size_pct: 150,
            ..Tuning::DEFAULT
        });
        let run = |def: &ScenarioDef, adaptive: bool| {
            let mut r = Run::with_seed(def.clone(), 1);
            r.adaptive = adaptive;
            r
        };
        assert!(moves_staircase(&run(&def, true)));
        assert!(!moves_staircase(&run(&def, false)));
        assert!(!moves_staircase(&run(&tuned, true)), "tuned runs never do");
        assert_eq!(
            tuning_text(&RunKey::parse(&tuned.id).tuning),
            "Tuned: size 150% · distance 100% · speed 100%"
        );
    }

    fn movement_def() -> ScenarioDef {
        aim_core::scenario::builtin::all()
            .into_iter()
            .find(|d| d.id == "counter-strafe")
            .unwrap()
    }

    #[test]
    fn play_areas_are_drawn_for_movement_or_raised_floors_only() {
        let flat = aim_core::scenario::builtin::all()
            .into_iter()
            .find(|d| !d.player.movement)
            .unwrap();
        assert_eq!(PlayArea::of(&flat), None);
        let mut raised = flat.clone();
        raised.player.elevation = 1.5;
        let area = PlayArea::of(&raised).unwrap();
        assert!(!area.edges, "no edges without movement");
        let moving = PlayArea::of(&movement_def()).unwrap();
        assert!(moving.edges);
        assert_ne!(
            moving,
            PlayArea::of(&movement_def().with_elevation(2.0)).unwrap(),
            "a new height rebuilds the area"
        );
    }
}
