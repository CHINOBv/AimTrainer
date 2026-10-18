//! Playing an arena (deathmatch or angle clearing on an original map): the same raw-count
//! look and input as scenarios, feeding `aim_world::arena::ArenaSession`, with the map's
//! boxes and the bots mirrored as meshes. All rules live in `aim_world`.

use super::feedback::{HitFrame, KILL_FLASH_SECS, MarkerKind, ShotFeedback, ShotOutcome};
use super::hit_effect::{self, EffectAssets, Struck};
use super::{
    AppState, LastResult, MainCamera, PlayerInput, TargetVisual, WorldAssets, apply_look,
    frame_input, place_camera, spawn_visual, unlit,
};
use crate::overlay::Overlay;
use crate::persistence::history::History;
use crate::persistence::settings::Settings;
use aim_core::math::ViewAngles;
use aim_core::scenario::{HitZone, Shape};
use aim_core::training::store::RunStore;
use aim_world::arena::{ArenaEvent, ArenaSession};
use aim_world::map::BoxKind;
use bevy::camera::Projection;
use bevy::ecs::system::SystemParam;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use std::collections::HashMap;

const COUNTDOWN_SECS: f32 = 3.0;

/// What happened in the running arena, for sound and HUD feedback. Translated from the
/// session's `ArenaEvent`s by the arena tick; it decides no gameplay. Positions are in
/// Bevy world space.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub enum ArenaFeedback {
    /// A whole second of the start countdown began: 3, 2, then 1.
    CountdownTick { seconds_left: u32 },
    /// The countdown ended and the arena is live.
    RoundStarted,
    /// The player took `damage` from a bot whose eye is at `from`. `zone` is the body zone
    /// the bullet hit, or `None` for a shoot-back deadline's sure shot.
    PlayerHit {
        from: Vec3,
        damage: f32,
        zone: Option<HitZone>,
    },
    /// The player died. `from` is the eye of the bot that dealt the killing damage.
    PlayerDied { from: Option<Vec3> },
    /// A bot fired a bullet at the player, hit or miss, from `origin` (its eye).
    BotFired { bot_id: u64, origin: Vec3 },
    /// The player killed a bot. `headshot` when the killing shot hit the head; `streak`
    /// counts the kills since the run started or the player last died, this one included.
    BotKilled {
        bot_id: u64,
        headshot: bool,
        streak: u32,
    },
    /// The run finished; the results screen follows.
    RoundEnded,
}

