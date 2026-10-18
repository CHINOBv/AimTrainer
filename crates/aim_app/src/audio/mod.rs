//! Sound effects: every clip is synthesized once at startup (`synth`). Shots play the
//! frame's strongest [`ShotFeedback`] and settings previews. Sound cues of perception
//! drills, bot gunfire and bot footsteps are panned to where they come from (`cue`).
//! Arena feedback, streaks, round cues and the low-time warning are decided in
//! `game_sounds`; every sound follows the master volume times its category (`mix`).

pub mod cue;
pub mod game_sounds;
pub mod mix;
pub mod synth;
pub mod ui_click;

use crate::game::arena::{ArenaFeedback, ArenaRun};
use crate::game::feedback::{ShotFeedback, ShotOutcome, strongest};
use crate::game::{AppState, Run};
use crate::persistence::settings::{Settings, SoundSettings};
use aim_core::math::ViewAngles;
use aim_core::scenario::cue::CueSound;
use bevy::audio::{AddAudioSource, ChannelCount, Decodable, Sample, SampleRate, Source, Volume};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};
pub use cue::CueHeard;
use game_sounds::{FootstepTracker, Play, Sfx, StreakTimer};
use mix::Category;
use std::collections::HashMap;
use std::num::NonZero;
use std::sync::Arc;
use std::time::Duration;
use synth::{HEAD_PITCH, HitSound, KillSound, SAMPLE_RATE};

/// One pre-built sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Clip {
    Hit { sound: HitSound, head: bool },
    Kill(KillSound),
    Miss,
}

impl Clip {
    /// Every clip that has a sound.
    fn all() -> impl Iterator<Item = Self> {
        let hits = HitSound::ALL
            .into_iter()
            .flat_map(|sound| [false, true].map(|head| Self::Hit { sound, head }));
        let kills = KillSound::ALL.into_iter().map(Self::Kill);
        hits.chain(kills)
            .chain([Self::Miss])
            .filter(|c| c.samples().is_some())
    }

    fn samples(self) -> Option<Vec<f32>> {
        let (recipe, pitch) = match self {
            Self::Hit { sound, head } => (sound.recipe()?, if head { HEAD_PITCH } else { 1.0 }),
            Self::Kill(sound) => (sound.recipe()?, 1.0),
            Self::Miss => (synth::miss_recipe(), 1.0),
        };
        Some(synth::render(&recipe, pitch))
    }
}

/// The clip a shot outcome plays with these settings, if any. A kill with the kill sound
/// off still sounds like the hit it was.
pub fn clip_for(outcome: ShotOutcome, sound: &SoundSettings) -> Option<Clip> {
    if !sound.enabled || sound.volume_percent == 0 {
        return None;
    }
    let hit = |head| {
        (sound.hit != HitSound::Off).then_some(Clip::Hit {
            sound: sound.hit,
            head,
        })
    };
    match outcome {
        ShotOutcome::Miss => sound.miss.then_some(Clip::Miss),
        ShotOutcome::Hit { head } => hit(head),
        ShotOutcome::Kill if sound.kill == KillSound::Off => hit(false),
        ShotOutcome::Kill => Some(Clip::Kill(sound.kill)),
    }
}

/// Linear gain for a volume percentage, on a squared curve so the slider feels even.
pub fn gain(volume_percent: u8) -> f32 {
    let v = f32::from(volume_percent.min(100)) / 100.0;
    v * v
}

/// A settings preview of `Clip`; plays even with sound effects disabled.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct SoundPreview(pub Clip);

/// Samples rendered in memory (mono, or interleaved stereo), played without any decoder.
#[derive(Asset, TypePath, Debug, Clone)]
pub struct SynthClip {
    samples: Arc<[f32]>,
    channels: NonZero<u16>,
}

impl SynthClip {
    fn mono(samples: Vec<f32>) -> Self {
        Self {
            samples: samples.into(),
            channels: NonZero::<u16>::MIN,
        }
    }

    fn stereo(interleaved: Vec<f32>) -> Self {
        Self {
            samples: interleaved.into(),
            channels: NonZero::new(2).expect("two is non-zero"),
        }
    }
}

impl Decodable for SynthClip {
    type Decoder = ClipDecoder;

    fn decoder(&self) -> ClipDecoder {
        ClipDecoder {
            samples: self.samples.clone(),
            pos: 0,
            channels: self.channels,
        }
    }
}

