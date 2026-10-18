//! Playing an arena (deathmatch or angle clearing on an original map): the same raw-count
//! look and input as scenarios, feeding `aim_world::arena::ArenaSession`, with the map's
//! boxes and the bots mirrored as meshes. All rules live in `aim_world`.

use super::feedback::{ShotFeedback, ShotOutcome};
use super::hit_effect::{self, EffectAssets, Struck};
use super::{
    AppState, LastResult, MainCamera, PlayerInput, TargetVisual, WorldAssets, apply_look,
    frame_input, place_camera, spawn_visual, unlit,
};
use crate::overlay::Overlay;
use crate::persistence::history::History;
use crate::persistence::settings::Settings;
use aim_core::math::ViewAngles;
use aim_core::scenario::Shape;
use aim_core::training::store::RunStore;
use aim_world::arena::{ArenaEvent, ArenaSession};
use aim_world::map::BoxKind;
use bevy::camera::Projection;
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use std::collections::HashMap;

const COUNTDOWN_SECS: f32 = 3.0;

#[derive(Resource)]
pub struct ArenaRun {
    pub session: ArenaSession,
    pub view: ViewAngles,
    pub countdown: f32,
    pub paused: bool,
    pub zoomed: bool,
    /// Seconds left on the hit-marker flash and on the "you got hit" vignette.
    pub hit_flash: f32,
    pub damage_flash: f32,
    bots: HashMap<u64, Entity>,
    /// Bots hit this frame, for their visual effect.
    struck: Vec<Struck>,
}

impl ArenaRun {
    pub fn new(session: ArenaSession) -> Self {
        let view = session.view();
        Self {
            session,
            view,
            countdown: COUNTDOWN_SECS,
            paused: false,
            zoomed: false,
            hit_flash: 0.0,
            damage_flash: 0.0,
            bots: HashMap::new(),
            struck: Vec::new(),
        }
    }

    fn zoom(&self) -> f64 {
        self.session.def().weapon.stats().zoom as f64
    }
}

/// Start an arena run, replacing any scenario run.
pub fn start_arena(commands: &mut Commands, next: &mut NextState<AppState>, run: ArenaRun) {
    commands.remove_resource::<super::Run>();
    commands.insert_resource(run);
    next.set(AppState::Playing);
}

#[derive(Component)]
struct MapVisual;

pub struct ArenaPlugin;

impl Plugin for ArenaPlugin {
    fn build(&self, app: &mut App) {
        let playing = in_state(AppState::Playing).and_then(resource_exists::<ArenaRun>);
        add_map_lifecycle(app);
        app.add_systems(
            Update,
            (pause_input, look, tick, sync_bots, update_camera)
                .chain()
                .run_if(playing),
        );
    }
}

/// Build the arena's map meshes every time `Playing` is entered with an arena run, and
/// remove them on leaving it. `start_arena` always sets `Playing` (`NextState::set` runs the
/// exit and enter schedules even from `Playing`), so a retry, another arena or a routine step
/// rebuilds the map. Watching for the `ArenaRun` resource to be added would miss those:
/// inserting over an existing run replaces it without counting as added.
fn add_map_lifecycle(app: &mut App) {
    app.add_systems(
        OnEnter(AppState::Playing),
        spawn_map.run_if(resource_exists::<ArenaRun>),
    )
    .add_systems(OnExit(AppState::Playing), clear_map);
}

fn spawn_map(
    mut commands: Commands,
    run: Res<ArenaRun>,
    old: Query<Entity, With<MapVisual>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for e in &old {
        commands.entity(e).despawn();
    }
    let wall = unlit(&mut materials, Color::srgba(0.32, 0.34, 0.4, 0.95));
    let cover = unlit(&mut materials, Color::srgba(0.45, 0.4, 0.3, 0.95));
    for b in &run.session.map().boxes {
        let size = b.max - b.min;
        let center = b.min + size * 0.5;
        let material = if b.kind == BoxKind::Wall {
            wall.clone()
        } else {
            cover.clone()
        };
        commands.spawn((
            MapVisual,
            Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
            MeshMaterial3d(material),
            Transform::from_xyz(center.x, center.y, center.z),
        ));
    }
}

fn clear_map(mut commands: Commands, visuals: Query<Entity, With<MapVisual>>) {
    for e in &visuals {
        commands.entity(e).despawn();
    }
}

fn pause_input(keys: Res<ButtonInput<KeyCode>>, mut run: ResMut<ArenaRun>, overlay: Res<Overlay>) {
    if keys.just_pressed(KeyCode::Escape) {
        run.paused = !run.paused;
    }
    if !overlay.visible {
        run.paused = true;
    }
}