fn to_bevy(v: aim_core::math::Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

/// Turns arena events into [`ArenaFeedback`], keeping the kill streak between frames.
#[derive(Debug, Default)]
struct FeedbackTranslator {
    streak: u32,
}

impl FeedbackTranslator {
    fn translate(&mut self, events: &[ArenaEvent]) -> Vec<ArenaFeedback> {
        // The session reports a killing shot before the kill, and a hit before the death.
        let mut last_hit: Option<(u64, HitZone)> = None;
        let mut last_damage_from = None;
        let mut out = Vec::new();
        for event in events {
            match *event {
                ArenaEvent::Shot { hit, .. } => last_hit = hit.or(last_hit),
                ArenaEvent::BotKilled { id, .. } => {
                    self.streak += 1;
                    out.push(ArenaFeedback::BotKilled {
                        bot_id: id,
                        headshot: last_hit == Some((id, HitZone::Head)),
                        streak: self.streak,
                    });
                }
                ArenaEvent::PlayerHit { damage, from, zone } => {
                    let from = to_bevy(from);
                    last_damage_from = Some(from);
                    out.push(ArenaFeedback::PlayerHit { from, damage, zone });
                }
                ArenaEvent::PlayerDied => {
                    self.streak = 0;
                    out.push(ArenaFeedback::PlayerDied {
                        from: last_damage_from,
                    });
                }
                ArenaEvent::BotFired { bot_id, origin } => out.push(ArenaFeedback::BotFired {
                    bot_id,
                    origin: to_bevy(origin),
                }),
                ArenaEvent::Finished => out.push(ArenaFeedback::RoundEnded),
                ArenaEvent::BotSpawned { .. } => {}
            }
        }
        out
    }
}

/// The cue for a countdown frame that went from `before` to `after` seconds left.
fn countdown_cue(before: f32, after: f32) -> Option<ArenaFeedback> {
    if after <= 0.0 {
        return (before > 0.0).then_some(ArenaFeedback::RoundStarted);
    }
    let seconds_left = after.ceil() as u32;
    let first_frame = before >= COUNTDOWN_SECS && after < before;
    (first_frame || seconds_left < before.ceil() as u32)
        .then_some(ArenaFeedback::CountdownTick { seconds_left })
}

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
    /// The kind of hit the marker shows.
    pub marker: MarkerKind,
    /// Seconds left on the kill flash (`feel.kill_flash_arena`).
    pub kill_flash: f32,
    bots: HashMap<u64, Entity>,
    /// Bots hit this frame, for their visual effect.
    struck: Vec<Struck>,
    translator: FeedbackTranslator,
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
            marker: MarkerKind::Body,
            kill_flash: 0.0,
            bots: HashMap::new(),
            struck: Vec::new(),
            translator: FeedbackTranslator::default(),
        }
    }

    /// What the HUD draws for hit feedback this frame.
    pub fn hit_frame(&self) -> HitFrame {
        HitFrame {
            marker_secs_left: self.hit_flash,
            marker: self.marker,
            kill_flash_secs: self.kill_flash,
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
        app.add_message::<ArenaFeedback>().add_systems(
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
    drill: Res<super::SensDrill>,
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
    run.view = apply_look(run.view, &motion, k * drill.multiplier());
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
        if let Some(d) = run.session.def().mode.deadline() {
            notes.push(format!(
                "Deadline {:.1} s: {} kills before it, {} deadline hits taken",
                d.secs, stats.deadline_kills, stats.deadline_hits
            ));
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
    mut arena_feedback: MessageWriter<ArenaFeedback>,
) {
    if run.paused {
        return;
    }
    let dt = input.time.delta_secs();
    run.hit_flash = (run.hit_flash - dt).max(0.0);
    run.damage_flash = (run.damage_flash - dt).max(0.0);
    run.kill_flash = (run.kill_flash - dt).max(0.0);
    if run.countdown > 0.0 {
        let before = run.countdown;
        run.countdown -= dt;
        if let Some(cue) = countdown_cue(before, run.countdown) {
            arena_feedback.write(cue);
        }
        return;
    }
    let frame = frame_input(dt, run.view, &input.keys, &input.buttons);
    let events = run.session.update(frame);
    arena_feedback.write_batch(run.translator.translate(&events));
    let mut marker = None;
    for event in events {
        if let Some(outcome) = ShotOutcome::from_arena(&event) {
            feedback.write(ShotFeedback(outcome));
            marker = marker.max(MarkerKind::of(outcome));
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
                if finisher.settings.feel.kill_flash_arena {
                    run.kill_flash = KILL_FLASH_SECS;
                }
            }
            ArenaEvent::PlayerHit { .. } if finisher.settings.feel.damage_vignette.enabled => {
                run.damage_flash = 0.25;
            }
            ArenaEvent::Finished => finisher.finish(&run),
            _ => {}
        }
    }
    if let Some(kind) = marker {
        run.marker = kind;
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

    fn world(x: f32, y: f32, z: f32) -> aim_core::math::Vec3 {
        aim_core::math::Vec3::new(x, y, z)
    }

    #[test]
    fn arena_events_become_feedback_with_the_shooter_position() {
        let mut t = FeedbackTranslator::default();
        let events = [
            ArenaEvent::BotFired {
                bot_id: 4,
                origin: world(1.0, 1.6, -3.0),
            },
            ArenaEvent::PlayerHit {
                damage: 40.0,
                from: world(1.0, 1.6, -3.0),
                zone: Some(HitZone::Body),
            },
            ArenaEvent::PlayerDied,
            ArenaEvent::Finished,
        ];
        let from = Vec3::new(1.0, 1.6, -3.0);
        assert_eq!(
            t.translate(&events),
            [
                ArenaFeedback::BotFired {
                    bot_id: 4,
                    origin: from
                },
                ArenaFeedback::PlayerHit {
                    from,
                    damage: 40.0,
                    zone: Some(HitZone::Body)
                },
                ArenaFeedback::PlayerDied { from: Some(from) },
                ArenaFeedback::RoundEnded,
            ]
        );
    }

    #[test]
    fn kills_count_a_streak_that_a_death_resets() {
        let mut t = FeedbackTranslator::default();
        let kill = |id, zone| {
            [
                ArenaEvent::Shot {
                    hit: Some((id, zone)),
                    moving: false,
                },
                ArenaEvent::BotKilled {
                    id,
                    time_to_kill_secs: 0.5,
                },
            ]
        };
        let killed = |t: &mut FeedbackTranslator, events: &[ArenaEvent]| {
            t.translate(events)
                .into_iter()
                .filter(|f| matches!(f, ArenaFeedback::BotKilled { .. }))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            killed(&mut t, &kill(1, HitZone::Head)),
            [ArenaFeedback::BotKilled {
                bot_id: 1,
                headshot: true,
                streak: 1
            }]
        );
        assert_eq!(
            killed(&mut t, &kill(2, HitZone::Body)),
            [ArenaFeedback::BotKilled {
                bot_id: 2,
                headshot: false,
                streak: 2
            }]
        );
        t.translate(&[ArenaEvent::PlayerDied]);
        assert_eq!(
            killed(&mut t, &kill(3, HitZone::Legs)),
            [ArenaFeedback::BotKilled {
                bot_id: 3,
                headshot: false,
                streak: 1
            }]
        );
    }

    #[test]
    fn shots_spawns_and_unshot_kills_add_no_other_feedback() {
        let mut t = FeedbackTranslator::default();
        let events = [
            ArenaEvent::Shot {
                hit: None,
                moving: true,
            },
            ArenaEvent::BotSpawned { id: 9 },
            ArenaEvent::BotKilled {
                id: 9,
                time_to_kill_secs: 1.0,
            },
        ];
        assert_eq!(
            t.translate(&events),
            [ArenaFeedback::BotKilled {
                bot_id: 9,
                headshot: false,
                streak: 1
            }]
        );
    }

    #[test]
    fn the_countdown_ticks_each_second_then_starts_the_round_once() {
        let mut countdown = COUNTDOWN_SECS;
        let mut cues = Vec::new();
        for _ in 0..400 {
            if countdown <= 0.0 {
                break;
            }
            let before = countdown;
            countdown -= 1.0 / 60.0;
            cues.extend(countdown_cue(before, countdown));
        }
        assert_eq!(
            cues,
            [
                ArenaFeedback::CountdownTick { seconds_left: 3 },
                ArenaFeedback::CountdownTick { seconds_left: 2 },
                ArenaFeedback::CountdownTick { seconds_left: 1 },
                ArenaFeedback::RoundStarted,
            ]
        );
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
