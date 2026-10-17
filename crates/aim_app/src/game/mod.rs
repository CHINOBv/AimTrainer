//! Playing a scenario: mouse look with exact degrees-per-count, feeding the core session,
//! and mirroring its targets as meshes. All gameplay rules live in `aim_core`.

use crate::overlay::Overlay;
use crate::persistence::history::History;
use crate::persistence::settings::{Paths, Settings};
use aim_core::fov;
use aim_core::math::ViewAngles;
use aim_core::movement::MoveInput;
use aim_core::scenario::{FrameInput, RunResult, ScenarioDef, Session, SessionEvent, Shape};
use aim_core::training::routine::PlannedRun;
use aim_core::valorant::HumanoidDims;
use bevy::camera::{PerspectiveProjection, Projection};
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
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
    pub name: String,
    pub plan: Vec<PlannedRun>,
    pub index: usize,
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
    entities: HashMap<u64, Entity>,
}

impl Run {
    pub fn new(def: ScenarioDef) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        Self {
            session: Session::new(def, seed),
            view: ViewAngles::default(),
            countdown: COUNTDOWN_SECS,
            paused: false,
            zoomed: false,
            hit_flash: 0.0,
            kill_flash: 0.0,
            entities: HashMap::new(),
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
}

#[derive(Component)]
struct TargetVisual;

#[derive(Component)]
pub struct MainCamera;

#[derive(Resource)]
struct WorldAssets {
    target: Handle<StandardMaterial>,
    head: Handle<StandardMaterial>,
    body: Handle<StandardMaterial>,
    unit_sphere: Handle<Mesh>,
    head_box: Handle<Mesh>,
    body_box: Handle<Mesh>,
    legs_box: Handle<Mesh>,
}

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .init_resource::<ActiveRoutine>()
            .add_systems(Startup, setup_world)
            .add_systems(
                Update,
                (pause_input, look, tick, sync_targets, update_camera)
                    .chain()
                    .run_if(in_state(AppState::Playing)),
            )
            .add_systems(Update, (manage_cursor, refresh_target_color))
            .add_systems(OnExit(AppState::Playing), clear_targets);
    }
}

/// Start a run of `def` (used by the menu and the results screen).
pub fn start_run(commands: &mut Commands, next: &mut NextState<AppState>, def: ScenarioDef) {
    commands.insert_resource(Run::new(def));
    next.set(AppState::Playing);
}

fn color(rgb: [f32; 3]) -> Color {
    Color::srgb(rgb[0], rgb[1], rgb[2])
}

fn unlit(materials: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
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
    let line = unlit(&mut materials, Color::srgba(0.35, 0.4, 0.5, 0.35));
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
    });
}

fn refresh_target_color(
    settings: Res<Settings>,
    assets: Option<Res<WorldAssets>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(assets) = assets else { return };
    if !settings.is_changed() {
        return;
    }
    for h in [&assets.target, &assets.head] {
        if let Some(mut m) = materials.get_mut(h) {
            m.base_color = color(settings.target_color);
        }
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
    if run.zoomed && zoom > 1.0 {
        settings
            .sensitivity
            .zoomed_deg_per_count(zoom, settings.ads_multiplier)
    } else {
        settings.sensitivity.deg_per_count()
    }
}

fn look(
    motion: Res<AccumulatedMouseMotion>,
    buttons: Res<ButtonInput<MouseButton>>,
    settings: Res<Settings>,
    mut run: ResMut<Run>,
) {
    if run.paused {
        return;
    }
    run.zoomed = buttons.pressed(MouseButton::Right);
    let k = deg_per_count(&settings, &run);
    // Mouse counts arrive unaccelerated (Raw Input / Wayland relative-pointer). +y is down.
    let (dx, dy) = (motion.delta.x as f64 * k, motion.delta.y as f64 * k);
    run.view = run.view.rotated(dx as f32, -dy as f32);
}

/// Raw player input for one frame.
#[derive(SystemParam)]
struct PlayerInput<'w> {
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
    paths: Res<'w, Paths>,
}

impl RunFinisher<'_, '_> {
    fn finish(&mut self, session: &Session) {
        let result = session.result();
        let previous_best = self.history.best(&result.scenario_id);
        self.history.append(&self.paths, &result);
        self.commands.insert_resource(LastResult {
            name: session.def().name.clone(),
            result,
            previous_best,
        });
        self.next.set(AppState::Results);
    }
}