fn look(
    motion: Res<AccumulatedMouseMotion>,
    buttons: Res<ButtonInput<MouseButton>>,
    settings: Res<Settings>,
    mut run: ResMut<ArenaRun>,
) {
    if run.paused {
        return;
    }
    run.zoomed = buttons.pressed(MouseButton::Right);
    let zoom = run.zoom();
    let k = if run.zoomed && zoom > 1.0 {
        settings
            .sensitivity
            .zoomed_deg_per_count(zoom, settings.ads_multiplier)
    } else {
        settings.sensitivity.deg_per_count()
    };
    run.view = apply_look(run.view, &motion, k);
}

/// Record the finished arena run and show the results screen.
#[derive(SystemParam)]
struct ArenaFinisher<'w, 's> {
    commands: Commands<'w, 's>,
    next: ResMut<'w, NextState<AppState>>,
    history: ResMut<'w, History>,
    settings: Res<'w, Settings>,
}

impl ArenaFinisher<'_, '_> {
    fn finish(&mut self, run: &ArenaRun) {
        let result = run.session.result();
        let previous_best = self.history.best(&result.scenario_id);
        self.history.record(&result);
        let stats = run.session.stats();
        let mut notes = super::progress_notes(&self.history, self.settings.daily_goal_minutes);
        notes.extend([format!(
            "Kills {} · deaths {} · damage taken {:.0}",
            stats.kills, stats.deaths, stats.damage_taken
        )]);
        if let Some(e) = stats.median_crosshair_error_deg() {
            notes.push(format!(
                "Crosshair placement: median {e:.1}° from the head when an enemy appeared (lower is better; under ~5° means pre-aimed)"
            ));
        }
        if let Some(h) = stats.median_head_level_error_deg() {
            notes.push(format!(
                "Head level: median {h:.1}° above or below the head at first sight"
            ));
        }
        if let Some(d) = stats.median_first_shot_delay_secs() {
            notes.push(format!(
                "Peek: median {:.0} ms from first sight to your first shot ({} engagements)",
                d * 1000.0,
                stats.first_shot_delays_secs.len()
            ));
        }
        if let Some(e) = stats.median_exposure_secs() {
            notes.push(format!(
                "Exposure: median {:.0} ms in sight of an enemy before the kill or breaking line of sight (lower is safer)",
                e * 1000.0
            ));
        }
        if let aim_world::arena::ArenaMode::AngleClear {
            pre_aim_tolerance_deg: Some(t),
            ..
        } = run.session.def().mode
        {
            notes.push(format!(
                "Pre-aimed {} of {} enemies (within {t:.0}° at first sight)",
                stats.pre_aimed,
                stats.crosshair_errors_deg.len()
            ));
        }
        if let Some(o) = run.session.objective() {
            notes.push(if o.lost {
                format!(
                    "Site lost: an attacker captured it with {} of {} attackers left",
                    o.attackers_left, o.attackers
                )
            } else {
                format!(
                    "Site held: {} of {} attackers killed",
                    o.attackers - o.attackers_left,
                    o.attackers
                )
            });
        }
        self.commands.insert_resource(LastResult {
            name: run.session.def().name.clone(),
            result,
            previous_best,
            recording: None,
            notes,
            adaptive_next: None,
            finder_test: false,
        });
        self.next.set(AppState::Results);
    }
}

fn tick(
    input: PlayerInput,
    mut run: ResMut<ArenaRun>,
    mut finisher: ArenaFinisher,
    mut feedback: MessageWriter<ShotFeedback>,
) {
    if run.paused {
        return;
    }
    let dt = input.time.delta_secs();
    run.hit_flash = (run.hit_flash - dt).max(0.0);
    run.damage_flash = (run.damage_flash - dt).max(0.0);
    if run.countdown > 0.0 {
        run.countdown -= dt;
        return;
    }
    let frame = frame_input(dt, run.view, &input.keys, &input.buttons);
    for event in run.session.update(frame) {
        if let Some(outcome) = ShotOutcome::from_arena(&event) {
            feedback.write(ShotFeedback(outcome));
        }
        match event {
            ArenaEvent::Shot {
                hit: Some((id, zone)),
                ..
            } => {
                run.hit_flash = finisher.settings.hit_feedback.marker_secs();
                hit_effect::record(&mut run.struck, id, Some(zone), false);
            }
            ArenaEvent::BotKilled { id, .. } => {
                hit_effect::record(&mut run.struck, id, None, true);
            }
            ArenaEvent::PlayerHit { .. } => run.damage_flash = 0.25,
            ArenaEvent::Finished => finisher.finish(&run),
            _ => {}
        }
    }
    // The session owns the player's view after respawns (it faces the spawn direction).
    if run.session.view() != frame.view {
        run.view = run.session.view();
    }
}

