//! Adaptive skill ladder of a deathmatch: the bots' aimer preset steps up after a streak
//! of kills and down after deaths, bounded between two presets. Deterministic: it only
//! counts kills and deaths. See `docs/maps.md`.

use super::def::bot_profile;
use super::squad::preset_names;
use aim_core::sim::AimerProfile;
use serde::{Deserialize, Serialize};

/// `[mode.adaptive]`: the bounds and the streaks that move the bots' skill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdaptiveLadder {
    /// The weakest preset the bots step down to.
    pub min_skill: String,
    /// The strongest preset the bots step up to.
    pub max_skill: String,
    /// Kills in a row, without dying, that step the skill up. Default 3.
    #[serde(default = "three")]
    pub kills_to_step_up: u32,
    /// Deaths in a row, without a kill, that step the skill down. Default 2.
    #[serde(default = "two")]
    pub deaths_to_step_down: u32,
}

fn three() -> u32 {
    3
}

fn two() -> u32 {
    2
}

/// Where the ladder stands, for the HUD and the results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdaptiveStatus {
    /// The bots' current preset.
    pub skill: &'static str,
    /// The strongest preset reached in the run.
    pub peak: &'static str,
    pub steps_up: u32,
    pub steps_down: u32,
}

fn level_of(name: &str) -> Option<usize> {
    AimerProfile::PRESETS.iter().position(|p| p.name == name)
}

/// The ladder while a run lasts.
#[derive(Debug, Clone)]
pub(crate) struct Ladder {
    min: usize,
    max: usize,
    level: usize,
    peak: usize,
    up_after: u32,
    down_after: u32,
    kills: u32,
    deaths: u32,
    steps_up: u32,
    steps_down: u32,
}

impl Ladder {
    /// A ladder starting at `start` (a validated preset name within the bounds).
    pub fn new(def: &AdaptiveLadder, start: &str) -> Self {
        let level = level_of(start).expect("validated preset");
        Self {
            min: level_of(&def.min_skill).expect("validated preset"),
            max: level_of(&def.max_skill).expect("validated preset"),
            level,
            peak: level,
            up_after: def.kills_to_step_up,
            down_after: def.deaths_to_step_down,
            kills: 0,
            deaths: 0,
            steps_up: 0,
            steps_down: 0,
        }
    }

    /// The bots' current preset.
    pub fn profile(&self) -> AimerProfile {
        AimerProfile::PRESETS[self.level]
    }

    /// The player killed a bot. Returns true when the skill stepped up.
    pub fn on_kill(&mut self) -> bool {
        self.deaths = 0;
        self.kills += 1;
        if self.kills < self.up_after {
            return false;
        }
        self.kills = 0;
        if self.level >= self.max {
            return false;
        }
        self.level += 1;
        self.peak = self.peak.max(self.level);
        self.steps_up += 1;
        true
    }

    /// The player died. Returns true when the skill stepped down.
    pub fn on_death(&mut self) -> bool {
        self.kills = 0;
        self.deaths += 1;
        if self.deaths < self.down_after {
            return false;
        }
        self.deaths = 0;
        if self.level <= self.min {
            return false;
        }
        self.level -= 1;
        self.steps_down += 1;
        true
    }

    pub fn status(&self) -> AdaptiveStatus {
        AdaptiveStatus {
            skill: AimerProfile::PRESETS[self.level].name,
            peak: AimerProfile::PRESETS[self.peak].name,
            steps_up: self.steps_up,
            steps_down: self.steps_down,
        }
    }
}

pub(super) fn validate_adaptive(
    a: &AdaptiveLadder,
    bot_skill: &str,
    push: &mut impl FnMut(&'static str, String),
) {
    let mut preset = |field: &'static str, name: &str| {
        let level = level_of(name);
        if bot_profile(name).is_none() {
            push(field, format!("'{name}' is not one of {}", preset_names()));
        }
        level
    };
    let min = preset("mode.adaptive.min_skill", &a.min_skill);
    let max = preset("mode.adaptive.max_skill", &a.max_skill);
    if let (Some(min), Some(max)) = (min, max) {
        if min > max {
            push(
                "mode.adaptive",
                "min_skill must not be stronger than max_skill".into(),
            );
        } else if level_of(bot_skill).is_some_and(|l| !(min..=max).contains(&l)) {
            push(
                "mode.adaptive",
                format!("bot_skill '{bot_skill}' must be within min_skill..=max_skill"),
            );
        }
    }
    if a.kills_to_step_up == 0 {
        push("mode.adaptive.kills_to_step_up", "must be >= 1".into());
    }
    if a.deaths_to_step_down == 0 {
        push("mode.adaptive.deaths_to_step_down", "must be >= 1".into());
    }
}