/// Iterator over a [`SynthClip`]'s samples, as the audio mixer reads it.
pub struct ClipDecoder {
    samples: Arc<[f32]>,
    pos: usize,
    channels: NonZero<u16>,
}

impl Iterator for ClipDecoder {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        let s = self.samples.get(self.pos)?;
        self.pos += 1;
        Some(*s)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = self.samples.len() - self.pos;
        (left, Some(left))
    }
}

impl Source for ClipDecoder {
    fn current_span_len(&self) -> Option<usize> {
        Some(self.samples.len().saturating_sub(self.pos))
    }

    fn channels(&self) -> ChannelCount {
        self.channels
    }

    fn sample_rate(&self) -> SampleRate {
        NonZero::new(SAMPLE_RATE).expect("the sample rate is a non-zero constant")
    }

    fn total_duration(&self) -> Option<Duration> {
        let frames = self.samples.len() / usize::from(self.channels.get());
        Some(Duration::from_secs_f64(
            frames as f64 / f64::from(SAMPLE_RATE),
        ))
    }
}

/// Every clip, built once so playing one never synthesizes or allocates samples.
#[derive(Resource, Default)]
struct SoundBank(HashMap<Clip, Handle<SynthClip>>);

/// The mono sound of every cue, synthesized once; each cue is panned from it when heard.
#[derive(Resource, Default)]
struct CueBank(HashMap<CueSound, Vec<f32>>);

/// Every game-feel clip, built once; positional ones also keep their mono samples to be
/// panned when played.
#[derive(Resource, Default)]
struct SfxBank {
    clips: HashMap<Sfx, Handle<SynthClip>>,
    mono: HashMap<Sfx, Vec<f32>>,
}

impl SfxBank {
    /// Play `p`: panned from where it comes from, heard by `listener`, or centered.
    fn play(
        &self,
        commands: &mut Commands,
        clips: &mut Assets<SynthClip>,
        p: Play,
        listener: Option<(aim_core::math::Vec3, ViewAngles)>,
    ) {
        let placed = p.from.zip(listener).zip(self.mono.get(&p.sfx));
        let handle = match placed {
            Some(((from, (eye, view)), mono)) => {
                let (offset_deg, distance_m) = cue::bearing(eye, view, from);
                let placement = cue::place_at(offset_deg, distance_m);
                clips.add(SynthClip::stereo(cue::stereo(mono, placement)))
            }
            None => match self.clips.get(&p.sfx) {
                Some(handle) => handle.clone(),
                None => return,
            },
        };
        spawn_clip(commands, handle, p.gain);
    }
}

fn spawn_clip(commands: &mut Commands, handle: Handle<SynthClip>, gain: f32) {
    commands.spawn((
        AudioPlayer(handle),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(gain)),
    ));
}

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<SynthClip>()
            .add_message::<SoundPreview>()
            .add_systems(Startup, build_bank)
            // After `Update`, so a shot is heard from the frame it was fired in.
            .add_systems(PostUpdate, (play_sounds, play_cues, play_arena_feedback))
            .add_systems(
                PostUpdate,
                (play_footsteps, play_low_time).run_if(in_state(AppState::Playing)),
            )
            .add_systems(EguiPrimaryContextPass, play_ui_clicks);
    }
}

fn build_bank(mut commands: Commands, mut clips: ResMut<Assets<SynthClip>>) {
    let bank = Clip::all()
        .filter_map(|clip| Some((clip, clips.add(SynthClip::mono(clip.samples()?)))))
        .collect();
    commands.insert_resource(SoundBank(bank));
    let cues = [CueSound::Footsteps, CueSound::Gunshot]
        .into_iter()
        .map(|s| (s, synth::render(&synth::cue_recipe(s), 1.0)))
        .collect();
    commands.insert_resource(CueBank(cues));
    let mut sfx = SfxBank::default();
    for s in Sfx::all() {
        let (effect, pitch) = s.effect();
        let samples = synth::render(&effect.recipe(), pitch);
        if matches!(s, Sfx::BotGunshot | Sfx::Footstep) {
            sfx.mono.insert(s, samples.clone());
        }
        sfx.clips.insert(s, clips.add(SynthClip::mono(samples)));
    }
    commands.insert_resource(sfx);
}

