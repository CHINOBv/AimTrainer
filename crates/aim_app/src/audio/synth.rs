//! Procedural sound effects: short mono clips mixed from oscillators and filtered noise
//! with attack/decay envelopes. Pure and deterministic, so the clips are unit-tested and
//! the app ships no audio files (see `docs/decisions.md`).

use aim_core::rng::Rng;
use serde::{Deserialize, Serialize};
use std::f32::consts::TAU;

/// Output rate of every clip, Hz.
pub const SAMPLE_RATE: u32 = 44_100;
/// Pitch ratio of the headshot variant of a hit sound (about four semitones up).
pub const HEAD_PITCH: f32 = 1.26;
/// Decay time constants a voice rings for before it is cut (e^-6 ≈ 0.25 %).
const DECAY_SPAN: f32 = 6.0;
/// Linear fade at the end of a clip, so a cut never clicks.
const FADE_OUT_SECS: f32 = 0.003;
/// Fixed noise seed: the same preset always sounds the same.
const NOISE_SEED: u64 = 0x5eed_50d5;

/// Oscillator shape of a voice. `Noise` is white noise through a one-pole low-pass whose
/// cutoff is the voice frequency.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Wave {
    Sine,
    Triangle,
    Square,
    Noise,
}

/// One layer of a sound: an oscillator gliding from `start_hz` to `end_hz` (exponentially,
/// with the decay's time constant) under a linear attack and an exponential decay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Voice {
    pub wave: Wave,
    pub start_hz: f32,
    pub end_hz: f32,
    pub gain: f32,
    pub delay_secs: f32,
    pub attack_secs: f32,
    /// Time constant of the exponential decay, seconds.
    pub decay_secs: f32,
}

impl Voice {
    const fn new(wave: Wave, hz: f32, gain: f32, attack_secs: f32, decay_secs: f32) -> Self {
        Self {
            wave,
            start_hz: hz,
            end_hz: hz,
            gain,
            delay_secs: 0.0,
            attack_secs,
            decay_secs,
        }
    }

    const fn glide_to(mut self, end_hz: f32) -> Self {
        self.end_hz = end_hz;
        self
    }

    const fn delayed(mut self, delay_secs: f32) -> Self {
        self.delay_secs = delay_secs;
        self
    }

    fn end_secs(&self) -> f32 {
        self.delay_secs + self.attack_secs + DECAY_SPAN * self.decay_secs
    }
}

/// A sound: its voices and the peak level the mix is normalized to (0–1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Recipe {
    pub voices: &'static [Voice],
    pub peak: f32,
}

use Wave::{Noise, Sine, Square, Triangle};

/// Sound played when a shot hits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HitSound {
    Off,
    #[default]
    Click,
    Tick,
    Pop,
    Ding,
    Thump,
}

impl HitSound {
    pub const ALL: [Self; 6] = [
        Self::Off,
        Self::Click,
        Self::Tick,
        Self::Pop,
        Self::Ding,
        Self::Thump,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Click => "Click",
            Self::Tick => "Tick",
            Self::Pop => "Pop",
            Self::Ding => "Ding",
            Self::Thump => "Thump",
        }
    }

    pub fn recipe(self) -> Option<Recipe> {
        const PEAK: f32 = 0.7;
        let voices: &'static [Voice] = match self {
            Self::Off => return None,
            Self::Click => {
                const {
                    &[
                        Voice::new(Noise, 7000.0, 1.0, 0.0005, 0.004),
                        Voice::new(Square, 2200.0, 0.3, 0.0005, 0.003).glide_to(1800.0),
                    ]
                }
            }
            Self::Tick => {
                const {
                    &[
                        Voice::new(Sine, 3200.0, 1.0, 0.001, 0.012).glide_to(3000.0),
                        Voice::new(Triangle, 6400.0, 0.2, 0.001, 0.006),
                    ]
                }
            }
            Self::Pop => {
                const {
                    &[
                        Voice::new(Sine, 900.0, 1.0, 0.001, 0.02).glide_to(300.0),
                        Voice::new(Noise, 2000.0, 0.3, 0.0005, 0.005),
                    ]
                }
            }
            Self::Ding => {
                const {
                    &[
                        Voice::new(Sine, 1760.0, 1.0, 0.001, 0.08),
                        Voice::new(Sine, 2640.0, 0.4, 0.001, 0.05),
                        Voice::new(Sine, 3520.0, 0.2, 0.001, 0.03),
                    ]
                }
            }
            Self::Thump => {
                const {
                    &[
                        Voice::new(Sine, 180.0, 1.0, 0.002, 0.04).glide_to(70.0),
                        Voice::new(Noise, 800.0, 0.4, 0.001, 0.01),
                    ]
                }
            }
        };
        Some(Recipe { voices, peak: PEAK })
    }
}

