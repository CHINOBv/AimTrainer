//! Target hit effects: a brief flash, pop, fade or ring burst on the visual of the target
//! that was hit. A killed target plays the effect before it disappears. Every material is
//! built once; effects only swap handles and scale transforms.

use super::feedback::{TargetEffect, alpha_step, fade_alpha, pop_scale, ring_scale_alpha};
use super::{MainCamera, TargetVisual, color};
use crate::persistence::settings::Settings;
use aim_core::games::valorant::HumanoidDims;
use aim_core::scenario::{HitZone, Shape};
use bevy::prelude::*;
use std::collections::HashMap;

/// Pre-built opacity levels for fading targets and ring bursts.
const ALPHA_STEPS: usize = 4;
/// Tube thickness of the ring burst, as a fraction of its radius.
const RING_TUBE: f32 = 0.05;

/// A target the player hit this frame (or killed), waiting for its visual effect.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Struck {
    pub id: u64,
    pub zone: Option<HitZone>,
    pub killed: bool,
}

/// Note a hit or a kill of target `id`; a kill folds into the hit of the same frame.
pub(crate) fn record(struck: &mut Vec<Struck>, id: u64, zone: Option<HitZone>, killed: bool) {
    match struck.iter_mut().find(|s| s.id == id) {
        Some(s) => {
            s.killed |= killed;
            s.zone = s.zone.or(zone);
        }
        None => struck.push(Struck { id, zone, killed }),
    }
}

/// Where on a target the effect is centered: height above the visual's origin and the
/// ring radius, meters.
pub(crate) fn anchor(shape: &Shape, zone: Option<HitZone>) -> (f32, f32) {
    let d = HumanoidDims::default();
    match (shape, zone) {
        (Shape::Sphere { radius }, _) => (0.0, *radius),
        (Shape::Humanoid { .. }, Some(HitZone::Head)) => (d.head_center_height(), d.head_size),
        (Shape::Humanoid { .. }, Some(HitZone::Legs)) => (d.waist_height / 2.0, d.body_width / 2.0),
        (Shape::Humanoid { .. }, _) => (
            (d.waist_height + d.height - d.head_size) / 2.0,
            d.body_width / 2.0,
        ),
    }
}

/// The material a target mesh is drawn with when no effect is playing.
#[derive(Component, Debug, Clone)]
pub(crate) struct BaseMaterial(pub Handle<StandardMaterial>);

/// An effect playing on a target visual.
#[derive(Component, Debug, Clone, Copy)]
pub(crate) struct HitEffect {
    style: TargetEffect,
    elapsed: f32,
    killed: bool,
    base_scale: Vec3,
}

/// An expanding ring left where a target was hit.
#[derive(Component, Debug, Clone, Copy)]
struct RingBurst {
    elapsed: f32,
    radius: f32,
}

#[derive(Resource)]
pub(crate) struct EffectAssets {
    flash: Handle<StandardMaterial>,
    /// Target color, faintest first.
    fade: [Handle<StandardMaterial>; ALPHA_STEPS],
    ring: [Handle<StandardMaterial>; ALPHA_STEPS],
    ring_mesh: Handle<Mesh>,
}

/// How a hit is shown on one target visual.
pub(crate) struct Strike {
    pub entity: Entity,
    pub transform: Transform,
    /// The visual's scale with no effect playing.
    pub base_scale: Vec3,
    /// From [`anchor`].
    pub anchor: (f32, f32),
    pub killed: bool,
}

/// Start `effect` for a hit target. A killed target is despawned now when the effect does
/// not show on it, or when the effect ends otherwise; the caller stops tracking it.
pub(crate) fn strike(
    commands: &mut Commands,
    assets: &EffectAssets,
    effect: TargetEffect,
    hit: Strike,
) {
    let (height, radius) = hit.anchor;
    match effect {
        TargetEffect::Off | TargetEffect::Ring => {
            if effect == TargetEffect::Ring {
                commands.spawn((
                    TargetVisual,
                    RingBurst {
                        elapsed: 0.0,
                        radius,
                    },
                    Mesh3d(assets.ring_mesh.clone()),
                    MeshMaterial3d(assets.ring[ALPHA_STEPS - 1].clone()),
                    Transform::from_translation(hit.transform.translation + Vec3::Y * height)
                        .with_scale(Vec3::splat(radius)),
                ));
            }
            if hit.killed {
                commands.entity(hit.entity).despawn();
            }
        }
        TargetEffect::Flash | TargetEffect::Pop | TargetEffect::Fade => {
            commands.entity(hit.entity).insert(HitEffect {
                style: effect,
                elapsed: 0.0,
                killed: hit.killed,
                base_scale: hit.base_scale,
            });
        }
    }
}

/// Start the effects of this frame's `struck` targets on their `visuals` (target id →
/// entity). Killed targets leave `visuals`: the effect owns them from now on.
pub(crate) fn apply_struck(
    commands: &mut Commands,
    assets: Option<&EffectAssets>,
    effect: TargetEffect,
    struck: &mut Vec<Struck>,
    visuals: &mut HashMap<u64, Entity>,
    transforms: &Query<&mut Transform, With<TargetVisual>>,
    shape: &Shape,
) {
    let Some(assets) = assets else {
        struck.clear();
        return;
    };
    let base_scale = match *shape {
        Shape::Sphere { radius } => Vec3::splat(radius),
        Shape::Humanoid { .. } => Vec3::ONE,
    };
    for s in struck.drain(..) {
        let Some(&entity) = visuals.get(&s.id) else {
            continue;
        };
        let Ok(transform) = transforms.get(entity) else {
            continue;
        };
        if s.killed {
            visuals.remove(&s.id);
        }
        let hit = Strike {
            entity,
            transform: *transform,
            base_scale,
            anchor: anchor(shape, s.zone),
            killed: s.killed,
        };
        strike(commands, assets, effect, hit);
    }
}