/// Plays each sound cue of a perception drill, panned to where it comes from. Cues are
/// the drill itself, so they play even with shot sound effects off; only a zero volume
/// (master or cues) silences them.
fn play_cues(
    mut commands: Commands,
    settings: Res<Settings>,
    bank: Option<Res<CueBank>>,
    mut clips: ResMut<Assets<SynthClip>>,
    mut heard: MessageReader<CueHeard>,
) {
    let Some(bank) = bank else { return };
    let gain = mix::category_gain(&settings.sound, Category::Cues);
    let volume = Volume::Linear(gain);
    for cue in heard.read() {
        let Some(mono) = bank.0.get(&cue.sound) else {
            continue;
        };
        if gain == 0.0 {
            continue;
        }
        let handle = clips.add(SynthClip::stereo(cue::stereo(mono, cue::place(cue))));
        commands.spawn((
            AudioPlayer(handle),
            PlaybackSettings::DESPAWN.with_volume(volume),
        ));
    }
}

fn play_sounds(
    mut commands: Commands,
    settings: Res<Settings>,
    bank: Option<Res<SoundBank>>,
    mut shots: MessageReader<ShotFeedback>,
    mut previews: MessageReader<SoundPreview>,
) {
    let Some(bank) = bank else { return };
    let sound = &settings.sound;
    let shot = strongest(shots.read().map(|s| s.0)).and_then(|o| clip_for(o, sound));
    let preview = previews.read().last().map(|p| p.0);
    let gain = mix::category_gain(sound, Category::Shots);
    for clip in shot.into_iter().chain(preview) {
        if let Some(handle) = bank.0.get(&clip) {
            spawn_clip(&mut commands, handle.clone(), gain);
        }
    }
}

/// The player's eye and view in a running arena, for positional sounds.
fn arena_listener(run: Option<&ArenaRun>) -> Option<(aim_core::math::Vec3, ViewAngles)> {
    run.map(|r| (r.session.eye_position(), r.view))
}

/// Plays the sounds of arena feedback: bot gunfire from the bot, damage, death, streaks
/// and round cues.
fn play_arena_feedback(
    mut commands: Commands,
    settings: Res<Settings>,
    bank: Option<Res<SfxBank>>,
    mut clips: ResMut<Assets<SynthClip>>,
    run: Option<Res<ArenaRun>>,
    mut feedback: MessageReader<ArenaFeedback>,
    mut streaks: Local<StreakTimer>,
) {
    let Some(bank) = bank else { return };
    let now = run.as_ref().map_or(0.0, |r| r.session.time());
    let listener = arena_listener(run.as_deref());
    for f in feedback.read() {
        if let Some(p) = game_sounds::feedback_sound(f, &settings.sound, now, &mut streaks) {
            bank.play(&mut commands, &mut clips, p, listener);
        }
    }
}

/// Plays the steps of running bots near the player, from where each bot is.
fn play_footsteps(
    mut commands: Commands,
    settings: Res<Settings>,
    bank: Option<Res<SfxBank>>,
    mut clips: ResMut<Assets<SynthClip>>,
    run: Option<Res<ArenaRun>>,
    mut tracker: Local<FootstepTracker>,
) {
    let (Some(bank), Some(run)) = (bank, run) else {
        return;
    };
    let (eye, view) = (run.session.eye_position(), run.view);
    let bots = run.session.bots().map(|b| (b.id, b.feet, b.alive));
    let steps = tracker.update(run.session.time(), eye, bots);
    let Some(gain) = game_sounds::world_gain(&settings.sound, settings.sound.footsteps) else {
        return;
    };
    for feet in steps {
        let p = Play {
            sfx: Sfx::Footstep,
            gain,
            from: Some(feet),
        };
        bank.play(&mut commands, &mut clips, p, Some((eye, view)));
    }
}

/// Ticks each of the last seconds of a scenario or arena run.
fn play_low_time(
    mut commands: Commands,
    settings: Res<Settings>,
    bank: Option<Res<SfxBank>>,
    scenario: Option<Res<Run>>,
    arena: Option<Res<ArenaRun>>,
    mut last_left: Local<Option<f32>>,
) {
    let left = arena
        .map(|r| r.session.remaining())
        .or_else(|| scenario.map(|r| r.session.remaining()));
    let before = std::mem::replace(&mut *last_left, left);
    let (Some(bank), Some(left)) = (bank, left) else {
        return;
    };
    if !game_sounds::low_time_tick(before, left) {
        return;
    }
    let sound = &settings.sound;
    let gain = game_sounds::audible(sound, Category::Cues, sound.low_time_warning);
    if let (Some(gain), Some(handle)) = (gain, bank.clips.get(&Sfx::LowTimeTick)) {
        spawn_clip(&mut commands, handle.clone(), gain);
    }
}