fn sync_bots(
    mut commands: Commands,
    mut run: ResMut<ArenaRun>,
    assets: Res<WorldAssets>,
    effects: Option<Res<EffectAssets>>,
    settings: Res<Settings>,
    mut transforms: Query<&mut Transform, With<TargetVisual>>,
) {
    let run = &mut *run;
    let humanoid = Shape::Humanoid { head_only: false };
    hit_effect::apply_struck(
        &mut commands,
        effects.as_deref(),
        settings.hit_feedback.target_effect,
        &mut run.struck,
        &mut run.bots,
        &transforms,
        &humanoid,
    );
    let alive: HashMap<u64, (Vec3, f32)> = run
        .session
        .bots()
        .filter(|b| b.alive)
        .map(|b| {
            (
                b.id,
                (Vec3::new(b.feet.x, b.feet.y, b.feet.z), b.facing_yaw_deg),
            )
        })
        .collect();
    run.bots.retain(|id, e| {
        let keep = alive.contains_key(id);
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
    for (id, (pos, facing)) in alive {
        let entity = *run
            .bots
            .entry(id)
            .or_insert_with(|| spawn_visual(&mut commands, &assets, &humanoid));
        let rotation = Quat::from_rotation_y(facing.to_radians());
        if let Ok(mut tf) = transforms.get_mut(entity) {
            tf.translation = pos;
            tf.rotation = rotation;
        } else {
            commands.entity(entity).insert(Transform {
                translation: pos,
                rotation,
                scale: Vec3::ONE,
            });
        }
    }
}

fn update_camera(
    run: Res<ArenaRun>,
    mut camera: Query<(&mut Transform, &mut Projection), With<MainCamera>>,
) {
    let Ok((mut tf, mut projection)) = camera.single_mut() else {
        return;
    };
    place_camera(
        &mut tf,
        &mut projection,
        run.session.eye_position(),
        run.view,
        run.zoomed,
        run.zoom(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::{RunSystemOnce, assert_is_system};

    #[test]
    fn systems_have_no_conflicting_access() {
        assert_is_system(spawn_map);
        assert_is_system(clear_map);
        assert_is_system(pause_input);
        assert_is_system(look);
        assert_is_system(tick);
        assert_is_system(sync_bots);
        assert_is_system(update_camera);
    }

    fn map_app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin)
            .init_state::<AppState>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>();
        add_map_lifecycle(&mut app);
        app
    }

    /// Start `id` through `start_arena`, as the menu and the results screen do, and return
    /// how many boxes its map has.
    fn start(app: &mut App, id: &str) -> usize {
        let (def, map) = aim_world::arena::builtin::load(id).unwrap().unwrap();
        let boxes = map.boxes.len();
        let mut run = Some(ArenaRun::new(ArenaSession::new(def, map, 1).unwrap()));
        app.world_mut()
            .run_system_once(
                move |mut commands: Commands, mut next: ResMut<NextState<AppState>>| {
                    if let Some(run) = run.take() {
                        start_arena(&mut commands, &mut next, run);
                    }
                },
            )
            .unwrap();
        app.update();
        app.update();
        boxes
    }

    fn leave(app: &mut App) {
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Results);
        app.update();
    }

    fn map_visuals(app: &mut App) -> usize {
        app.world_mut()
            .query_filtered::<(), With<MapVisual>>()
            .iter(app.world())
            .count()
    }

    #[test]
    fn every_arena_start_draws_exactly_one_map() {
        let mut app = map_app();
        let yard = start(&mut app, "yard-deathmatch");
        assert!(yard > 0);
        assert_eq!(map_visuals(&mut app), yard, "first start");

        leave(&mut app);
        assert_eq!(map_visuals(&mut app), 0, "results clear the map");

        assert_eq!(start(&mut app, "yard-deathmatch"), yard);
        assert_eq!(map_visuals(&mut app), yard, "retry of the same arena");

        leave(&mut app);
        let corridor = start(&mut app, "corridor-duels");
        assert_ne!(corridor, yard);
        assert_eq!(
            map_visuals(&mut app),
            corridor,
            "another arena after results"
        );

        // Straight from one arena into another (a routine step), without the results screen.
        let lab = start(&mut app, "angles-lab-clear");
        assert_eq!(map_visuals(&mut app), lab, "another arena while playing");
    }

    #[test]
    fn every_builtin_arena_starts_a_session() {
        for def in aim_world::arena::builtin::all() {
            let (def, map) = aim_world::arena::builtin::load(&def.id).unwrap().unwrap();
            let run = ArenaRun::new(ArenaSession::new(def, map, 1).unwrap());
            assert!(run.countdown > 0.0 && !run.paused);
        }
    }
}
