//! Volume categories: every sound belongs to one, and plays at the master volume scaled by
//! its category's volume.

use crate::persistence::settings::SoundSettings;

/// The volume slider a sound follows, on top of the master volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    /// The player's own shots: hit, kill and miss.
    Shots,
    /// Sounds of the arena around the player: bot gunfire and footsteps.
    World,
    /// Menu clicks.
    Ui,
    /// Damage taken, death, streaks, round and timer cues, and perception-drill cues.
    Cues,
}

impl Category {
    /// The category's volume, percent of the master volume.
    pub fn percent(self, sound: &SoundSettings) -> u8 {
        match self {
            Self::Shots => sound.shots_percent,
            Self::World => sound.world_percent,
            Self::Ui => sound.ui_percent,
            Self::Cues => sound.cues_percent,
        }
    }
}

/// Linear gain of a sound in `category`: master × category, on the squared curve of
/// [`super::gain`]. A category at 100 % plays at the master volume alone.
pub fn category_gain(sound: &SoundSettings, category: Category) -> f32 {
    // (master × category)² = master² × category²: the same curve on both sliders.
    super::gain(sound.volume_percent) * super::gain(category.percent(sound))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::gain;

    #[test]
    fn a_full_category_plays_at_the_master_volume() {
        let s = SoundSettings::default();
        for c in [
            Category::Shots,
            Category::World,
            Category::Ui,
            Category::Cues,
        ] {
            assert_eq!(category_gain(&s, c), gain(s.volume_percent), "{c:?}");
        }
    }

    #[test]
    fn the_category_scales_the_master_volume() {
        let s = SoundSettings {
            volume_percent: 100,
            world_percent: 50,
            cues_percent: 0,
            ..SoundSettings::default()
        };
        // Master × category, then the squared curve: 0.5² = 0.25.
        assert_eq!(category_gain(&s, Category::World), 0.25);
        assert_eq!(category_gain(&s, Category::Cues), 0.0);
        assert_eq!(category_gain(&s, Category::Shots), 1.0);
        let half = SoundSettings {
            volume_percent: 50,
            ui_percent: 50,
            ..SoundSettings::default()
        };
        assert_eq!(category_gain(&half, Category::Ui), 0.0625);
    }

    #[test]
    fn values_above_100_are_clamped() {
        let s = SoundSettings {
            volume_percent: 200,
            shots_percent: 250,
            ..SoundSettings::default()
        };
        assert_eq!(category_gain(&s, Category::Shots), 1.0);
    }
}