/// Plays one click for menu buttons clicked this frame (see `ui_click`), when enabled.
fn play_ui_clicks(
    mut contexts: EguiContexts,
    mut commands: Commands,
    settings: Res<Settings>,
    bank: Option<Res<SfxBank>>,
) -> Result {
    let clicked = ui_click::take(contexts.ctx_mut()?);
    let sound = &settings.sound;
    let gain = game_sounds::audible(sound, Category::Ui, sound.ui_clicks);
    let handle = bank.as_ref().and_then(|b| b.clips.get(&Sfx::UiClick));
    if let (true, Some(gain), Some(handle)) = (clicked, gain, handle) {
        spawn_clip(&mut commands, handle.clone(), gain);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::assert_is_system;

    #[test]
    fn span_len_counts_the_samples_left() {
        let mut decoder = SynthClip::mono(vec![0.0; 4]).decoder();
        assert_eq!(decoder.current_span_len(), Some(4));
        decoder.next();
        assert_eq!(decoder.current_span_len(), Some(3));
        decoder.by_ref().for_each(drop);
        assert_eq!(decoder.current_span_len(), Some(0));
    }

    #[test]
    fn systems_have_no_conflicting_access() {
        assert_is_system(build_bank);
        assert_is_system(play_sounds);
        assert_is_system(play_cues);
        assert_is_system(play_arena_feedback);
        assert_is_system(play_footsteps);
        assert_is_system(play_low_time);
        assert_is_system::<(), Result, _>(play_ui_clicks);
    }

    #[test]
    fn stereo_clips_report_two_channels_and_their_length_in_frames() {
        let clip = SynthClip::stereo(vec![0.0; 2 * SAMPLE_RATE as usize]);
        let decoder = clip.decoder();
        assert_eq!(decoder.channels().get(), 2);
        assert_eq!(decoder.total_duration(), Some(Duration::from_secs(1)));
        let mono = SynthClip::mono(vec![0.0; SAMPLE_RATE as usize]).decoder();
        assert_eq!(mono.channels().get(), 1);
        assert_eq!(mono.total_duration(), Some(Duration::from_secs(1)));
    }

    #[test]
    fn every_clip_with_a_sound_is_prebuilt() {
        // 5 hit sounds × body/head, 4 kill sounds and the miss.
        assert_eq!(Clip::all().count(), 5 * 2 + 4 + 1);
        assert!(Clip::all().all(|c| c.samples().is_some_and(|s| !s.is_empty())));
    }

    #[test]
    fn outcomes_pick_the_configured_clip() {
        let s = SoundSettings::default();
        let hit = |head| Some(Clip::Hit { sound: s.hit, head });
        assert_eq!(clip_for(ShotOutcome::Hit { head: true }, &s), hit(true));
        assert_eq!(clip_for(ShotOutcome::Kill, &s), Some(Clip::Kill(s.kill)));
        assert_eq!(clip_for(ShotOutcome::Miss, &s), None, "miss is opt-in");
        let miss_on = SoundSettings {
            miss: true,
            ..s.clone()
        };
        assert_eq!(clip_for(ShotOutcome::Miss, &miss_on), Some(Clip::Miss));
    }

    #[test]
    fn a_kill_without_a_kill_sound_plays_the_hit_sound() {
        let s = SoundSettings {
            kill: KillSound::Off,
            ..SoundSettings::default()
        };
        assert_eq!(
            clip_for(ShotOutcome::Kill, &s),
            Some(Clip::Hit {
                sound: s.hit,
                head: false
            })
        );
        let silent = SoundSettings {
            hit: HitSound::Off,
            ..s
        };
        assert_eq!(clip_for(ShotOutcome::Kill, &silent), None);
    }

    #[test]
    fn disabled_or_muted_sound_plays_nothing() {
        let off = SoundSettings {
            enabled: false,
            ..SoundSettings::default()
        };
        let muted = SoundSettings {
            volume_percent: 0,
            ..SoundSettings::default()
        };
        for s in [off, muted] {
            assert_eq!(clip_for(ShotOutcome::Kill, &s), None);
        }
    }

    #[test]
    fn volume_follows_a_squared_curve() {
        assert_eq!(gain(0), 0.0);
        assert_eq!(gain(50), 0.25);
        assert_eq!(gain(100), 1.0);
        assert_eq!(gain(200), 1.0, "values above 100 are clamped");
    }
}