/// Sound played when a target or bot is eliminated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KillSound {
    Off,
    #[default]
    Chime,
    Ping,
    Pluck,
    Burst,
}

impl KillSound {
    pub const ALL: [Self; 5] = [Self::Off, Self::Chime, Self::Ping, Self::Pluck, Self::Burst];

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Chime => "Chime",
            Self::Ping => "Ping",
            Self::Pluck => "Pluck",
            Self::Burst => "Burst",
        }
    }

    pub fn recipe(self) -> Option<Recipe> {
        const PEAK: f32 = 0.8;
        let voices: &'static [Voice] = match self {
            Self::Off => return None,
            Self::Chime => {
                const {
                    &[
                        Voice::new(Sine, 1320.0, 0.8, 0.001, 0.05),
                        Voice::new(Sine, 1980.0, 1.0, 0.002, 0.06).delayed(0.06),
                        Voice::new(Triangle, 3960.0, 0.15, 0.002, 0.04).delayed(0.06),
                    ]
                }
            }
            Self::Ping => {
                const {
                    &[
                        Voice::new(Sine, 2500.0, 1.0, 0.001, 0.07),
                        Voice::new(Sine, 5000.0, 0.15, 0.001, 0.04),
                    ]
                }
            }
            Self::Pluck => {
                const {
                    &[
                        Voice::new(Triangle, 660.0, 1.0, 0.001, 0.08).glide_to(640.0),
                        Voice::new(Square, 1320.0, 0.15, 0.001, 0.02),
                    ]
                }
            }
            Self::Burst => {
                const {
                    &[
                        Voice::new(Noise, 4000.0, 0.7, 0.001, 0.04),
                        Voice::new(Sine, 400.0, 1.0, 0.001, 0.05).glide_to(120.0),
                    ]
                }
            }
        };
        Some(Recipe { voices, peak: PEAK })
    }
}

/// A soft, low "whiff" for a missed shot; quieter than hits on purpose.
pub fn miss_recipe() -> Recipe {
    Recipe {
        voices: const { &[Voice::new(Noise, 1200.0, 1.0, 0.004, 0.02)] },
        peak: 0.35,
    }
}

/// Render `recipe` with every frequency scaled by `pitch`: mono samples at [`SAMPLE_RATE`],
/// normalized to the recipe's peak, starting and ending at silence.
pub fn render(recipe: &Recipe, pitch: f32) -> Vec<f32> {
    let rate = SAMPLE_RATE as f32;
    let secs = recipe
        .voices
        .iter()
        .map(Voice::end_secs)
        .fold(0.0, f32::max);
    let len = (secs * rate).ceil() as usize;
    let mut mix = vec![0.0f32; len];
    let mut rng = Rng::new(NOISE_SEED);
    for voice in recipe.voices {
        add_voice(&mut mix, voice, pitch, &mut rng);
    }
    let fade = ((FADE_OUT_SECS * rate) as usize).min(len);
    for (i, s) in mix[len - fade..].iter_mut().enumerate() {
        *s *= 1.0 - (i + 1) as f32 / fade as f32;
    }
    let peak = mix.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak > 0.0 {
        let k = recipe.peak / peak;
        mix.iter_mut().for_each(|s| *s *= k);
    }
    mix
}