fn tick(input: PlayerInput, mut run: ResMut<Run>, mut finisher: RunFinisher) {
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
    let axis = |pos: KeyCode, neg: KeyCode| {
        keys.pressed(pos) as i8 as f32 - keys.pressed(neg) as i8 as f32
    };
    let input = FrameInput {
        dt,
        view: run.view,
        fire_pressed: buttons.just_pressed(MouseButton::Left),
        fire_held: buttons.pressed(MouseButton::Left),
        movement: MoveInput {
            right: axis(KeyCode::KeyD, KeyCode::KeyA),
            forward: axis(KeyCode::KeyW, KeyCode::KeyS),
            walk: keys.pressed(KeyCode::ShiftLeft),
        },
    };
    for event in run.session.update(input) {
        match event {
            SessionEvent::Shot { hit: Some(_), .. } => run.hit_flash = 0.08,
            SessionEvent::Killed { .. } => run.kill_flash = 0.12,
            SessionEvent::Finished => finisher.finish(&run.session),
            _ => {}
        }
    }
}

fn spawn_visual(commands: &mut Commands, assets: &WorldAssets, shape: &Shape) -> Entity {
    let mut e = commands.spawn((TargetVisual, Transform::default(), Visibility::default()));
    match *shape {
        Shape::Sphere { radius } => {
            e.insert((
                Mesh3d(assets.unit_sphere.clone()),
                MeshMaterial3d(assets.target.clone()),
            ));
            e.insert(Transform::from_scale(Vec3::splat(radius)));
        }
        Shape::Humanoid { .. } => {
            let d = HumanoidDims::default();
            let torso_h = d.height - d.head_size - d.waist_height;
            e.with_children(|p| {
                p.spawn((
                    Mesh3d(assets.head_box.clone()),
                    MeshMaterial3d(assets.head.clone()),
                    Transform::from_xyz(0.0, d.head_center_height(), 0.0),
                ));
                p.spawn((
                    Mesh3d(assets.body_box.clone()),
                    MeshMaterial3d(assets.body.clone()),
                    Transform::from_xyz(0.0, d.waist_height + torso_h / 2.0, 0.0),
                ));
                p.spawn((
                    Mesh3d(assets.legs_box.clone()),
                    MeshMaterial3d(assets.body.clone()),
                    Transform::from_xyz(0.0, d.waist_height / 2.0, 0.0),
                ));
            });
        }
    }
    e.id()
}

fn sync_targets(
    mut commands: Commands,
    mut run: ResMut<Run>,
    assets: Res<WorldAssets>,
    mut transforms: Query<&mut Transform, With<TargetVisual>>,
) {
    let run = &mut *run;
    let shape = run.session.def().targets.shape.clone();
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
        let entity = *run
            .entities
            .entry(id)
            .or_insert_with(|| spawn_visual(&mut commands, &assets, &shape));
        let rotation = Quat::from_rotation_y(facing.to_radians());
        if let Ok(mut tf) = transforms.get_mut(entity) {
            tf.translation = pos;
            tf.rotation = rotation;
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
    let eye = run.session.eye_position();
    tf.translation = Vec3::new(eye.x, eye.y, eye.z);
    // Core yaw turns right (clockwise from above); Bevy's Y rotation is counter-clockwise.
    tf.rotation = Quat::from_euler(
        EulerRot::YXZ,
        -run.view.yaw_deg.to_radians(),
        run.view.pitch_deg.to_radians(),
        0.0,
    );

    let zoom = run.session.def().weapon.kind.stats().zoom as f64;
    let vfov = if run.zoomed && zoom > 1.0 {
        fov::valorant_zoomed_vertical_fov_deg(zoom)
    } else {
        fov::valorant_vertical_fov_deg()
    };
    if let Projection::Perspective(p) = &mut *projection {
        p.fov = (vfov as f32).to_radians();
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
    mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    let Ok(mut cursor) = cursors.single_mut() else {
        return;
    };
    let capture =
        overlay.visible && *state.get() == AppState::Playing && run.is_some_and(|r| !r.paused);
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

    #[test]
    fn systems_have_no_conflicting_access() {
        assert_is_system(pause_input);
        assert_is_system(look);
        assert_is_system(tick);
        assert_is_system(sync_targets);
        assert_is_system(update_camera);
        assert_is_system(manage_cursor);
        assert_is_system(refresh_target_color);
        assert_is_system(clear_targets);
        assert_is_system(setup_world);
    }
}
