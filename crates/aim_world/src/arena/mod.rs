//! Arena activities: bots on an original map, driven by the same
//! [`aim_core::scenario::FrameInput`] as the scenario session.
//!
//! An arena ([`ArenaDef`], loaded from TOML) names a map, the player's weapon and a mode:
//! deathmatch against roaming bots (optionally a site hold, [`SiteObjective`]), or clearing a
//! list of angle markers in order. Bot behavior lives in small, testable state machines. The
//! session is pure and deterministic for a given definition, map, seed and input sequence.
//! See `docs/maps.md` for the format, the bot model and the simplifications.

mod aim;
mod behavior;
mod bot;
pub mod builtin;
mod def;
#[cfg(test)]
mod duel_tests;
#[cfg(test)]
mod rules_tests;
mod session;
#[cfg(test)]
mod tests;

pub use behavior::{
    COVER_HOLD_SECS, PLAYER_STRAFE_MIN_SPEED, PUSH_ADVANCE_SECS, PUSH_DUEL_SECS, STRAFE_SECS,
};
pub use bot::{
    BOT_HEALTH, BOT_NOTICE_HALF_ANGLE_DEG, PEEK_DELAY_SECS, RETREAT_MAX_ROUTE_M, ROAM_PAUSE_SECS,
};
pub use def::{
    ArenaDef, ArenaError, ArenaMode, BehaviorRule, BotBehavior, MAX_ATTACKERS, MAX_BOTS, MAX_RULES,
    PRACTICE_SUFFIX, ReactiveBots, RuleConditions, ShootBackDeadline, SiteObjective, bot_profile,
    split_practice,
};
pub use session::{
    ArenaEvent, ArenaSession, ArenaStats, BOT_WEAPON, BotView, DEATH_POINTS, HEADSHOT_BONUS,
    KILL_POINTS, ObjectiveStatus, PLAYER_HEALTH, PRE_AIM_POINTS, RESPAWN_GRACE_SECS,
    SITE_HELD_BONUS,
};
