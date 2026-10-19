//! Bot personalities of a deathmatch: squad roles built on the behavior rules, and per-bot
//! skill. See `docs/maps.md`.

use super::def::{BehaviorRule, BotBehavior, MAX_BOTS, RuleConditions, bot_profile};
use aim_core::sim::AimerProfile;
use serde::{Deserialize, Serialize};

/// The personality of a roaming deathmatch bot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BotRole {
    /// The plain roaming bot: roams, stops to fight, falls back to cover when shot.
    Roamer,
    /// Hunts the player: once it has seen them (or been shot), it walks to where it last
    /// knew them to be while it cannot see them, and strafes instead of falling back.
    Pusher,
    /// Holds the angle from its spawn without moving; under half health it falls back to
    /// cover and stays hidden until it sees the player.
    AngleHolder,
    /// Roams and strafes (ADAD) while it fights, falls back to cover when shot and
    /// re-peeks: the jiggle of a player who never stands still in a fight.
    JigglePeeker,
}

const ANY: RuleConditions = RuleConditions {
    health_below: None,
    damaged_within_secs: None,
    alive_over_secs: None,
    sees_player: None,
    distance_below_m: None,
    distance_above_m: None,
    player_speed_above: None,
    player_speed_below: None,
};

const fn rule(when: RuleConditions, behavior: BotBehavior) -> BehaviorRule {
    BehaviorRule {
        when,
        behavior: Some(behavior),
        speed_multiplier: None,
    }
}

const PUSHER: [BehaviorRule; 2] = [
    rule(
        RuleConditions {
            sees_player: Some(false),
            ..ANY
        },
        BotBehavior::Peek,
    ),
    rule(ANY, BotBehavior::Strafe),
];

const ANGLE_HOLDER: [BehaviorRule; 2] = [
    rule(
        RuleConditions {
            health_below: Some(0.5),
            ..ANY
        },
        BotBehavior::Retreat,
    ),
    rule(ANY, BotBehavior::Hold),
];

impl BotRole {
    /// The behavior rules of the role, checked after the arena's own `[[mode.rules]]`.
    pub(crate) fn rules(self) -> &'static [BehaviorRule] {
        match self {
            Self::Roamer | Self::JigglePeeker => &[],
            Self::Pusher => &PUSHER,
            Self::AngleHolder => &ANGLE_HOLDER,
        }
    }

    /// Whether the role strafes while it fights even without `bot_strafe`.
    pub(crate) fn strafes(self) -> bool {
        self == Self::JigglePeeker
    }
}

/// One group of a deathmatch squad: `count` bots with the same role and skill.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SquadEntry {
    pub role: BotRole,
    /// An aimer preset name; default the arena's `bot_skill`.
    #[serde(default)]
    pub skill: Option<String>,
    /// Bots in the group, at least 1. Default 1.
    #[serde(default = "one")]
    pub count: u32,
}

fn one() -> u32 {
    1
}

/// The role and fixed skill (if any) of each bot slot, in squad order.
pub(crate) fn slots(squad: &[SquadEntry]) -> Vec<(BotRole, Option<AimerProfile>)> {
    squad
        .iter()
        .flat_map(|e| {
            let skill = e.skill.as_deref().and_then(bot_profile);
            std::iter::repeat_n((e.role, skill), e.count as usize)
        })
        .collect()
}

pub(super) fn preset_names() -> String {
    AimerProfile::PRESETS
        .iter()
        .map(|p| p.name)
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn validate_squad(
    squad: &[SquadEntry],
    bots: u32,
    push: &mut impl FnMut(&'static str, String),
) {
    if squad.is_empty() {
        return;
    }
    for (i, e) in squad.iter().enumerate() {
        if !(1..=MAX_BOTS).contains(&e.count) {
            push(
                "mode.squad",
                format!("entry {}: count must be in 1..={MAX_BOTS}", i + 1),
            );
        }
        if let Some(skill) = &e.skill
            && bot_profile(skill).is_none()
        {
            push(
                "mode.squad",
                format!(
                    "entry {}: skill '{skill}' is not one of {}",
                    i + 1,
                    preset_names()
                ),
            );
        }
    }
    let total: u32 = squad.iter().map(|e| e.count).sum();
    if total != bots {
        push(
            "mode.squad",
            format!("the counts add up to {total}, but bots = {bots}"),
        );
    }
}