pub struct HitEffectPlugin;

impl Plugin for HitEffectPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_effect_assets).add_systems(
            Update,
            (animate_hit_effects, animate_rings, refresh_fade_color)
                .run_if(resource_exists::<EffectAssets>),
        );
    }
}

fn translucent(materials: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: c,
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    })
}

fn step_alpha(i: usize) -> f32 {
    (i + 1) as f32 / ALPHA_STEPS as f32
}

fn setup_effect_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    settings: Res<Settings>,
) {
    let target = color(settings.target_color);
    commands.insert_resource(EffectAssets {
        flash: super::unlit(&mut materials, Color::WHITE),
        fade: std::array::from_fn(|i| {
            translucent(&mut materials, target.with_alpha(step_alpha(i)))
        }),
        ring: std::array::from_fn(|i| {
            translucent(&mut materials, Color::WHITE.with_alpha(step_alpha(i)))
        }),
        ring_mesh: meshes.add(Torus {
            minor_radius: RING_TUBE,
            major_radius: 1.0,
        }),
    });
}

fn refresh_fade_color(
    settings: Res<Settings>,
    assets: Res<EffectAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !settings.is_changed() {
        return;
    }
    for (i, h) in assets.fade.iter().enumerate() {
        if let Some(mut m) = materials.get_mut(h) {
            m.base_color = color(settings.target_color).with_alpha(step_alpha(i));
        }
    }
}

fn animate_hit_effects(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<EffectAssets>,
    mut effects: Query<(Entity, &mut HitEffect, &mut Transform, Option<&Children>)>,
    mut parts: Query<(&BaseMaterial, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    for (entity, mut fx, mut tf, children) in &mut effects {
        fx.elapsed += time.delta_secs();
        let t = fx.elapsed / fx.style.duration_secs();
        let done = t >= 1.0;
        if done && fx.killed {
            commands.entity(entity).despawn();
            continue;
        }
        let material = match fx.style {
            _ if done => None,
            TargetEffect::Flash => Some(&assets.flash),
            TargetEffect::Fade => {
                Some(&assets.fade[alpha_step(fade_alpha(t, fx.killed), ALPHA_STEPS)])
            }
            _ => None,
        };
        let meshes = std::iter::once(entity).chain(children.into_iter().flatten().copied());
        for part in meshes {
            if let Ok((base, mut current)) = parts.get_mut(part) {
                let wanted = material.unwrap_or(&base.0);
                if current.0 != *wanted {
                    current.0 = wanted.clone();
                }
            }
        }
        if fx.style == TargetEffect::Pop {
            tf.scale = fx.base_scale * if done { 1.0 } else { pop_scale(t) };
        }
        if done {
            commands.entity(entity).remove::<HitEffect>();
        }
    }
}

fn animate_rings(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<EffectAssets>,
    camera: Query<&GlobalTransform, With<MainCamera>>,
    mut rings: Query<(
        Entity,
        &mut RingBurst,
        &mut Transform,
        &mut MeshMaterial3d<StandardMaterial>,
    )>,
) {
    let eye = camera.single().map(|c| c.translation()).ok();
    for (entity, mut ring, mut tf, mut material) in &mut rings {
        ring.elapsed += time.delta_secs();
        let t = ring.elapsed / TargetEffect::Ring.duration_secs();
        if t >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let (scale, alpha) = ring_scale_alpha(t);
        tf.scale = Vec3::splat(ring.radius * scale);
        // The torus lies in the XZ plane; turn its axis towards the player.
        if let Some(dir) = eye.and_then(|e| (e - tf.translation).try_normalize()) {
            tf.rotation = Quat::from_rotation_arc(Vec3::Y, dir);
        }
        let wanted = &assets.ring[alpha_step(alpha, ALPHA_STEPS)];
        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::assert_is_system;

    #[test]
    fn systems_have_no_conflicting_access() {
        assert_is_system(setup_effect_assets);
        assert_is_system(refresh_fade_color);
        assert_is_system(animate_hit_effects);
        assert_is_system(animate_rings);
    }

    #[test]
    fn a_kill_folds_into_the_hit_of_the_same_frame() {
        let mut struck = Vec::new();
        record(&mut struck, 7, Some(HitZone::Head), false);
        record(&mut struck, 7, None, true);
        record(&mut struck, 8, Some(HitZone::Body), false);
        assert_eq!(
            struck,
            [
                Struck {
                    id: 7,
                    zone: Some(HitZone::Head),
                    killed: true
                },
                Struck {
                    id: 8,
                    zone: Some(HitZone::Body),
                    killed: false
                },
            ]
        );
    }

    #[test]
    fn effects_center_on_the_part_that_was_hit() {
        let sphere = Shape::Sphere { radius: 0.3 };
        assert_eq!(anchor(&sphere, Some(HitZone::Sphere)), (0.0, 0.3));
        let body = Shape::Humanoid { head_only: false };
        let (head_y, _) = anchor(&body, Some(HitZone::Head));
        let (torso_y, _) = anchor(&body, Some(HitZone::Body));
        let (legs_y, _) = anchor(&body, Some(HitZone::Legs));
        assert!(head_y > torso_y && torso_y > legs_y && legs_y > 0.0);
    }
}
