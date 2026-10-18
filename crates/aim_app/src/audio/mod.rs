//! Sound effects for shots: every clip is synthesized once at startup (`synth`), then
//! played on the frame's strongest [`ShotFeedback`] and on settings previews.

pub mod synth;

use crate::game::feedback::{ShotFeedback, ShotOutcome, strongest};
use crate::persistence::settings::{Settings, SoundSettings};
use bevy::audio::{AddAudioSource, ChannelCount, Decodable, Sample, SampleRate, Source, Volume};
use bevy::prelude::*;
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

/// Mono samples rendered in memory, played without any decoder.
#[derive(Asset, TypePath, Debug, Clone)]
pub struct SynthClip {
    samples: Arc<[f32]>,
}

impl Decodable for SynthClip {
    type Decoder = ClipDecoder;

    fn decoder(&self) -> ClipDecoder {
        ClipDecoder {
            samples: self.samples.clone(),
            pos: 0,
        }
    }
}

/// Iterator over a [`SynthClip`]'s samples, as the audio mixer reads it.
pub struct ClipDecoder {
    samples: Arc<[f32]>,
    pos: usize,
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
        NonZero::<u16>::MIN
    }

    fn sample_rate(&self) -> SampleRate {
        NonZero::new(SAMPLE_RATE).expect("the sample rate is a non-zero constant")
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(
            self.samples.len() as f64 / f64::from(SAMPLE_RATE),
        ))
    }
}

/// Every clip, built once so playing one never synthesizes or allocates samples.
#[derive(Resource, Default)]
struct SoundBank(HashMap<Clip, Handle<SynthClip>>);

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<SynthClip>()
            .add_message::<SoundPreview>()
            .add_systems(Startup, build_bank)
            // After `Update`, so a shot is heard from the frame it was fired in.
            .add_systems(PostUpdate, play_sounds);
    }
}

fn build_bank(mut commands: Commands, mut clips: ResMut<Assets<SynthClip>>) {
    let bank = Clip::all()
        .filter_map(|clip| {
            let samples = clip.samples()?;
            Some((
                clip,
                clips.add(SynthClip {
                    samples: samples.into(),
                }),
            ))
        })
        .collect();
    commands.insert_resource(SoundBank(bank));
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
    let volume = Volume::Linear(gain(sound.volume_percent));
    for clip in shot.into_iter().chain(preview) {
        if let Some(handle) = bank.0.get(&clip) {
            commands.spawn((
                AudioPlayer(handle.clone()),
                PlaybackSettings::DESPAWN.with_volume(volume),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::assert_is_system;

    #[test]
    fn span_len_counts_the_samples_left() {
        let mut decoder = ClipDecoder {
            samples: vec![0.0; 4].into(),
            pos: 0,
        };
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