fn add_voice(mix: &mut [f32], v: &Voice, pitch: f32, rng: &mut Rng) {
    let rate = SAMPLE_RATE as f32;
    let start = (v.delay_secs * rate) as usize;
    let (f0, f1) = (v.start_hz * pitch, v.end_hz * pitch);
    let mut phase = 0.0f32;
    let mut lowpass = 0.0f32;
    let smoothing = 1.0 - (-TAU * f0 / rate).exp();
    for (i, out) in mix.iter_mut().skip(start).enumerate() {
        let t = i as f32 / rate;
        let envelope = if t < v.attack_secs {
            t / v.attack_secs
        } else {
            (-(t - v.attack_secs) / v.decay_secs).exp()
        };
        let freq = f1 + (f0 - f1) * (-t / v.decay_secs).exp();
        let sample = match v.wave {
            Sine => (TAU * phase).sin(),
            Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
            Square => {
                if phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Noise => {
                lowpass += smoothing * (rng.next_f32() * 2.0 - 1.0 - lowpass);
                lowpass
            }
        };
        phase = (phase + freq / rate).fract();
        *out += v.gain * envelope * sample;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_recipe() -> Vec<(&'static str, Recipe)> {
        let hits = HitSound::ALL
            .iter()
            .filter_map(|s| s.recipe().map(|r| (s.label(), r)));
        let kills = KillSound::ALL
            .iter()
            .filter_map(|s| s.recipe().map(|r| (s.label(), r)));
        hits.chain(kills).chain([("Miss", miss_recipe())]).collect()
    }

    #[test]
    fn off_presets_have_no_sound() {
        assert_eq!(HitSound::Off.recipe(), None);
        assert_eq!(KillSound::Off.recipe(), None);
        assert_eq!(every_recipe().len(), 5 + 4 + 1);
    }

    #[test]
    fn every_preset_is_a_short_clip_that_starts_and_ends_silent() {
        for (name, recipe) in every_recipe() {
            let clip = render(&recipe, 1.0);
            let secs = clip.len() as f32 / SAMPLE_RATE as f32;
            assert!(secs > 0.005 && secs < 0.5, "{name}: {secs} s");
            assert!(clip[0].abs() < 0.01, "{name} starts with a click");
            assert!(
                clip[clip.len() - 1].abs() < 0.01,
                "{name} ends with a click"
            );
            let peak = clip.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!((peak - recipe.peak).abs() < 1e-3, "{name}: peak {peak}");
        }
    }

    #[test]
    fn rendering_is_deterministic() {
        for (name, recipe) in every_recipe() {
            assert_eq!(render(&recipe, 1.0), render(&recipe, 1.0), "{name}");
        }
    }

    #[test]
    fn presets_sound_different() {
        let clips: Vec<_> = every_recipe()
            .into_iter()
            .map(|(n, r)| (n, render(&r, 1.0)))
            .collect();
        for (i, (a, ca)) in clips.iter().enumerate() {
            for (b, cb) in &clips[i + 1..] {
                assert_ne!(ca, cb, "{a} and {b} are the same sound");
            }
        }
    }

    #[test]
    fn headshot_pitch_raises_the_tone_and_keeps_the_length() {
        let crossings = |c: &[f32]| c.windows(2).filter(|w| w[0] * w[1] < 0.0).count();
        let recipe = HitSound::Tick.recipe().unwrap();
        let (body, head) = (render(&recipe, 1.0), render(&recipe, HEAD_PITCH));
        assert_eq!(body.len(), head.len());
        assert!(crossings(&head) > crossings(&body));
    }
}
